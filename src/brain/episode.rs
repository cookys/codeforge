use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub date: String,
    pub project: String,
    pub summary: String,
    pub outcome: String,  // success | failure | partial
    pub tags: Vec<String>,
}

impl Episode {
    pub fn new(project: &str, summary: &str, outcome: &str) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            date: now.format("%Y-%m-%d").to_string(),
            project: project.to_string(),
            summary: summary.to_string(),
            outcome: outcome.to_string(),
            tags: vec![],
        }
    }
}

pub fn record(ctx: &db::Context, episode: &Episode) -> Result<()> {
    let episodes_dir = ctx.brain_dir.join("episodes");
    std::fs::create_dir_all(&episodes_dir)?;

    let filename = format!("{}-{}.json", episode.date, &episode.id[..8]);
    let path = episodes_dir.join(filename);
    std::fs::write(path, serde_json::to_string_pretty(episode)?)?;
    Ok(())
}

pub fn count(ctx: &db::Context) -> usize {
    let episodes_dir = ctx.brain_dir.join("episodes");
    std::fs::read_dir(&episodes_dir)
        .map(|d| d.filter_map(|e| e.ok()).count())
        .unwrap_or(0)
}
