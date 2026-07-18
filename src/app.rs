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

const STORE_RETRY_ATTEMPTS: usize = 3;

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
    #[error("invalid {0} duration")]
    InvalidDuration(&'static str),
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
        let child_depth = if let Some(text) = normalized_markdown(comment.text.as_deref()) {
            let author = comment
                .author
                .as_deref()
                .filter(|author| !author.trim().is_empty())
                .map(|author| author.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_else(|| "[deleted]".to_owned());
            sections.push(format!("{} {author}\n\n{text}", "#".repeat(depth.min(6))));
            depth + 1
        } else {
            depth
        };
        if let Some(children) = &comment.children {
            stack.extend(children.iter().rev().map(|child| (child, child_depth)));
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
        let (client, comments_http_client, openai_http_client) = build_http_clients(&settings)?;
        let feed = Arc::new(HnFeedSource::new(
            client.clone(),
            &settings.feed_base_url,
            settings.feed_points,
        ));
        let comments = Arc::new(HnCommentSource::new(
            comments_http_client,
            settings.comments_api_base_url.clone(),
            duration_from_seconds(
                "comments fetch minimum interval",
                settings.comments_fetch_min_interval_seconds,
                0.0,
            )?,
            duration_from_seconds(
                "comments fetch 429 cooldown",
                settings.comments_fetch_429_cooldown_seconds,
                0.0,
            )?,
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
        let interval = duration_from_seconds("poll interval", poll_interval_seconds, 1.0)?;
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
        let batch_sleep =
            duration_from_seconds("batch sleep", self.settings.batch_sleep_seconds, 0.0)?;
        let feed = self.feed.fetch().await?;
        tokio::time::sleep(batch_sleep).await;
        let fetch_semaphore = Arc::new(Semaphore::new(self.settings.comments_fetch_concurrency));
        let pipeline_semaphore =
            Arc::new(Semaphore::new(self.settings.article_pipeline_concurrency));
        let mut seen_ids = std::collections::HashSet::new();
        let entries = feed.entries.into_iter().filter(|entry| {
            let is_new = seen_ids.insert(entry.id.clone());
            if !is_new {
                tracing::warn!(entry_id = %entry.id, "duplicate feed entry skipped");
            }
            is_new
        });
        let results = join_all(entries.map(|entry| {
            self.process_entry_once(entry, fetch_semaphore.clone(), pipeline_semaphore.clone())
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

    async fn process_entry_once(
        &self,
        entry: HnEntry,
        fetch_semaphore: Arc<Semaphore>,
        pipeline_semaphore: Arc<Semaphore>,
    ) -> Result<bool, AppError> {
        let key = format!("hnbot:entry:{}", entry.id);
        if self.store_exists_with_retry(&key).await? {
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
        self.store_set_with_retry(&key, &entry.comment_url).await?;
        tracing::info!(entry_id = %entry.id, "entry processed");
        Ok(true)
    }

    async fn store_exists_with_retry(&self, key: &str) -> Result<bool, StoreError> {
        for attempt in 1..=STORE_RETRY_ATTEMPTS {
            match self.store.exists(key).await {
                Ok(exists) => return Ok(exists),
                Err(error) if attempt < STORE_RETRY_ATTEMPTS => {
                    tracing::warn!(attempt, error = %error, "Redis lookup failed; retrying");
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("store retry loop always returns")
    }

    async fn store_set_with_retry(&self, key: &str, value: &str) -> Result<(), StoreError> {
        for attempt in 1..=STORE_RETRY_ATTEMPTS {
            match self.store.set(key, value).await {
                Ok(()) => return Ok(()),
                Err(error) if attempt < STORE_RETRY_ATTEMPTS => {
                    tracing::warn!(attempt, error = %error, "Redis write failed; retrying");
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("store retry loop always returns")
    }
}

fn duration_from_seconds(
    name: &'static str,
    seconds: f64,
    minimum: f64,
) -> Result<Duration, AppError> {
    if !seconds.is_finite() || seconds < minimum {
        return Err(AppError::InvalidDuration(name));
    }
    Duration::try_from_secs_f64(seconds).map_err(|_| AppError::InvalidDuration(name))
}

fn build_http_clients(settings: &Settings) -> Result<(Client, Client, Client), AppError> {
    let build = |name, timeout_seconds| {
        Client::builder()
            .timeout(duration_from_seconds(name, timeout_seconds, 0.0)?)
            .user_agent(&settings.http_user_agent)
            .build()
            .map_err(|error| AppError::Client(error.to_string()))
    };
    Ok((
        build("HTTP timeout", settings.http_timeout_seconds)?,
        build(
            "comments fetch timeout",
            settings.comments_fetch_timeout_seconds,
        )?,
        build("OpenAI timeout", settings.openai_timeout_seconds)?,
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
mod tests;
