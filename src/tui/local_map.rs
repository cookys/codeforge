//! Local Map data provider — Phase 2c.
//!
//! Reads the `mobs` table and groups rows by the **top-level directory** of
//! their `origin_path`, producing a compact summary suitable for the Local
//! Map panel. Pure function of DB + scan root path.
//!
//! Why top-level only: the Local Map shows the repo at a glance (`src/`,
//! `doc/`, `target/` …). A nested path like `src/cli/learn.rs` rolls up into
//! the `src` row. Deeper navigation is Phase 3a.
//!
//! Legacy MOBs (Phase 2b rows with NULL `origin_path`) roll into a
//! synthetic `"(unknown)"` group — visible to the user so they know those
//! mobs exist, but clearly marked.

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

/// A single row in the Local Map panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomSummary {
    /// Top-level directory name (e.g. "src", "doc"). `"(unknown)"` for mobs
    /// with NULL origin_path (Phase 2b legacy data).
    pub directory: String,
    /// Count of alive mobs grouped under this directory.
    pub alive: u32,
    /// Count of defeated mobs under this directory. Useful for "(N ✓)"
    /// display — shows the user progress without a separate history view.
    pub defeated: u32,
    /// True iff the user's current working directory lives under this group.
    /// Renderer uses this to prepend `▶`.
    pub is_current: bool,
}

/// Compute the Local Map for a given scan root. `cwd` lets the provider flag
/// the row matching the user's current directory; pass `None` to skip that.
pub fn compute(
    conn: &Connection,
    scan_root: &Path,
    cwd: Option<&Path>,
) -> Result<Vec<RoomSummary>> {
    let current_top = cwd.and_then(|c| top_segment_under(c, scan_root));
    let rows = query_grouped(conn)?;

    let mut out: Vec<RoomSummary> = rows
        .into_iter()
        .map(|(dir, alive, defeated)| {
            let is_current = current_top.as_deref() == Some(dir.as_str());
            RoomSummary {
                directory: dir,
                alive,
                defeated,
                is_current,
            }
        })
        .collect();

    // Stable alphabetical by directory; `(unknown)` floats to the bottom
    // so it doesn't take the top slot when mixed with real dirs.
    out.sort_by(|a, b| {
        match (a.directory.as_str(), b.directory.as_str()) {
            ("(unknown)", "(unknown)") => std::cmp::Ordering::Equal,
            ("(unknown)", _) => std::cmp::Ordering::Greater,
            (_, "(unknown)") => std::cmp::Ordering::Less,
            (x, y) => x.cmp(y),
        }
    });
    Ok(out)
}

/// Raw DB query: group by first path segment of origin_path, count
/// alive/defeated. NULL origin_path folds into `"(unknown)"`.
fn query_grouped(conn: &Connection) -> Result<Vec<(String, u32, u32)>> {
    let mut stmt = conn.prepare(
        "SELECT
             COALESCE(
                 CASE
                     WHEN instr(origin_path, '/') > 0
                         THEN substr(origin_path, 1, instr(origin_path, '/') - 1)
                     ELSE origin_path
                 END,
                 '(unknown)'
             ) AS top,
             SUM(CASE WHEN defeated_at IS NULL THEN 1 ELSE 0 END) AS alive,
             SUM(CASE WHEN defeated_at IS NOT NULL THEN 1 ELSE 0 END) AS defeated
         FROM mobs
         GROUP BY top",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? as u32,
                r.get::<_, i64>(2)? as u32,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Returns the top-level segment of `cwd` relative to `scan_root`, or None
/// if `cwd` is not under `scan_root`. e.g.
/// `top_segment_under("/repo/src/cli", "/repo") -> Some("src")`.
fn top_segment_under(cwd: &Path, scan_root: &Path) -> Option<String> {
    let rel = cwd.strip_prefix(scan_root).ok()?;
    let first = rel.components().next()?;
    let s = first.as_os_str().to_string_lossy().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    fn insert_mob(
        conn: &Connection,
        zone: &str,
        kind: &str,
        name: &str,
        origin_path: Option<&str>,
        defeated: bool,
    ) {
        conn.execute(
            "INSERT INTO mobs
                (zone_id, kind, name, hp, hp_max, atk, def, spawned_at,
                 defeated_at, origin_path)
             VALUES (?1, ?2, ?3, 10, 10, 1, 1, 100,
                     CASE WHEN ?4 = 1 THEN 200 ELSE NULL END, ?5)",
            rusqlite::params![zone, kind, name, defeated as i64, origin_path],
        )
        .unwrap();
    }

    #[test]
    fn empty_db_returns_empty_map() {
        let conn = fresh();
        let rooms = compute(&conn, Path::new("/repo"), None).unwrap();
        assert!(rooms.is_empty());
    }

    #[test]
    fn groups_by_top_level_directory() {
        let conn = fresh();
        insert_mob(&conn, "rust", "zombie", "a", Some("src/cli/foo.rs"), false);
        insert_mob(&conn, "rust", "ghost", "b", Some("src/daemon/bar.rs"), false);
        insert_mob(&conn, "rust", "boss", "c", Some("doc/x.md"), false);

        let rooms = compute(&conn, Path::new("/repo"), None).unwrap();
        assert_eq!(rooms.len(), 2);
        let src = rooms.iter().find(|r| r.directory == "src").unwrap();
        assert_eq!(src.alive, 2);
        assert_eq!(src.defeated, 0);
        let doc = rooms.iter().find(|r| r.directory == "doc").unwrap();
        assert_eq!(doc.alive, 1);
    }

    #[test]
    fn counts_alive_and_defeated_separately() {
        let conn = fresh();
        insert_mob(&conn, "rust", "zombie", "a", Some("src/a.rs"), false);
        insert_mob(&conn, "rust", "zombie", "b", Some("src/b.rs"), true);
        insert_mob(&conn, "rust", "zombie", "c", Some("src/c.rs"), true);

        let rooms = compute(&conn, Path::new("/repo"), None).unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].directory, "src");
        assert_eq!(rooms[0].alive, 1);
        assert_eq!(rooms[0].defeated, 2);
    }

    #[test]
    fn null_origin_path_folds_into_unknown_group() {
        let conn = fresh();
        insert_mob(&conn, "rust", "ghost", "a", None, false);
        insert_mob(&conn, "rust", "ghost", "b", None, false);
        insert_mob(&conn, "rust", "zombie", "c", Some("src/x.rs"), false);

        let rooms = compute(&conn, Path::new("/repo"), None).unwrap();
        let unknown = rooms.iter().find(|r| r.directory == "(unknown)").unwrap();
        assert_eq!(unknown.alive, 2);
        let src = rooms.iter().find(|r| r.directory == "src").unwrap();
        assert_eq!(src.alive, 1);
    }

    #[test]
    fn unknown_group_sorts_last() {
        let conn = fresh();
        insert_mob(&conn, "rust", "ghost", "a", None, false);
        insert_mob(&conn, "rust", "zombie", "b", Some("zzz/x.rs"), false);
        insert_mob(&conn, "rust", "zombie", "c", Some("aaa/x.rs"), false);

        let rooms = compute(&conn, Path::new("/repo"), None).unwrap();
        assert_eq!(rooms.len(), 3);
        assert_eq!(rooms[0].directory, "aaa");
        assert_eq!(rooms[1].directory, "zzz");
        assert_eq!(rooms[2].directory, "(unknown)");
    }

    #[test]
    fn marks_current_directory_from_cwd_under_scan_root() {
        let conn = fresh();
        insert_mob(&conn, "rust", "zombie", "a", Some("src/cli/foo.rs"), false);
        insert_mob(&conn, "rust", "zombie", "b", Some("doc/x.md"), false);

        let rooms =
            compute(&conn, Path::new("/repo"), Some(Path::new("/repo/src/cli"))).unwrap();
        let src = rooms.iter().find(|r| r.directory == "src").unwrap();
        assert!(src.is_current);
        let doc = rooms.iter().find(|r| r.directory == "doc").unwrap();
        assert!(!doc.is_current);
    }

    #[test]
    fn cwd_outside_scan_root_marks_nothing() {
        let conn = fresh();
        insert_mob(&conn, "rust", "zombie", "a", Some("src/foo.rs"), false);
        let rooms = compute(
            &conn,
            Path::new("/repo"),
            Some(Path::new("/unrelated/path")),
        )
        .unwrap();
        assert!(rooms.iter().all(|r| !r.is_current));
    }

    #[test]
    fn cwd_equals_scan_root_marks_nothing() {
        // At the repo root, no top-level dir applies. Don't mark any row.
        let conn = fresh();
        insert_mob(&conn, "rust", "zombie", "a", Some("src/foo.rs"), false);
        let rooms = compute(&conn, Path::new("/repo"), Some(Path::new("/repo"))).unwrap();
        assert!(rooms.iter().all(|r| !r.is_current));
    }

    #[test]
    fn origin_path_without_slash_groups_under_itself() {
        // Top-level files (e.g. "Cargo.toml") map to their own group
        let conn = fresh();
        insert_mob(&conn, "rust", "ghost", "a", Some("Cargo.toml"), false);
        let rooms = compute(&conn, Path::new("/repo"), None).unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].directory, "Cargo.toml");
    }
}
