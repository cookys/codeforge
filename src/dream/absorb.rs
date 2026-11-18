use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};
/// Dream Absorb：掃描 .claude/memory/ 並收編為 L0 signals
use anyhow::Result;

pub struct AbsorbResult {
    pub absorbed: usize,
}

/// The CWD project's root (the dir that holds `.codeforge`). With a shared store
/// (`CODEFORGE_DIR` not named `.codeforge`) the CWD itself is the project: never widened.
fn project_root(ctx: &db::Context) -> std::path::PathBuf {
    match (ctx.project_dir.file_name(), ctx.project_dir.parent()) {
        (Some(n), Some(parent)) if n == ".codeforge" => parent.to_path_buf(),
        _ => std::env::current_dir().unwrap_or_default(),
    }
}

pub fn run(ctx: &db::Context) -> Result<AbsorbResult> {
    let home = dirs::home_dir().unwrap_or_default();
    run_for(ctx, &home, &project_root(ctx))
}

/// Absorb ONLY this project's own Claude memory: `<home>/.claude/projects/<slug(root)>/memory/*.md`.
/// Other projects' memory never enters this project's L0 (repo rule: memory stays per project).
fn run_for(
    ctx: &db::Context,
    home: &std::path::Path,
    project_root: &std::path::Path,
) -> Result<AbsorbResult> {
    let claude_memory = home
        .join(".claude")
        .join("projects")
        .join(crate::mnemos::autocite::claude_project_slug(project_root))
        .join("memory");

    if !claude_memory.exists() {
        return Ok(AbsorbResult { absorbed: 0 });
    }

    // 找出本專案 .claude/projects/<slug>/memory/*.md 檔案
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

    for proj_entry in walkdir_shallow(&claude_memory, 1) {
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

    fn mem(home: &std::path::Path, root: &str, name: &str, body: &str) {
        let d = home
            .join(".claude/projects")
            .join(crate::mnemos::autocite::claude_project_slug(
                std::path::Path::new(root),
            ))
            .join("memory");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(name), body).unwrap();
    }

    fn l0_text(ctx: &db::Context) -> String {
        let mut all = String::new();
        if let Ok(rd) = std::fs::read_dir(ctx.project_dir.join("signals")) {
            for e in rd.flatten() {
                all.push_str(&std::fs::read_to_string(e.path()).unwrap());
            }
        }
        all
    }

    /// R1: project A's dream must not absorb project B's Claude memory.
    #[test]
    fn absorbs_only_current_projects_memory() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");
        mem(
            &home,
            "/w/proj-a",
            "a.md",
            "memory of project A: prefers tabs always",
        );
        mem(
            &home,
            "/w/proj-b",
            "b.md",
            "memory of project B: SECRET-B-CONTENT here",
        );
        let ctx = db::Context {
            project_dir: tmp.path().join("a-store"),
            brain_dir: tmp.path().join("brain"),
            db_path: tmp.path().join("state.db"),
        };
        let r = run_for(&ctx, &home, std::path::Path::new("/w/proj-a")).unwrap();
        let l0 = l0_text(&ctx);
        assert_eq!(r.absorbed, 1, "only A's own memory: {l0}");
        assert!(l0.contains("project A"), "{l0}");
        assert!(
            !l0.contains("SECRET-B-CONTENT"),
            "B leaked into A's L0: {l0}"
        );
    }
}
