use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};
/// Dream Absorb：只收編「當前專案」自己的 Claude Code memory 目錄（projects/<slug>/memory/*.md）為 L0 signals
use anyhow::Result;

pub struct AbsorbResult {
    pub absorbed: usize,
}

/// The project dream runs for: the dir that holds `.codeforge`. With a shared store
/// (`CODEFORGE_DIR` not named `.codeforge`) the CWD itself is the project: never widened.
fn project_root(ctx: &db::Context) -> std::path::PathBuf {
    match (ctx.project_dir.file_name(), ctx.project_dir.parent()) {
        (Some(n), Some(parent)) if n == ".codeforge" => parent.to_path_buf(),
        _ => std::env::current_dir().unwrap_or_default(),
    }
}

pub fn run(ctx: &db::Context) -> Result<AbsorbResult> {
    let projects_root = dirs::home_dir()
        .map(|h| h.join(".claude").join("projects"))
        .unwrap_or_default();
    run_from(ctx, &projects_root, &project_root(ctx))
}

/// `projects_root` = the Claude projects dir; `cwd` = the project dream runs for
/// (`project_root`: the store's repo, else the SessionEnd hook's CWD).
pub fn run_from(
    ctx: &db::Context,
    projects_root: &std::path::Path,
    cwd: &std::path::Path,
) -> Result<AbsorbResult> {
    // Only this project's own memory dir (PM 01M7QZ9Y): dream runs at SessionEnd
    // in every project, so scanning all of `projects_root` copied every other
    // project's memory into this one's L0 (and from there into LLM prompts).
    let slug = crate::mnemos::autocite::claude_project_slug(cwd);
    let claude_memory = projects_root.join(&slug).join("memory");

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
                        let file_name = proj_entry
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let signal = Signal::new(content, SignalSource::AbsorbedMemory)
                            .with_absorb_source(slug.clone(), file_name);
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

#[derive(Debug, Default, PartialEq, Eq)]
pub struct AbsorbAudit {
    pub cross_project: usize,
    pub legacy_no_slug: usize,
}

/// 只計數稽核：統計 L0 signals 中 `AbsorbedMemory` 列的跨專案與無 slug 舊列數量。
pub fn audit(ctx: &db::Context) -> Result<AbsorbAudit> {
    let current_slug = crate::mnemos::autocite::claude_project_slug(&project_root(ctx));
    audit_for_slug(ctx, &current_slug)
}

pub fn audit_for_slug(ctx: &db::Context, current_slug: &str) -> Result<AbsorbAudit> {
    let signal_dir = ctx.project_dir.join("signals");
    if !signal_dir.exists() {
        return Ok(AbsorbAudit::default());
    }

    let mut result = AbsorbAudit::default();
    for entry in std::fs::read_dir(&signal_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(signal) = serde_json::from_str::<Signal>(trimmed) else {
                continue;
            };
            if matches!(signal.source, SignalSource::AbsorbedMemory) {
                match signal.source_slug.as_deref() {
                    None => result.legacy_no_slug += 1,
                    Some(s) if s != current_slug => result.cross_project += 1,
                    Some(_) => {}
                }
            }
        }
    }
    Ok(result)
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
    /// A `<repo>/.codeforge` store resolves to its repo even when the CWD is elsewhere;
    /// a shared store (`CODEFORGE_DIR` not named `.codeforge`) falls back to the CWD.
    #[test]
    fn project_root_is_the_stores_repo() {
        let mk = |pd: std::path::PathBuf| db::Context {
            project_dir: pd,
            brain_dir: "b".into(),
            db_path: "d".into(),
        };
        let repo = std::path::Path::new("work/proj-a");
        assert_eq!(project_root(&mk(repo.join(".codeforge"))), repo);
        assert_eq!(
            project_root(&mk("shared-store".into())),
            std::env::current_dir().unwrap()
        );
    }

    #[test]
    fn absorb_records_source_slug_and_filename_only() {
        let temp = TempDir::new().unwrap();
        let projects_root = temp.path().join("claude-projects");
        let repo_a = temp.path().join("work").join("repo-a");
        let expected_slug = crate::mnemos::autocite::claude_project_slug(&repo_a);
        let mem = projects_root.join(&expected_slug).join("memory");
        std::fs::create_dir_all(&mem).unwrap();
        std::fs::write(
            mem.join("note.md"),
            "a memory note long enough to absorb into L0",
        )
        .unwrap();

        let project_dir = repo_a.join(".codeforge");
        std::fs::create_dir_all(project_dir.join("signals")).unwrap();
        let ctx = db::Context {
            project_dir,
            brain_dir: temp.path().join("brain"),
            db_path: temp.path().join("state.db"),
        };

        let r = run_from(&ctx, &projects_root, &repo_a).unwrap();
        assert_eq!(r.absorbed, 1);

        let text = signals_text(&ctx);
        let sig: Signal = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(sig.source_slug.as_deref(), Some(expected_slug.as_str()));
        assert_eq!(sig.source_file.as_deref(), Some("note.md"));
    }

    #[test]
    fn legacy_signal_without_source_fields_deserializes() {
        let legacy_json = r#"{"id":"11111111-1111-1111-1111-111111111111","content":"legacy absorbed memory","source":"AbsorbedMemory","timestamp":"2026-01-01T00:00:00Z","compiled":false}"#;
        let sig: Signal = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(sig.source_slug, None);
        assert_eq!(sig.source_file, None);
    }

    #[test]
    fn audit_counts_cross_project_and_legacy_signals() {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path().join("proj").join(".codeforge");
        let signals_dir = project_dir.join("signals");
        std::fs::create_dir_all(&signals_dir).unwrap();
        let ctx = db::Context {
            project_dir,
            brain_dir: temp.path().join("brain"),
            db_path: temp.path().join("state.db"),
        };
        let current_slug = "slug-current";

        // Case 1: all same project
        let own = Signal::new("own memory".into(), SignalSource::AbsorbedMemory)
            .with_absorb_source(current_slug.into(), "a.md".into());
        let manual = Signal::new("manual note".into(), SignalSource::Manual);
        std::fs::write(
            signals_dir.join("2026-01-01.jsonl"),
            format!(
                "{}\n{}\n",
                serde_json::to_string(&own).unwrap(),
                serde_json::to_string(&manual).unwrap()
            ),
        )
        .unwrap();
        assert_eq!(
            audit_for_slug(&ctx, current_slug).unwrap(),
            AbsorbAudit {
                cross_project: 0,
                legacy_no_slug: 0
            }
        );

        // Case 2: mixed with another slug
        let other = Signal::new("other memory".into(), SignalSource::AbsorbedMemory)
            .with_absorb_source("slug-other".into(), "b.md".into());
        std::fs::write(
            signals_dir.join("2026-01-02.jsonl"),
            format!("{}\n", serde_json::to_string(&other).unwrap()),
        )
        .unwrap();
        assert_eq!(
            audit_for_slug(&ctx, current_slug).unwrap(),
            AbsorbAudit {
                cross_project: 1,
                legacy_no_slug: 0
            }
        );

        // Case 3: legacy without slug
        let legacy = Signal::new("legacy memory".into(), SignalSource::AbsorbedMemory);
        std::fs::write(
            signals_dir.join("2026-01-01.jsonl"),
            format!("{}\n", serde_json::to_string(&legacy).unwrap()),
        )
        .unwrap();
        std::fs::remove_file(signals_dir.join("2026-01-02.jsonl")).unwrap();
        assert_eq!(
            audit_for_slug(&ctx, current_slug).unwrap(),
            AbsorbAudit {
                cross_project: 0,
                legacy_no_slug: 1
            }
        );
    }
}
