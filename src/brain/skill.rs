// SkillRecord read path — Phase 3+ skill-tree UI will consume `list`.
#![allow(dead_code)]

// Re-export dream::track 的 SkillRecord，統一存取點
pub use crate::dream::track::SkillRecord;

use anyhow::Result;
use crate::db;

pub fn list(ctx: &db::Context) -> Result<Vec<SkillRecord>> {
    let skills_dir = ctx.brain_dir.join("skills");
    if !skills_dir.exists() {
        return Ok(vec![]);
    }
    let mut records = Vec::new();
    for entry in std::fs::read_dir(&skills_dir)?.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(r) = serde_json::from_str::<SkillRecord>(&content) {
                    records.push(r);
                }
            }
        }
    }
    records.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    Ok(records)
}
