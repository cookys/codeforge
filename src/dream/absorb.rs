use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};
/// Dream Absorb：只收編「當前專案」自己的 Claude Code memory 目錄（projects/<slug>/memory/*.md）為 L0 signals
use anyhow::Result;

pub struct AbsorbResult {
    pub absorbed: usize,
}

pub fn run(ctx: &db::Context) -> Result<AbsorbResult> {
    let projects_root = dirs::home_dir()
        .map(|h| h.join(".claude").join("projects"))
        .unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    run_from(ctx, &projects_root, &cwd)
}

/// `projects_root` = `~/.claude/projects`; `cwd` = the project dream runs for
/// (the SessionEnd hook's CWD is the project root).
pub fn run_from(
    ctx: &db::Context,
    projects_root: &std::path::Path,
    cwd: &std::path::Path,
) -> Result<AbsorbResult> {
    // Only this project's own memory dir (PM 01M7QZ9Y): dream runs at SessionEnd
    // in every project, so scanning all of `projects_root` copied every other
    // project's memory into this one's L0 (and from there into LLM prompts).
    let claude_memory = projects_root
        .join(crate::mnemos::autocite::claude_project_slug(cwd))
        .join("memory");

    if !claude_memory.exists() {
        return Ok(AbsorbResult { absorbed: 0 });
    }

    // 只看當前專案的 memory/*.md（不遞迴、不碰其他專案）
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

    for proj_entry in walkdir_shallow(&claude_memory, 0) {
        if proj_entry.extension().map(|e| e == "md").unwrap_or(false) {
            // 檢查修改時間（只吸收新的）
            let modified = std::fs::metadata(&proj_entry)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| {
                    chrono::DateTime::from_timestamp(d.as_secs() as i64, 0).unwrap_or_default()
                });

            let should_absorb = match (modified, last_absorb) {
                (Some(m), Some(l)) => m > l,
                (Some(_), None) => true,
                _ => false,
            };

            if should_absorb {
                if let Ok(content) = std::fs::read_to_string(&proj_entry) {
                    let content = content.trim().to_string();
                    if content.len() > 20 {
                        let signal = Signal::new(content, SignalSource::AbsorbedMemory);
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
        if depth > max {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn signals_text(ctx: &db::Context) -> String {
        let mut all = String::new();
        for e in std::fs::read_dir(ctx.project_dir.join("signals")).unwrap() {
            all.push_str(&std::fs::read_to_string(e.unwrap().path()).unwrap_or_default());
        }
        all
    }

    /// PM 01M7QZ9Y: dream runs at SessionEnd in every project; absorb must take
    /// only the CWD project's own Claude memory, never another project's.
    #[test]
    fn absorb_takes_only_the_current_projects_memory() {
        let temp = TempDir::new().unwrap();
        let projects_root = temp.path().join("claude-projects");
        let repo_a = temp.path().join("work").join("repo-a");
        let repo_b = temp.path().join("work").join("repo-b");
        for (repo, marker) in [(&repo_a, "MARKER_OWN_A"), (&repo_b, "MARKER_OTHER_B")] {
            let mem = projects_root
                .join(crate::mnemos::autocite::claude_project_slug(repo))
                .join("memory");
            std::fs::create_dir_all(&mem).unwrap();
            std::fs::write(
                mem.join("note.md"),
                format!("{marker} a memory note long enough to absorb"),
            )
            .unwrap();
        }
        let project_dir = repo_a.join(".codeforge");
        std::fs::create_dir_all(project_dir.join("signals")).unwrap();
        let ctx = db::Context {
            project_dir,
            brain_dir: temp.path().join("brain"),
            db_path: temp.path().join("state.db"),
        };

        let r = run_from(&ctx, &projects_root, &repo_a).unwrap();

        let text = signals_text(&ctx);
        assert!(
            !text.contains("MARKER_OTHER_B"),
            "another project's memory leaked into this project's L0"
        );
        assert!(
            text.contains("MARKER_OWN_A"),
            "own memory must still be absorbed"
        );
        assert_eq!(r.absorbed, 1);
    }
}
