/// Dream Dedup：標記相似度高的重複條目（rule-based）
use anyhow::Result;
use crate::db;
use crate::memory::l1;

pub struct DedupResult {
    pub marked: usize,
}

pub fn run(ctx: &db::Context) -> Result<DedupResult> {
    let store_dir = ctx.project_dir.join("store");
    let entries = l1::scan_all(&store_dir)?;
    let mut marked = 0;

    // 比較 topic 相似度（簡單字詞重疊）
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            if entries[i].frontmatter.status != "active" { continue; }
            if entries[j].frontmatter.status != "active" { continue; }

            let sim = topic_similarity(&entries[i].frontmatter.topic, &entries[j].frontmatter.topic);
            if sim > 0.85 {
                // 保留 strength 較高的，標記另一個為 superseded
                let (keep, supersede) = if entries[i].frontmatter.strength >= entries[j].frontmatter.strength {
                    (i, j)
                } else {
                    (j, i)
                };

                let mut entry = entries[supersede].clone();
                entry.frontmatter.status = format!("superseded-by:{}", l1::L1Entry::slugify(&entries[keep].frontmatter.topic));
                entry.save()?;
                marked += 1;
            }
        }
    }

    Ok(DedupResult { marked })
}

fn topic_similarity(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();
    if union == 0 { return 0.0; }
    intersection as f64 / union as f64
}
