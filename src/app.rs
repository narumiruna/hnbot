use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::article::{Article, ArticleError, generate_article};
use crate::config::Settings;
use crate::content::html_to_markdown;
use crate::http::{
    HttpFailure, RequestPacer, reqwest_error_message, response_bytes, retry_transient,
};
use crate::openai::OpenAiClient;
use crate::rss::{FeedError, FeedSource, HnEntry, HnFeedSource};
use crate::store::{DedupeStore, RedisStore, StoreError};
use crate::telegram::{Notifier, TelegramClient, TelegramError, build_message};
use crate::telegraph::{PagePublisher, TelegraphClient, TelegraphError};

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Feed(#[from] FeedError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Telegram(#[from] TelegramError),
    #[error("one or more entries failed: {0}")]
    Entries(String),
    #[error("HTTP client configuration failed: {0}")]
    Client(String),
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error(transparent)]
    Article(#[from] ArticleError),
    #[error(transparent)]
    Telegraph(#[from] TelegraphError),
}

#[async_trait]
pub trait CommentSource: Send + Sync {
    async fn fetch(&self, entry: &HnEntry) -> Result<String, HttpFailure>;
}

#[async_trait]
pub trait ArticleProcessor: Send + Sync {
    async fn generate_and_publish(&self, content: &str)
    -> Result<(Article, String), PipelineError>;
}

pub struct HnCommentSource {
    client: Client,
    base_url: String,
    pacer: RequestPacer,
    cooldown: Duration,
}

impl HnCommentSource {
    pub fn new(
        client: Client,
        base_url: String,
        min_interval: Duration,
        cooldown: Duration,
    ) -> Self {
        Self {
            client,
            base_url,
            pacer: RequestPacer::new(min_interval),
            cooldown,
        }
    }
}

#[async_trait]
impl CommentSource for HnCommentSource {
    async fn fetch(&self, entry: &HnEntry) -> Result<String, HttpFailure> {
        let client = self.client.clone();
        let url = comment_api_url(&self.base_url, &entry.id)?;
        let pacer = self.pacer.clone();
        let cooldown = self.cooldown;
        let bytes = retry_transient(|| {
            let client = client.clone();
            let url = url.clone();
            let pacer = pacer.clone();
            async move {
                pacer.wait().await;
                let response = client
                    .get(url)
                    .send()
                    .await
                    .map_err(|error| HttpFailure::Transport(reqwest_error_message(&error)))?;
                match response_bytes(response).await {
                    Err(error) if error.status() == Some(429) => {
                        let wait = error
                            .retry_after(std::time::SystemTime::now())
                            .unwrap_or(cooldown);
                        pacer.defer(wait).await;
                        Err(error)
                    }
                    result => result,
                }
            }
        })
        .await?;
        render_comment_api_payload(&bytes)
    }
}

#[derive(Deserialize)]
struct CommentApiItem {
    title: Option<String>,
    text: Option<String>,
    children: Option<Vec<CommentApiComment>>,
}

#[derive(Deserialize)]
struct CommentApiComment {
    author: Option<String>,
    text: Option<String>,
    children: Option<Vec<CommentApiComment>>,
}

fn comment_api_url(base_url: &str, entry_id: &str) -> Result<Url, HttpFailure> {
    let mut url = Url::parse(base_url).map_err(|error| {
        HttpFailure::Transport(format!("invalid HN comments API base URL: {error}"))
    })?;
    url.set_query(None);
    url.set_fragment(None);
    url.path_segments_mut()
        .map_err(|_| HttpFailure::Transport("HN comments API URL cannot be a base".to_owned()))?
        .pop_if_empty()
        .push(entry_id);
    Ok(url)
}

fn render_comment_api_payload(bytes: &[u8]) -> Result<String, HttpFailure> {
    let item: CommentApiItem = serde_json::from_slice(bytes).map_err(|error| {
        HttpFailure::Transport(format!("HN comments API response parse failed: {error}"))
    })?;
    let mut sections = Vec::new();
    if let Some(title) = normalized_markdown(item.title.as_deref()) {
        sections.push(format!("# {title}"));
    }
    if let Some(text) = normalized_markdown(item.text.as_deref()) {
        sections.push(text);
    }

    let mut stack = item
        .children
        .as_deref()
        .unwrap_or_default()
        .iter()
        .rev()
        .map(|comment| (comment, 2_usize))
        .collect::<Vec<_>>();
    while let Some((comment, depth)) = stack.pop() {
        if let Some(text) = normalized_markdown(comment.text.as_deref()) {
            let author = comment
                .author
                .as_deref()
                .filter(|author| !author.trim().is_empty())
                .map(|author| author.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_else(|| "[deleted]".to_owned());
            sections.push(format!("{} {author}\n\n{text}", "#".repeat(depth.min(6))));
        }
        if let Some(children) = &comment.children {
            stack.extend(children.iter().rev().map(|child| (child, depth + 1)));
        }
    }

    if sections.is_empty() {
        return Err(HttpFailure::Transport(
            "HN comments API response contained no discussion content".to_owned(),
        ));
    }
    Ok(sections.join("\n\n"))
}

fn normalized_markdown(html: Option<&str>) -> Option<String> {
    html.map(html_to_markdown)
        .map(|markdown| markdown.trim().to_owned())
        .filter(|markdown| !markdown.is_empty())
}

pub struct ProductionArticleProcessor {
    article_client: OpenAiClient,
    page_publisher: TelegraphClient,
    settings: Settings,
}

impl ProductionArticleProcessor {
    pub fn new(
        article_client: OpenAiClient,
        page_publisher: TelegraphClient,
        settings: Settings,
    ) -> Self {
        Self {
            article_client,
            page_publisher,
            settings,
        }
    }
}

#[async_trait]
impl ArticleProcessor for ProductionArticleProcessor {
    async fn generate_and_publish(
        &self,
        content: &str,
    ) -> Result<(Article, String), PipelineError> {
        let article = generate_article(&self.article_client, content, &self.settings).await?;
        let rendered = article.render_content_text();
        let html = html_escape::encode_text(&rendered).replace('\n', "<br>");
        let page_url = self
            .page_publisher
            .create_page(&article.title, &html)
            .await?;
        Ok((article, page_url))
    }
}

pub struct App {
    settings: Settings,
    feed: Arc<dyn FeedSource>,
    comments: Arc<dyn CommentSource>,
    pipeline: Arc<dyn ArticleProcessor>,
    notifier: Arc<dyn Notifier>,
    store: Arc<dyn DedupeStore>,
}

impl App {
    pub async fn production(settings: Settings) -> Result<Self, AppError> {
        let (client, openai_http_client) = build_http_clients(&settings)?;
        let feed = Arc::new(HnFeedSource::new(
            client.clone(),
            &settings.feed_base_url,
            settings.feed_points,
        ));
        let comments = Arc::new(HnCommentSource::new(
            client.clone(),
            settings.comments_api_base_url.clone(),
            Duration::from_secs_f64(settings.comments_fetch_min_interval_seconds),
            Duration::from_secs_f64(settings.comments_fetch_429_cooldown_seconds),
        ));
        let article_client = OpenAiClient::new(
            openai_http_client,
            settings.openai_base_url.clone(),
            settings.openai_api_key.clone(),
            settings.openai_model.clone(),
        );
        let telegraph = TelegraphClient::new(client.clone(), settings.telegraph_base_url.clone());
        let pipeline = Arc::new(ProductionArticleProcessor::new(
            article_client,
            telegraph,
            settings.clone(),
        ));
        let notifier = Arc::new(TelegramClient::new(
            client,
            &settings.telegram_base_url,
            &settings.bot_token,
            settings.chat_id.clone(),
        ));
        let store = Arc::new(
            RedisStore::connect(
                &settings.redis_host,
                settings.redis_port,
                settings.redis_db,
                settings.redis_password.as_deref(),
            )
            .await?,
        );
        Ok(Self::with_components(
            settings,
            settings_feed(feed),
            comments,
            pipeline,
            notifier,
            store,
        ))
    }

    pub fn with_components(
        settings: Settings,
        feed: Arc<dyn FeedSource>,
        comments: Arc<dyn CommentSource>,
        pipeline: Arc<dyn ArticleProcessor>,
        notifier: Arc<dyn Notifier>,
        store: Arc<dyn DedupeStore>,
    ) -> Self {
        Self {
            settings,
            feed,
            comments,
            pipeline,
            notifier,
            store,
        }
    }

    pub async fn serve(self, poll_interval_seconds: f64) -> Result<(), AppError> {
        let cancellation = CancellationToken::new();
        let signal_token = cancellation.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            signal_token.cancel();
        });
        self.serve_with_token(poll_interval_seconds, cancellation)
            .await
    }

    pub async fn serve_with_token(
        &self,
        poll_interval_seconds: f64,
        cancellation: CancellationToken,
    ) -> Result<(), AppError> {
        let interval = Duration::from_secs_f64(poll_interval_seconds);
        loop {
            if cancellation.is_cancelled() {
                tracing::info!("service cancellation requested");
                return Ok(());
            }
            let batch_result = tokio::select! {
                biased;
                () = cancellation.cancelled() => {
                    tracing::info!("service stopped during feed batch");
                    return Ok(());
                }
                result = self.run_feed_batch() => result,
            };
            if let Err(error) = batch_result {
                tracing::error!(error = %error, "feed batch failed");
            } else {
                tracing::info!("feed batch completed");
            }
            tokio::select! {
                () = cancellation.cancelled() => {
                    tracing::info!("service stopped");
                    return Ok(());
                }
                () = tokio::time::sleep(interval) => {}
            }
        }
    }

    pub async fn run_feed_batch(&self) -> Result<(), AppError> {
        let feed = self.feed.fetch().await?;
        tokio::time::sleep(Duration::from_secs_f64(self.settings.batch_sleep_seconds)).await;
        let fetch_semaphore = Arc::new(Semaphore::new(self.settings.comments_fetch_concurrency));
        let pipeline_semaphore =
            Arc::new(Semaphore::new(self.settings.article_pipeline_concurrency));
        let results = join_all(feed.entries.into_iter().map(|entry| {
            self.process_entry_with_retry(
                entry,
                fetch_semaphore.clone(),
                pipeline_semaphore.clone(),
            )
        }))
        .await;

        let errors = results
            .into_iter()
            .filter_map(Result::err)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(AppError::Entries(errors.join("; ")))
        }
    }

    async fn process_entry_with_retry(
        &self,
        entry: HnEntry,
        fetch_semaphore: Arc<Semaphore>,
        pipeline_semaphore: Arc<Semaphore>,
    ) -> Result<bool, AppError> {
        for attempt in 1..=3 {
            match self
                .process_entry_once(
                    entry.clone(),
                    fetch_semaphore.clone(),
                    pipeline_semaphore.clone(),
                )
                .await
            {
                Ok(processed) => return Ok(processed),
                Err(error) if attempt < 3 => {
                    tracing::warn!(entry_id = %entry.id, attempt, error = %error, "unexpected entry failure; retrying");
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("entry retry loop always returns")
    }

    async fn process_entry_once(
        &self,
        entry: HnEntry,
        fetch_semaphore: Arc<Semaphore>,
        pipeline_semaphore: Arc<Semaphore>,
    ) -> Result<bool, AppError> {
        let key = format!("hnbot:entry:{}", entry.id);
        if self.store.exists(&key).await? {
            tracing::info!(entry_id = %entry.id, "entry already processed");
            return Ok(true);
        }

        let content = {
            let _permit = fetch_semaphore
                .acquire()
                .await
                .expect("fetch semaphore is never closed");
            match self.comments.fetch(&entry).await {
                Ok(content) => content,
                Err(error) => {
                    tracing::warn!(entry_id = %entry.id, error = %error, "comment fetch failed; skipping entry");
                    return Ok(false);
                }
            }
        };

        let (article, page_url) = {
            let _permit = pipeline_semaphore
                .acquire()
                .await
                .expect("pipeline semaphore is never closed");
            match self.pipeline.generate_and_publish(&content).await {
                Ok(result) => result,
                Err(error) => {
                    tracing::warn!(entry_id = %entry.id, error = %error, "article pipeline failed; skipping entry");
                    return Ok(false);
                }
            }
        };
        let message = build_message(&entry, &article, &page_url);
        self.notifier.send(&message).await?;
        self.store.set(&key, &entry.comment_url).await?;
        tracing::info!(entry_id = %entry.id, "entry processed");
        Ok(true)
    }
}

fn build_http_clients(settings: &Settings) -> Result<(Client, Client), AppError> {
    let build = |timeout_seconds| {
        Client::builder()
            .timeout(Duration::from_secs_f64(timeout_seconds))
            .user_agent(&settings.http_user_agent)
            .build()
            .map_err(|error| AppError::Client(error.to_string()))
    };
    Ok((
        build(settings.http_timeout_seconds)?,
        build(settings.openai_timeout_seconds)?,
    ))
}

fn settings_feed(feed: Arc<HnFeedSource>) -> Arc<dyn FeedSource> {
    feed
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            if let Err(error) = result {
                tracing::error!(error = %error, "SIGINT handler failed");
            }
        }
        _ = terminate.recv() => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %error, "signal handler failed");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use chrono::Utc;
    use tokio::sync::Notify;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::article::Section;
    use crate::rss::HnFeed;

    struct FakeFeed {
        entries: Vec<HnEntry>,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl FeedSource for FakeFeed {
        async fn fetch(&self) -> Result<HnFeed, FeedError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(HnFeed {
                title: "HN".to_owned(),
                entries: self.entries.clone(),
            })
        }
    }

    struct BlockingFeed {
        started: Arc<Notify>,
        cancelled: Arc<AtomicBool>,
    }

    struct CancellationGuard(Arc<AtomicBool>);

    impl Drop for CancellationGuard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl FeedSource for BlockingFeed {
        async fn fetch(&self) -> Result<HnFeed, FeedError> {
            let _guard = CancellationGuard(self.cancelled.clone());
            self.started.notify_one();
            std::future::pending().await
        }
    }

    struct FakeComments {
        failures: HashSet<String>,
    }

    #[async_trait]
    impl CommentSource for FakeComments {
        async fn fetch(&self, entry: &HnEntry) -> Result<String, HttpFailure> {
            if self.failures.contains(&entry.id) {
                Err(HttpFailure::Transport("failed".to_owned()))
            } else {
                Ok(format!("comments-{}", entry.id))
            }
        }
    }

    struct FakePipeline;

    #[async_trait]
    impl ArticleProcessor for FakePipeline {
        async fn generate_and_publish(
            &self,
            content: &str,
        ) -> Result<(Article, String), PipelineError> {
            Ok((
                Article {
                    title: content.to_owned(),
                    summary: "summary".to_owned(),
                    sections: vec![Section {
                        title: "section".to_owned(),
                        emoji: "🦀".to_owned(),
                        content: "body".to_owned(),
                    }],
                },
                "https://telegra.ph/page".to_owned(),
            ))
        }
    }

    #[derive(Default)]
    struct TrackingComments {
        active: AtomicUsize,
        max_active: AtomicUsize,
    }

    #[async_trait]
    impl CommentSource for TrackingComments {
        async fn fetch(&self, entry: &HnEntry) -> Result<String, HttpFailure> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(format!("comments-{}", entry.id))
        }
    }

    #[derive(Default)]
    struct TrackingPipeline {
        active: AtomicUsize,
        max_active: AtomicUsize,
    }

    #[async_trait]
    impl ArticleProcessor for TrackingPipeline {
        async fn generate_and_publish(
            &self,
            _content: &str,
        ) -> Result<(Article, String), PipelineError> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok((
                Article {
                    title: "title".to_owned(),
                    summary: "summary".to_owned(),
                    sections: Vec::new(),
                },
                "https://telegra.ph/page".to_owned(),
            ))
        }
    }

    #[derive(Default)]
    struct FakeNotifier {
        messages: Mutex<Vec<String>>,
        fail: bool,
    }

    #[async_trait]
    impl Notifier for FakeNotifier {
        async fn send(&self, message: &str) -> Result<(), TelegramError> {
            if self.fail {
                return Err(TelegramError::Api("failed".to_owned()));
            }
            self.messages.lock().unwrap().push(message.to_owned());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeStore {
        values: Mutex<HashMap<String, String>>,
    }

    #[async_trait]
    impl DedupeStore for FakeStore {
        async fn exists(&self, key: &str) -> Result<bool, StoreError> {
            Ok(self.values.lock().unwrap().contains_key(key))
        }

        async fn set(&self, key: &str, value: &str) -> Result<(), StoreError> {
            self.values
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
    }

    fn settings() -> Settings {
        Settings::from_map(&HashMap::from([
            ("OPENAI_API_KEY".to_owned(), "key".to_owned()),
            ("BOT_TOKEN".to_owned(), "token".to_owned()),
            ("CHAT_ID".to_owned(), "chat".to_owned()),
            ("BATCH_SLEEP_SECONDS".to_owned(), "0".to_owned()),
            (
                "COMMENTS_FETCH_MIN_INTERVAL_SECONDS".to_owned(),
                "0".to_owned(),
            ),
        ]))
        .unwrap()
    }

    fn entry(id: &str) -> HnEntry {
        HnEntry {
            title: format!("title-{id}"),
            link: format!("https://example.com/{id}"),
            comment_url: format!("https://news.ycombinator.com/item?id={id}"),
            id: id.to_owned(),
            published_at: Utc::now(),
            points: Some(100),
            num_comments: Some(10),
        }
    }

    fn app(
        entries: Vec<HnEntry>,
        failures: HashSet<String>,
        notifier: Arc<FakeNotifier>,
        store: Arc<FakeStore>,
    ) -> App {
        App::with_components(
            settings(),
            Arc::new(FakeFeed {
                entries,
                calls: AtomicUsize::new(0),
            }),
            Arc::new(FakeComments { failures }),
            Arc::new(FakePipeline),
            notifier,
            store,
        )
    }

    #[test]
    fn comment_api_payload_renders_story_and_nested_discussion() {
        let content = render_comment_api_payload(
            br#"{
                "title":"Story title",
                "text":"<p>Story text</p>",
                "children":[
                    {
                        "author":"alice",
                        "text":"<p>Top <strong>comment</strong></p>",
                        "children":[
                            {"author":"bob","text":"<p>Nested reply</p>","children":[]}
                        ]
                    },
                    {
                        "author":null,
                        "text":null,
                        "children":[
                            {"author":"carol","text":"<p>Reply to deleted comment</p>","children":[]}
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();

        assert!(content.contains("# Story title"));
        assert!(content.contains("Story text"));
        assert!(content.contains("## alice\n\nTop **comment**"));
        assert!(content.contains("### bob\n\nNested reply"));
        assert!(content.contains("### carol\n\nReply to deleted comment"));
    }

    #[tokio::test]
    async fn comment_source_uses_configured_api_and_retries_429_three_times() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/items/limited"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .expect(3)
            .mount(&server)
            .await;
        let source = HnCommentSource::new(
            Client::new(),
            format!("{}/items", server.uri()),
            Duration::ZERO,
            Duration::ZERO,
        );

        assert_eq!(
            source.fetch(&entry("limited")).await.unwrap_err().status(),
            Some(429)
        );
    }

    #[tokio::test]
    async fn production_clients_apply_dedicated_openai_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(50)))
            .expect(2)
            .mount(&server)
            .await;
        let mut settings = settings();
        settings.http_timeout_seconds = 0.01;
        settings.openai_timeout_seconds = 1.0;
        let (general_client, openai_client) = build_http_clients(&settings).unwrap();
        let url = format!("{}/slow", server.uri());

        assert!(
            general_client
                .get(&url)
                .send()
                .await
                .unwrap_err()
                .is_timeout()
        );
        assert!(
            openai_client
                .get(&url)
                .send()
                .await
                .unwrap()
                .status()
                .is_success()
        );
    }

    #[tokio::test]
    async fn full_flow_marks_only_success_and_second_batch_dedupes() {
        let notifier = Arc::new(FakeNotifier::default());
        let store = Arc::new(FakeStore::default());
        let app = app(
            vec![entry("1"), entry("2")],
            HashSet::from(["1".to_owned()]),
            notifier.clone(),
            store.clone(),
        );

        app.run_feed_batch().await.unwrap();
        app.run_feed_batch().await.unwrap();

        assert_eq!(notifier.messages.lock().unwrap().len(), 1);
        let values = store.values.lock().unwrap();
        assert!(!values.contains_key("hnbot:entry:1"));
        assert_eq!(
            values.get("hnbot:entry:2").unwrap(),
            "https://news.ycombinator.com/item?id=2"
        );
    }

    #[tokio::test]
    async fn fetch_is_serial_while_three_pipelines_can_overlap() {
        let comments = Arc::new(TrackingComments::default());
        let pipeline = Arc::new(TrackingPipeline::default());
        let app = App::with_components(
            settings(),
            Arc::new(FakeFeed {
                entries: vec![entry("1"), entry("2"), entry("3")],
                calls: AtomicUsize::new(0),
            }),
            comments.clone(),
            pipeline.clone(),
            Arc::new(FakeNotifier::default()),
            Arc::new(FakeStore::default()),
        );

        app.run_feed_batch().await.unwrap();

        assert_eq!(comments.max_active.load(Ordering::SeqCst), 1);
        assert_eq!(pipeline.max_active.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn failed_send_does_not_mark_entry() {
        let notifier = Arc::new(FakeNotifier {
            fail: true,
            ..FakeNotifier::default()
        });
        let store = Arc::new(FakeStore::default());
        let app = app(vec![entry("1")], HashSet::new(), notifier, store.clone());

        assert!(app.run_feed_batch().await.is_err());
        assert!(store.values.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancellation_interrupts_an_in_flight_batch() {
        let started = Arc::new(Notify::new());
        let batch_cancelled = Arc::new(AtomicBool::new(false));
        let app = Arc::new(App::with_components(
            settings(),
            Arc::new(BlockingFeed {
                started: started.clone(),
                cancelled: batch_cancelled.clone(),
            }),
            Arc::new(FakeComments {
                failures: HashSet::new(),
            }),
            Arc::new(FakePipeline),
            Arc::new(FakeNotifier::default()),
            Arc::new(FakeStore::default()),
        ));
        let token = CancellationToken::new();
        let mut running = tokio::spawn({
            let app = app.clone();
            let token = token.clone();
            async move { app.serve_with_token(5.0, token).await }
        });
        started.notified().await;

        token.cancel();
        match tokio::time::timeout(Duration::from_millis(100), &mut running).await {
            Ok(result) => result.unwrap().unwrap(),
            Err(_) => {
                running.abort();
                panic!("service did not cancel its in-flight batch");
            }
        }
        assert!(batch_cancelled.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn service_completes_three_sequential_batches_then_cancels() {
        let notifier = Arc::new(FakeNotifier::default());
        let store = Arc::new(FakeStore::default());
        let feed = Arc::new(FakeFeed {
            entries: Vec::new(),
            calls: AtomicUsize::new(0),
        });
        let app = Arc::new(App::with_components(
            settings(),
            feed.clone(),
            Arc::new(FakeComments {
                failures: HashSet::new(),
            }),
            Arc::new(FakePipeline),
            notifier,
            store,
        ));
        let token = CancellationToken::new();
        let running = tokio::spawn({
            let app = app.clone();
            let token = token.clone();
            async move { app.serve_with_token(5.0, token).await }
        });
        tokio::task::yield_now().await;
        for _ in 0..2 {
            tokio::time::advance(Duration::from_secs(5)).await;
            tokio::task::yield_now().await;
        }
        assert_eq!(feed.calls.load(Ordering::SeqCst), 3);
        token.cancel();
        running.await.unwrap().unwrap();
    }
}
