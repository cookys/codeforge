/// Dream Absorb：掃描 .claude/memory/ 並收編為 L0 signals
use anyhow::Result;
use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};

pub struct AbsorbResult {
    pub absorbed: usize,
}

pub fn run(ctx: &db::Context) -> Result<AbsorbResult> {
    let claude_memory = dirs::home_dir()
        .map(|h| h.join(".claude").join("projects"))
        .unwrap_or_default();

    if !claude_memory.exists() {
        return Ok(AbsorbResult { absorbed: 0 });
    }

    // 找出所有 .claude/projects/*/memory/*.md 檔案
    let mut absorbed = 0;
    let writer = SignalWriter::new(ctx);

    // 讀取 absorb 狀態（上次掃描的時間戳，避免重複吸收）
    let state_file = ctx.project_dir.join(".absorb_cursor");
    let last_absorb = state_file
        .exists()
        .then(|| std::fs::read_to_string(&state_file).ok())
        .flatten()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s.trim()).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    for proj_entry in walkdir_shallow(&claude_memory, 3) {
        if proj_entry.extension().map(|e| e == "md").unwrap_or(false) {
            // 檢查修改時間（只吸收新的）
            let modified = std::fs::metadata(&proj_entry)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0).unwrap_or_default());

            let should_absorb = match (modified, last_absorb) {
                (Some(m), Some(l)) => m > l,
                (Some(_), None) => true,
                _ => false,
            };

            if should_absorb {
                if let Ok(content) = std::fs::read_to_string(&proj_entry) {
                    let content = content.trim().to_string();
                    if content.len() > 20 {
                        let signal = Signal::new(content, SignalSource::ClaudeCodeSession);
                        if writer.append(&signal).is_ok() {
                            absorbed += 1;
                        }
                    }
                }
            }
        }
    }

    // 更新 absorb cursor
    let now = chrono::Utc::now().to_rfc3339();
    let _ = std::fs::write(&state_file, now);

    Ok(AbsorbResult { absorbed })
}

fn walkdir_shallow(base: &std::path::Path, max_depth: usize) -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, depth: usize, max: usize, result: &mut Vec<std::path::PathBuf>) {
        if depth > max { return; }
        let Ok(entries) = std::fs::read_dir(dir) else { return; };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, depth + 1, max, result);
            } else {
                result.push(path);
            }
        }
    }
    let mut result = Vec::new();
    walk(base, 0, max_depth, &mut result);
    result
}
