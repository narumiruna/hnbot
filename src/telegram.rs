use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use thiserror::Error;
use url::Url;

use crate::article::Article;
use crate::http::{HttpFailure, response_json};
use crate::rss::HnEntry;

#[derive(Debug, Error)]
pub enum TelegramError {
    #[error(transparent)]
    Http(#[from] HttpFailure),
    #[error("Telegram API error: {0}")]
    Api(String),
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, message: &str) -> Result<(), TelegramError>;
}

pub struct TelegramClient {
    client: Client,
    endpoint: String,
    chat_id: String,
}

impl TelegramClient {
    pub fn new(client: Client, base_url: &str, bot_token: &str, chat_id: String) -> Self {
        Self {
            client,
            endpoint: format!(
                "{}/bot{bot_token}/sendMessage",
                base_url.trim_end_matches('/')
            ),
            chat_id,
        }
    }
}

#[derive(Deserialize)]
struct TelegramResponse {
    ok: bool,
    description: Option<String>,
}

#[async_trait]
impl Notifier for TelegramClient {
    async fn send(&self, message: &str) -> Result<(), TelegramError> {
        let response = self
            .client
            .post(&self.endpoint)
            .json(&json!({
                "chat_id": self.chat_id,
                "text": message,
                "parse_mode": "HTML"
            }))
            .send()
            .await
            .map_err(|error| HttpFailure::Transport(error.without_url().to_string()))?;
        let body: TelegramResponse = response_json(response).await?;
        if body.ok {
            Ok(())
        } else {
            Err(TelegramError::Api(
                body.description
                    .unwrap_or_else(|| "unknown error".to_owned()),
            ))
        }
    }
}

pub fn build_message(entry: &HnEntry, article: &Article, page_url: &str) -> String {
    let title = html_escape::encode_text(&entry.title);
    let link = html_escape::encode_double_quoted_attribute(&entry.link);
    let title_line = format!("📰 <b><a href=\"{link}\">{title}</a></b>");

    let mut metadata = Vec::new();
    if let Some(points) = entry.points {
        metadata.push(format!("⭐ {points}"));
    }
    if let Some(comments) = entry.num_comments {
        metadata.push(format!("💬 {comments}"));
    }
    if let Some(domain) = Url::parse(&entry.link)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
    {
        metadata.push(format!(
            "🌐 {}",
            domain.strip_prefix("www.").unwrap_or(&domain)
        ));
    }

    let header = if metadata.is_empty() {
        title_line
    } else {
        format!("{title_line}\n{}", metadata.join(" · "))
    };
    let mut parts = vec![header];
    if !article.summary.is_empty() {
        parts.push(html_escape::encode_text(&article.summary).into_owned());
    }
    let comment_url = html_escape::encode_double_quoted_attribute(&entry.comment_url);
    let page_url = html_escape::encode_double_quoted_attribute(page_url);
    parts.push(format!(
        "💬 <a href=\"{comment_url}\">討論</a>  ·  📝 <a href=\"{page_url}\">筆記</a>"
    ));
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn article(summary: &str) -> Article {
        Article {
            title: "article".to_owned(),
            summary: summary.to_owned(),
            sections: Vec::new(),
        }
    }

    fn entry() -> HnEntry {
        HnEntry {
            title: "title-<1>".to_owned(),
            link: "https://www.example.com/?a=<tag>".to_owned(),
            comment_url: "https://news.ycombinator.com/item?id=1&x=<z>".to_owned(),
            id: "1".to_owned(),
            published_at: Utc::now(),
            points: Some(123),
            num_comments: Some(45),
        }
    }

    #[test]
    fn message_escapes_content_and_includes_metadata() {
        let message = build_message(
            &entry(),
            &article("摘要 <b>text</b>"),
            "https://telegra.ph/?x=<x>",
        );
        assert!(message.contains("title-&lt;1&gt;"));
        assert!(message.contains("⭐ 123 · 💬 45 · 🌐 example.com"));
        assert!(message.contains("摘要 &lt;b&gt;text&lt;/b&gt;"));
        assert!(message.contains("id=1&amp;x=&lt;z&gt;"));
    }

    #[test]
    fn message_omits_missing_metadata_and_empty_summary() {
        let mut entry = entry();
        entry.points = None;
        entry.num_comments = None;
        entry.link = "not a URL".to_owned();
        let message = build_message(&entry, &article(""), "https://telegra.ph/page");
        assert!(!message.contains('⭐'));
        assert!(!message.contains("💬 45"));
        assert_eq!(message.matches("\n\n").count(), 1);
    }

    #[tokio::test]
    async fn sends_expected_bot_payload() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/bottoken/sendMessage"))
            .and(body_partial_json(json!({
                "chat_id": "chat",
                "text": "message",
                "parse_mode": "HTML"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .mount(&server)
            .await;

        TelegramClient::new(Client::new(), &server.uri(), "token", "chat".to_owned())
            .send("message")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn transport_errors_do_not_expose_bot_token() {
        let error = TelegramClient::new(
            Client::new(),
            "http://127.0.0.1:9",
            "supersecret",
            "chat".to_owned(),
        )
        .send("message")
        .await
        .unwrap_err();
        assert!(!error.to_string().contains("supersecret"));
    }

    #[tokio::test]
    async fn rejects_non_success_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/bottoken/sendMessage"))
            .respond_with(ResponseTemplate::new(500).set_body_string("failed"))
            .mount(&server)
            .await;

        assert!(
            TelegramClient::new(Client::new(), &server.uri(), "token", "chat".to_owned())
                .send("message")
                .await
                .is_err()
        );
    }
}
