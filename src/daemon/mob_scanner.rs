//! Scan source files for MOB-worthy patterns (Phase 2b P2).
//!
//! Heuristics are deliberately dumb — no AST, no type resolution. Good
//! enough to produce a steady stream of MOBs during `cargo run` on a real
//! codebase and exercise the combat loop. Strong analysis lives in
//! Phase 3+ (dead-code → `cargo check` JSON, duplicates → token hashing).
//!
//! Call path: `tick::run_one` → `rate_limited_scan` (every 10 ticks) →
//! `scan_dir` → `analyze_file` → `persist_scan` (`INSERT OR IGNORE`).

use anyhow::Result;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::mob::{MobKind, MobSpec};

/// Max files walked per scan — bound worst-case cost.
pub const MAX_FILES_TO_SCAN: usize = 1000;
/// Max MOBs emitted per single scan — keeps the zone manageable.
pub const MAX_MOBS_PER_SCAN: usize = 20;
/// Scanner runs on tick_count 1, 11, 21, ... (every N ticks).
pub const SCAN_EVERY_N_TICKS: u64 = 10;
/// Zombie threshold — TODO/FIXME lines per file.
pub const ZOMBIE_TODO_THRESHOLD: usize = 5;
/// Boss threshold — function lines (brace-counted).
pub const BOSS_FN_LINES_THRESHOLD: usize = 100;
/// Elite threshold — file-level branching keyword count.
pub const ELITE_BRANCH_THRESHOLD: usize = 20;

const SOURCE_EXTS: &[&str] = &["rs", "py", "ts", "js", "go"];

/// Returns true when the current tick should run the scanner.
/// Fires on tick 1, 11, 21, ... — avoids scanning every tick.
pub fn should_scan_this_tick(tick_count: u64) -> bool {
    tick_count > 0 && (tick_count - 1).is_multiple_of(SCAN_EVERY_N_TICKS)
}

/// End-to-end: if scan fires this tick AND a scan dir is configured, walk
/// it, analyze files, and upsert MOBs. Swallows scanner-level errors
/// (filesystem hiccups shouldn't break the tick) — logs to stderr.
pub fn rate_limited_scan(
    tx: &Connection,
    zone_id: &str,
    tick_count: u64,
) -> Result<usize> {
    if !should_scan_this_tick(tick_count) {
        return Ok(0);
    }
    let dir = match resolve_scan_dir() {
        Some(d) => d,
        None => return Ok(0),
    };
    let specs = match scan_dir(&dir, zone_id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("codeforge daemon: scanner error on {}: {e}", dir.display());
            return Ok(0);
        }
    };
    persist_scan(tx, &specs)
}

/// Reads `CODEFORGE_SCAN_DIR`. Absent = scanner no-op (daemon run via
/// systemd from $HOME has no meaningful source root by default).
pub fn resolve_scan_dir() -> Option<PathBuf> {
    let raw = std::env::var("CODEFORGE_SCAN_DIR").ok()?;
    let p = PathBuf::from(raw);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// Walk `root`, apply heuristics, return MobSpecs capped at MAX_MOBS_PER_SCAN.
pub fn scan_dir(root: &Path, zone_id: &str) -> std::io::Result<Vec<MobSpec>> {
    let files = collect_source_files(root, MAX_FILES_TO_SCAN)?;
    let mut specs: Vec<MobSpec> = Vec::new();
    for path in files {
        if specs.len() >= MAX_MOBS_PER_SCAN {
            break;
        }
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        for spec in analyze_file(&path, &text, zone_id) {
            if specs.len() >= MAX_MOBS_PER_SCAN {
                break;
            }
            specs.push(spec);
        }
    }
    Ok(specs)
}

/// Upsert MobSpecs. `INSERT OR IGNORE` respects the partial unique
/// `idx_mobs_unique_alive` index — re-scans never duplicate alive mobs,
/// and existing combat HP is preserved across scans.
pub fn persist_scan(conn: &Connection, specs: &[MobSpec]) -> Result<usize> {
    if specs.is_empty() {
        return Ok(0);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let mut inserted = 0usize;
    for s in specs {
        let n = conn.execute(
            "INSERT OR IGNORE INTO mobs
                 (zone_id, kind, name, hp, hp_max, atk, def, difficulty, spawned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                s.zone_id,
                s.kind.as_str(),
                s.name,
                s.hp as i64,
                s.hp_max as i64,
                s.atk as i64,
                s.def as i64,
                s.difficulty as i64,
                now,
            ],
        )?;
        inserted += n;
    }
    Ok(inserted)
}

// ─── File collection ────────────────────────────────────────────────

fn collect_source_files(root: &Path, cap: usize) -> std::io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if out.len() >= cap {
            break;
        }
        if should_skip_dir(&dir) {
            continue;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() && has_source_ext(&p) {
                out.push(p);
                if out.len() >= cap {
                    break;
                }
            }
        }
    }
    Ok(out)
}

fn should_skip_dir(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    matches!(
        name,
        "target"
            | "node_modules"
            | ".git"
            | "dist"
            | "build"
            | "__pycache__"
            | ".venv"
            | "venv"
            | ".codeforge"
            | ".next"
    )
}

fn has_source_ext(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| SOURCE_EXTS.contains(&e))
        .unwrap_or(false)
}

// ─── Heuristics ─────────────────────────────────────────────────────

fn analyze_file(path: &Path, text: &str, zone_id: &str) -> Vec<MobSpec> {
    let mut out = Vec::new();
    let pretty = path.display().to_string();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Zombie — TODO/FIXME clusters
    let todos = count_todos(text);
    if todos >= ZOMBIE_TODO_THRESHOLD {
        out.push(MobSpec::new(
            zone_id,
            MobKind::Zombie,
            format!("TODOs × {todos} @ {pretty}"),
        ));
    }

    // Boss — long function (brace-family languages; Python indent-dedent not done in MVP)
    let is_brace = matches!(ext, "rs" | "ts" | "js" | "go");
    if is_brace {
        if let Some(len) = longest_brace_function_lines(text) {
            if len > BOSS_FN_LINES_THRESHOLD {
                out.push(MobSpec::new(
                    zone_id,
                    MobKind::Boss,
                    format!("{len}-line fn @ {pretty}"),
                ));
            }
        }
    }

    // Elite — high file-level branching count (cyclomatic complexity proxy)
    let branches = count_branches(text);
    if branches >= ELITE_BRANCH_THRESHOLD {
        out.push(MobSpec::new(
            zone_id,
            MobKind::Elite,
            format!("branches × {branches} @ {pretty}"),
        ));
    }

    // Ghost — crude unused-import heuristic (Rust only in MVP)
    if ext == "rs" {
        if let Some(unused) = first_unused_use_item(text) {
            out.push(MobSpec::new(
                zone_id,
                MobKind::Ghost,
                format!("unused `{unused}` @ {pretty}"),
            ));
        }
    }

    out
}

fn count_todos(s: &str) -> usize {
    s.lines()
        .filter(|l| l.contains("TODO") || l.contains("FIXME"))
        .count()
}

fn count_branches(s: &str) -> usize {
    // Keyword boundaries chosen to avoid false positives on identifiers
    // like `if_let` or `match_pattern`. Not perfect but good enough.
    let mut n = 0usize;
    for pat in [" if ", "\tif ", "}else", " else ", " match ", " for ", " while ", "case "] {
        n += s.matches(pat).count();
    }
    n
}

/// Greedy brace-depth walk. Finds the longest top-level `fn`/`function`/`func`
/// body by line count. Nested closures inside a function body don't create
/// new top-level functions (the outer depth-0 counter keeps counting).
fn longest_brace_function_lines(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut max_lines = 0usize;

    // Collect all candidate function-start positions
    for fn_start in find_function_keywords(text) {
        // Find the next `{` after fn_start (skipping signatures that span lines)
        let tail = &bytes[fn_start..];
        let brace_rel = match find_first_brace(tail) {
            Some(i) => i,
            None => continue,
        };
        let open = fn_start + brace_rel;
        if let Some(lines) = trace_brace_block_lines(bytes, open) {
            if lines > max_lines {
                max_lines = lines;
            }
        }
    }

    if max_lines > 0 {
        Some(max_lines)
    } else {
        None
    }
}

fn find_function_keywords(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for kw in ["fn ", "function ", "func "] {
        let mut pos = 0;
        while let Some(rel) = text[pos..].find(kw) {
            let abs = pos + rel;
            // Filter: must start of line OR preceded by whitespace/visibility keyword
            if abs == 0
                || text.as_bytes()[abs - 1].is_ascii_whitespace()
                || text.as_bytes()[abs - 1] == b';'
            {
                out.push(abs);
            }
            pos = abs + kw.len();
        }
    }
    out.sort_unstable();
    out
}

fn find_first_brace(b: &[u8]) -> Option<usize> {
    b.iter().position(|&c| c == b'{')
}

fn trace_brace_block_lines(b: &[u8], open: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    for (idx, &ch) in b.iter().enumerate().skip(open) {
        match ch {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let body = &b[open..=idx];
                    let lines = body.iter().filter(|&&c| c == b'\n').count();
                    return Some(lines.max(1));
                }
            }
            _ => {}
        }
        if depth > 1000 {
            return None;
        }
    }
    None
}

/// Naive unused-import detector (Rust). Looks for `use a::b::c::Item;`
/// where `Item` appears exactly once in the file (the use line itself).
/// Skips grouped / wildcard / aliased imports — too hard to reliably
/// de-reference without a real parser.
fn first_unused_use_item(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("use ") || !trimmed.ends_with(';') {
            continue;
        }
        let path = trimmed
            .trim_start_matches("use ")
            .trim_end_matches(';')
            .trim();
        if path.contains('{') || path.contains('*') || path.contains(" as ") {
            continue;
        }
        let last = path.rsplit("::").next()?.trim();
        if last.is_empty() {
            continue;
        }
        let count = text.matches(last).count();
        if count <= 1 {
            return Some(last.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use tempfile::tempdir;

    // ─── Helpers ───────────────────────────────────────────────

    fn write(p: &Path, content: &str) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    // ─── Rate limiting ─────────────────────────────────────────

    #[test]
    fn rate_limit_fires_on_tick_1_11_21() {
        assert!(should_scan_this_tick(1));
        assert!(should_scan_this_tick(11));
        assert!(should_scan_this_tick(21));
        assert!(!should_scan_this_tick(0));
        assert!(!should_scan_this_tick(2));
        assert!(!should_scan_this_tick(10));
        assert!(!should_scan_this_tick(12));
    }

    // ─── Individual heuristics ─────────────────────────────────

    #[test]
    fn todo_count_counts_todos_and_fixmes() {
        let text = "// TODO: foo\n// FIXME: bar\nok\n// TODO: baz\n";
        assert_eq!(count_todos(text), 3);
    }

    #[test]
    fn branch_count_gets_common_keywords() {
        let text = r#"
            fn x() {
                if a { }
                if b { }
                match z {
                    _ => { for i in 0..10 { while true { } } }
                }
                if c { } else { }
            }
        "#;
        assert!(count_branches(text) >= 5);
    }

    #[test]
    fn longest_fn_brace_counts_lines() {
        // 8-line function body
        let text = "\
fn small() {\n    a;\n    b;\n}\n\
fn big() {\n    a;\n    b;\n    c;\n    d;\n    e;\n    f;\n    g;\n}\n";
        let len = longest_brace_function_lines(text).expect("should find fns");
        assert!(len >= 8, "got {len}");
    }

    #[test]
    fn longest_fn_handles_nested_braces() {
        let text = "fn outer() {\n    if x {\n        if y {\n            do_thing();\n        }\n    }\n}\n";
        assert!(longest_brace_function_lines(text).is_some());
    }

    #[test]
    fn unused_import_detector_catches_single_occurrence() {
        let text = "use std::path::PathBuf;\n\nfn main() { println!(\"hi\"); }\n";
        assert_eq!(first_unused_use_item(text), Some("PathBuf".to_string()));
    }

    #[test]
    fn unused_import_detector_skips_used_imports() {
        let text = "use std::path::PathBuf;\n\nfn x(p: PathBuf) { let _ = p; }\n";
        assert_eq!(first_unused_use_item(text), None);
    }

    #[test]
    fn unused_import_detector_skips_braced_imports() {
        // Braced groups are too ambiguous for the simple regex-free check
        let text = "use std::{path::PathBuf, fs};\n\nfn main() {}\n";
        assert_eq!(first_unused_use_item(text), None);
    }

    // ─── End-to-end scan on tempdir ────────────────────────────

    #[test]
    fn scan_produces_zombie_for_todo_cluster() {
        let dir = tempdir().unwrap();
        write(
            &dir.path().join("src/todo.rs"),
            "// TODO: 1\n// TODO: 2\n// FIXME: 3\n// TODO: 4\n// FIXME: 5\nfn main() {}\n",
        );
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(
            specs.iter().any(|s| s.kind == MobKind::Zombie),
            "expected a Zombie MOB, got: {specs:?}"
        );
    }

    #[test]
    fn scan_produces_boss_for_long_function() {
        let dir = tempdir().unwrap();
        let mut body = String::from("fn long() {\n");
        for i in 0..150 {
            body.push_str(&format!("    let _line{i} = {i};\n"));
        }
        body.push_str("}\n");
        write(&dir.path().join("src/boss.rs"), &body);
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(specs.iter().any(|s| s.kind == MobKind::Boss));
    }

    #[test]
    fn scan_produces_elite_for_branchy_file() {
        let dir = tempdir().unwrap();
        let mut body = String::from("fn branchy() {\n");
        for i in 0..25 {
            body.push_str(&format!("    if x{i} {{ }}\n"));
        }
        body.push_str("}\n");
        write(&dir.path().join("src/branchy.rs"), &body);
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(specs.iter().any(|s| s.kind == MobKind::Elite));
    }

    #[test]
    fn scan_produces_ghost_for_unused_import() {
        let dir = tempdir().unwrap();
        write(
            &dir.path().join("src/ghost.rs"),
            "use std::collections::HashMap;\n\nfn main() {}\n",
        );
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(specs.iter().any(|s| s.kind == MobKind::Ghost));
    }

    #[test]
    fn scan_respects_max_mobs_cap() {
        let dir = tempdir().unwrap();
        // 30 files each producing a zombie
        for i in 0..30 {
            write(
                &dir.path().join(format!("src/todo_{i}.rs")),
                "// TODO: a\n// TODO: b\n// TODO: c\n// TODO: d\n// TODO: e\nfn main() {}\n",
            );
        }
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(specs.len() <= MAX_MOBS_PER_SCAN);
    }

    #[test]
    fn scan_skips_target_and_node_modules() {
        let dir = tempdir().unwrap();
        write(
            &dir.path().join("target/debug/junk.rs"),
            "// TODO: a\n// TODO: b\n// TODO: c\n// TODO: d\n// TODO: e\n",
        );
        write(
            &dir.path().join("node_modules/dep/x.js"),
            "// TODO: a\n// TODO: b\n// TODO: c\n// TODO: d\n// TODO: e\n",
        );
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(
            specs.is_empty(),
            "skip-dirs should suppress all files under target/ and node_modules/, got: {specs:?}"
        );
    }

    // ─── Persistence ───────────────────────────────────────────

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn persist_scan_inserts_new_mobs() {
        let conn = fresh_conn();
        let specs = vec![
            MobSpec::new("rust", MobKind::Boss, "src/a.rs".to_string()),
            MobSpec::new("rust", MobKind::Zombie, "TODOs × 8 @ src/b.rs".to_string()),
        ];
        let n = persist_scan(&conn, &specs).unwrap();
        assert_eq!(n, 2);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM mobs WHERE defeated_at IS NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 2);
    }

    #[test]
    fn persist_scan_is_idempotent_for_alive_mobs() {
        // Re-running scanner shouldn't duplicate alive mobs — respects
        // partial unique index idx_mobs_unique_alive.
        let conn = fresh_conn();
        let specs = vec![MobSpec::new("rust", MobKind::Boss, "src/a.rs".to_string())];
        persist_scan(&conn, &specs).unwrap();
        let inserted_again = persist_scan(&conn, &specs).unwrap();
        assert_eq!(inserted_again, 0, "second insert must be ignored");
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM mobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn persist_scan_respawns_after_defeat() {
        let conn = fresh_conn();
        let specs = vec![MobSpec::new("rust", MobKind::Ghost, "src/a.rs".to_string())];
        persist_scan(&conn, &specs).unwrap();
        conn.execute("UPDATE mobs SET defeated_at = 9999", []).unwrap();
        // Partial index excludes defeated rows — new spawn allowed
        let n = persist_scan(&conn, &specs).unwrap();
        assert_eq!(n, 1);
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM mobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 2);
    }
}
