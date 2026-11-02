//! Scan source files for MOB-worthy patterns (Phase 2b P2).
//!
//! Heuristics are deliberately dumb — no AST, no type resolution. Good
//! enough to produce a steady stream of MOBs during `cargo run` on a real
//! codebase and exercise the combat loop. Strong analysis (dead-code →
//! `cargo check` JSON) stays future work.
//!
//! Duplicate code blocks (Doppelganger, B10) are a cross-file pass:
//! `scan_dir` feeds every file it reads into `dup_detect::DupIndex` and
//! appends up to `MAX_DOPPELGANGERS_PER_SCAN` pairs after the per-file
//! heuristics.
//!
//! Call path: `tick::run_one` → `rate_limited_scan` (every 10 ticks) →
//! `scan_dir` → `analyze_file` + `DupIndex` → `persist_scan`
//! (`INSERT OR IGNORE`).

use anyhow::Result;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::dup_detect::DupIndex;
use super::mob::{MobKind, MobSpec};

/// Max files walked per scan — bound worst-case cost.
pub const MAX_FILES_TO_SCAN: usize = 1000;
/// Max MOBs emitted per single scan — keeps the zone manageable.
pub const MAX_MOBS_PER_SCAN: usize = 20;
/// Per-file byte cap for the full-read heuristics below. Any single file
/// exceeding this is skipped rather than loaded — prevents a bundled
/// `dist/*.min.js` or a generated `.rs` from blowing out daemon memory.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Max Doppelgangers (duplicate-block pairs) per scan. Not reserved up
/// front: per-file heuristics may fill all `MAX_MOBS_PER_SCAN` slots and
/// give back one slot per Doppelganger actually found.
pub const MAX_DOPPELGANGERS_PER_SCAN: usize = 3;
/// Cumulative bytes read per scan. The scan runs inside the tick
/// transaction, so the whole read loop stops once this is spent.
pub const SCAN_READ_BUDGET_BYTES: u64 = 32 * 1024 * 1024;
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
pub fn rate_limited_scan(tx: &Connection, zone_id: &str, tick_count: u64) -> Result<usize> {
    rate_limited_scan_with_dir(tx, zone_id, tick_count, resolve_scan_dir().as_deref())
}

/// Same as `rate_limited_scan` but with the scan root injected — keeps
/// tests free of the `CODEFORGE_SCAN_DIR` env-var dance, which under
/// `cargo test`'s parallel runner would let parallel scanner tests see
/// each other's tempdirs.
pub fn rate_limited_scan_with_dir(
    tx: &Connection,
    zone_id: &str,
    tick_count: u64,
    dir_override: Option<&Path>,
) -> Result<usize> {
    if !should_scan_this_tick(tick_count) {
        return Ok(0);
    }
    let dir = match dir_override {
        Some(d) => d.to_path_buf(),
        None => return Ok(0),
    };
    let mut specs = match scan_dir(&dir, zone_id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("codeforge daemon: scanner error on {}: {e}", dir.display());
            return Ok(0);
        }
    };
    // Phase 3e Ghost Repellent: when an active `suppress_ghost_spawn`
    // effect covers this zone (or is global), drop every Ghost from the
    // newly scanned batch before persisting. Existing alive Ghost mobs
    // are untouched — the item only stops *new* spawns, matching the
    // spec wording "不再在此 Zone 生成".
    let now = unix_now();
    if crate::craft::is_effect_active(
        tx,
        crate::craft::EffectKind::SuppressGhostSpawn,
        zone_id,
        now,
    )? {
        specs.retain(|s| s.kind != crate::daemon::mob::MobKind::Ghost);
    }
    persist_scan(tx, &specs)
}

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
///
/// Every file (up to `MAX_FILES_TO_SCAN` / `SCAN_READ_BUDGET_BYTES`) is
/// read once: per-file heuristics stop *collecting* at the cap but the
/// loop keeps feeding the duplicate index. Output = first
/// `MAX_MOBS_PER_SCAN - d` per-file specs + the `d` Doppelgangers.
pub fn scan_dir(root: &Path, zone_id: &str) -> std::io::Result<Vec<MobSpec>> {
    let files = collect_source_files(root, MAX_FILES_TO_SCAN)?;
    let mut specs: Vec<MobSpec> = Vec::new();
    let mut dups = DupIndex::new();
    let mut read_bytes: u64 = 0;
    for path in files {
        // Stat first — skip files larger than MAX_FILE_BYTES before loading.
        // Any stat error (permission denied, broken symlink) is treated the
        // same as a read error below: skip silently, continue the scan.
        let len = match fs::metadata(&path) {
            Ok(m) if m.len() > MAX_FILE_BYTES => continue,
            Err(_) => continue,
            Ok(m) => m.len(),
        };
        if read_bytes + len > SCAN_READ_BUDGET_BYTES {
            break;
        }
        read_bytes += len;
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        // origin_path: path relative to scan root, using forward slashes so
        // Local Map can group by "src", "doc", etc. regardless of OS.
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if specs.len() < MAX_MOBS_PER_SCAN {
            for spec in analyze_file(&path, &text, zone_id) {
                if specs.len() >= MAX_MOBS_PER_SCAN {
                    break;
                }
                specs.push(spec.with_origin_path(rel.clone()));
            }
        }
        if !DupIndex::is_excluded(&rel, &text) {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            dups.add_file(&rel, ext, &text);
        }
    }
    let doppelgangers: Vec<MobSpec> = dups
        .find_pairs()
        .into_iter()
        .take(MAX_DOPPELGANGERS_PER_SCAN)
        .map(|p| {
            MobSpec::new(
                zone_id,
                MobKind::Doppelganger,
                format!("dup @ {} ↔ {}", p.a, p.b),
            )
            .with_origin_path(p.a)
        })
        .collect();
    specs.truncate(MAX_MOBS_PER_SCAN - doppelgangers.len());
    specs.extend(doppelgangers);
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
                 (zone_id, kind, name, hp, hp_max, atk, def, difficulty,
                  spawned_at, origin_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
                s.origin_path,
            ],
        )?;
        inserted += n;
    }
    Ok(inserted)
}

// ─── File collection ────────────────────────────────────────────────

/// Depth-first walk in ascending file-name order within every directory
/// (files and sub-directories interleaved by name), so the `cap` subset
/// and every downstream choice are the same on every machine — `read_dir`
/// order is platform-defined.
fn collect_source_files(root: &Path, cap: usize) -> std::io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    // Pending paths; children are pushed in reverse name order so pops
    // come out ascending.
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if out.len() >= cap {
            break;
        }
        if p.is_dir() {
            if should_skip_dir(&p) {
                continue;
            }
            let mut children: Vec<PathBuf> = match fs::read_dir(&p) {
                Ok(e) => e.flatten().map(|entry| entry.path()).collect(),
                Err(_) => continue,
            };
            children.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            stack.extend(children.into_iter().rev());
        } else if p.is_file() && has_source_ext(&p) {
            out.push(p);
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
    for pat in [
        " if ", "\tif ", "}else", " else ", " match ", " for ", " while ", "case ",
    ] {
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
            .query_row(
                "SELECT COUNT(*) FROM mobs WHERE defeated_at IS NULL",
                [],
                |r| r.get(0),
            )
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
    fn persist_scan_writes_origin_path() {
        let conn = fresh_conn();
        let specs =
            vec![MobSpec::new("rust", MobKind::Boss, "foo".to_string())
                .with_origin_path("src/foo.rs")];
        persist_scan(&conn, &specs).unwrap();
        let path: Option<String> = conn
            .query_row("SELECT origin_path FROM mobs LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(path.as_deref(), Some("src/foo.rs"));
    }

    #[test]
    fn persist_scan_leaves_origin_path_null_when_unset() {
        let conn = fresh_conn();
        let specs = vec![MobSpec::new("rust", MobKind::Void, "untested".to_string())];
        persist_scan(&conn, &specs).unwrap();
        let path: Option<String> = conn
            .query_row("SELECT origin_path FROM mobs LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert!(path.is_none());
    }

    #[test]
    fn scan_dir_populates_origin_path_as_relative_forward_slash() {
        // Build a temp tree with a single .rs file containing TODOs → Zombie.
        let tmp = tempfile::tempdir().unwrap();
        let subdir = tmp.path().join("src");
        fs::create_dir(&subdir).unwrap();
        let file = subdir.join("x.rs");
        let todos = (0..10)
            .map(|i| format!("// TODO: item {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&file, todos).unwrap();

        let specs = scan_dir(tmp.path(), "rust").unwrap();
        assert!(!specs.is_empty(), "fixture should spawn at least one mob");
        let p = specs[0].origin_path.as_deref().expect("origin_path set");
        // Forward-slash normalized relative path
        assert_eq!(p, "src/x.rs");
        // Never absolute
        assert!(!p.starts_with('/'), "origin_path must be relative, got {p}");
    }

    #[test]
    fn persist_scan_respawns_after_defeat() {
        let conn = fresh_conn();
        let specs = vec![MobSpec::new("rust", MobKind::Ghost, "src/a.rs".to_string())];
        persist_scan(&conn, &specs).unwrap();
        conn.execute("UPDATE mobs SET defeated_at = 9999", [])
            .unwrap();
        // Partial index excludes defeated rows — new spawn allowed
        let n = persist_scan(&conn, &specs).unwrap();
        assert_eq!(n, 1);
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM mobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 2);
    }

    // ─── Phase 3e: Ghost Repellent integration ────────────────────────

    #[test]
    fn rate_limited_scan_drops_ghosts_when_repellent_active() {
        // Seeds an active Ghost Repellent effect for zone rust, scans a
        // directory with exactly one ghost-producing file, and confirms
        // the persist step skipped it. Uses `rate_limited_scan_with_dir`
        // to inject the tempdir directly so no env-var mutation leaks
        // under parallel `cargo test` runs.
        let conn = fresh_conn();
        let tmp = tempdir().unwrap();
        write(
            &tmp.path().join("src/ghost.rs"),
            "use std::collections::HashMap;\n\nfn main() {}\n",
        );

        // Apply Ghost Repellent scoped to rust with expires_at far
        // enough into the future that `unix_now()` reads it as active.
        // i64::MAX avoids any "wall clock drifted past my test constant"
        // flake class.
        conn.execute(
            "INSERT INTO active_effects
                 (effect_kind, zone_id, applied_at, expires_at, source_item)
             VALUES ('suppress_ghost_spawn', 'rust', 0, ?1, 'Ghost Repellent')",
            rusqlite::params![i64::MAX],
        )
        .unwrap();

        rate_limited_scan_with_dir(&conn, "rust", 1, Some(tmp.path())).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mobs WHERE kind = 'ghost' AND defeated_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "Ghost Repellent must suppress new ghost spawns");
    }

    #[test]
    fn rate_limited_scan_admits_ghosts_when_repellent_expired() {
        // Same setup as above but the effect row has already expired.
        let conn = fresh_conn();
        let tmp = tempdir().unwrap();
        write(
            &tmp.path().join("src/ghost.rs"),
            "use std::collections::HashMap;\n\nfn main() {}\n",
        );

        // expires_at=1 → very long ago by the time unix_now() fires.
        conn.execute(
            "INSERT INTO active_effects
                 (effect_kind, zone_id, applied_at, expires_at, source_item)
             VALUES ('suppress_ghost_spawn', 'rust', 0, 1, 'Ghost Repellent')",
            [],
        )
        .unwrap();

        rate_limited_scan_with_dir(&conn, "rust", 1, Some(tmp.path())).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mobs WHERE kind = 'ghost' AND defeated_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(n >= 1, "expired repellent must no longer suppress spawns");
    }

    // ─── Doppelganger: duplicate code blocks (B10) ─────────────

    /// `n` distinct, non-trivial lines (each ~60 bytes) tagged by `tag`
    /// so two calls with the same tag produce an identical block.
    fn dup_block(n: usize, tag: &str) -> String {
        let mut s = String::new();
        for i in 0..n {
            s.push_str(&format!(
                "    let value_{i}_{tag} = compute_thing(alpha_{i}, beta_{i}) + offset_{tag};\n"
            ));
        }
        s
    }

    fn wrap_fn(name: &str, body: &str) -> String {
        format!("fn {name}() {{\n{body}}}\n")
    }

    fn dop_names(specs: &[MobSpec]) -> Vec<String> {
        specs
            .iter()
            .filter(|s| s.kind == MobKind::Doppelganger)
            .map(|s| s.name.clone())
            .collect()
    }

    #[test]
    fn dup_two_files_same_block_spawns_one_doppelganger() {
        let dir = tempdir().unwrap();
        write(
            &dir.path().join("src/a.rs"),
            &wrap_fn("one", &dup_block(10, "x")),
        );
        write(
            &dir.path().join("src/b.rs"),
            &wrap_fn("two", &dup_block(10, "x")),
        );
        let specs = scan_dir(dir.path(), "rust").unwrap();
        let dops: Vec<&MobSpec> = specs
            .iter()
            .filter(|s| s.kind == MobKind::Doppelganger)
            .collect();
        assert_eq!(dops.len(), 1, "got: {specs:?}");
        assert_eq!(dops[0].name, "dup @ src/a.rs ↔ src/b.rs");
        assert_eq!(dops[0].origin_path.as_deref(), Some("src/a.rs"));
    }

    #[test]
    fn dup_changed_copy_spawns_none() {
        let dir = tempdir().unwrap();
        write(
            &dir.path().join("src/a.rs"),
            &wrap_fn("one", &dup_block(10, "x")),
        );
        write(
            &dir.path().join("src/b.rs"),
            &wrap_fn("two", &dup_block(10, "y")),
        );
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(dop_names(&specs).is_empty(), "got: {specs:?}");
    }

    #[test]
    fn dup_trivial_lines_spawn_none() {
        let dir = tempdir().unwrap();
        let mut trivial = String::new();
        for i in 0..12 {
            trivial.push_str(&format!("use crate::module_{i}::SomeLongTypeName{i};\n"));
            trivial.push_str("#[derive(Debug, Clone, PartialEq)]\n");
            trivial.push_str("\n}\n} else {\n});\n");
            trivial.push_str(&format!("// a fairly long comment line number {i} here\n"));
        }
        write(&dir.path().join("src/a.rs"), &trivial);
        write(&dir.path().join("src/b.rs"), &trivial);
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(dop_names(&specs).is_empty(), "got: {specs:?}");
    }

    #[test]
    fn dup_block_comment_body_spawns_none() {
        let dir = tempdir().unwrap();
        let mut c = String::from("/*\n");
        for i in 0..12 {
            c.push_str(&format!(
                "   Prose line {i} describing the module behaviour at length.\n"
            ));
        }
        c.push_str("*/\nfn main() {}\n");
        write(&dir.path().join("src/a.rs"), &c);
        write(&dir.path().join("src/b.rs"), &c);
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(dop_names(&specs).is_empty(), "got: {specs:?}");
    }

    #[test]
    fn dup_block_comment_opened_after_code_spawns_none() {
        // Review g1 (both seats): `/*` mid-line must start comment state.
        let dir = tempdir().unwrap();
        let mut body = String::new();
        for i in 0..12 {
            body.push_str(&format!(
                "   Prose line {i} describing the module behaviour at length.\n"
            ));
        }
        body.push_str("*/\nfn main() {}\n");
        write(
            &dir.path().join("src/a.rs"),
            &format!("let opener_a = prepare(ctx); /*\n{body}"),
        );
        write(
            &dir.path().join("src/b.rs"),
            &format!("let opener_b = setup(other); /*\n{body}"),
        );
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(dop_names(&specs).is_empty(), "got: {specs:?}");
    }

    #[test]
    fn dup_multiline_grouped_use_spawns_none() {
        let dir = tempdir().unwrap();
        let mut u = String::from("use crate::{\n");
        for i in 0..12 {
            u.push_str(&format!(
                "    some_module_{i}::{{FirstLongName{i}, SecondLongName{i}}},\n"
            ));
        }
        u.push_str("};\nfn main() {}\n");
        write(&dir.path().join("src/a.rs"), &u);
        write(&dir.path().join("src/b.rs"), &u);
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(dop_names(&specs).is_empty(), "got: {specs:?}");
    }

    #[test]
    fn dup_pub_use_lines_and_groups_spawn_none() {
        let dir = tempdir().unwrap();
        let mut single = String::new();
        for i in 0..12 {
            single.push_str(&format!(
                "pub use crate::some_module_{i}::LongReexportedName{i};\n"
            ));
        }
        write(&dir.path().join("src/a.rs"), &single);
        write(&dir.path().join("src/b.rs"), &single);
        let mut group = String::from("pub(crate) use crate::{\n");
        for i in 0..12 {
            group.push_str(&format!(
                "    other_module_{i}::{{FirstLongName{i}, SecondLongName{i}}},\n"
            ));
        }
        group.push_str("};\n");
        write(&dir.path().join("src/c.rs"), &group);
        write(&dir.path().join("src/d.rs"), &group);
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(dop_names(&specs).is_empty(), "got: {specs:?}");
    }

    #[test]
    fn dup_name_stable_when_block_grows() {
        let dir = tempdir().unwrap();
        write(
            &dir.path().join("src/a.rs"),
            &wrap_fn("one", &dup_block(10, "x")),
        );
        write(
            &dir.path().join("src/b.rs"),
            &wrap_fn("two", &dup_block(10, "x")),
        );
        let before = dop_names(&scan_dir(dir.path(), "rust").unwrap());
        let conn = fresh_conn();
        let first = persist_scan(&conn, &scan_dir(dir.path(), "rust").unwrap()).unwrap();
        assert!(first >= 1);
        write(
            &dir.path().join("src/a.rs"),
            &wrap_fn("one", &dup_block(14, "x")),
        );
        write(
            &dir.path().join("src/b.rs"),
            &wrap_fn("two", &dup_block(14, "x")),
        );
        let after_specs = scan_dir(dir.path(), "rust").unwrap();
        assert_eq!(before, dop_names(&after_specs));
        assert_eq!(persist_scan(&conn, &after_specs).unwrap(), 0);
    }

    /// Writes `files` (rel path, content) into a fresh tempdir in the given
    /// order and returns the scan result.
    fn scan_fixture(files: &[(String, String)]) -> Vec<MobSpec> {
        let dir = tempdir().unwrap();
        for (rel, content) in files {
            write(&dir.path().join(rel), content);
        }
        scan_dir(dir.path(), "rust").unwrap()
    }

    fn summary(specs: &[MobSpec]) -> Vec<(String, Option<String>)> {
        specs
            .iter()
            .map(|s| (s.name.clone(), s.origin_path.clone()))
            .collect()
    }

    #[test]
    fn dup_result_independent_of_creation_order() {
        let mut files: Vec<(String, String)> = Vec::new();
        for i in 0..6 {
            let tag = if i % 2 == 0 { "even" } else { "odd" };
            files.push((
                format!("src/m{i}/f{i}.rs"),
                wrap_fn("f", &dup_block(10, tag)),
            ));
        }
        let fwd = scan_fixture(&files);
        files.reverse();
        let rev = scan_fixture(&files);
        assert_eq!(summary(&fwd), summary(&rev));
        assert!(!dop_names(&fwd).is_empty());
    }

    #[test]
    fn dup_many_files_still_spawns_bounded_and_deterministic() {
        let mut files: Vec<(String, String)> = Vec::new();
        for i in 0..12 {
            files.push((format!("src/f{i:02}.rs"), wrap_fn("f", &dup_block(10, "x"))));
        }
        let fwd = scan_fixture(&files);
        files.reverse();
        let rev = scan_fixture(&files);
        let n = dop_names(&fwd).len();
        assert!((1..=MAX_DOPPELGANGERS_PER_SCAN).contains(&n), "got {n}");
        assert_eq!(summary(&fwd), summary(&rev));
        // Only the earliest files can win slots.
        assert!(dop_names(&fwd).iter().all(|nm| nm.contains("src/f0")));
    }

    #[test]
    fn dup_found_even_after_ordinary_slots_fill() {
        let mut files: Vec<(String, String)> = Vec::new();
        for i in 0..20 {
            files.push((
                format!("src/a_todo_{i:02}.rs"),
                "// TODO: a\n// TODO: b\n// TODO: c\n// TODO: d\n// TODO: e\nfn main() {}\n".into(),
            ));
        }
        files.push(("src/z1.rs".into(), wrap_fn("one", &dup_block(10, "x"))));
        files.push(("src/z2.rs".into(), wrap_fn("two", &dup_block(10, "x"))));
        let specs = scan_fixture(&files);
        assert_eq!(dop_names(&specs).len(), 1, "got: {specs:?}");
        assert_eq!(specs.len(), MAX_MOBS_PER_SCAN);
    }

    #[test]
    fn dup_same_file_repeat_spawns_none() {
        let dir = tempdir().unwrap();
        let body = format!(
            "{}{}",
            wrap_fn("one", &dup_block(10, "x")),
            wrap_fn("two", &dup_block(10, "x"))
        );
        write(&dir.path().join("src/a.rs"), &body);
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(dop_names(&specs).is_empty(), "got: {specs:?}");
    }

    #[test]
    fn dup_generated_file_ignored() {
        let dir = tempdir().unwrap();
        let generated = format!(
            "// @generated by a tool\n{}",
            wrap_fn("one", &dup_block(10, "x"))
        );
        write(&dir.path().join("src/a.rs"), &generated);
        write(
            &dir.path().join("src/b.rs"),
            &wrap_fn("two", &dup_block(10, "x")),
        );
        let specs = scan_dir(dir.path(), "rust").unwrap();
        assert!(dop_names(&specs).is_empty(), "got: {specs:?}");
    }

    #[test]
    fn dup_file_cap_applies_to_sorted_walk() {
        // Single directory: in-directory name order == global lexicographic.
        let filler = "fn filler() {}\n".to_string();
        let mut late: Vec<(String, String)> = (0..999)
            .map(|i| (format!("f{i:04}.rs"), filler.clone()))
            .collect();
        late.push(("zz1.rs".into(), wrap_fn("one", &dup_block(10, "x"))));
        late.push(("zz2.rs".into(), wrap_fn("two", &dup_block(10, "x"))));
        let fwd = scan_fixture(&late);
        late.reverse();
        let rev = scan_fixture(&late);
        assert_eq!(summary(&fwd), summary(&rev));
        assert!(
            dop_names(&fwd).is_empty(),
            "zz2.rs is file 1001 and must be cut"
        );

        let mut early: Vec<(String, String)> = (0..999)
            .map(|i| (format!("f{i:04}.rs"), filler.clone()))
            .collect();
        early.push(("a1.rs".into(), wrap_fn("one", &dup_block(10, "x"))));
        early.push(("a2.rs".into(), wrap_fn("two", &dup_block(10, "x"))));
        let fwd = scan_fixture(&early);
        early.reverse();
        let rev = scan_fixture(&early);
        assert_eq!(summary(&fwd), summary(&rev));
        assert_eq!(dop_names(&fwd), vec!["dup @ a1.rs ↔ a2.rs".to_string()]);
    }

    #[test]
    fn dup_no_duplicates_keeps_full_ordinary_quota() {
        let mut files: Vec<(String, String)> = Vec::new();
        for i in 0..30 {
            files.push((
                format!("src/todo_{i:02}.rs"),
                "// TODO: a\n// TODO: b\n// TODO: c\n// TODO: d\n// TODO: e\nfn main() {}\n".into(),
            ));
        }
        let specs = scan_fixture(&files);
        assert_eq!(specs.len(), MAX_MOBS_PER_SCAN);
        assert!(dop_names(&specs).is_empty());
    }

    #[test]
    fn dup_quota_total_stays_full_for_zero_to_three_pairs() {
        for pairs in 0..=3usize {
            let mut files: Vec<(String, String)> = Vec::new();
            for i in 0..30 {
                files.push((
                    format!("src/todo_{i:02}.rs"),
                    "// TODO: a\n// TODO: b\n// TODO: c\n// TODO: d\n// TODO: e\nfn main() {}\n"
                        .into(),
                ));
            }
            for p in 0..pairs {
                let tag = format!("pair{p}");
                files.push((
                    format!("src/dup{p}_a.rs"),
                    wrap_fn("one", &dup_block(10, &tag)),
                ));
                files.push((
                    format!("src/dup{p}_b.rs"),
                    wrap_fn("two", &dup_block(10, &tag)),
                ));
            }
            let specs = scan_fixture(&files);
            assert_eq!(specs.len(), MAX_MOBS_PER_SCAN, "pairs={pairs}");
            assert_eq!(dop_names(&specs).len(), pairs, "pairs={pairs}");
        }
    }

    #[test]
    fn dup_one_file_repeating_block_does_not_starve_other_file() {
        let mut a = String::new();
        for k in 0..8 {
            a.push_str(&wrap_fn(&format!("copy{k}"), &dup_block(10, "x")));
        }
        let files = vec![
            ("src/a.rs".to_string(), a),
            ("src/b.rs".to_string(), wrap_fn("one", &dup_block(10, "x"))),
        ];
        let specs = scan_fixture(&files);
        assert_eq!(
            dop_names(&specs),
            vec!["dup @ src/a.rs ↔ src/b.rs".to_string()]
        );
    }

    #[test]
    fn dup_name_uses_byte_lexicographic_paths() {
        let files = vec![
            ("a/b.rs".to_string(), wrap_fn("one", &dup_block(10, "x"))),
            ("a.rs".to_string(), wrap_fn("two", &dup_block(10, "x"))),
        ];
        let specs = scan_fixture(&files);
        let dop = specs
            .iter()
            .find(|s| s.kind == MobKind::Doppelganger)
            .expect("doppelganger");
        assert_eq!(dop.name, "dup @ a.rs ↔ a/b.rs");
        assert_eq!(dop.origin_path.as_deref(), Some("a.rs"));
    }
}
