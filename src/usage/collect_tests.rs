//! Tests for the G36 collector (plan phase 2). Each guards a rule a negative control flips.

use super::*;
use crate::usage::{price::Tokens, quota::Snapshot, store};
use serde_json::json;
use std::io::Write;

/// 2026-11-03T12:00:00Z
const NOW: i64 = 1_793_707_200_000;
const TS: &str = "2026-11-03T10:00:00.000Z";

struct Env {
    _tmp: tempfile::TempDir,
    cfg: PathBuf,
    codex: PathBuf,
    data: PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("cfg");
    let codex = tmp.path().join("codex");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(cfg.join("projects")).unwrap();
    std::fs::create_dir_all(codex.join("sessions")).unwrap();
    Env {
        cfg,
        codex,
        data,
        _tmp: tmp,
    }
}

fn claude(rid: &str, input: u64, output: u64, extra: Value) -> String {
    let mut v = json!({
        "type": "assistant", "requestId": rid, "timestamp": TS, "cwd": "/srv/work/repo",
        "message": {"model": "claude-opus-5-5", "content": [{"type": "text", "text": "hello"}],
                    "usage": {"input_tokens": input, "output_tokens": output, "cache_read_input_tokens": 0,
                              "cache_creation_input_tokens": 0,
                              "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 0}}}
    });
    merge(&mut v, extra);
    v.to_string() + "\n"
}

fn merge(a: &mut Value, b: Value) {
    if let (Value::Object(a), Value::Object(b)) = (a, b) {
        for (k, v) in b {
            match a.get_mut(&k) {
                Some(x) if x.is_object() && v.is_object() => merge(x, v),
                _ => {
                    a.insert(k, v);
                }
            }
        }
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn append(path: &Path, text: &str) {
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

fn run(e: &Env, snaps: &[Snapshot]) -> Connection {
    let (mut conn, _) = store::open(&store::usage_dir(&e.data)).unwrap();
    let rs = roots(std::slice::from_ref(&e.cfg), Some(&e.codex));
    collect(&mut conn, &rs, snaps, NOW).unwrap();
    conn
}

fn totals(conn: &Connection) -> (Tokens, u64) {
    let rows = rows_since(conn, "2000-01-01").unwrap();
    let mut t = Tokens::default();
    let mut req = 0;
    for r in rows {
        t.input += r.tokens.input;
        t.output += r.tokens.output;
        t.cache_write_5m += r.tokens.cache_write_5m;
        t.cache_write_1h += r.tokens.cache_write_1h;
        t.cache_read += r.tokens.cache_read;
        req += r.requests;
    }
    (t, req)
}

#[test]
fn nothing_textual_or_path_like_is_persisted() {
    let e = env();
    let slug = e.cfg.join("projects/-MARKERSLUG-home-repo");
    let line = claude(
        "req_a1",
        10,
        20,
        json!({
            "cwd": "/MARKERPARENT/secretdir/repo",
            "message": {"content": [{"type": "text", "text": "MARKERTEXT marker@example.com"},
                                    {"type": "tool_use", "input": {"command": "MARKERTOOLIN"}}]},
            "toolUseResult": {"stdout": "MARKERTOOLOUT", "file": {"content": "MARKERFILE"}},
            "transcript_path": "/MARKERTP/x.jsonl"
        }),
    );
    write(&slug.join("s1.jsonl"), &line);
    let conn = run(&e, &[]);
    conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")
        .unwrap();
    let dir = store::usage_dir(&e.data);
    let mut bytes = Vec::new();
    for f in ["usage.db", "usage.db-wal"] {
        if let Ok(b) = std::fs::read(dir.join(f)) {
            bytes.extend(b);
        }
    }
    let hay = String::from_utf8_lossy(&bytes);
    for m in [
        "MARKER",
        "marker@example.com",
        "secretdir",
        "/srv",
        e.cfg.to_str().unwrap(),
    ] {
        assert!(!hay.contains(m), "{m} leaked into usage.db");
    }
    // what is kept: counts, the model id, the cwd basename
    let rows = rows_since(&conn, "2000-01-01").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].project.as_str(), rows[0].model.as_str()),
        ("repo", "claude-opus-5-5")
    );
}

#[test]
fn one_request_counts_once_by_per_counter_max() {
    let e = env();
    let f = e.cfg.join("projects/p/s1.jsonl");
    // three lines of one response, identical usage
    write(
        &f,
        &(claude("req_1", 100, 5, json!({})).repeat(3) + &claude("req_2", 7, 1, json!({}))),
    );
    let conn = run(&e, &[]);
    assert_eq!(
        totals(&conn),
        (
            Tokens {
                input: 107,
                output: 6,
                ..Default::default()
            },
            2
        )
    );
    // a later line of req_1 with more output adds only the increase
    append(&f, &claude("req_1", 100, 9, json!({})));
    let conn = run(&e, &[]);
    assert_eq!(
        totals(&conn),
        (
            Tokens {
                input: 107,
                output: 10,
                ..Default::default()
            },
            2
        )
    );
    // collecting again changes nothing
    let conn2 = run(&e, &[]);
    assert_eq!(totals(&conn2), totals(&conn));
    // a line without a request id is skipped; no cache_creation breakdown → 5-minute writes
    append(&f, &claude("", 1000, 1000, json!({})));
    let mut v: Value = serde_json::from_str(&claude("req_3", 0, 0, json!({}))).unwrap();
    v["message"]["usage"]
        .as_object_mut()
        .unwrap()
        .remove("cache_creation");
    v["message"]["usage"]["cache_creation_input_tokens"] = json!(33);
    append(&f, &(v.to_string() + "\n"));
    let conn = run(&e, &[]);
    let (t, req) = totals(&conn);
    assert_eq!((t.input, t.cache_write_5m, req), (107, 33, 3));
}

#[test]
fn a_record_split_across_collects_counts_once_and_subagents_are_read() {
    let e = env();
    let f = e.cfg.join("projects/p/s1.jsonl");
    let line = claude("req_s", 50, 5, json!({}));
    let (a, b) = line.split_at(line.len() / 2);
    write(&f, a);
    let conn = run(&e, &[]);
    assert_eq!(totals(&conn).1, 0, "an unterminated tail waits");
    append(&f, b);
    write(
        &e.cfg.join("projects/p/s1/subagents/agent-1.jsonl"),
        &claude("req_sub", 3, 4, json!({})),
    );
    let conn = run(&e, &[]);
    assert_eq!(
        totals(&conn),
        (
            Tokens {
                input: 53,
                output: 9,
                ..Default::default()
            },
            2
        )
    );
}

#[test]
fn an_uncommitted_collect_leaves_no_trace() {
    let e = env();
    let (mut conn, _) = store::open(&store::usage_dir(&e.data)).unwrap();
    {
        let tx = conn.transaction().unwrap();
        let v: Value = serde_json::from_str(&claude("req_x", 10, 10, json!({}))).unwrap();
        claude_line(&tx, &v, "acct-a", NOW).unwrap();
        // dropped without commit: an interrupted collect
    }
    assert_eq!(totals(&conn).1, 0);
    write(
        &e.cfg.join("projects/p/s.jsonl"),
        &claude("req_x", 10, 10, json!({})),
    );
    let rs = roots(std::slice::from_ref(&e.cfg), None);
    collect(&mut conn, &rs, &[], NOW).unwrap();
    assert_eq!(
        totals(&conn),
        (
            Tokens {
                input: 10,
                output: 10,
                ..Default::default()
            },
            1
        )
    );
}

#[test]
fn rereads_after_a_rewrite_do_not_recount_and_growth_keeps_the_cursor() {
    let e = env();
    let f = e.cfg.join("projects/p/s.jsonl");
    write(&f, &claude("req_1", 10, 1, json!({})));
    let conn = run(&e, &[]);
    let head0: (i64, i64) = conn
        .query_row("SELECT offset, head_len FROM sources", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    // growth: the stored head still matches over its length, the cursor advances
    append(&f, &claude("req_2", 20, 2, json!({})));
    let conn = run(&e, &[]);
    let (off, hl): (i64, i64) = conn
        .query_row("SELECT offset, head_len FROM sources", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(off > head0.0 && hl == off.min(4096));
    assert_eq!(totals(&conn).1, 2);
    // rewritten with a different start (head changes): reread from 0, old ids not recounted
    write(
        &f,
        &(claude("req_new", 5, 5, json!({}))
            + &claude("req_1", 10, 1, json!({}))
            + &claude("req_2", 20, 2, json!({}))),
    );
    let conn = run(&e, &[]);
    assert_eq!(
        totals(&conn),
        (
            Tokens {
                input: 35,
                output: 8,
                ..Default::default()
            },
            3
        )
    );
    // shrunk: reread, nothing recounted
    write(&f, &claude("req_1", 10, 1, json!({})));
    let conn = run(&e, &[]);
    assert_eq!(totals(&conn).1, 3);
}

#[test]
fn records_past_the_horizon_are_not_counted() {
    let e = env();
    let old = (chrono::DateTime::from_timestamp_millis(NOW - (HORIZON_DAYS + 1) * 86_400_000)
        .unwrap())
    .to_rfc3339();
    write(
        &e.cfg.join("projects/p/s.jsonl"),
        &(claude("req_old", 99, 99, json!({"timestamp": old}))
            + &claude("req_now", 1, 1, json!({}))),
    );
    let conn = run(&e, &[]);
    assert_eq!(
        totals(&conn),
        (
            Tokens {
                input: 1,
                output: 1,
                ..Default::default()
            },
            1
        )
    );
}

fn codex_file(e: &Env, day: &str, uuid: &str, totals: &[(u64, u64, u64)]) -> PathBuf {
    let f = e.codex.join(format!(
        "sessions/{day}/rollout-2026-11-03T10-00-00-{uuid}.jsonl"
    ));
    let mut s = json!({"timestamp": TS, "type": "turn_context", "payload": {"model": "gpt-6.1-sol", "cwd": "/MARKERCX/proj"}}).to_string() + "\n";
    for (i, c, o) in totals {
        s += &(json!({"timestamp": TS, "type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": {"input_tokens": i, "cached_input_tokens": c, "cache_write_input_tokens": 0, "output_tokens": o, "reasoning_output_tokens": 0, "total_tokens": i + o}}}}).to_string() + "\n");
    }
    write(&f, &s);
    f
}

#[test]
fn codex_counts_the_cumulative_once_per_session() {
    let e = env();
    let uuid = "0199aaaa-bbbb-7ccc-8ddd-eeeeffff0001";
    codex_file(&e, "2026/11/03", uuid, &[(100, 40, 10), (250, 100, 30)]);
    let conn = run(&e, &[]);
    let rows = rows_since(&conn, "2000-01-01").unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(
        (r.source.as_str(), r.model.as_str(), r.project.as_str()),
        ("codex", "gpt-6.1-sol", "proj")
    );
    assert!(r.account.starts_with("codex-"));
    assert_eq!(
        r.tokens,
        Tokens {
            input: 150,
            output: 30,
            cache_read: 100,
            ..Default::default()
        }
    );
    // a copy of the same session elsewhere, and a decreasing total, add nothing
    codex_file(&e, "2026/11/02", uuid, &[(250, 100, 30), (120, 10, 5)]);
    let conn = run(&e, &[]);
    assert_eq!(
        totals(&conn).0,
        Tokens {
            input: 150,
            output: 30,
            cache_read: 100,
            ..Default::default()
        }
    );
}

#[cfg(unix)]
#[test]
fn nothing_outside_the_globs_or_behind_a_link_is_read() {
    let e = env();
    let outside = e._tmp.path().join("outside");
    write(
        &outside.join("x.jsonl"),
        &claude("req_out", 1000, 1000, json!({})),
    );
    std::os::unix::fs::symlink(&outside, e.cfg.join("projects/linked")).unwrap();
    write(
        &e.cfg.join("projects/p/real.jsonl"),
        &claude("req_in", 1, 1, json!({})),
    );
    std::os::unix::fs::symlink(outside.join("x.jsonl"), e.cfg.join("projects/p/link.jsonl"))
        .unwrap();
    let fifo = std::ffi::CString::new(
        e.cfg
            .join("projects/p/fifo.jsonl")
            .as_os_str()
            .as_encoded_bytes(),
    )
    .unwrap();
    // SAFETY: a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    write(
        &e.cfg.join("settings.json"),
        &claude("req_settings", 500, 500, json!({})),
    );
    write(
        &e.cfg.join("projects/p/notes.txt"),
        &claude("req_txt", 500, 500, json!({})),
    );
    let rs = roots(std::slice::from_ref(&e.cfg), None);
    let names: Vec<String> = sources(&rs[0])
        .iter()
        .map(|p| name_of(p).to_string())
        .collect();
    assert_eq!(names, ["real.jsonl"]);
    let conn = run(&e, &[]);
    assert_eq!(
        totals(&conn),
        (
            Tokens {
                input: 1,
                output: 1,
                ..Default::default()
            },
            1
        )
    );
}

#[test]
fn a_root_takes_the_alias_of_the_snapshot_naming_it() {
    let e = env();
    write(
        &e.cfg.join("projects/p/s.jsonl"),
        &claude("req_1", 1, 1, json!({})),
    );
    let rs = roots(std::slice::from_ref(&e.cfg), None);
    let hash = rs[0].hash.clone();
    let snap = |alias: &str, root: Option<&str>, at: i64| Snapshot {
        alias: alias.into(),
        org: None,
        email: None,
        root: root.map(String::from),
        written_at_ms: at,
        five_hour: None,
        seven_day: None,
    };
    let (conn, _) = store::open(&store::usage_dir(&e.data)).unwrap();
    // no snapshot names it: the hash alias
    assert_eq!(
        root_alias(&conn, &rs[0], &[], NOW).unwrap(),
        format!("acct-{hash}")
    );
    // the newest snapshot naming it wins; one for another root does not
    let snaps = [
        snap("acct-b", Some(&hash), 1),
        snap("acct-c", Some(&hash), 2),
        snap("acct-z", Some("00000000"), 3),
    ];
    assert_eq!(root_alias(&conn, &rs[0], &snaps, NOW).unwrap(), "acct-c");
    // the mapping is stored: without snapshots it stays
    assert_eq!(root_alias(&conn, &rs[0], &[], NOW).unwrap(), "acct-c");
}

#[test]
fn model_ids_outside_the_grammar_become_unknown() {
    assert_eq!(model_id(Some("claude-opus-5-5")), "claude-opus-5-5");
    assert_eq!(model_id(Some("gpt-5.5")), "gpt-5.5");
    for bad in [
        "<synthetic>",
        "/home/x/model",
        "me@example.com",
        "",
        &"a".repeat(65),
    ] {
        assert_eq!(model_id(Some(bad)), "unknown", "{bad}");
    }
    assert_eq!(model_id(None), "unknown");
}

#[test]
fn a_home_dir_cwd_is_stored_as_tilde_and_old_files_are_not_read() {
    let e = env();
    let home = dirs::home_dir().unwrap();
    write(
        &e.cfg.join("projects/p/s.jsonl"),
        &claude("req_h", 1, 1, json!({"cwd": home.to_str().unwrap()})),
    );
    // a file last written before the horizon is not read, even if its lines claim a recent time
    let old = e.cfg.join("projects/p/old.jsonl");
    write(&old, &claude("req_stale", 500, 500, json!({})));
    let t = std::time::SystemTime::UNIX_EPOCH
        + std::time::Duration::from_millis((NOW - (HORIZON_DAYS + 2) * 86_400_000) as u64);
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(t)
        .unwrap();
    let conn = run(&e, &[]);
    let rows = rows_since(&conn, "2000-01-01").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].project, "~");
    assert_eq!(rows[0].requests, 1);
}

#[test]
fn a_line_longer_than_the_bound_is_read_through_and_dropped() {
    // the reader: a long line is consumed whole, not kept, and the next line is intact
    let data = b"aaaaaaaaaaaaaaaaaaaa\nok\ntail";
    let mut r = std::io::BufReader::with_capacity(4, &data[..]);
    let mut buf = Vec::new();
    assert_eq!(next_line(&mut r, &mut buf, 8).unwrap(), Some((21, false)));
    assert!(buf.is_empty() && buf.capacity() <= 8);
    assert_eq!(next_line(&mut r, &mut buf, 8).unwrap(), Some((3, true)));
    assert_eq!(buf, b"ok\n");
    assert_eq!(
        next_line(&mut r, &mut buf, 8).unwrap(),
        None,
        "unterminated tail waits"
    );
    // a collect: a usage line over MAX_LINE is not counted, the next one is, the cursor passes both
    let e = env();
    let f = e.cfg.join("projects/p/s.jsonl");
    let pad = "x".repeat(MAX_LINE);
    write(
        &f,
        &(claude("req_big", 999, 999, json!({"pad": pad})) + &claude("req_ok", 1, 1, json!({}))),
    );
    let conn = run(&e, &[]);
    assert_eq!(
        totals(&conn),
        (
            Tokens {
                input: 1,
                output: 1,
                ..Default::default()
            },
            1
        )
    );
    let off: i64 = conn
        .query_row("SELECT offset FROM sources", [], |r| r.get(0))
        .unwrap();
    assert_eq!(off as u64, std::fs::metadata(&f).unwrap().len());
}

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc3339()
}

fn codex_events(e: &Env, uuid: &str, events: &[(i64, u64)]) -> PathBuf {
    let f = e.codex.join(format!(
        "sessions/2026/11/03/rollout-2026-11-03T10-00-00-{uuid}.jsonl"
    ));
    let mut s = json!({"timestamp": TS, "type": "turn_context", "payload": {"model": "gpt-6.1-sol", "cwd": "/w/proj"}}).to_string() + "\n";
    for (ms, out) in events {
        s += &(json!({"timestamp": iso(*ms), "type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": {"input_tokens": 0, "cached_input_tokens": 0, "cache_write_input_tokens": 0, "output_tokens": out}}}}).to_string() + "\n");
    }
    write(&f, &s);
    f
}

fn run_at(e: &Env, now: i64) -> (Connection, Report) {
    let (mut conn, _) = store::open(&store::usage_dir(&e.data)).unwrap();
    let rs = roots(std::slice::from_ref(&e.cfg), Some(&e.codex));
    let rep = collect(&mut conn, &rs, &[], now).unwrap();
    (conn, rep)
}

#[test]
fn a_codex_cumulative_from_before_the_horizon_is_the_base_not_new_usage() {
    let e = env();
    let day = 86_400_000;
    codex_events(
        &e,
        "0199aaaa-bbbb-7ccc-8ddd-eeeeffff0002",
        &[(NOW - (HORIZON_DAYS + 5) * day, 1000), (NOW - day, 1100)],
    );
    let (conn, _) = run_at(&e, NOW);
    assert_eq!(totals(&conn).0.output, 100);
}

#[test]
fn a_codex_session_resumed_after_its_base_was_pruned_counts_only_the_increase() {
    let e = env();
    let day = 86_400_000;
    let t0 = NOW - 45 * day;
    let f = codex_events(&e, "0199aaaa-bbbb-7ccc-8ddd-eeeeffff0003", &[(t0, 1000)]);
    let (conn, _) = run_at(&e, t0 + day);
    assert_eq!(totals(&conn).0.output, 1000);
    drop(conn);
    // 44 days later, nothing new: the session's base is pruned
    let (conn, _) = run_at(&e, NOW);
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM codex_sessions WHERE seen_ms > 0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0, "base pruned");
    drop(conn);
    // the session goes on: only the increase is new usage
    append(&f, &(json!({"timestamp": iso(NOW - 3_600_000), "type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": {"input_tokens": 0, "cached_input_tokens": 0, "cache_write_input_tokens": 0, "output_tokens": 1100}}}}).to_string() + "\n"));
    let (conn, _) = run_at(&e, NOW);
    assert_eq!(totals(&conn).0.output, 1100, "1000 earlier + 100 now");
}

#[test]
fn counts_off_the_bound_are_skipped_and_an_overflowing_bucket_rolls_the_file_back() {
    let e = env();
    let big = i64::MAX as u64;
    let f = e.cfg.join("projects/p/s.jsonl");
    write(
        &f,
        &(claude("req_m1", big, big, json!({}))
            + &claude("req_m2", big, big, json!({}))
            + &claude("req_ok", MAX_COUNT, 1, json!({}))),
    );
    let (conn, rep) = run_at(&e, NOW);
    assert_eq!(rep.skipped, 0);
    assert_eq!(
        totals(&conn),
        (
            Tokens {
                input: MAX_COUNT,
                output: 1,
                ..Default::default()
            },
            1
        )
    );
    // the report still builds
    let rows = rows_since(&conn, "2000-01-01").unwrap();
    crate::usage::summary::validate(&crate::usage::summary::build(
        &rows,
        &[],
        "host-1a2b3c4d",
        NOW,
        &Default::default(),
    ))
    .unwrap();
    // a bucket one add away from i64::MAX: the add is refused, the file's whole transaction rolls back
    conn.execute("UPDATE buckets SET input = ?1", [i64::MAX - 5])
        .unwrap();
    drop(conn);
    append(&f, &claude("req_more", 10, 1, json!({})));
    let (conn, rep) = run_at(&e, NOW);
    assert_eq!(rep.skipped, 1);
    let (input, req): (i64, i64) = conn
        .query_row("SELECT input, requests FROM buckets", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(
        (input, req),
        (i64::MAX - 5, 1),
        "nothing of the file counted"
    );
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM requests WHERE request_id LIKE 'req_more%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
    // and the sums over it saturate instead of wrapping
    let rows = rows_since(&conn, "2000-01-01").unwrap();
    let v = crate::usage::summary::build(
        &[rows[0].clone(), rows[0].clone(), rows[0].clone()],
        &[],
        "host-1a2b3c4d",
        NOW,
        &Default::default(),
    );
    assert_eq!(
        v["days"][0]["input"],
        json!(crate::usage::summary::UINT_MAX),
        "saturated, then capped at the contract bound"
    );
    crate::usage::summary::validate(&v).unwrap();
}

#[test]
fn a_root_without_a_snapshot_takes_its_login_alias_over_a_stored_one() {
    let e = env();
    write(
        &e.cfg.join("projects/p/s.jsonl"),
        &claude("req_1", 1, 1, json!({})),
    );
    let mut rs = roots(std::slice::from_ref(&e.cfg), None);
    let (conn, _) = store::open(&store::usage_dir(&e.data)).unwrap();
    assert_eq!(
        root_alias(&conn, &rs[0], &[], NOW).unwrap(),
        format!("acct-{}", rs[0].hash)
    );
    rs[0].account = Some("acct-1111aaaa".into());
    assert_eq!(
        root_alias(&conn, &rs[0], &[], NOW).unwrap(),
        "acct-1111aaaa"
    );
    // a re-login changes it; a lost login keeps the stored one
    rs[0].account = Some("acct-2222bbbb".into());
    assert_eq!(
        root_alias(&conn, &rs[0], &[], NOW).unwrap(),
        "acct-2222bbbb"
    );
    rs[0].account = None;
    assert_eq!(
        root_alias(&conn, &rs[0], &[], NOW).unwrap(),
        "acct-2222bbbb"
    );
}
