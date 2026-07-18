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
    fail_exists: bool,
    exists_calls: AtomicUsize,
    fail_set: bool,
    set_calls: AtomicUsize,
}

#[async_trait]
impl DedupeStore for FakeStore {
    async fn exists(&self, key: &str) -> Result<bool, StoreError> {
        self.exists_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_exists {
            return Err(StoreError::Redis(redis::RedisError::from((
                redis::ErrorKind::Io,
                "exists failed",
            ))));
        }
        Ok(self.values.lock().unwrap().contains_key(key))
    }

    async fn set(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.set_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_set {
            return Err(StoreError::Redis(redis::RedisError::from((
                redis::ErrorKind::Io,
                "set failed",
            ))));
        }
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
    assert!(content.contains("## carol\n\nReply to deleted comment"));
    assert!(!content.contains("### carol\n\nReply to deleted comment"));
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
async fn production_clients_apply_dedicated_request_timeouts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(50)))
        .expect(3)
        .mount(&server)
        .await;
    let mut settings = settings();
    settings.http_timeout_seconds = 0.01;
    settings.comments_fetch_timeout_seconds = 1.0;
    settings.openai_timeout_seconds = 1.0;
    let (general_client, comments_client, openai_client) = build_http_clients(&settings).unwrap();
    let url = format!("{}/slow", server.uri());

    assert!(
        general_client
            .get(&url)
            .send()
            .await
            .unwrap_err()
            .is_timeout()
    );
    for client in [comments_client, openai_client] {
        assert!(client.get(&url).send().await.unwrap().status().is_success());
    }
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
async fn duplicate_feed_ids_are_processed_once() {
    let comments = Arc::new(TrackingComments::default());
    let notifier = Arc::new(FakeNotifier::default());
    let app = App::with_components(
        settings(),
        Arc::new(FakeFeed {
            entries: vec![entry("1"), entry("1")],
            calls: AtomicUsize::new(0),
        }),
        comments,
        Arc::new(FakePipeline),
        notifier.clone(),
        Arc::new(FakeStore::default()),
    );

    app.run_feed_batch().await.unwrap();

    assert_eq!(notifier.messages.lock().unwrap().len(), 1);
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
async fn failed_store_lookup_retries_without_starting_pipeline() {
    let notifier = Arc::new(FakeNotifier::default());
    let store = Arc::new(FakeStore {
        fail_exists: true,
        ..FakeStore::default()
    });
    let app = app(
        vec![entry("1")],
        HashSet::new(),
        notifier.clone(),
        store.clone(),
    );

    assert!(app.run_feed_batch().await.is_err());
    assert!(notifier.messages.lock().unwrap().is_empty());
    assert_eq!(store.exists_calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn failed_store_write_does_not_replay_notification() {
    let notifier = Arc::new(FakeNotifier::default());
    let store = Arc::new(FakeStore {
        fail_set: true,
        ..FakeStore::default()
    });
    let app = app(
        vec![entry("1")],
        HashSet::new(),
        notifier.clone(),
        store.clone(),
    );

    assert!(app.run_feed_batch().await.is_err());
    assert_eq!(notifier.messages.lock().unwrap().len(), 1);
    assert_eq!(store.set_calls.load(Ordering::SeqCst), 3);
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

#[tokio::test]
async fn service_rejects_an_unrepresentable_poll_interval() {
    let app = app(
        Vec::new(),
        HashSet::new(),
        Arc::new(FakeNotifier::default()),
        Arc::new(FakeStore::default()),
    );

    assert!(
        app.serve_with_token(2e19, CancellationToken::new())
            .await
            .is_err()
    );
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
