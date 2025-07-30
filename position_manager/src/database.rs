use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub id: i64,
    pub strategy_id: String,
    pub token_address: String,
    pub side: String,
    pub amount_usd: f64,
    pub entry_price_usd: f64,
    pub highest_price_usd: Option<f64>,
    pub status: String,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(db_path: &str) -> Result<Self> {
        let path = Path::new(db_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        info!("Position database opened at {}", db_path);
        Self::init_db(&conn)?;
        Ok(Self { conn })
    }

    fn init_db(conn: &Connection) -> Result<()> {
        // Use same schema as executor database
        Ok(())
    }

    pub fn get_open_trades(&self) -> Result<Vec<TradeRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, strategy_id, token_address, side, amount_usd, entry_price_usd, highest_price_usd, status 
             FROM trades WHERE status = 'OPEN'",
        )?;

        let trades = stmt.query_map([], |row| {
            Ok(TradeRecord {
                id: row.get(0)?,
                strategy_id: row.get(1)?,
                token_address: row.get(2)?,
                side: row.get(3)?,
                amount_usd: row.get(4)?,
                entry_price_usd: row.get(5)?,
                highest_price_usd: row.get(6)?,
                status: row.get(7)?,
            })
        })?;

        trades
            .collect::<Result<Vec<TradeRecord>, rusqlite::Error>>()
            .map_err(anyhow::Error::from)
    }

    pub fn update_highest_price(&self, trade_id: i64, price: f64) -> Result<()> {
        self.conn.execute(
            "UPDATE trades SET highest_price_usd = ?1 WHERE id = ?2",
            params![price, trade_id],
        )?;
        Ok(())
    }
}
