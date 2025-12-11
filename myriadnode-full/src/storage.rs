use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;
use zstd::{decode_all, encode_all};

use crate::config::StorageConfig;

/// Persistent storage manager with configurable compression
pub struct Storage {
    pool: SqlitePool,
    config: StorageConfig,
}

impl Storage {
    /// Get current Unix timestamp with graceful fallback on system time errors
    ///
    /// SECURITY: If system clock goes backwards or other time errors occur,
    /// returns a fallback timestamp instead of panicking. This is better than
    /// crashing the node during metrics recording.
    fn get_current_timestamp() -> Result<i64> {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => Ok(duration.as_secs() as i64),
            Err(e) => {
                eprintln!(
                    "WARNING: System time error in metrics storage: {}. Using fallback timestamp.",
                    e
                );
                // Return a reasonable fallback (1.5 billion seconds since epoch, ~2017)
                Ok(1500000000)
            }
        }
    }

    pub async fn new(data_dir: &Path, config: StorageConfig) -> Result<Self> {
        let db_path = data_dir.join("myriadnode.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        info!("Opening database: {}", db_path.display());

        if config.compression_enabled {
            info!(
                "Compression enabled: zstd level {} (reduces storage by ~60-80%)",
                config.compression_level
            );
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;

        // Run migrations
        Self::migrate(&pool).await?;

        Ok(Self { pool, config })
    }

    /// Compress data using zstd
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if !self.config.compression_enabled {
            return Ok(data.to_vec());
        }

        encode_all(data, self.config.compression_level)
            .map_err(|e| anyhow::anyhow!("Compression failed: {}", e))
    }

    /// Decompress data using zstd
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if !self.config.compression_enabled {
            return Ok(data.to_vec());
        }

        decode_all(data).map_err(|e| anyhow::anyhow!("Decompression failed: {}", e))
    }

    async fn migrate(pool: &SqlitePool) -> Result<()> {
        info!("Running database migrations...");

        // Create messages table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                destination TEXT NOT NULL,
                payload BLOB NOT NULL,
                priority INTEGER NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Create adapters table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS adapters (
                name TEXT PRIMARY KEY,
                adapter_type TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                last_seen INTEGER
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Create metrics table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS metrics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                adapter_name TEXT NOT NULL,
                destination TEXT NOT NULL,
                latency_ms REAL,
                bandwidth_bps REAL,
                reliability REAL,
                timestamp INTEGER NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await?;

        info!("Database migrations complete");
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        // Explicitly close the pool to ensure all connections are closed
        self.pool.close().await;
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Store adapter metrics
    pub async fn store_metrics(
        &self,
        adapter_name: &str,
        destination: &str,
        latency_ms: Option<f64>,
        bandwidth_bps: Option<f64>,
        reliability: Option<f64>,
    ) -> Result<()> {
        let timestamp = Self::get_current_timestamp()?;

        sqlx::query(
            r#"
            INSERT INTO metrics (adapter_name, destination, latency_ms, bandwidth_bps, reliability, timestamp)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(adapter_name)
        .bind(destination)
        .bind(latency_ms)
        .bind(bandwidth_bps)
        .bind(reliability)
        .bind(timestamp)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Query recent metrics for an adapter
    pub async fn get_recent_metrics(
        &self,
        adapter_name: &str,
        limit: i64,
    ) -> Result<Vec<MetricRecord>> {
        let records = sqlx::query_as::<_, MetricRecord>(
            r#"
            SELECT id, adapter_name, destination, latency_ms, bandwidth_bps, reliability, timestamp
            FROM metrics
            WHERE adapter_name = ?1
            ORDER BY timestamp DESC
            LIMIT ?2
            "#,
        )
        .bind(adapter_name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }

    /// Clean up old metrics (older than specified days)
    pub async fn cleanup_old_metrics(&self, days: i64) -> Result<u64> {
        let current_timestamp = Self::get_current_timestamp()?;
        let cutoff = current_timestamp - (days * 86400);

        let result = sqlx::query(
            r#"
            DELETE FROM metrics
            WHERE timestamp < ?1
            "#,
        )
        .bind(cutoff)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

/// Metric record from database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MetricRecord {
    pub id: i64,
    pub adapter_name: String,
    pub destination: String,
    pub latency_ms: Option<f64>,
    pub bandwidth_bps: Option<f64>,
    pub reliability: Option<f64>,
    pub timestamp: i64,
}
