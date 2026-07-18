use std::collections::BTreeMap;

use async_trait::async_trait;
use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, parse_document};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::http::{HttpFailure, response_json};

const ALLOWED_TAGS: &[&str] = &[
    "a",
    "aside",
    "b",
    "blockquote",
    "br",
    "code",
    "em",
    "figcaption",
    "figure",
    "h3",
    "h4",
    "hr",
    "i",
    "iframe",
    "img",
    "li",
    "ol",
    "p",
    "pre",
    "s",
    "strong",
    "u",
    "ul",
    "video",
];
const VOID_TAGS: &[&str] = &["br", "hr", "img"];

#[derive(Debug, Error)]
pub enum TelegraphError {
    #[error(transparent)]
    Http(#[from] HttpFailure),
    #[error("Telegraph API error: {0}")]
    Api(String),
}

#[async_trait]
pub trait PagePublisher: Send + Sync {
    async fn create_page(&self, title: &str, html: &str) -> Result<String, TelegraphError>;
}

pub struct TelegraphClient {
    client: Client,
    base_url: String,
}

impl TelegraphClient {
    pub fn new(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }
}

#[derive(Deserialize)]
struct ApiResponse<T> {
    ok: bool,
    result: Option<T>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct AccountResult {
    access_token: String,
}

#[derive(Deserialize)]
struct PageResult {
    url: String,
}

#[async_trait]
impl PagePublisher for TelegraphClient {
    async fn create_page(&self, title: &str, html: &str) -> Result<String, TelegraphError> {
        let account_response = self
            .client
            .post(format!(
                "{}/createAccount",
                self.base_url.trim_end_matches('/')
            ))
            .form(&[("short_name", "Narumi's Bot")])
            .send()
            .await
            .map_err(|error| HttpFailure::Transport(error.to_string()))?;
        let account: ApiResponse<AccountResult> = response_json(account_response).await?;
        let token = api_result(account)?.access_token;

        let sanitized = sanitize_telegraph_html(html);
        let content = serde_json::to_string(&html_to_nodes(&sanitized))
            .map_err(|error| TelegraphError::Api(error.to_string()))?;
        let page_response = self
            .client
            .post(format!(
                "{}/createPage",
                self.base_url.trim_end_matches('/')
            ))
            .form(&[
                ("access_token", token.as_str()),
                ("title", title),
                ("content", content.as_str()),
                ("return_content", "false"),
            ])
            .send()
            .await
            .map_err(|error| HttpFailure::Transport(error.to_string()))?;
        let page: ApiResponse<PageResult> = response_json(page_response).await?;
        Ok(api_result(page)?.url)
    }
}

fn api_result<T>(response: ApiResponse<T>) -> Result<T, TelegraphError> {
    if response.ok {
        response
            .result
            .ok_or_else(|| TelegraphError::Api("missing result".to_owned()))
    } else {
        Err(TelegraphError::Api(
            response.error.unwrap_or_else(|| "unknown error".to_owned()),
        ))
    }
}

pub fn sanitize_telegraph_html(input: &str) -> String {
    let token_re = Regex::new(r"(?s)<[^>]+>|[^<]+").expect("static token regex");
    let start_re = Regex::new(r"(?is)^<\s*([a-z0-9]+)([^>]*)>$").expect("static start regex");
    let end_re = Regex::new(r"(?is)^</\s*([a-z0-9]+)\s*>$").expect("static end regex");
    let attr_re = Regex::new(r#"(?is)([a-z0-9_-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#)
        .expect("static attribute regex");
    let mut output = String::new();
    let mut open_tags: Vec<String> = Vec::new();

    for token in token_re.find_iter(input).map(|matched| matched.as_str()) {
        if !token.starts_with('<') {
            let decoded = html_escape::decode_html_entities(token);
            output.push_str(&html_escape::encode_text(&decoded));
            continue;
        }
        if let Some(captures) = end_re.captures(token) {
            let original = captures.get(1).unwrap().as_str().to_ascii_lowercase();
            let mapped = remap_tag(&original);
            if VOID_TAGS.contains(&mapped) {
                continue;
            }
            if !ALLOWED_TAGS.contains(&mapped) || open_tags.last().is_none_or(|tag| tag != mapped) {
                output.push_str(&html_escape::encode_text(token));
                continue;
            }
            open_tags.pop();
            output.push_str(&format!("</{mapped}>"));
            continue;
        }
        let Some(captures) = start_re.captures(token) else {
            output.push_str(&html_escape::encode_text(token));
            continue;
        };
        let original = captures.get(1).unwrap().as_str().to_ascii_lowercase();
        let mapped = remap_tag(&original);
        if !ALLOWED_TAGS.contains(&mapped) {
            output.push_str(&html_escape::encode_text(token));
            continue;
        }

        let attrs_source = captures.get(2).map_or("", |value| value.as_str());
        let allowed_attrs = allowed_attrs(mapped);
        let mut attrs = BTreeMap::new();
        for captures in attr_re.captures_iter(attrs_source) {
            let key = captures.get(1).unwrap().as_str().to_ascii_lowercase();
            if !allowed_attrs.contains(&key.as_str()) {
                continue;
            }
            let value = captures
                .get(2)
                .or_else(|| captures.get(3))
                .or_else(|| captures.get(4))
                .map_or("", |value| value.as_str());
            attrs.insert(key, html_escape::decode_html_entities(value).into_owned());
        }
        output.push('<');
        output.push_str(mapped);
        for (key, value) in attrs {
            output.push(' ');
            output.push_str(&key);
            output.push_str("=\"");
            output.push_str(&html_escape::encode_double_quoted_attribute(&value));
            output.push('"');
        }
        output.push('>');
        if !VOID_TAGS.contains(&mapped) {
            open_tags.push(mapped.to_owned());
        }
    }

    for tag in open_tags.iter().rev() {
        output.push_str(&format!("</{tag}>"));
    }
    output
}

fn remap_tag(tag: &str) -> &str {
    match tag {
        "del" | "strike" => "s",
        "h1" | "h2" => "h3",
        "h5" | "h6" => "h4",
        other => other,
    }
}

fn allowed_attrs(tag: &str) -> &'static [&'static str] {
    match tag {
        "a" => &["href"],
        "iframe" | "video" => &["src"],
        "img" => &["src", "alt"],
        _ => &[],
    }
}

pub fn html_to_nodes(input: &str) -> Vec<Value> {
    let wrapped = format!("<body>{input}</body>");
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(wrapped);
    let body = find_body(&dom.document).unwrap_or_else(|| dom.document.clone());
    body.children
        .borrow()
        .iter()
        .filter_map(node_to_json)
        .collect()
}

fn find_body(handle: &Handle) -> Option<Handle> {
    if let NodeData::Element { name, .. } = &handle.data {
        if name.local.as_ref() == "body" {
            return Some(handle.clone());
        }
    }
    handle.children.borrow().iter().find_map(find_body)
}

fn node_to_json(handle: &Handle) -> Option<Value> {
    match &handle.data {
        NodeData::Text { contents } => {
            let text = contents.borrow().to_string();
            (!text.is_empty()).then_some(Value::String(text))
        }
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.to_string();
            if !ALLOWED_TAGS.contains(&tag.as_str()) {
                let children = handle
                    .children
                    .borrow()
                    .iter()
                    .filter_map(node_to_json)
                    .collect::<Vec<_>>();
                return Some(Value::Array(children));
            }
            let attrs = attrs
                .borrow()
                .iter()
                .map(|attr| {
                    (
                        attr.name.local.to_string(),
                        Value::String(attr.value.to_string()),
                    )
                })
                .collect::<Map<_, _>>();
            let children = handle
                .children
                .borrow()
                .iter()
                .filter_map(node_to_json)
                .collect::<Vec<_>>();
            let mut node = Map::from_iter([("tag".to_owned(), Value::String(tag))]);
            if !attrs.is_empty() {
                node.insert("attrs".to_owned(), Value::Object(attrs));
            }
            if !children.is_empty() {
                node.insert("children".to_owned(), Value::Array(children));
            }
            Some(Value::Object(node))
        }
        _ => {
            let values = handle
                .children
                .borrow()
                .iter()
                .filter_map(node_to_json)
                .collect::<Vec<_>>();
            (!values.is_empty()).then_some(json!(values))
        }
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[test]
    fn sanitizer_remaps_tags_filters_attributes_and_closes_open_tags() {
        let input = r#"<h1>Title</h1><a href="https://example.com" onclick="bad">link</a><script>x</script><b>open"#;
        assert_eq!(
            sanitize_telegraph_html(input),
            "<h3>Title</h3><a href=\"https://example.com\">link</a>&lt;script&gt;x&lt;/script&gt;<b>open</b>"
        );
    }

    #[test]
    fn sanitized_html_converts_to_telegraph_nodes() {
        let nodes = html_to_nodes("<p>Hello<br>world</p>");
        assert_eq!(nodes[0]["tag"], "p");
        assert_eq!(nodes[0]["children"][1]["tag"], "br");
    }

    #[test]
    fn sanitizer_escapes_mismatched_closing_tags() {
        assert_eq!(
            sanitize_telegraph_html("<b>bold</i>"),
            "<b>bold&lt;/i&gt;</b>"
        );
    }

    #[tokio::test]
    async fn creates_account_then_page() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/createAccount"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "result": {"access_token": "token"}
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/createPage"))
            .and(body_string_contains("access_token=token"))
            .and(body_string_contains("title=Title"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "result": {"url": "https://telegra.ph/page"}
            })))
            .mount(&server)
            .await;

        let client = TelegraphClient::new(Client::new(), server.uri());
        assert_eq!(
            client.create_page("Title", "<p>Body</p>").await.unwrap(),
            "https://telegra.ph/page"
        );
    }

    #[tokio::test]
    async fn returns_api_error_when_account_creation_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/createAccount"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": false,
                "error": "ACCOUNT_LIMIT"
            })))
            .mount(&server)
            .await;

        let error = TelegraphClient::new(Client::new(), server.uri())
            .create_page("Title", "Body")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("ACCOUNT_LIMIT"));
    }
}
