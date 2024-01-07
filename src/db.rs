use std::time::{SystemTime, UNIX_EPOCH};

use log::debug;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct Database {
    pool : SqlitePool
}

#[derive(sqlx::FromRow, Debug)]
pub struct Packet {
    pub id : i64,
    pub received_at : i64,
    pub content : Vec<u8>,
}

impl Database {
    pub async fn new() -> Self {
        let pool = SqlitePool::connect("sqlite:cats-radio-node.db").await.unwrap();
        let mut conn = pool.acquire().await.unwrap();

        sqlx::migrate!()
            .run(&mut conn)
            .await
            .expect("could not run SQLx migrations");

        Self { pool }
    }

    pub async fn store_packet(&mut self, packet: &[u8]) -> anyhow::Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");

        let timestamp_i64 : i64 = timestamp.as_secs().try_into()?;

        let id = sqlx::query(r#"INSERT INTO frames_received (received_at, content) VALUES ( ?1 , ?2 )"#)
            .bind(timestamp_i64).bind(packet)
            .execute(&self.pool)
            .await?
            .last_insert_rowid();

        debug!("INSERTed row {id}");
        Ok(())
    }

    pub async fn get_most_recent_packets(&mut self, count: i64) -> anyhow::Result<Vec<Packet>> {
        let results = sqlx::query_as(r#"
               SELECT id, received_at, content
               FROM frames_received
               ORDER BY received_at DESC
               LIMIT ?1"#)
            .bind(count)
            .fetch_all(&self.pool)
            .await?;

        Ok(results)
    }
}
