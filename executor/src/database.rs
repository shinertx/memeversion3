use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use shared_models::OrderDetails;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::info;

#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub id: i64,
    pub strategy_id: String,
    pub token_address: String,
    pub symbol: String,
    pub amount_usd: f64,
    pub status: String,
    pub signature: Option<String>,
    pub entry_time: i64,
    pub entry_price_usd: f64,
    pub close_time: Option<i64>,
    pub close_price_usd: Option<f64>,
    pub pnl_usd: Option<f64>,
    pub confidence: f64,
    pub side: String,
    pub highest_price_usd: Option<f64>,
}

pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(db_path: &str) -> Result<Self> {
        let path = Path::new(db_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).with_context(|| format!("Failed to open database at {}", db_path))?;
        info!("Database opened at {}", db_path);
        Self::init_db(&conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    fn init_db(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS trades (
                id INTEGER PRIMARY KEY,
                strategy_id TEXT NOT NULL,
                token_address TEXT NOT NULL,
                symbol TEXT NOT NULL,
                amount_usd REAL NOT NULL,
                status TEXT NOT NULL,
                signature TEXT,
                entry_time INTEGER NOT NULL,
                entry_price_usd REAL NOT NULL,
                close_time INTEGER,
                close_price_usd REAL,
                pnl_usd REAL,
                confidence REAL NOT NULL,
                side TEXT NOT NULL,
                highest_price_usd REAL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn log_trade_attempt(&self, details: &OrderDetails, strategy_id: &str, entry_price_usd: f64) -> Result<i64> {
        let now: DateTime<Utc> = Utc::now();
        let conn = self.conn.lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;
        conn.execute(
            "INSERT INTO trades (strategy_id, token_address, symbol, amount_usd, status, entry_time, entry_price_usd, confidence, side, highest_price_usd)
             VALUES (?1, ?2, ?3, ?4, 'PENDING', ?5, ?6, ?7, ?8, ?9)",
            params![
                strategy_id,
                details.token_address,
                details.token_address,
                details.suggested_size_usd,
                now.timestamp(),
                entry_price_usd,
                details.confidence,
                details.side.to_string(),
                entry_price_usd,
            ],
        ).context("Failed to insert trade attempt into database")?;
        Ok(conn.last_insert_rowid())
    }

    pub fn open_trade(&self, trade_id: i64, signature: &str) -> Result<()> {
        let conn = self.conn.lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;
        conn.execute("UPDATE trades SET status = 'OPEN', signature = ?1 WHERE id = ?2", params![signature, trade_id])
            .context("Failed to update trade status to OPEN")?;
        Ok(())
    }

    pub fn update_trade_pnl(&self, trade_id: i64, status: &str, close_price_usd: f64, pnl_usd: f64) -> Result<()> {
        let now: DateTime<Utc> = Utc::now();
        let conn = self.conn.lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;
        conn.execute(
            "UPDATE trades SET status = ?1, close_time = ?2, close_price_usd = ?3, pnl_usd = ?4 WHERE id = ?5",
            params![status, now.timestamp(), close_price_usd, pnl_usd, trade_id],
        ).context("Failed to update trade PnL")?;
        Ok(())
    }
    
    pub fn get_all_trades(&self) -> Result<Vec<TradeRecord>> {
        let conn = self.conn.lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT * FROM trades ORDER BY entry_time DESC")
            .context("Failed to prepare trade query")?;
        let trades_iter = stmt.query_map([], |row| {
            Ok(TradeRecord {
                id: row.get(0)?,
                strategy_id: row.get(1)?,
                token_address: row.get(2)?,
                symbol: row.get(3)?,
                amount_usd: row.get(4)?,
                status: row.get(5)?,
                signature: row.get(6)?,
                entry_time: row.get(7)?,
                entry_price_usd: row.get(8)?,
                close_time: row.get(9)?,
                close_price_usd: row.get(10)?,
                pnl_usd: row.get(11)?,
                confidence: row.get(12)?,
                side: row.get(13)?,
                highest_price_usd: row.get(14)?,
            })
        }).context("Failed to execute trade query")?;

        trades_iter.collect::<Result<Vec<TradeRecord>, rusqlite::Error>>()
            .map_err(anyhow::Error::from)
            .context("Failed to collect trade records")
    }

    pub fn get_total_pnl(&self) -> Result<f64> {
        let conn = self.conn.lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;
        let total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(pnl_usd), 0.0) FROM trades WHERE status LIKE 'CLOSED_%'",
            [],
            |row| row.get(0),
        ).context("Failed to calculate total PnL")?;
        Ok(total)
    }

    /// Get total realized PnL for circuit breaker monitoring
    pub fn get_total_realized_pnl(&self) -> Result<f64> {
        let conn = self.conn.lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;
        let total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(pnl_usd), 0.0) FROM trades WHERE status LIKE 'CLOSED_%' AND pnl_usd IS NOT NULL",
            [],
            |row| row.get(0),
        ).context("Failed to calculate total realized PnL")?;
        Ok(total)
    }

    /// Get maximum NAV (for drawdown calculation)
    pub fn get_max_nav(&self, initial_capital_usd: f64) -> Result<f64> {
        let conn = self.conn.lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;
        // For now, calculate max NAV as initial capital + max cumulative PnL
        // In production, this should track actual NAV over time
        let max_pnl: f64 = conn.query_row(
            "SELECT COALESCE(MAX(running_pnl), 0.0) FROM (
                SELECT SUM(pnl_usd) OVER (ORDER BY close_time) as running_pnl 
                FROM trades 
                WHERE status LIKE 'CLOSED_%' AND pnl_usd IS NOT NULL AND close_time IS NOT NULL
                ORDER BY close_time
            )",
            [],
            |row| row.get(0),
        ).context("Failed to calculate maximum NAV")?;
        
        Ok(initial_capital_usd + max_pnl.max(0.0))
    }
}
