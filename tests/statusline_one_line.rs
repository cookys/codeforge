//! Golden (PM 01M7MEY1, operator 2026-11-17): `codeforge statusline` prints exactly one
//! line, at any width and in any state, and never the pet. It runs the compiled binary
//! with an adopted pet in a sandboxed HOME / data dir / live dir, never the real ones.
//! Before this test, an empty stdin or a missing forge-stats heartbeat drew the 5-line
//! RPG box (pet sprite, ATK/DEF); bringing any multi-line form back turns this red.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

struct Sandbox {
    _tmp: tempfile::TempDir,
    home: std::path::PathBuf,
    proj: std::path::PathBuf,
    data: std::path::PathBuf,
    live: std::path::PathBuf,
}

fn sandbox() -> Sandbox {
    let tmp = tempfile::TempDir::new().unwrap();
    let p = tmp.path();
    let s = Sandbox {
        home: p.join("home"),
        proj: p.join("proj"),
        data: p.join("data"),
        live: p.join("live"),
        _tmp: tmp,
    };
    for d in [&s.home, &s.proj, &s.data, &s.live] {
        std::fs::create_dir_all(d).unwrap();
    }
    s
}

fn codeforge(s: &Sandbox) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_codeforge"));
    c.current_dir(&s.proj)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("CODEFORGE_DIR")
        .env_remove("CODEFORGE_WIDTH")
        .env_remove("CODEFORGE_ACCOUNT_ALIAS")
        .env("HOME", &s.home)
        .env("CODEFORGE_DATA_DIR", &s.data)
        .env("AUTOPILOT_LIVE_DIR", &s.live)
        .env("CODEFORGE_USAGE_AUTOCOLLECT", "0")
        .env("CODEFORGE_LOCALE", "zh-TW");
    c
}

fn run_with_stdin(mut c: Command, stdin: &str) -> String {
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn adopt(s: &Sandbox) {
    let mut init = codeforge(s);
    init.arg("init");
    run_with_stdin(init, "");
    let mut adopt = codeforge(s);
    adopt.arg("adopt");
    run_with_stdin(adopt, "1\n1\nFerris\ny\n");
    let mut pet = codeforge(s);
    pet.arg("pet");
    let shown = run_with_stdin(pet, "");
    assert!(shown.contains("Lv"), "adopt did not leave a pet:\n{shown}");
}

fn write_heartbeat(live: &Path, sid: &str, layout: &str) {
    let dir = live.join("codeforge").join("mod-alive");
    std::fs::create_dir_all(&dir).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let body = serde_json::json!({
        "schema": "codeforge.mod-alive/1", "session_id": sid, "written_at_ms": now,
        "layout": layout, "summary_row": true, "band_seen": false,
    });
    std::fs::write(dir.join(format!("{sid}.json")), body.to_string()).unwrap();
}

fn assert_one_line(out: &str, case: &str) {
    assert_eq!(
        out.matches('\n').count(),
        1,
        "{case}: not exactly one line:\n{out}"
    );
    assert!(out.ends_with('\n'), "{case}: {out:?}");
    for pet in ["Ferris", "Lv.", "ATK", "DEF", "╭", "╰", "│"] {
        assert!(
            !out.contains(pet),
            "{case}: pet/box text {pet:?} in:\n{out}"
        );
    }
}

#[test]
fn statusline_is_exactly_one_line_without_the_pet_in_every_state() {
    let s = sandbox();
    adopt(&s);
    let sid = "6f0c1a52-25cb-47ca-821c-cec391832eed";
    let full = serde_json::json!({
        "session_id": sid,
        "model": {"display_name": "Opus 5.5"},
        "cwd": "/tmp/一個很長很長的中文專案名稱/子目錄/再一層",
        "context_window": {"used_percentage": 42.0, "context_window_size": 1_000_000},
        "rate_limits": {
            "five_hour": {"used_percentage": 81.0, "resets_at": 4_102_444_800i64},
            "seven_day": {"used_percentage": 99.0, "resets_at": 4_102_444_800i64},
        },
        "message": "a speech bubble the pet used to say",
    })
    .to_string();
    let stdins = [
        ("empty stdin", String::new()),
        ("bad json", "{not json".to_string()),
        (
            "no session id",
            r#"{"model":{"display_name":"Opus 5.5"}}"#.to_string(),
        ),
        ("full", full),
    ];
    // States of the forge-stats heartbeat that used to switch the form.
    for hb in ["none", "band", "pane"] {
        if hb != "none" {
            write_heartbeat(&s.live, sid, hb);
        }
        for (name, stdin) in &stdins {
            for cols in [
                "", "0", "1", "10", "24", "40", "60", "80", "100", "120", "160", "300",
            ] {
                let mut c = codeforge(&s);
                c.arg("statusline").env("NO_COLOR", "1");
                if cols.is_empty() {
                    c.env_remove("COLUMNS");
                } else {
                    c.env("COLUMNS", cols);
                }
                let out = run_with_stdin(c, stdin);
                let case = format!("heartbeat={hb} stdin={name} COLUMNS={cols:?}");
                assert_one_line(&out, &case);
                if let Ok(n) = cols.parse::<usize>() {
                    let w = unicode_width::UnicodeWidthStr::width(out.trim_end_matches('\n'));
                    assert!(w <= n.saturating_sub(4), "{case}: {w} columns:\n{out}");
                }
            }
        }
    }
}

fn settings_get(s: &Sandbox, key: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(s.data.join("state.db")).unwrap();
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .ok()
}

/// The statusline is the "a session was open" signal only: it bumps
/// `last_player_seen_at` and must leave `last_interaction_at` (human use) alone.
#[test]
fn statusline_bumps_player_seen_but_not_last_interaction() {
    let s = sandbox();
    adopt(&s);
    let conn = rusqlite::Connection::open(s.data.join("state.db")).unwrap();
    for (k, v) in [
        ("last_player_seen_at", "100"),
        ("last_interaction_at", "200"),
    ] {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [k, v],
        )
        .unwrap();
    }
    drop(conn);
    let mut c = codeforge(&s);
    c.arg("statusline");
    run_with_stdin(c, "{}");
    let seen: i64 = settings_get(&s, "last_player_seen_at")
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        seen > 1_000_000,
        "statusline must bump last_player_seen_at, got {seen}"
    );
    assert_eq!(
        settings_get(&s, "last_interaction_at").as_deref(),
        Some("200")
    );
}
