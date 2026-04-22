/// Dream Decay：ACT-R activation 計算，更新 L1 strength
use anyhow::Result;
use crate::db;
use crate::memory::l1;

pub struct DecayResult {
    pub updated: usize,
}

/// ACT-R decay formula: B = ln(Σ t_i^(-0.5))
/// t_i = 距離第 i 次引用的秒數
/// 簡化版：使用建立天數作為 decay factor
pub fn run(ctx: &db::Context) -> Result<DecayResult> {
    let store_dir = ctx.project_dir.join("store");
    let entries = l1::scan_all(&store_dir)?;
    let now = chrono::Utc::now().date_naive();
    let mut updated = 0;

    for mut entry in entries {
        if entry.frontmatter.status != "active" { continue; }

        // 計算從建立日到現在的天數
        let created = chrono::NaiveDate::parse_from_str(&entry.frontmatter.created, "%Y-%m-%d")
            .unwrap_or(now);
        let days_old = (now - created).num_days().max(1) as f64;

        // 引用次數加速衰減（被引用越多，衰減越慢）
        let refs_bonus = (entry.frontmatter.refs as f64 + 1.0).ln();

        // strength ∈ [0.0, 1.0]，以天數衰減
        let new_strength = (refs_bonus / days_old.sqrt()).clamp(0.0, 1.0) as f32;

        if (new_strength - entry.frontmatter.strength).abs() > 0.01 {
            entry.frontmatter.strength = new_strength;
            entry.frontmatter.updated = now.to_string();
            entry.save()?;
            updated += 1;
        }
    }

    Ok(DecayResult { updated })
}
