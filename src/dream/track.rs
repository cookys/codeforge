use crate::db;
/// Dream Track：session 統計 → brain/skills/ 更新
use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub struct TrackResult {
    pub skills_updated: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub name: String,
    pub confidence: f32, // 0.0（novice）→ 1.0（expert）
    pub attempts: u32,
    pub successes: u32,
    pub last_seen: String,
}

pub fn run(ctx: &db::Context, conn: &Connection) -> Result<TrackResult> {
    let skills_dir = ctx.brain_dir.join("skills");
    std::fs::create_dir_all(&skills_dir)?;

    // 統計本次 session 的 dream 操作（從 dream_runs）
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let runs_today: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM dream_runs WHERE date(ran_at) = ?1",
            rusqlite::params![today],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if runs_today == 0 {
        return Ok(TrackResult { skills_updated: 0 });
    }

    // 更新 "memory-management" skill
    update_skill(&skills_dir, "memory-management", true, &today)?;
    update_skill(&skills_dir, "knowledge-organization", true, &today)?;

    Ok(TrackResult { skills_updated: 2 })
}

fn update_skill(skills_dir: &std::path::Path, name: &str, success: bool, date: &str) -> Result<()> {
    let path = skills_dir.join(format!("{}.json", name));
    let mut record = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str::<SkillRecord>(&content)?
    } else {
        SkillRecord {
            name: name.to_string(),
            confidence: 0.1,
            attempts: 0,
            successes: 0,
            last_seen: date.to_string(),
        }
    };

    record.attempts += 1;
    if success {
        record.successes += 1;
    }
    record.last_seen = date.to_string();

    // 更新 confidence（成功率 × Dreyfus 倍率）
    let success_rate = record.successes as f32 / record.attempts as f32;
    let dreyfus = dreyfus_multiplier(record.attempts);
    record.confidence = (success_rate * dreyfus).min(1.0);

    std::fs::write(&path, serde_json::to_string_pretty(&record)?)?;
    Ok(())
}

/// Dreyfus skill acquisition：隨 attempts 增加，信心加速成長
fn dreyfus_multiplier(attempts: u32) -> f32 {
    match attempts {
        0..=5 => 0.3,     // Novice
        6..=20 => 0.5,    // Advanced Beginner
        21..=50 => 0.7,   // Competent
        51..=100 => 0.85, // Proficient
        _ => 1.0,         // Expert
    }
}
