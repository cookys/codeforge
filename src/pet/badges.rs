use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Badge {
    pub badge_id: String,
    pub name: String,
    pub description: Option<String>,
}

pub fn list(conn: &Connection) -> Result<Vec<Badge>> {
    let mut stmt = conn.prepare(
        "SELECT badge_id, name, description FROM badges ORDER BY earned_at ASC"
    )?;
    let badges = stmt.query_map([], |row| {
        Ok(Badge {
            badge_id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
        })
    })?
    .filter_map(|r| r.ok())
    .collect();
    Ok(badges)
}

pub fn award(conn: &Connection, badge_id: &str, name: &str, description: Option<&str>) -> Result<bool> {
    // 如果已有此徽章，直接回傳 false
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM badges WHERE badge_id = ?1",
        rusqlite::params![badge_id],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if exists { return Ok(false); }

    conn.execute(
        "INSERT INTO badges (badge_id, name, description) VALUES (?1, ?2, ?3)",
        rusqlite::params![badge_id, name, description],
    )?;
    Ok(true)
}
