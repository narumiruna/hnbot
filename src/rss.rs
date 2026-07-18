use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use feed_rs::model::Entry;
use regex::Regex;
use reqwest::Client;
use thiserror::Error;
use url::Url;

use crate::http::{HttpFailure, response_bytes, retry_transient};

#[derive(Clone, Debug, PartialEq)]
pub struct HnEntry {
    pub title: String,
    pub link: String,
    pub comment_url: String,
    pub id: String,
    pub published_at: DateTime<Utc>,
    pub points: Option<u32>,
    pub num_comments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HnFeed {
    pub title: String,
    pub entries: Vec<HnEntry>,
}

#[derive(Debug, Error)]
pub enum FeedError {
    #[error(transparent)]
    Http(#[from] HttpFailure),
    #[error("feed parse failed: {0}")]
    Parse(String),
}

#[async_trait]
pub trait FeedSource: Send + Sync {
    async fn fetch(&self) -> Result<HnFeed, FeedError>;
}

pub struct HnFeedSource {
    client: Client,
    url: String,
}

impl HnFeedSource {
    pub fn new(client: Client, base_url: &str, points: u32) -> Self {
        Self {
            client,
            url: format!("{}/newest?points={points}", base_url.trim_end_matches('/')),
        }
    }
}

#[async_trait]
impl FeedSource for HnFeedSource {
    async fn fetch(&self) -> Result<HnFeed, FeedError> {
        let client = self.client.clone();
        let url = self.url.clone();
        let bytes = retry_transient(|| {
            let client = client.clone();
            let url = url.clone();
            async move {
                let response = client
                    .get(url)
                    .send()
                    .await
                    .map_err(|error| HttpFailure::Transport(error.to_string()))?;
                response_bytes(response).await
            }
        })
        .await?;
        parse_feed(&bytes)
    }
}

pub fn parse_feed(content: &[u8]) -> Result<HnFeed, FeedError> {
    let feed = feed_rs::parser::parse(Cursor::new(content))
        .map_err(|error| FeedError::Parse(error.to_string()))?;
    let title = feed.title.map_or_else(String::new, |title| title.content);
    let mut entries = feed
        .entries
        .iter()
        .map(parse_entry)
        .collect::<Result<Vec<_>, _>>()?;
    entries.reverse();
    Ok(HnFeed { title, entries })
}

fn parse_entry(entry: &Entry) -> Result<HnEntry, FeedError> {
    let comment_url = if entry.id.contains("news.ycombinator.com/item") {
        entry.id.clone()
    } else {
        entry
            .links
            .iter()
            .find(|link| link.href.contains("news.ycombinator.com/item"))
            .or_else(|| entry.links.first())
            .map(|link| link.href.clone())
            .ok_or_else(|| FeedError::Parse("entry has no comment URL".to_owned()))?
    };
    let link = entry
        .links
        .first()
        .map_or_else(|| comment_url.clone(), |link| link.href.clone());
    let summary = entry
        .summary
        .as_ref()
        .map_or("", |summary| summary.content.as_str());
    let published_at = entry
        .published
        .or(entry.updated)
        .ok_or_else(|| FeedError::Parse("entry has no publication date".to_owned()))?;

    Ok(HnEntry {
        title: entry
            .title
            .as_ref()
            .map_or_else(String::new, |title| title.content.clone()),
        link,
        id: parse_id(&comment_url)?,
        comment_url,
        published_at,
        points: parse_metric(summary, r"Points:\s*(\d+)"),
        num_comments: parse_metric(summary, r"#\s*Comments:\s*(\d+)"),
    })
}

pub fn parse_id(value: &str) -> Result<String, FeedError> {
    let url = Url::parse(value).map_err(|error| FeedError::Parse(error.to_string()))?;
    url.query_pairs()
        .find(|(key, _)| key == "id")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| FeedError::Parse("comment URL has no id".to_owned()))
}

fn parse_metric(content: &str, pattern: &str) -> Option<u32> {
    Regex::new(pattern)
        .expect("static metric regex")
        .captures(content)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse().ok())
}

pub type SharedFeedSource = Arc<dyn FeedSource>;

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[test]
    fn parses_real_fixture_and_reverses_entries() {
        let feed = parse_feed(include_bytes!("../tests/data/sample_rss.xml")).unwrap();

        assert_eq!(feed.title, "Hacker News: Best Comments");
        assert_eq!(feed.entries[0].id, "47737182");
        assert_eq!(feed.entries[1].id, "47737434");
        let entry = &feed.entries[0];
        assert_eq!(entry.link, "https://news.ycombinator.com/item?id=47737182");
        assert_eq!(entry.points, None);
        assert_eq!(entry.num_comments, None);
    }

    #[test]
    fn parses_id_and_metrics() {
        assert_eq!(
            parse_id("https://news.ycombinator.com/item?id=123").unwrap(),
            "123"
        );
        assert_eq!(
            parse_metric("Points: 12 # Comments: 3", r"Points:\s*(\d+)"),
            Some(12)
        );
        assert_eq!(parse_metric("missing", r"Points:\s*(\d+)"), None);
    }

    #[test]
    fn malformed_feed_is_rejected() {
        assert!(parse_feed(b"not xml").is_err());
    }

    #[tokio::test]
    async fn feed_source_uses_points_and_parses_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/newest"))
            .and(query_param("points", "200"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(include_bytes!("../tests/data/sample_rss.xml")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let feed = HnFeedSource::new(Client::new(), &server.uri(), 200)
            .fetch()
            .await
            .unwrap();
        assert_eq!(feed.entries[0].id, "47737182");
    }

    #[tokio::test]
    async fn feed_source_retries_transient_status_three_times() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/newest"))
            .respond_with(ResponseTemplate::new(503).insert_header("Retry-After", "0"))
            .expect(3)
            .mount(&server)
            .await;

        let error = HnFeedSource::new(Client::new(), &server.uri(), 200)
            .fetch()
            .await
            .unwrap_err();
        assert!(error.to_string().contains("503"));
    }
}
