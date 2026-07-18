use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::Settings;
use crate::content::chunk_chars;

pub const ARTICLE_INSTRUCTIONS: &str = r#"
Task:
Convert the input into a coherent blog post written entirely in {lang}. Return output strictly as the given schema.

Hard constraints:
- Preserve all materially important information from the input.
- Do not add new facts, entities, events, numbers, or claims.
- Use a professional, neutral, easy-to-read tone.
- Simplify complex wording when needed, but keep original meaning.
- Summary must be <= 500 characters.
- Each section.content must be <= 1000 characters.
- All content must be less than 5000 characters in total.

Section rules:
- Every section title must be specific and in {lang}.
- Every section emoji must be exactly one emoji.
- Every section body can have one or more paragraphs.
- Keep transitions smooth and the whole post cohesive
- The final section should be a closing section that only restates earlier points.

Final checks:
- Include opening, body, and closing coverage.
- Ensure all content is in {lang}.
- Ensure every constraint is satisfied.
"#;

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct Section {
    /// The title of the section.
    pub title: String,
    /// An emoji representing the section.
    pub emoji: String,
    /// The section body, possibly containing multiple paragraphs.
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct Article {
    /// The title of the article.
    pub title: String,
    /// A brief summary of the article.
    pub summary: String,
    /// The article sections.
    pub sections: Vec<Section>,
}

impl Article {
    pub fn render_content_text(&self) -> String {
        self.sections
            .iter()
            .map(|section| format!("{} {}\n\n{}", section.emoji, section.title, section.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn validate(&self) -> Result<(), ArticleError> {
        let title_len = self.title.chars().count();
        if self.title.trim().is_empty() || title_len > 256 {
            return Err(ArticleError::Constraint(
                "title must contain text and not exceed 256 characters".to_owned(),
            ));
        }
        if self.summary.chars().count() > 500 {
            return Err(ArticleError::Constraint(
                "summary exceeds 500 characters".to_owned(),
            ));
        }
        if self
            .sections
            .iter()
            .any(|section| section.content.chars().count() > 1_000)
        {
            return Err(ArticleError::Constraint(
                "section content exceeds 1000 characters".to_owned(),
            ));
        }
        let total = self.title.chars().count()
            + self.summary.chars().count()
            + self
                .sections
                .iter()
                .map(|section| {
                    section.title.chars().count()
                        + section.emoji.chars().count()
                        + section.content.chars().count()
                })
                .sum::<usize>();
        if total >= 5_000 {
            return Err(ArticleError::Constraint(
                "article content must be less than 5000 characters".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ArticleError {
    #[error("article generation failed: {0}")]
    Generation(String),
    #[error("article constraint failed: {0}")]
    Constraint(String),
}

#[async_trait]
pub trait ArticleClient: Send + Sync {
    async fn generate_once(
        &self,
        content: &str,
        instructions: &str,
    ) -> Result<Article, ArticleError>;
}

pub async fn generate_article(
    client: &dyn ArticleClient,
    content: &str,
    settings: &Settings,
) -> Result<Article, ArticleError> {
    if content.trim().is_empty() {
        return Ok(Article {
            title: "No Content Available".to_owned(),
            summary: String::new(),
            sections: Vec::new(),
        });
    }

    let instructions = ARTICLE_INSTRUCTIONS.replace("{lang}", &settings.article_lang);
    let mut input = content.to_owned();
    for _ in 0..8 {
        let chunks = chunk_chars(&input, settings.chunk_size);
        if chunks.len() <= 1 {
            let article = client.generate_once(&input, &instructions).await?;
            article.validate()?;
            return Ok(article);
        }

        let mut articles = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let article = client.generate_once(&chunk, &instructions).await?;
            article.validate()?;
            articles.push(article);
        }
        input = articles
            .iter()
            .map(Article::render_content_text)
            .collect::<Vec<_>>()
            .join("\n\n");
    }

    Err(ArticleError::Generation(
        "recursive summarization did not converge".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    struct FakeClient {
        prompts: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ArticleClient for FakeClient {
        async fn generate_once(
            &self,
            content: &str,
            _instructions: &str,
        ) -> Result<Article, ArticleError> {
            self.prompts.lock().unwrap().push(content.to_owned());
            Ok(Article {
                title: "title".to_owned(),
                summary: "summary".to_owned(),
                sections: vec![Section {
                    title: "section".to_owned(),
                    emoji: "🦀".to_owned(),
                    content: "short".to_owned(),
                }],
            })
        }
    }

    fn settings(chunk_size: usize) -> Settings {
        let mut values = HashMap::from([
            ("OPENAI_API_KEY".to_owned(), "key".to_owned()),
            ("BOT_TOKEN".to_owned(), "token".to_owned()),
            ("CHAT_ID".to_owned(), "chat".to_owned()),
        ]);
        values.insert("CHUNK_SIZE".to_owned(), chunk_size.to_string());
        Settings::from_map(&values).unwrap()
    }

    #[tokio::test]
    async fn empty_content_avoids_client() {
        let client = FakeClient {
            prompts: Mutex::new(Vec::new()),
        };
        let article = generate_article(&client, "  ", &settings(10))
            .await
            .unwrap();
        assert_eq!(article.title, "No Content Available");
        assert!(client.prompts.lock().unwrap().is_empty());
    }

    struct ShrinkingClient {
        prompts: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ArticleClient for ShrinkingClient {
        async fn generate_once(
            &self,
            content: &str,
            _instructions: &str,
        ) -> Result<Article, ArticleError> {
            self.prompts.lock().unwrap().push(content.to_owned());
            Ok(Article {
                title: "title".to_owned(),
                summary: "summary".to_owned(),
                sections: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn single_chunk_calls_client_once() {
        let client = FakeClient {
            prompts: Mutex::new(Vec::new()),
        };
        generate_article(&client, "content", &settings(100))
            .await
            .unwrap();
        assert_eq!(client.prompts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn multiple_chunks_are_summarized_then_combined() {
        let client = ShrinkingClient {
            prompts: Mutex::new(Vec::new()),
        };
        let article = generate_article(&client, "abcdefgh", &settings(4))
            .await
            .unwrap();
        assert_eq!(article.title, "title");
        assert_eq!(client.prompts.lock().unwrap().len(), 3);
    }

    #[test]
    fn validation_rejects_telegraph_invalid_titles() {
        let article = Article {
            title: String::new(),
            summary: String::new(),
            sections: Vec::new(),
        };
        assert!(article.validate().is_err());

        let invalid = Article {
            title: "x".repeat(257),
            ..article
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn render_and_constraints_match_contract() {
        let article = Article {
            title: "title".to_owned(),
            summary: "summary".to_owned(),
            sections: vec![Section {
                title: "section".to_owned(),
                emoji: "🦀".to_owned(),
                content: "body".to_owned(),
            }],
        };
        assert_eq!(article.render_content_text(), "🦀 section\n\nbody");
        assert!(article.validate().is_ok());

        let invalid = Article {
            summary: "x".repeat(501),
            ..article
        };
        assert!(invalid.validate().is_err());
    }
}
