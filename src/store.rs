use async_trait::async_trait;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
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
    pub async fn connect(host: &str, port: u16, db: i64) -> Result<Self, StoreError> {
        let client = redis::Client::open(format!("redis://{host}:{port}/{db}"))?;
        let connection = client.get_connection_manager().await?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
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
