use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, ConnectionInfo, IntoConnectionInfo, RedisConnectionInfo};
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
}

#[async_trait]
pub trait DedupeStore: Send + Sync {
    async fn exists(&self, key: &str) -> Result<bool, StoreError>;
    async fn set(&self, key: &str, value: &str) -> Result<(), StoreError>;
}

pub struct RedisStore {
    connection: Mutex<ConnectionManager>,
}

impl RedisStore {
    pub async fn connect(
        host: &str,
        port: u16,
        db: i64,
        password: Option<&str>,
    ) -> Result<Self, StoreError> {
        let client = redis::Client::open(connection_info(host, port, db, password))?;
        let connection = client.get_connection_manager().await?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

fn connection_info(host: &str, port: u16, db: i64, password: Option<&str>) -> ConnectionInfo {
    let redis = password.map_or_else(
        || RedisConnectionInfo::default().set_db(db),
        |password| {
            RedisConnectionInfo::default()
                .set_db(db)
                .set_password(password)
        },
    );
    (host, port)
        .into_connection_info()
        .expect("a host and port always form valid Redis connection information")
        .set_redis_settings(redis)
}

#[async_trait]
impl DedupeStore for RedisStore {
    async fn exists(&self, key: &str) -> Result<bool, StoreError> {
        let mut connection = self.connection.lock().await;
        Ok(connection.exists(key).await?)
    }

    async fn set(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().await;
        connection.set::<_, _, ()>(key, value).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_info_propagates_password_without_url_encoding() {
        let info = connection_info("redis", 6379, 2, Some("p@ss:/word"));

        assert_eq!(info.redis_settings().db(), 2);
        assert_eq!(info.redis_settings().password(), Some("p@ss:/word"));
    }
}
