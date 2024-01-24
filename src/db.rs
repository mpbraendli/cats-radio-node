use std::time::{SystemTime, UNIX_EPOCH};
use std::io;

use log::debug;
use sqlx::{SqlitePool, sqlite::SqliteRow, Row};

#[derive(Clone)]
pub struct Database {
    pool : SqlitePool,
    num_frames_received : u64,
}

#[derive(Debug)]
pub struct Packet {
    pub id : i64,
    pub received_at: chrono::DateTime<chrono::Utc>,
    pub content : Vec<u8>,
}

impl sqlx::FromRow<'_, SqliteRow> for Packet {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            received_at: {
                let row : i64 = row.try_get("received_at")?;
                chrono::DateTime::from_timestamp(row, 0).expect("Convert timestamp to chrono")
            },
            content: row.try_get("content")?,

        })
    }
}

impl Database {
    pub async fn new() -> Self {
        {
            // Ensure the database file exists
            match std::fs::OpenOptions::new().write(true)
                .create_new(true)
                .open("cats-radio-node.db") {
                    Ok(_f) => (),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
                    Err(e) => {
                        panic!("Failed to ensure DB exists: {e}");
                    },
                }
        }

        let pool = SqlitePool::connect("sqlite:cats-radio-node.db").await.unwrap();
        let mut conn = pool.acquire().await.unwrap();

        sqlx::migrate!()
            .run(&mut conn)
            .await
            .expect("could not run SQLx migrations");

        let num_frames_received : i64 = sqlx::query_scalar(r#"SELECT COUNT(id) FROM frames_received"#)
            .fetch_one(&pool)
            .await
            .expect("could not count frames");

        Self { pool, num_frames_received: num_frames_received.try_into().unwrap() }
    }

    pub fn get_num_received_frames(&self) -> u64 {
        self.num_frames_received
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

        self.num_frames_received += 1;

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

    pub async fn get_packets_since(&mut self, unix_timestamp: i64) -> anyhow::Result<Vec<Packet>> {
        let results = sqlx::query_as(r#"
               SELECT id, received_at, content
               FROM frames_received
               WHERE received_at > ?1
               ORDER BY received_at DESC"#)
            .bind(unix_timestamp)
            .fetch_all(&self.pool)
            .await?;

        Ok(results)
    }
}
