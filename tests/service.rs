use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use hnbot::app::{App, HnCommentSource, ProductionArticleProcessor};
use hnbot::config::Settings;
use hnbot::openai::OpenAiClient;
use hnbot::rss::HnFeedSource;
use hnbot::store::{DedupeStore, StoreError};
use hnbot::telegram::TelegramClient;
use hnbot::telegraph::TelegraphClient;
use reqwest::Client;
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

fn settings(server: &MockServer) -> Settings {
    Settings::from_map(&HashMap::from([
        ("OPENAI_API_KEY".to_owned(), "secret".to_owned()),
        ("OPENAI_BASE_URL".to_owned(), format!("{}/v1", server.uri())),
        ("OPENAI_MODEL".to_owned(), "test-model".to_owned()),
        ("BOT_TOKEN".to_owned(), "token".to_owned()),
        ("CHAT_ID".to_owned(), "chat".to_owned()),
        ("BATCH_SLEEP_SECONDS".to_owned(), "0".to_owned()),
        (
            "COMMENTS_FETCH_MIN_INTERVAL_SECONDS".to_owned(),
            "0".to_owned(),
        ),
        ("HNBOT_FEED_BASE_URL".to_owned(), server.uri()),
        (
            "HNBOT_COMMENTS_API_BASE_URL".to_owned(),
            format!("{}/items", server.uri()),
        ),
        ("HNBOT_TELEGRAPH_BASE_URL".to_owned(), server.uri()),
        ("HNBOT_TELEGRAM_BASE_URL".to_owned(), server.uri()),
    ]))
    .unwrap()
}

#[tokio::test]
async fn mocked_service_runs_full_flow_and_dedupes_second_batch() {
    let server = MockServer::start().await;
    let comment_url = format!("{}/comments?id=1", server.uri());
    let feed = format!(
        r#"<?xml version="1.0"?><rss version="2.0"><channel><title>HN</title>
        <item><title>Story</title><description><![CDATA[<p>Points: 123</p><p># Comments: 45</p>]]></description>
        <pubDate>Wed, 01 Jan 2026 00:00:00 +0000</pubDate><link>{comment_url}</link>
        <comments>{comment_url}</comments><guid isPermaLink="false">{comment_url}</guid></item>
        </channel></rss>"#
    );
    Mock::given(method("GET"))
        .and(path("/newest"))
        .and(query_param("points", "200"))
        .respond_with(ResponseTemplate::new(200).set_body_string(feed))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/items/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "title": "Story",
            "text": null,
            "children": [{
                "author": "alice",
                "text": "<p>useful comments</p>",
                "children": []
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(body_partial_json(json!({"model": "test-model"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "output": [{"content": [{"text": "{\"title\":\"Article\",\"summary\":\"Summary\",\"sections\":[{\"title\":\"Section\",\"emoji\":\"🦀\",\"content\":\"Body\"}]}"}]}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/createAccount"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "result": {"access_token": "access"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/createPage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "result": {"url": "https://telegra.ph/page"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/bottoken/sendMessage"))
        .and(body_partial_json(
            json!({"chat_id": "chat", "parse_mode": "HTML"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;

    let settings = settings(&server);
    let client = Client::new();
    let store = Arc::new(FakeStore::default());
    let app = App::with_components(
        settings.clone(),
        Arc::new(HnFeedSource::new(client.clone(), &server.uri(), 200)),
        Arc::new(HnCommentSource::new(
            client.clone(),
            settings.comments_api_base_url.clone(),
            Duration::ZERO,
            Duration::ZERO,
        )),
        Arc::new(ProductionArticleProcessor::new(
            OpenAiClient::new(
                client.clone(),
                settings.openai_base_url.clone(),
                settings.openai_api_key.clone(),
                settings.openai_model.clone(),
            ),
            TelegraphClient::new(client.clone(), server.uri()),
            settings,
        )),
        Arc::new(TelegramClient::new(
            client,
            &server.uri(),
            "token",
            "chat".to_owned(),
        )),
        store.clone(),
    );

    app.run_feed_batch().await.unwrap();
    app.run_feed_batch().await.unwrap();

    assert_eq!(
        store.values.lock().unwrap().get("hnbot:entry:1"),
        Some(&comment_url)
    );
}
