use async_trait::async_trait;
use reqwest::Client;
use schemars::schema_for;
use serde_json::{Value, json};

use crate::article::{Article, ArticleClient, ArticleError};
use crate::http::{HttpFailure, response_bytes};

pub struct OpenAiClient {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiClient {
    pub fn new(client: Client, base_url: String, api_key: String, model: String) -> Self {
        Self {
            client,
            base_url,
            api_key,
            model,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/responses", self.base_url.trim_end_matches('/'))
    }
}

#[async_trait]
impl ArticleClient for OpenAiClient {
    async fn generate_once(
        &self,
        content: &str,
        instructions: &str,
    ) -> Result<Article, ArticleError> {
        let schema = schema_for!(Article);
        let payload = json!({
            "model": self.model,
            "input": content,
            "instructions": instructions,
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "article",
                    "strict": true,
                    "schema": schema
                }
            }
        });
        let response = self
            .client
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|error| ArticleError::Generation(error.to_string()))?;
        let status = response.status();
        let bytes = response_bytes(response)
            .await
            .map_err(|error| map_http_error(status.as_u16(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| ArticleError::Generation(error.to_string()))?;
        let output = extract_output_text(&value).ok_or_else(|| {
            ArticleError::Generation("OpenAI response has no output text".to_owned())
        })?;
        serde_json::from_str(output).map_err(|error| ArticleError::Generation(error.to_string()))
    }
}

fn map_http_error(status: u16, error: HttpFailure) -> ArticleError {
    let kind = if status == 400 {
        "non-processable OpenAI input"
    } else if status == 429 || status >= 500 {
        "transient OpenAI failure"
    } else {
        "OpenAI request failure"
    };
    ArticleError::Generation(format!("{kind}: {error}"))
}

fn extract_output_text(value: &Value) -> Option<&str> {
    if let Some(output) = value.get("output_text").and_then(Value::as_str) {
        return Some(output);
    }
    value
        .get("output")?
        .as_array()?
        .iter()
        .flat_map(|item| {
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find_map(|content| content.get("text").and_then(Value::as_str))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn sends_strict_schema_request_and_parses_article() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .and(header("authorization", "Bearer secret"))
            .and(body_partial_json(json!({
                "model": "test-model",
                "input": "comments",
                "text": {"format": {"type": "json_schema", "strict": true}}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "output": [{"content": [{
                    "type": "output_text",
                    "text": "{\"title\":\"title\",\"summary\":\"summary\",\"sections\":[]}"
                }]}]
            })))
            .mount(&server)
            .await;

        let client = OpenAiClient::new(
            Client::new(),
            format!("{}/v1", server.uri()),
            "secret".to_owned(),
            "test-model".to_owned(),
        );
        let article = client
            .generate_once("comments", "instructions")
            .await
            .unwrap();
        assert_eq!(article.title, "title");
    }

    #[tokio::test]
    async fn maps_bad_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(400).set_body_string("invalid_prompt"))
            .mount(&server)
            .await;
        let client = OpenAiClient::new(
            Client::new(),
            server.uri(),
            "secret".to_owned(),
            "model".to_owned(),
        );
        assert!(
            client
                .generate_once("content", "instructions")
                .await
                .unwrap_err()
                .to_string()
                .contains("non-processable")
        );
    }

    #[tokio::test]
    async fn maps_transient_status_and_invalid_output() {
        for status in [429, 503] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/responses"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let client = OpenAiClient::new(
                Client::new(),
                server.uri(),
                "secret".to_owned(),
                "model".to_owned(),
            );
            assert!(
                client
                    .generate_once("content", "instructions")
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("transient")
            );
        }

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"output": []})))
            .mount(&server)
            .await;
        let client = OpenAiClient::new(
            Client::new(),
            server.uri(),
            "secret".to_owned(),
            "model".to_owned(),
        );
        assert!(
            client
                .generate_once("content", "instructions")
                .await
                .unwrap_err()
                .to_string()
                .contains("no output text")
        );
    }
}
