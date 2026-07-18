use std::error::Error as _;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use rand::RngExt;
use reqwest::{Response, StatusCode};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

pub const HTTP_RETRY_ATTEMPTS: usize = 3;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum HttpFailure {
    #[error("request failed: {0}")]
    Transport(String),
    #[error("HTTP {status}: {body}")]
    Status {
        status: u16,
        retry_after: Option<String>,
        body: String,
    },
}

impl HttpFailure {
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::Status { status, .. } => *status == 429 || (500..600).contains(status),
        }
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Status { status, .. } => Some(*status),
            Self::Transport(_) => None,
        }
    }

    pub fn retry_after(&self, now: SystemTime) -> Option<Duration> {
        let Self::Status {
            retry_after: Some(value),
            ..
        } = self
        else {
            return None;
        };
        retry_after_duration(value, now)
    }
}

pub async fn response_bytes(response: Response) -> Result<Vec<u8>, HttpFailure> {
    let status = response.status();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = response
        .bytes()
        .await
        .map_err(|error| HttpFailure::Transport(reqwest_error_message(&error)))?;
    if status.is_success() {
        return Ok(bytes.to_vec());
    }
    Err(HttpFailure::Status {
        status: status.as_u16(),
        retry_after,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

pub async fn response_json<T: serde::de::DeserializeOwned>(
    response: Response,
) -> Result<T, HttpFailure> {
    let bytes = response_bytes(response).await?;
    serde_json::from_slice(&bytes).map_err(|error| HttpFailure::Transport(error.to_string()))
}

pub fn reqwest_error_message(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_message = cause.to_string();
        if !cause_message.is_empty() && !message.ends_with(&cause_message) {
            message.push_str(": ");
            message.push_str(&cause_message);
        }
        source = cause.source();
    }
    message
}

pub async fn retry_transient<F, Fut, T>(mut operation: F) -> Result<T, HttpFailure>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, HttpFailure>>,
{
    retry_transient_with_sleep(&mut operation, |duration| sleep(duration)).await
}

async fn retry_transient_with_sleep<F, Fut, S, SleepFut, T>(
    operation: &mut F,
    mut sleeper: S,
) -> Result<T, HttpFailure>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, HttpFailure>>,
    S: FnMut(Duration) -> SleepFut,
    SleepFut: Future<Output = ()>,
{
    for attempt in 1..=HTTP_RETRY_ATTEMPTS {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) if error.is_transient() && attempt < HTTP_RETRY_ATTEMPTS => {
                let wait = error.retry_after(SystemTime::now()).unwrap_or_else(|| {
                    let upper = (1_u64 << (attempt - 1)).min(8);
                    Duration::from_millis(rand::rng().random_range(0..=upper * 1_000))
                });
                tracing::warn!(attempt, wait_ms = wait.as_millis(), error = %error, "transient HTTP error");
                sleeper(wait).await;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("retry loop always returns")
}

pub fn retry_after_duration(value: &str, now: SystemTime) -> Option<Duration> {
    if let Ok(seconds) = value.parse::<f64>() {
        if seconds.is_finite() {
            return Some(Duration::from_secs_f64(seconds.max(0.0)));
        }
        return None;
    }
    let date = httpdate::parse_http_date(value).ok()?;
    Some(date.duration_since(now).unwrap_or(Duration::ZERO))
}

#[derive(Clone)]
pub struct RequestPacer {
    min_interval: Duration,
    next_request_at: Arc<Mutex<Instant>>,
}

impl RequestPacer {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            next_request_at: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub async fn wait(&self) {
        loop {
            let delay = {
                let mut next = self.next_request_at.lock().await;
                let now = Instant::now();
                if *next <= now {
                    *next = now + self.min_interval;
                    return;
                }
                *next - now
            };
            sleep(delay).await;
        }
    }

    pub async fn defer(&self, duration: Duration) {
        let mut next = self.next_request_at.lock().await;
        *next = (*next).max(Instant::now() + duration);
    }
}

pub fn status_is_success(status: StatusCode) -> bool {
    status.is_success()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn retry_after_parses_numeric_and_http_date() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        assert_eq!(
            retry_after_duration("2.5", now),
            Some(Duration::from_secs_f64(2.5))
        );
        let date = httpdate::fmt_http_date(now + Duration::from_secs(10));
        assert_eq!(
            retry_after_duration(&date, now),
            Some(Duration::from_secs(10))
        );
        assert_eq!(retry_after_duration("NaN", now), None);
        assert_eq!(retry_after_duration("invalid", now), None);
    }

    #[tokio::test]
    async fn retries_transient_failures_and_stops_after_success() {
        let calls = AtomicUsize::new(0);
        let mut waits = Vec::new();
        let result = retry_transient_with_sleep(
            &mut || {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if call < 2 {
                        Err(HttpFailure::Status {
                            status: 429,
                            retry_after: Some("0".to_owned()),
                            body: String::new(),
                        })
                    } else {
                        Ok("ok")
                    }
                }
            },
            |duration| {
                waits.push(duration);
                std::future::ready(())
            },
        )
        .await
        .unwrap();

        assert_eq!(result, "ok");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(waits, vec![Duration::ZERO, Duration::ZERO]);
    }

    #[tokio::test]
    async fn retry_exhaustion_returns_final_error() {
        let calls = AtomicUsize::new(0);
        let error = retry_transient_with_sleep(
            &mut || {
                calls.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Err::<(), _>(HttpFailure::Status {
                    status: 503,
                    retry_after: Some("0".to_owned()),
                    body: "unavailable".to_owned(),
                }))
            },
            |_| std::future::ready(()),
        )
        .await
        .unwrap_err();

        assert_eq!(calls.load(Ordering::SeqCst), HTTP_RETRY_ATTEMPTS);
        assert_eq!(error.status(), Some(503));
    }

    #[tokio::test(start_paused = true)]
    async fn pacer_spaces_concurrent_requests_and_honors_cooldown() {
        let pacer = RequestPacer::new(Duration::from_secs(2));
        pacer.wait().await;
        let waiting = tokio::spawn({
            let pacer = pacer.clone();
            async move {
                pacer.wait().await;
                Instant::now()
            }
        });
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        tokio::time::advance(Duration::from_secs(2)).await;
        waiting.await.unwrap();

        pacer.defer(Duration::from_secs(30)).await;
        let start = Instant::now();
        pacer.wait().await;
        assert!(Instant::now().duration_since(start) >= Duration::from_secs(30));
    }
}
