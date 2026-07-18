use std::collections::HashMap;
use std::env;
use std::time::Duration;

use thiserror::Error;

#[derive(Clone)]
pub struct Settings {
    pub openai_api_key: String,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_timeout_seconds: f64,
    pub bot_token: String,
    pub chat_id: String,
    pub article_lang: String,
    pub redis_host: String,
    pub redis_port: u16,
    pub redis_db: i64,
    pub redis_password: Option<String>,
    pub http_timeout_seconds: f64,
    pub http_user_agent: String,
    pub comments_fetch_concurrency: usize,
    pub comments_fetch_timeout_seconds: f64,
    pub comments_fetch_min_interval_seconds: f64,
    pub comments_fetch_429_cooldown_seconds: f64,
    pub article_pipeline_concurrency: usize,
    pub chunk_size: usize,
    pub feed_points: u32,
    pub batch_sleep_seconds: f64,
    pub feed_poll_interval_seconds: f64,
    pub feed_base_url: String,
    pub comments_api_base_url: String,
    pub telegraph_base_url: String,
    pub telegram_base_url: String,
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Settings")
            .field("openai_api_key", &"<redacted>")
            .field("bot_token", &"<redacted>")
            .field("chat_id", &"<redacted>")
            .field("openai_model", &self.openai_model)
            .field("redis_host", &self.redis_host)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
    #[error("missing required setting {0}")]
    Missing(&'static str),
    #[error("invalid setting {name}: {reason}")]
    Invalid { name: &'static str, reason: String },
}

impl Settings {
    pub fn from_env() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        Self::from_map(&env::vars().collect())
    }

    pub fn from_map(values: &HashMap<String, String>) -> Result<Self, ConfigError> {
        Ok(Self {
            openai_api_key: required(values, "OPENAI_API_KEY")?,
            openai_base_url: string(values, "OPENAI_BASE_URL", "https://api.openai.com/v1"),
            openai_model: string(values, "OPENAI_MODEL", "gpt-5-mini"),
            openai_timeout_seconds: float(values, "OPENAI_TIMEOUT_SECONDS", 120.0, 0.0, false)?,
            bot_token: required(values, "BOT_TOKEN")?,
            chat_id: required(values, "CHAT_ID")?,
            article_lang: string(values, "ARTICLE_LANG", "Traditional Chinese (台灣正體中文)"),
            redis_host: string(values, "REDIS_HOST", "localhost"),
            redis_port: integer(values, "REDIS_PORT", 6379, 1, u16::MAX)?,
            redis_db: integer(values, "REDIS_DB", 0, 0, i64::MAX)?,
            redis_password: optional_string(values, "REDIS_PASSWORD"),
            http_timeout_seconds: float(values, "HTTP_TIMEOUT_SECONDS", 10.0, 0.0, false)?,
            http_user_agent: string(values, "HTTP_USER_AGENT", "hnbot/0.0.0"),
            comments_fetch_concurrency: integer(
                values,
                "COMMENTS_FETCH_CONCURRENCY",
                1,
                1,
                usize::MAX,
            )?,
            comments_fetch_timeout_seconds: float(
                values,
                "COMMENTS_FETCH_TIMEOUT_SECONDS",
                60.0,
                0.0,
                false,
            )?,
            comments_fetch_min_interval_seconds: float(
                values,
                "COMMENTS_FETCH_MIN_INTERVAL_SECONDS",
                2.0,
                0.0,
                true,
            )?,
            comments_fetch_429_cooldown_seconds: float(
                values,
                "COMMENTS_FETCH_429_COOLDOWN_SECONDS",
                30.0,
                0.0,
                true,
            )?,
            article_pipeline_concurrency: integer(
                values,
                "ARTICLE_PIPELINE_CONCURRENCY",
                3,
                1,
                usize::MAX,
            )?,
            chunk_size: integer(values, "CHUNK_SIZE", 200_000, 1, usize::MAX)?,
            feed_points: integer(values, "FEED_POINTS", 200, 1, u32::MAX)?,
            batch_sleep_seconds: float(values, "BATCH_SLEEP_SECONDS", 0.5, 0.0, true)?,
            feed_poll_interval_seconds: float(
                values,
                "FEED_POLL_INTERVAL_SECONDS",
                30.0,
                1.0,
                true,
            )?,
            feed_base_url: string(values, "HNBOT_FEED_BASE_URL", "https://hnrss.org"),
            comments_api_base_url: string(
                values,
                "HNBOT_COMMENTS_API_BASE_URL",
                "https://hn.algolia.com/api/v1/items",
            ),
            telegraph_base_url: string(
                values,
                "HNBOT_TELEGRAPH_BASE_URL",
                "https://api.telegra.ph",
            ),
            telegram_base_url: string(
                values,
                "HNBOT_TELEGRAM_BASE_URL",
                "https://api.telegram.org",
            ),
        })
    }
}

fn required(values: &HashMap<String, String>, name: &'static str) -> Result<String, ConfigError> {
    values
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(ConfigError::Missing(name))
}

fn string(values: &HashMap<String, String>, name: &str, default: &str) -> String {
    values
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

fn optional_string(values: &HashMap<String, String>, name: &str) -> Option<String> {
    values
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
}

fn integer<T>(
    values: &HashMap<String, String>,
    name: &'static str,
    default: T,
    min: T,
    max: T,
) -> Result<T, ConfigError>
where
    T: Copy + PartialOrd + std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let Some(raw) = values.get(name).filter(|value| !value.trim().is_empty()) else {
        return Ok(default);
    };
    let value = raw.parse::<T>().map_err(|error| ConfigError::Invalid {
        name,
        reason: error.to_string(),
    })?;
    if value < min || value > max {
        return Err(ConfigError::Invalid {
            name,
            reason: "out of range".to_owned(),
        });
    }
    Ok(value)
}

fn float(
    values: &HashMap<String, String>,
    name: &'static str,
    default: f64,
    min: f64,
    allow_min: bool,
) -> Result<f64, ConfigError> {
    let raw = values.get(name).filter(|value| !value.trim().is_empty());
    let value = match raw {
        Some(raw) => raw.parse::<f64>().map_err(|error| ConfigError::Invalid {
            name,
            reason: error.to_string(),
        })?,
        None => default,
    };
    let valid_min = if allow_min { value >= min } else { value > min };
    if !value.is_finite() || !valid_min || Duration::try_from_secs_f64(value).is_err() {
        return Err(ConfigError::Invalid {
            name,
            reason: "must be finite, within range, and representable as a duration".to_owned(),
        });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required_values() -> HashMap<String, String> {
        HashMap::from([
            ("OPENAI_API_KEY".to_owned(), "openai-secret".to_owned()),
            ("BOT_TOKEN".to_owned(), "bot-secret".to_owned()),
            ("CHAT_ID".to_owned(), "chat".to_owned()),
        ])
    }

    #[test]
    fn defaults_match_runtime_contract() {
        let settings = Settings::from_map(&required_values()).unwrap();

        assert_eq!(settings.openai_model, "gpt-5-mini");
        assert_eq!(settings.openai_timeout_seconds, 120.0);
        assert_eq!(settings.redis_password, None);
        assert_eq!(
            settings.comments_api_base_url,
            "https://hn.algolia.com/api/v1/items"
        );
        assert_eq!(settings.feed_points, 200);
        assert_eq!(settings.comments_fetch_concurrency, 1);
        assert_eq!(settings.comments_fetch_timeout_seconds, 60.0);
        assert_eq!(settings.article_pipeline_concurrency, 3);
        assert_eq!(settings.feed_poll_interval_seconds, 30.0);
        assert_eq!(settings.article_lang, "Traditional Chinese (台灣正體中文)");
    }

    #[test]
    fn required_values_are_validated() {
        assert_eq!(
            Settings::from_map(&HashMap::new()).unwrap_err(),
            ConfigError::Missing("OPENAI_API_KEY")
        );
    }

    #[test]
    fn invalid_numeric_values_are_rejected() {
        for value in ["0", "NaN", "inf", "-1"] {
            let mut values = required_values();
            values.insert("FEED_POLL_INTERVAL_SECONDS".to_owned(), value.to_owned());
            assert!(Settings::from_map(&values).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn unrepresentable_durations_are_rejected() {
        let mut values = required_values();
        values.insert("HTTP_TIMEOUT_SECONDS".to_owned(), "2e19".to_owned());

        assert!(Settings::from_map(&values).is_err());
    }

    #[test]
    fn debug_output_redacts_secrets() {
        let mut values = required_values();
        values.insert("REDIS_PASSWORD".to_owned(), "redis-secret".to_owned());
        let settings = Settings::from_map(&values).unwrap();
        let debug = format!("{settings:?}");
        assert!(!debug.contains("openai-secret"));
        assert!(!debug.contains("bot-secret"));
        assert!(!debug.contains("redis-secret"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn overrides_are_applied_and_unknown_values_ignored() {
        let mut values = required_values();
        values.insert("OPENAI_MODEL".to_owned(), "custom".to_owned());
        values.insert("OPENAI_TIMEOUT_SECONDS".to_owned(), "45".to_owned());
        values.insert("COMMENTS_FETCH_TIMEOUT_SECONDS".to_owned(), "90".to_owned());
        values.insert("REDIS_PASSWORD".to_owned(), "redis-secret".to_owned());
        values.insert(
            "HNBOT_COMMENTS_API_BASE_URL".to_owned(),
            "https://comments.example/items".to_owned(),
        );
        values.insert("FEED_POINTS".to_owned(), "123".to_owned());
        values.insert("UNKNOWN".to_owned(), "ignored".to_owned());

        let settings = Settings::from_map(&values).unwrap();
        assert_eq!(settings.openai_model, "custom");
        assert_eq!(settings.openai_timeout_seconds, 45.0);
        assert_eq!(settings.comments_fetch_timeout_seconds, 90.0);
        assert_eq!(settings.redis_password.as_deref(), Some("redis-secret"));
        assert_eq!(
            settings.comments_api_base_url,
            "https://comments.example/items"
        );
        assert_eq!(settings.feed_points, 123);
    }
}
