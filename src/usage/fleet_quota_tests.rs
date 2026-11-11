use super::fleet_quota::*;
use super::names::Names;
use super::quota_view::{self as qv, Tr};
use serde_json::{json, Value};
use unicode_width::UnicodeWidthStr;

const NOW_S: i64 = 1_793_700_000;
const NOW_MS: i64 = NOW_S * 1000;
const H: i64 = 3600;
const D: i64 = 86400;

fn acct(alias: &str, age_s: i64, five: Option<(f64, i64)>, seven: Option<(f64, i64)>) -> Value {
    let w =
        |x: Option<(f64, i64)>| x.map_or(Value::Null, |(p, r)| json!({"pct": p, "resets_at": r}));
    json!({"alias": alias, "written_at_ms": (NOW_S - age_s) * 1000, "five_hour": w(five), "seven_day": w(seven)})
}

fn day(alias: &str, source: &str) -> Value {
    json!({"day": "2026-11-08", "source": source, "account": alias, "model": "m-1", "speed": "standard",
        "input": 1, "output": 1, "cache_write_5m": 0, "cache_write_1h": 0, "cache_read": 0, "requests": 1,
        "cost_usd": null})
}

fn summary(host: &str, accounts: Vec<Value>, days: Vec<Value>) -> Value {
    let v = json!({"schema": "codeforge.usage-summary/1", "generated_at_ms": NOW_MS, "host": host,
        "prices_as_of": "2026-10-06", "accounts": accounts, "days": days});
    crate::usage::summary::validate(&v).expect("fixture must be a valid summary");
    v
}

fn fixture_dir(files: &[(&str, &Value)]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for (n, v) in files {
        std::fs::write(d.path().join(n), serde_json::to_vec(v).unwrap()).unwrap();
    }
    d
}

fn run(files: &[(&str, &Value)]) -> Fleet {
    let d = fixture_dir(files);
    let (ok, skipped) = load(d.path()).unwrap();
    merge(&ok, skipped, NOW_MS, DEFAULT_USE_TARGET, &Names::default())
}

fn row<'a>(f: &'a Fleet, alias: &str) -> &'a Row {
    f.rows.iter().find(|r| r.alias == alias).unwrap()
}

#[test]
fn alias_on_two_hosts_merges_and_takes_the_newer_observation() {
    let a = summary(
        "host-aaaaaaaa",
        vec![acct("work", 7200, None, Some((0.2, NOW_S + 3 * D)))],
        vec![],
    );
    let b = summary(
        "host-bbbbbbbb",
        vec![acct("work", 600, None, Some((0.5, NOW_S + 3 * D)))],
        vec![],
    );
    let f = run(&[("gpu-a.json", &a), ("gpu-b.json", &b)]);
    let r = row(&f, "work");
    assert_eq!(r.hosts, vec!["gpu-a", "gpu-b"]);
    assert_eq!(r.weekly.as_ref().unwrap().win.pct, 0.5);
    assert_eq!(r.observed_at_ms, Some((NOW_S - 600) * 1000));
}

#[test]
fn invalid_file_is_skipped_and_counted_not_fatal() {
    let a = summary(
        "host-aaaaaaaa",
        vec![acct("work", 60, None, Some((0.2, NOW_S + 3 * D)))],
        vec![],
    );
    let mut bad = a.clone();
    bad["host"] = json!("/home/x/secret");
    let d = fixture_dir(&[("ok.json", &a), ("bad.json", &bad)]);
    std::fs::write(d.path().join("junk.json"), b"{nope").unwrap();
    let (ok, skipped) = load(d.path()).unwrap();
    let f = merge(&ok, skipped, NOW_MS, DEFAULT_USE_TARGET, &Names::default());
    assert_eq!(f.skipped.get("invalid"), Some(&1));
    assert_eq!(f.skipped.get("not_json"), Some(&1));
    assert!(f.rows.iter().any(|r| r.alias == "work"));
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("2 份略過"), "{txt}");
    assert!(!txt.contains("secret"));
}

#[test]
fn waste_case() {
    let s = summary(
        "host-aaaaaaaa",
        vec![acct("w", 60, None, Some((0.31, NOW_S + 30 * H)))],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let w = f.rows[0].weekly.as_ref().unwrap();
    assert_eq!(w.warn, Some(Warn::Waste));
    assert_eq!(f.rows[0].alias, "w");
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(
        txt.contains("會浪費 · w · 週 31% · 重置 ◷ 1d6h · 目標 85%"),
        "{txt}"
    );
}

#[test]
fn short_case_projects_and_estimates_exhaustion() {
    // elapsed 3 d, 72 % -> projected 1.68; rate 0.24/d -> 0.28 left = 1.1667 d
    let s = summary(
        "host-aaaaaaaa",
        vec![acct("s", 60, None, Some((0.72, NOW_S + 4 * D)))],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let w = f.rows[0].weekly.as_ref().unwrap();
    assert_eq!(w.warn, Some(Warn::Short));
    assert!((w.projected.unwrap() - 1.68).abs() < 1e-9);
    let x = w.exhaust_at.unwrap() - NOW_S;
    assert!((x - (D as f64 * 0.28 / 0.24) as i64).abs() <= 1, "{x}");
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(
        txt.contains("會不夠 估 · s · 週 72% · 估 1d4h 後用完 · 重置 ◷ 4d"),
        "{txt}"
    );
}

#[test]
fn warmup_has_no_projection_and_no_warning() {
    // elapsed 2 h
    let s = summary(
        "host-aaaaaaaa",
        vec![acct("c", 60, None, Some((0.5, NOW_S + 7 * D - 2 * H)))],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let w = row(&f, "c").weekly.as_ref().unwrap();
    assert!(w.projected.is_none() && w.warn.is_none() && w.exhaust_at.is_none());
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("暖機"), "{txt}");
}

#[test]
fn full_window_is_not_short_and_target_met_is_not_waste() {
    let s = summary(
        "host-aaaaaaaa",
        vec![
            acct("full", 60, None, Some((1.0, NOW_S + 4 * D))),
            acct("fine", 60, None, Some((0.9, NOW_S + 10 * H))),
        ],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    assert!(row(&f, "full").weekly.as_ref().unwrap().warn.is_none());
    assert!(row(&f, "fine").weekly.as_ref().unwrap().warn.is_none());
}

#[test]
fn older_than_six_hours_is_expired_without_warning() {
    let s = summary(
        "host-aaaaaaaa",
        vec![acct("old", 6 * H + 60, None, Some((0.1, NOW_S + 10 * H)))],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let r = row(&f, "old");
    assert_eq!(r.state, State::Expired);
    assert!(r.weekly.is_none() && r.five_hour.is_none());
    let j = to_json(&f, NOW_MS);
    let a = j["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["alias"] == "old")
        .unwrap();
    assert_eq!(a["state"], "expired");
    assert!(a["weekly"].is_null());
}

#[test]
fn passed_reset_is_not_drawn() {
    let s = summary(
        "host-aaaaaaaa",
        vec![
            acct("gone", 60, Some((0.4, NOW_S - 10)), Some((0.3, NOW_S - 1))),
            acct(
                "half",
                60,
                Some((0.4, NOW_S - 10)),
                Some((0.3, NOW_S + 3 * D)),
            ),
        ],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let g = row(&f, "gone");
    assert_eq!(g.state, State::Stale);
    assert!(g.weekly.is_none() && g.five_hour.is_none());
    let h = row(&f, "half");
    assert_eq!(h.state, State::Ok);
    assert!(h.five_hour.is_some() && h.five_reset && h.weekly.is_some());
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("已重置，等新觀察"), "{txt}");
}

#[test]
fn codex_only_alias_and_every_provider_are_no_source_with_a_reason() {
    let s = summary("host-aaaaaaaa", vec![], vec![day("cx", "codex")]);
    let f = run(&[("box.json", &s)]);
    let r = row(&f, "cx");
    assert_eq!(r.state, State::NoSource);
    assert_eq!(r.provider, "codex");
    assert_eq!(r.hosts, vec!["box"]);
    assert_eq!(r.source_note, Some(SourceNote::NoWindow));
    // a codex account exists, so no extra `codex` placeholder row
    assert!(f.rows.iter().all(|r| r.alias != "codex"));
    for (p, note) in PROVIDERS.into_iter().filter(|(p, _)| *p != "codex") {
        let r = row(&f, p);
        assert_eq!((r.state, r.provider.as_str()), (State::NoSource, p));
        assert_eq!(r.source_note, Some(note), "{p}");
        assert_eq!(p, p.to_lowercase(), "labels are lowercase");
    }
    let v = to_json(&f, NOW_MS);
    let notes: std::collections::BTreeMap<&str, &str> = v["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["alias"].as_str().unwrap(),
                a["source_note"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(notes["kimi"], "web_only");
    assert_eq!(notes["glm"], "needs_token");
    assert_eq!(notes["minimax"], "needs_token");
    assert_eq!(notes["cx"], "no_window");
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("無來源 · 只有網頁"), "{txt}");
    assert!(
        txt.contains("無來源 · 要 token，待 autopilot 鏡像"),
        "{txt}"
    );
    assert!(txt.contains("無來源 · 帳號沒有窗口資料"), "{txt}");
    let en = text(&f, 100, NOW_MS, &Tr("en".into()), false);
    assert!(en.contains("no source · only on the web page"), "{en}");
}

#[test]
fn with_no_codex_account_a_codex_row_says_no_window() {
    let s = summary(
        "host-aaaaaaaa",
        vec![acct("work", 60, None, Some((0.2, NOW_S + 3 * D)))],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let r = row(&f, "codex");
    assert_eq!(
        (r.state, r.source_note),
        (State::NoSource, Some(SourceNote::NoWindow))
    );
    // a Claude row never carries a source note, in the JSON either
    assert_eq!(row(&f, "work").source_note, None);
    let v = to_json(&f, NOW_MS);
    let w = v["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["alias"] == "work")
        .unwrap();
    assert!(
        w.get("source_note").is_none() && w.get("note").is_none(),
        "{w}"
    );
}

#[test]
fn a_codex_account_with_windows_is_merged_like_a_claude_one() {
    // the collector puts codex windows in the summary account entry keyed `codex-<hex8>`
    let a = summary(
        "host-aaaaaaaa",
        vec![acct(
            "codex-1a2b3c4d",
            7200,
            Some((0.1, NOW_S + 2 * H)),
            Some((0.2, NOW_S + 3 * D)),
        )],
        vec![day("codex-1a2b3c4d", "codex")],
    );
    let b = summary(
        "host-bbbbbbbb",
        vec![acct(
            "codex-1a2b3c4d",
            60,
            None,
            Some((0.72, NOW_S + 4 * D)),
        )],
        vec![],
    );
    let f = run(&[("gpu-a.json", &a), ("gpu-b.json", &b)]);
    let r = row(&f, "codex-1a2b3c4d");
    assert_eq!(r.provider, "codex");
    assert_eq!(r.name, "codex");
    assert_eq!(r.state, State::Ok);
    assert_eq!(r.source_note, None);
    let w = r.weekly.as_ref().unwrap();
    assert_eq!((w.win.pct, w.warn), (0.72, Some(Warn::Short)));
    assert_eq!(r.old_hosts, vec!["gpu-a"]);
    assert!(f.rows.iter().all(|r| r.alias != "codex"), "no placeholder");
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("會不夠 估 · codex · 週 72%"), "{txt}");
}

#[test]
fn agy_pool_keys_are_two_rows_with_their_pool_providers() {
    // the collector records the host's agy account as `agy-<hex8>` (Gemini pool) and
    // `agyclaude-<hex8>` (Claude/GPT pool): two accounts, one key per pool
    let s = summary(
        "host-aaaaaaaa",
        vec![
            acct(
                "agy-1a2b3c4d",
                60,
                Some((0.02, NOW_S + 2 * H)),
                Some((0.4, NOW_S + 3 * D)),
            ),
            acct(
                "agyclaude-1a2b3c4d",
                60,
                Some((0.0, NOW_S + 2 * H)),
                Some((0.16, NOW_S + 3 * D)),
            ),
        ],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let g = row(&f, "agy-1a2b3c4d");
    assert_eq!(g.provider, "agy");
    assert_eq!(g.name, "agy");
    assert_eq!(g.state, State::Ok);
    let c = row(&f, "agyclaude-1a2b3c4d");
    assert_eq!(c.provider, "agyclaude");
    assert_eq!(c.name, "agyclaude");
    assert_eq!(c.state, State::Ok);
    // a host with no agy account has no agy row at all: no placeholder
    assert!(f.rows.iter().all(|r| r.alias != "agy"));
    assert!(f.rows.iter().all(|r| r.alias != "agyclaude"));
}

fn notes(v: &[(&str, &str)]) -> super::names::Notes {
    super::names::Notes(
        v.iter()
            .map(|(k, n)| (k.to_string(), n.to_string()))
            .collect(),
    )
}

#[test]
fn notes_attach_by_account_key_first_then_provider_and_show_dim_at_the_row_end() {
    let s = summary(
        "host-aaaaaaaa",
        vec![
            acct("codex-1a2b3c4d", 60, None, Some((0.2, NOW_S + 3 * D))),
            acct("codex-99999999", 60, None, Some((0.3, NOW_S + 3 * D))),
            acct("work", 60, None, Some((0.4, NOW_S + 3 * D))),
        ],
        vec![],
    );
    let mut f = run(&[("a.json", &s)]);
    attach_notes(
        &mut f,
        &notes(&[
            ("kimi", "週配額用完"),
            ("codex", "team plan"),
            ("codex-1a2b3c4d", "mine"),
        ]),
    );
    assert_eq!(row(&f, "codex-1a2b3c4d").note.as_deref(), Some("mine"));
    assert_eq!(row(&f, "codex-99999999").note.as_deref(), Some("team plan"));
    assert_eq!(row(&f, "kimi").note.as_deref(), Some("週配額用完"));
    assert_eq!(row(&f, "work").note, None);
    let v = to_json(&f, NOW_MS);
    let by = |alias: &str| {
        v["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["alias"] == alias)
            .unwrap()
            .clone()
    };
    assert_eq!(by("kimi")["note"], "週配額用完");
    assert!(by("work").get("note").is_none());
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("無來源 · 只有網頁  週配額用完"), "{txt}");
    assert!(txt.contains("mine"), "{txt}");
    // dim when painted
    let colored = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), true);
    let line = colored.lines().find(|l| l.contains("週配額用完")).unwrap();
    let at = line.find("週配額用完").unwrap();
    assert!(line[..at].ends_with("\u{1b}[2m"), "{line:?}");
}

#[test]
fn a_note_key_with_no_row_adds_a_no_data_claude_row_unless_a_name_is_taken() {
    let s = summary(
        "host-aaaaaaaa",
        vec![acct("work", 60, None, Some((0.4, NOW_S + 3 * D)))],
        vec![],
    );
    let mut f = run(&[("a.json", &s)]);
    let before = f.rows.len();
    attach_notes(
        &mut f,
        &notes(&[
            ("twgame", "週四重置"),
            ("work", "mine"),
            ("kimi", "provider"),
            ("claude", "provider"),
            ("acct-11111111", "gone"),
            ("codex-1a2b3c4d", "gone"),
        ]),
    );
    // only twgame is added: `work` has a row, labels and keys are not accounts
    assert_eq!(f.rows.len(), before + 1);
    let r = row(&f, "twgame");
    assert_eq!(r.name, "twgame");
    assert_eq!(r.provider, "claude");
    assert_eq!(r.state, State::NoSource);
    assert!(r.hosts.is_empty() && r.weekly.is_none() && r.five_hour.is_none());
    assert_eq!(r.source_note, None);
    assert_eq!(r.note.as_deref(), Some("週四重置"));
    let v = to_json(&f, NOW_MS);
    let a = v["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["alias"] == "twgame")
        .unwrap();
    assert_eq!(a["name"], "twgame");
    assert_eq!(a["hosts"], serde_json::json!([]));
    assert_eq!(a["note"], "週四重置");
    assert!(a.get("source_note").is_none());
    for cols in [100, 60, 40, 20] {
        let txt = text(&f, cols, NOW_MS, &Tr("zh-TW".into()), false);
        assert!(txt.contains("twgame") || cols < 40, "{cols}: {txt}");
        for l in txt.lines() {
            assert!(UnicodeWidthStr::width(l) <= cols, "{cols}: {l:?}");
        }
    }
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(
        txt.contains("twgame") && txt.contains("無來源") && txt.contains("週四重置"),
        "{txt}"
    );

    // a name another account shows: the account wins, the note row is dropped
    let mut f = run(&[("a.json", &s)]);
    let before = f.rows.len();
    let shown = f
        .rows
        .iter()
        .find(|r| r.alias == "work")
        .unwrap()
        .name
        .clone();
    attach_notes(&mut f, &notes(&[(&shown, "x"), ("twgame", "y")]));
    assert_eq!(f.rows.len(), before + 1);
    assert_eq!(row(&f, "twgame").note.as_deref(), Some("y"));
}

#[test]
fn a_long_cjk_note_is_cut_to_the_width_and_the_numbers_stay() {
    let s = summary(
        "host-aaaaaaaa",
        vec![acct(
            "work",
            60,
            Some((0.3, NOW_S + 2 * H)),
            Some((0.4, NOW_S + 3 * D)),
        )],
        vec![],
    );
    let mut f = run(&[("a.json", &s)]);
    let long = "週".repeat(20);
    attach_notes(&mut f, &notes(&[("work", &long), ("kimi", &long)]));
    for cols in [100, 80, 60, 40, 20] {
        for tr in ["zh-TW", "en"] {
            let txt = text(&f, cols, NOW_MS, &Tr(tr.into()), false);
            for l in txt.lines() {
                assert!(UnicodeWidthStr::width(l) <= cols, "{cols} {tr}: {l:?}");
            }
            if cols >= 40 {
                assert!(txt.contains("40%"), "{cols} {tr}: {txt}");
                assert!(
                    txt.contains("週週週"),
                    "{cols} {tr}: the note is shown: {txt}"
                );
            }
        }
    }
}

#[test]
fn warnings_sort_first_waste_then_short_by_reset_then_alias() {
    let s = summary(
        "host-aaaaaaaa",
        vec![
            acct("aaa", 60, None, Some((0.5, NOW_S + 5 * D))),
            acct("short-late", 60, None, Some((0.72, NOW_S + 4 * D))),
            acct("short-soon", 60, None, Some((0.8, NOW_S + 3 * D))),
            acct("waste-late", 60, None, Some((0.2, NOW_S + 40 * H))),
            acct("waste-soon", 60, None, Some((0.2, NOW_S + 10 * H))),
        ],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let order: Vec<&str> = f.rows.iter().take(5).map(|r| r.alias.as_str()).collect();
    assert_eq!(
        order,
        [
            "waste-soon",
            "waste-late",
            "short-soon",
            "short-late",
            "aaa"
        ]
    );
}

#[test]
fn host_label_is_file_stem_and_no_hash_or_path_in_output() {
    let s = summary(
        "host-deadbeef",
        vec![acct(
            "work",
            60,
            Some((0.3, NOW_S + 2 * H)),
            Some((0.2, NOW_S + 3 * D)),
        )],
        vec![],
    );
    let d = fixture_dir(&[("gpu-a.json", &s)]);
    let (ok, skipped) = load(d.path()).unwrap();
    let f = merge(&ok, skipped, NOW_MS, DEFAULT_USE_TARGET, &Names::default());
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    let js = serde_json::to_string(&to_json(&f, NOW_MS)).unwrap();
    assert!(txt.contains("gpu-a"));
    for out in [&txt, &js] {
        assert!(!out.contains("host-"), "{out}");
        assert!(!out.contains("deadbeef"));
        assert!(!out.contains(d.path().to_str().unwrap()));
    }
}

#[test]
fn every_line_fits_100_columns_with_cjk_and_narrow_widths() {
    let s = summary(
        "host-aaaaaaaa",
        vec![
            acct(
                "work",
                2 * H,
                Some((0.95, NOW_S + 2 * H)),
                Some((0.31, NOW_S + 30 * H)),
            ),
            acct("burn", 60, None, Some((0.72, NOW_S + 4 * D))),
        ],
        vec![day("cx", "codex")],
    );
    let f = run(&[
        ("很長的主機名稱很長的主機名稱.json", &s),
        ("gpu-b.json", &s),
    ]);
    for cols in [100, 80, 60, 40, 20] {
        for tr in ["zh-TW", "en"] {
            let txt = text(&f, cols, NOW_MS, &Tr(tr.into()), false);
            for l in txt.lines() {
                assert!(UnicodeWidthStr::width(l) <= cols, "{cols} {tr}: {l:?}");
            }
            assert!(!txt.contains('$'));
        }
    }
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("緊"), "{txt}");
}

#[test]
fn json_round_trips_and_out_file_is_written_atomically() {
    let s = summary(
        "host-aaaaaaaa",
        vec![acct(
            "s",
            60,
            Some((0.1, NOW_S + H)),
            Some((0.72, NOW_S + 4 * D)),
        )],
        vec![day("cx", "codex")],
    );
    let f = run(&[("a.json", &s)]);
    let v = to_json(&f, NOW_MS);
    assert_eq!(v["schema"], "codeforge.fleet-quota/1");
    assert_eq!(v["generated_at_ms"], NOW_MS);
    assert_eq!(v["use_target"], 0.85);
    let a = &v["accounts"][0];
    assert_eq!(a["alias"], "s");
    assert_eq!(a["provider"], "claude");
    assert_eq!(a["state"], "ok");
    assert_eq!(a["age_s"], 60);
    assert_eq!(a["weekly"]["warn"], "short");
    assert_eq!(a["five_hour"]["pct"], 0.1);
    let cx = v["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["alias"] == "cx")
        .unwrap();
    assert_eq!(cx["state"], "no_source");
    assert!(cx["weekly"].is_null() && cx["observed_at_ms"].is_null() && cx["age_s"].is_null());
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("q.json");
    write_atomic(&p, &v).unwrap();
    let back: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(back, v);
    assert_eq!(
        std::fs::read_dir(d.path()).unwrap().count(),
        1,
        "no temp file left"
    );
}

fn with_org(mut a: Value, org: &str) -> Value {
    a["org"] = json!(org);
    a
}

#[test]
fn eight_hosts_at_80_cols_keep_as_many_labels_as_fit_and_count_the_rest() {
    let names = [
        "4124gs", "7840hs", "aimax395", "c4140", "cuda", "gentoo", "openclaw", "xl270d",
    ];
    let s = summary(
        "host-aaaaaaaa",
        vec![with_org(
            acct(
                "acct-ae88ee5d",
                60,
                Some((0.3, NOW_S + 2 * H)),
                Some((0.2, NOW_S + 3 * D)),
            ),
            "personal",
        )],
        vec![],
    );
    let files: Vec<(String, &Value)> = names.iter().map(|n| (format!("{n}.json"), &s)).collect();
    let refs: Vec<(&str, &Value)> = files.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let f = run(&refs);
    assert_eq!(
        f.rows
            .iter()
            .find(|r| r.alias == "acct-ae88ee5d")
            .unwrap()
            .hosts
            .len(),
        8
    );
    for (tr, more) in [("zh-TW", "等 "), ("en", "more")] {
        let txt = text(&f, 80, NOW_MS, &Tr(tr.into()), false);
        assert!(!txt.contains("acct-"), "the key is never drawn: {txt}");
        let head = txt.lines().find(|l| l.starts_with("personal")).unwrap();
        assert!(UnicodeWidthStr::width(head) <= 80, "{head:?}");
        assert!(
            head.contains("4124gs") && head.contains(more),
            "{tr}: {head:?}"
        );
        let pos = |s: &str| head.find(s).unwrap();
        assert!(pos("personal") < pos("claude") && pos("claude") < pos("4124gs"));
    }
    // JSON carries the org; an account without one has null
    let js = to_json(&f, NOW_MS);
    let a = js["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["alias"] == "acct-ae88ee5d")
        .unwrap();
    assert_eq!(a["org"], "personal");
    assert!(js["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["org"].is_null()));
}

#[test]
fn summary_org_validates_when_present_and_is_optional() {
    let mut v = json!({"schema": "codeforge.usage-summary/1", "generated_at_ms": NOW_MS, "host": "host-aaaaaaaa",
        "prices_as_of": "2026-10-06", "accounts": [acct("a", 1, None, None)], "days": []});
    assert!(crate::usage::summary::validate(&v).is_ok());
    for ok in ["personal", "Acme 公司"] {
        v["accounts"][0]["org"] = json!(ok);
        assert!(crate::usage::summary::validate(&v).is_ok(), "{ok}");
    }
    for bad in [
        json!("me@x.com"),
        json!("a\nb"),
        json!(""),
        json!("x".repeat(41)),
        json!(5),
    ] {
        v["accounts"][0]["org"] = bad.clone();
        assert!(crate::usage::summary::validate(&v).is_err(), "{bad}");
    }
}

#[test]
fn a_40_char_cjk_org_is_cut_and_keeps_the_numbers_on_screen() {
    let org: String = "研發部門".repeat(10);
    let s = summary(
        "host-aaaaaaaa",
        vec![with_org(
            acct(
                "acct-12345678",
                60,
                Some((0.95, NOW_S + 2 * H)),
                Some((0.97, NOW_S + 3 * D)),
            ),
            &org,
        )],
        vec![],
    );
    let f = run(&[("a.json", &s), ("b.json", &s)]);
    for cols in [30, 40, 50, 60, 70, 80, 100] {
        for tr in ["zh-TW", "en"] {
            let txt = text(&f, cols, NOW_MS, &Tr(tr.into()), false);
            for l in txt.lines() {
                assert!(UnicodeWidthStr::width(l) <= cols, "{cols} {tr}: {l:?}");
            }
            // the name (org short form) is cut, so the weekly % stays on its line (compact) or below it
            let head = txt.lines().find(|l| l.starts_with("研發部門")).unwrap();
            assert!(!txt.contains("acct-"));
            assert!(
                txt.contains("97%") && (cols >= qv::COMPACT_COLS || head.contains("97%")),
                "{cols} {tr}: {txt}"
            );
        }
    }
}

fn with_email(mut a: Value, email: &str) -> Value {
    a["email"] = json!(email);
    a
}

#[test]
fn the_label_is_the_name_then_the_email_and_never_the_key() {
    let s = summary(
        "host-aaaaaaaa",
        vec![
            with_org(
                with_email(
                    acct("acct-11111111", 60, None, Some((0.4, NOW_S + 3 * D))),
                    "me@example.com",
                ),
                "Acme",
            ),
            with_email(
                acct("acct-22222222", 60, None, Some((0.5, NOW_S + 3 * D))),
                "solo@example.com",
            ),
            with_org(
                acct("acct-33333333", 60, None, Some((0.6, NOW_S + 3 * D))),
                "Beta",
            ),
            acct("acct-44444444", 60, None, Some((0.7, NOW_S + 3 * D))),
        ],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    // an acct- key with neither org nor email is an identity from before the login-derived
    // alias: dropped and counted, never drawn
    assert_eq!(
        f.rows
            .iter()
            .filter(|r| r.alias.starts_with("acct-"))
            .count(),
        3
    );
    assert_eq!(f.skipped.get("no_identity"), Some(&1));
    let txt = text(&f, 100, NOW_MS, &Tr("en".into()), false);
    assert!(!txt.contains("acct-"), "the key is never drawn: {txt}");
    let starts = |p: &str| txt.lines().any(|l| l.starts_with(p));
    assert!(starts("acme · me@example.com "), "{txt}");
    assert!(starts("solo · solo@example.com "), "{txt}");
    assert!(starts("beta "), "{txt}");
    // JSON: the name for screens, the alias as the key
    let v = to_json(&f, NOW_MS);
    let a = v["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["alias"] == "acct-11111111")
        .unwrap();
    assert_eq!(a["name"], "acme");
    // a compact line joins the email with a space when it fits
    let txt = text(&f, 50, NOW_MS, &Tr("en".into()), false);
    assert!(starts_with_any(&txt, "acme me@example.com"), "{txt}");
}

fn starts_with_any(txt: &str, p: &str) -> bool {
    txt.lines().any(|l| l.starts_with(p))
}

#[test]
fn a_warn_line_uses_the_same_label() {
    // 90% of the week used with 3 days left: a short warning
    let s = summary(
        "host-aaaaaaaa",
        vec![with_org(
            with_email(
                acct("acct-11111111", 60, None, Some((0.97, NOW_S + 3 * D))),
                "me@example.com",
            ),
            "Acme",
        )],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let txt = text(&f, 100, NOW_MS, &Tr("en".into()), false);
    let warn = txt.lines().find(|l| l.contains(" · week 97%")).unwrap();
    assert!(warn.contains("acme · me@example.com"), "{warn}");
    assert!(!warn.contains("acct-11111111"));
}

#[test]
fn json_email_is_the_newest_observation_that_carried_one_else_null() {
    let old = summary(
        "host-aaaaaaaa",
        vec![
            with_email(
                acct("w", 7200, None, Some((0.2, NOW_S + 3 * D))),
                "old@example.com",
            ),
            acct("n", 7200, None, Some((0.2, NOW_S + 3 * D))),
        ],
        vec![],
    );
    // newer, but from a writer that predates the key: the older email is kept (like org)
    let newer = summary(
        "host-bbbbbbbb",
        vec![
            acct("w", 60, None, Some((0.5, NOW_S + 3 * D))),
            acct("n", 60, None, Some((0.5, NOW_S + 3 * D))),
        ],
        vec![],
    );
    // whatever the file order (a pushed blob has no email and is often the newer copy)
    for files in [
        [("a.json", &old), ("b.json", &newer)],
        [("a.json", &newer), ("b.json", &old)],
    ] {
        let f = run(&files);
        assert_eq!(row(&f, "w").email.as_deref(), Some("old@example.com"));
    }
    // a newer one that carries an email replaces it
    let newest = summary(
        "host-cccccccc",
        vec![with_email(
            acct("w", 10, None, Some((0.6, NOW_S + 3 * D))),
            "new@example.com",
        )],
        vec![],
    );
    let f = run(&[("a.json", &old), ("b.json", &newer), ("c.json", &newest)]);
    let js = to_json(&f, NOW_MS);
    let get = |alias: &str| {
        js["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["alias"] == alias)
            .unwrap()["email"]
            .clone()
    };
    assert_eq!(get("w"), "new@example.com");
    assert!(get("n").is_null());
}

#[test]
fn a_254_char_email_keeps_every_line_inside_the_width_and_the_weekly_on_the_line() {
    let email = format!("{}@example.com", "x".repeat(240));
    assert!(email.chars().count() > 250);
    let s = summary(
        "host-aaaaaaaa",
        vec![with_org(
            with_email(
                acct(
                    "acct-12345678",
                    60,
                    Some((0.95, NOW_S + 2 * H)),
                    Some((0.97, NOW_S + 3 * D)),
                ),
                &email,
            ),
            &"研發部門".repeat(10),
        )],
        vec![],
    );
    let f = run(&[("a.json", &s), ("b.json", &s)]);
    for cols in [30, 40, 60, 80, 100] {
        for tr in ["zh-TW", "en"] {
            let txt = text(&f, cols, NOW_MS, &Tr(tr.into()), false);
            for l in txt.lines() {
                assert!(UnicodeWidthStr::width(l) <= cols, "{cols} {tr}: {l:?}");
            }
            assert!(!txt.contains(&email), "the email is cut: {txt}");
            if cols >= 40 {
                let head = txt.lines().find(|l| l.starts_with("研發部門")).unwrap();
                assert!(
                    txt.contains("97%") && (cols >= qv::COMPACT_COLS || head.contains("97%")),
                    "{cols} {tr}: {txt}"
                );
            }
        }
    }
}

#[test]
fn summary_email_validates_when_present_and_is_optional() {
    let mut v = json!({"schema": "codeforge.usage-summary/1", "generated_at_ms": NOW_MS, "host": "host-aaaaaaaa",
        "prices_as_of": "2026-10-06", "accounts": [acct("a", 1, None, None)], "days": []});
    v["accounts"][0]["email"] = json!("me@example.com");
    assert!(crate::usage::summary::validate(&v).is_ok());
    v["accounts"][0]["email"] = json!("not-an-email");
    assert!(crate::usage::summary::validate(&v).is_err());
}

// ---- newest captured quota wins (seen_at_ms), old hosts, reset windows ----

/// An account whose windows carry a capture time: (pct, resets_at, seen_at_ms).
fn acct_seen(
    alias: &str,
    age_s: i64,
    five: Option<(f64, i64, Option<i64>)>,
    seven: Option<(f64, i64, Option<i64>)>,
) -> Value {
    let w = |x: Option<(f64, i64, Option<i64>)>| {
        x.map_or(Value::Null, |(p, r, s)| {
            let mut v = json!({"pct": p, "resets_at": r});
            if let Some(s) = s {
                v["seen_at_ms"] = json!(s);
            }
            v
        })
    };
    json!({"alias": alias, "written_at_ms": (NOW_S - age_s) * 1000, "five_hour": w(five), "seven_day": w(seven)})
}

fn seen_ms(age_s: i64) -> Option<i64> {
    Some((NOW_S - age_s) * 1000)
}

/// The real failure: both files written 17 min ago; 4124gs captured a weekly 4 % that has since
/// reset, c4140 still carries a 97 % captured days ago with a future reset.
fn real_case() -> Fleet {
    let a = summary(
        "host-aaaaaaaa",
        vec![with_org(
            acct_seen(
                "acct-65d2bffc",
                1020,
                Some((0.25, NOW_S + 3 * H + 36 * 60, seen_ms(60))),
                Some((0.04, NOW_S - 360, seen_ms(900))),
            ),
            "Stranity",
        )],
        vec![],
    );
    let b = summary(
        "host-bbbbbbbb",
        vec![with_org(
            acct_seen(
                "acct-65d2bffc",
                1020,
                Some((0.27, NOW_S + 3 * H + 36 * 60, seen_ms(120))),
                Some((0.97, NOW_S + 73 * H + 54 * 60, seen_ms(2 * D))),
            ),
            "Stranity",
        )],
        vec![],
    );
    run(&[("4124gs.json", &a), ("c4140.json", &b)])
}

#[test]
fn real_case_newest_capture_wins_so_the_weekly_is_reset_not_97() {
    let f = real_case();
    let r = row(&f, "acct-65d2bffc");
    let w = r.weekly.as_ref().unwrap();
    assert!(
        w.reset,
        "4124gs has the newest weekly capture and it has reset"
    );
    assert_eq!(w.win.pct, 0.04);
    assert_eq!((w.warn, w.projected, w.exhaust_at), (None, None, None));
    // 5h: c4140 captured 120 s ago vs 60 s: 4124gs is newer, c4140's 0.27 disagrees
    assert_eq!(r.five_hour.unwrap().pct, 0.25);
    assert!(!r.five_reset);
    assert_eq!(r.old_hosts, vec!["c4140"]);
    assert_eq!(r.hosts, vec!["4124gs", "c4140"]);
    assert_eq!(r.state, State::Ok);
    let js = to_json(&f, NOW_MS);
    let a = js["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["alias"] == "acct-65d2bffc")
        .unwrap();
    assert_eq!(a["old_hosts"], json!(["c4140"]));
    assert_eq!(a["weekly"]["reset"], json!(true));
    assert!(a["weekly"]["warn"].is_null() && a["weekly"]["projected"].is_null());
    assert_eq!(a["five_hour"]["reset"], json!(false));
    for (tr, reset, old) in [
        ("zh-TW", "已重置", "c4140 舊"),
        ("en", "reset", "c4140 old"),
    ] {
        for cols in [40usize, 60, 80, 100] {
            let txt = text(&f, cols, NOW_MS, &Tr(tr.into()), false);
            assert!(!txt.contains("97%"), "{tr} {cols}: {txt}");
            for l in txt.lines() {
                assert!(UnicodeWidthStr::width(l) <= cols, "{tr} {cols}: {l:?}");
            }
            if cols >= 80 {
                assert!(txt.contains(reset), "{tr} {cols}: {txt}");
                assert!(txt.contains(old), "{tr} {cols}: {txt}");
            }
        }
    }
    let t = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(t.contains("週 已重置"), "{t}");
    assert!(t.contains("目前沒有會浪費或會不夠的帳號"), "{t}");
}

#[test]
fn newer_capture_with_a_future_reset_beats_an_older_higher_pct() {
    let a = summary(
        "host-aaaaaaaa",
        vec![acct_seen(
            "work",
            60,
            None,
            Some((0.30, NOW_S + 3 * D, seen_ms(60))),
        )],
        vec![],
    );
    let b = summary(
        "host-bbbbbbbb",
        vec![acct_seen(
            "work",
            60,
            None,
            Some((0.90, NOW_S + 3 * D, seen_ms(3 * H))),
        )],
        vec![],
    );
    let f = run(&[("a.json", &a), ("b.json", &b)]);
    let r = row(&f, "work");
    assert_eq!(r.weekly.as_ref().unwrap().win.pct, 0.30);
    assert!(!r.weekly.as_ref().unwrap().reset);
    assert_eq!(r.old_hosts, vec!["b"]);
    // the other way round: a low newer capture, a higher older one, hosts swapped
    let f2 = run(&[("a.json", &b), ("b.json", &a)]);
    assert_eq!(row(&f2, "work").weekly.as_ref().unwrap().win.pct, 0.30);
    assert_eq!(row(&f2, "work").old_hosts, vec!["a"]);
}

#[test]
fn without_seen_at_ms_the_written_time_is_the_capture_time() {
    let a = summary(
        "host-aaaaaaaa",
        vec![acct("work", 7200, None, Some((0.9, NOW_S + 3 * D)))],
        vec![],
    );
    let b = summary(
        "host-bbbbbbbb",
        vec![acct("work", 60, None, Some((0.2, NOW_S + 3 * D)))],
        vec![],
    );
    let f = run(&[("a.json", &a), ("b.json", &b)]);
    let r = row(&f, "work");
    assert_eq!(r.weekly.as_ref().unwrap().win.pct, 0.2);
    assert_eq!(r.old_hosts, vec!["a"]);
    assert_eq!(r.observed_at_ms, Some((NOW_S - 60) * 1000));
    // a mixed pair: an old file with no seen_at_ms (written 1 h ago) loses to a capture 5 min ago
    let c = summary(
        "host-cccccccc",
        vec![acct_seen(
            "work",
            4000,
            None,
            Some((0.5, NOW_S + 3 * D, seen_ms(300))),
        )],
        vec![],
    );
    let d = summary(
        "host-dddddddd",
        vec![acct("work", 3600, None, Some((0.8, NOW_S + 3 * D)))],
        vec![],
    );
    let f = run(&[("c.json", &c), ("d.json", &d)]);
    assert_eq!(row(&f, "work").weekly.as_ref().unwrap().win.pct, 0.5);
}

#[test]
fn an_older_host_that_agrees_is_not_marked_old() {
    let a = summary(
        "host-aaaaaaaa",
        vec![acct_seen(
            "work",
            60,
            Some((0.3, NOW_S + 2 * H, seen_ms(60))),
            Some((0.2, NOW_S + 3 * D, seen_ms(60))),
        )],
        vec![],
    );
    let b = summary(
        "host-bbbbbbbb",
        vec![acct_seen(
            "work",
            60,
            Some((0.3, NOW_S + 2 * H, seen_ms(600))),
            Some((0.2, NOW_S + 3 * D, seen_ms(600))),
        )],
        vec![],
    );
    let f = run(&[("a.json", &a), ("b.json", &b)]);
    assert!(row(&f, "work").old_hosts.is_empty());
    let js = to_json(&f, NOW_MS);
    let w = js["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["alias"] == "work")
        .unwrap();
    assert_eq!(w["old_hosts"], json!([]));
}

#[test]
fn a_reset_chosen_window_never_warns_even_when_the_stale_one_would() {
    // the only fresh capture is a weekly that has reset; the old 97 % must not produce a warning
    let f = real_case();
    assert!(row(&f, "acct-65d2bffc")
        .weekly
        .as_ref()
        .unwrap()
        .warn
        .is_none());
    // a reset 5h with a live weekly: 5h shown reset, weekly still evaluated
    let s = summary(
        "host-aaaaaaaa",
        vec![acct(
            "work",
            60,
            Some((0.4, NOW_S - 5)),
            Some((0.1, NOW_S + 20 * H)),
        )],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let r = row(&f, "work");
    assert!(r.five_reset);
    assert_eq!(r.weekly.as_ref().unwrap().warn, Some(Warn::Waste));
    let t = text(&f, 100, NOW_MS, &Tr("en".into()), false);
    assert!(t.contains("5h reset"), "{t}");
}

fn resets(v: &[(&str, &str)]) -> super::names::Resets {
    super::names::Resets(
        v.iter()
            .map(|(k, n)| (k.to_string(), n.to_string()))
            .collect(),
    )
}

// NOW is Tue 2026-11-03 10:00 UTC: fri 17:00+08:00 = Fri 09:00 UTC (2d23h), thu 00:00+08:00 =
// Wed 16:00 UTC (1d6h)
const FRI_0900_UTC: i64 = 1_793_955_600;
const WED_1600_UTC: i64 = 1_793_808_000;

#[test]
fn a_reset_rule_adds_the_next_occurrence_to_json_text_and_never_replaces_a_real_reset() {
    let s = summary(
        "host-aaaaaaaa",
        vec![acct(
            "work",
            60,
            Some((0.3, NOW_S + 2 * H)),
            Some((0.4, NOW_S + 3 * D)),
        )],
        vec![],
    );
    let mut f = run(&[("a.json", &s)]);
    attach_notes(
        &mut f,
        &notes(&[("kimi", "本週已用完"), ("twgame", "週四重置")]),
    );
    attach_resets(
        &mut f,
        &resets(&[
            ("kimi", "fri@17:00+08:00"),
            ("twgame", "thu@00:00+08:00"),
            ("work", "mon@01:00+00:00"),
            ("claude", "sun@01:00+00:00"),
        ]),
        NOW_MS,
    );
    let k = row(&f, "kimi");
    assert_eq!(k.note_resets_at, Some(FRI_0900_UTC));
    assert_eq!(k.note_reset_rule.as_deref(), Some("fri@17:00+08:00"));
    assert_eq!(row(&f, "twgame").note_resets_at, Some(WED_1600_UTC));
    // the account's own key beats its provider's rule; it still keeps its real windows
    let w = row(&f, "work");
    assert_eq!(w.note_reset_rule.as_deref(), Some("mon@01:00+00:00"));
    assert_eq!(w.weekly.as_ref().unwrap().win.resets_at, NOW_S + 3 * D);
    let j = to_json(&f, NOW_MS);
    let acc = |a: &str| {
        j["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["alias"] == a)
            .unwrap()
            .clone()
    };
    assert_eq!(acc("kimi")["note_resets_at"], FRI_0900_UTC);
    assert_eq!(acc("kimi")["note_reset_rule"], "fri@17:00+08:00");
    assert_eq!(acc("work")["weekly"]["resets_at"], NOW_S + 3 * D);
    // no rule, no keys
    attach_resets(&mut f, &resets(&[]), NOW_MS);
    let j = to_json(&f, NOW_MS);
    assert!(j["accounts"][0].get("note_resets_at").is_none());
    assert!(j["accounts"][0].get("note_reset_rule").is_none());
}

#[test]
fn the_text_table_shows_a_dim_countdown_after_the_note_and_cuts_the_note_not_the_countdown() {
    let s = summary("host-aaaaaaaa", vec![], vec![]);
    let mut f = run(&[("a.json", &s)]);
    attach_notes(
        &mut f,
        &notes(&[("kimi", "本週已用完"), ("glm", &"週".repeat(20))]),
    );
    attach_resets(
        &mut f,
        &resets(&[
            ("kimi", "fri@17:00+08:00"),
            ("glm", "fri@17:00+08:00"),
            ("qoder", "thu@00:00+08:00"),
        ]),
        NOW_MS,
    );
    for cols in [100, 60, 40] {
        let txt = text(&f, cols, NOW_MS, &Tr("zh-TW".into()), false);
        for l in txt.lines() {
            assert!(UnicodeWidthStr::width(l) <= cols, "{cols}: {l:?}");
        }
        assert!(txt.contains("◷ 2d23h"), "{cols}: {txt}");
        // a rule without a note still shows its countdown
        assert!(txt.contains("◷ 1d6h"), "{cols}: {txt}");
    }
    let wide = text(&f, 100, NOW_MS, &Tr("en".into()), false);
    assert!(wide.contains("本週已用完 ◷ 2d23h"), "{wide}");
    // a cut note ends in `…`, as in forge-stats
    let narrow = text(&f, 60, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(narrow.contains("… ◷ 2d23h"), "{narrow}");
}

fn plans(v: &[(&str, &str)]) -> super::names::Plans {
    super::names::Plans(
        v.iter()
            .map(|(k, n)| (k.to_string(), n.to_string()))
            .collect(),
    )
}

#[test]
fn a_row_carries_plan_in_json_only_when_set_and_the_text_says_unknown_otherwise() {
    let s = summary(
        "host-aaaaaaaa",
        vec![
            acct(
                "work",
                60,
                Some((0.3, NOW_S + 2 * H)),
                Some((0.4, NOW_S + 3 * D)),
            ),
            acct(
                "other",
                60,
                Some((0.3, NOW_S + 2 * H)),
                Some((0.4, NOW_S + 3 * D)),
            ),
        ],
        vec![],
    );
    let mut f = run(&[("a.json", &s)]);
    attach_plans(&mut f, &plans(&[("work", "max5x"), ("kimi", "api")]));
    assert_eq!(row(&f, "work").plan, Some("max5x"));
    assert_eq!(row(&f, "other").plan, None);
    let v = to_json(&f, NOW_MS);
    let by = |a: &str| {
        v["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["alias"] == a)
            .unwrap()
            .clone()
    };
    assert_eq!(by("work")["plan"], "max5x");
    assert!(by("other").get("plan").is_none());
    assert_eq!(
        by("kimi")["plan"],
        "api",
        "a provider key applies to its rows"
    );
    for cols in [100, 80, 60, 40, 20] {
        let zh = text(&f, cols, NOW_MS, &Tr("zh-TW".into()), false);
        let en = text(&f, cols, NOW_MS, &Tr("en".into()), false);
        for l in zh.lines().chain(en.lines()) {
            assert!(UnicodeWidthStr::width(l) <= cols, "{cols}: {l:?}");
        }
        if cols >= 80 {
            assert!(zh.contains("Max 5x"), "{cols}: {zh}");
            assert!(zh.contains("未知"), "{cols}: {zh}");
            assert!(en.contains("unknown"), "{cols}: {en}");
        }
    }
}

#[test]
fn monthly_and_one_shot_rules_count_down_and_a_passed_one_shot_drops_out() {
    let s = summary("host-aaaaaaaa", vec![], vec![]);
    let mut f = run(&[("a.json", &s)]);
    attach_resets(
        &mut f,
        &resets(&[
            // 21st 00:00 +08:00 = 2026-11-20 16:00 UTC = NOW + 17d6h
            ("qodercn", "21@00:00+08:00"),
            // = 2026-11-16 16:00 UTC = NOW + 13d6h
            ("cursor", "2026-11-17T00:00:00+08:00"),
            // already passed
            ("glm", "2026-11-01T00:00:00Z"),
        ]),
        NOW_MS,
    );
    assert_eq!(
        row(&f, "qodercn").note_resets_at,
        Some(NOW_S + 17 * D + 6 * H)
    );
    assert_eq!(
        row(&f, "cursor").note_resets_at,
        Some(NOW_S + 13 * D + 6 * H)
    );
    let g = row(&f, "glm");
    assert_eq!(g.note_resets_at, None);
    let j = to_json(&f, NOW_MS);
    let acc = |a: &str| {
        j["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["provider"] == a)
            .unwrap()
            .clone()
    };
    assert_eq!(acc("qodercn")["note_reset_rule"], "21@00:00+08:00");
    assert_eq!(acc("cursor")["note_resets_at"], NOW_S + 13 * D + 6 * H);
    // the pair travels together: a passed one-shot sends neither key
    assert!(acc("glm").get("note_resets_at").is_none());
    assert!(acc("glm").get("note_reset_rule").is_none());
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("◷ 17d6h") || txt.contains("◷ 17d"), "{txt}");
    assert!(txt.contains("◷ 13d"), "{txt}");
}

#[test]
fn the_newest_reading_of_an_account_wins_whatever_os_account_or_host_and_each_window_has_its_age() {
    // 2026-11-09: aimax395-team (the team OS account) last saw acct-337c98de at 06:13 TW, 7d 11% in
    // a week resetting Mon 11-16 04:00; aimax395 (the cookys OS account) saw 63% at 20:07 in the week
    // resetting Thu 11-12 12:00. NOW = 20:08.
    let team_seen = 13 * H + 55 * 60;
    let team = summary(
        "host-aaaaaaaa",
        vec![with_org(
            acct_seen(
                "acct-337c98de",
                H + 24 * 60,
                Some((0.99, NOW_S - 12 * H - 28 * 60, seen_ms(team_seen))),
                Some((0.11, NOW_S + 6 * D + 8 * H, seen_ms(team_seen))),
            ),
            "personal",
        )],
        vec![],
    );
    let cookys = summary(
        "host-bbbbbbbb",
        vec![
            with_org(
                acct_seen(
                    "acct-337c98de",
                    60,
                    None,
                    Some((0.63, NOW_S + 2 * D + 16 * H, seen_ms(60))),
                ),
                "personal",
            ),
            // the only reading of this account is three hours old: still shown, marked stale
            with_org(
                acct_seen(
                    "acct-0000beef",
                    3 * H,
                    None,
                    Some((0.30, NOW_S + 4 * D, seen_ms(3 * H))),
                ),
                "org-b",
            ),
        ],
        vec![],
    );
    let f = run(&[("aimax395-team.json", &team), ("aimax395.json", &cookys)]);
    let r = row(&f, "acct-337c98de");
    assert_eq!(r.weekly.as_ref().unwrap().win.pct, 0.63);
    assert_eq!(r.old_hosts, vec!["aimax395-team"]);
    let v = to_json(&f, NOW_MS);
    let a = |k: &str| {
        v["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["alias"] == k)
            .unwrap()
            .clone()
    };
    let cf = a("acct-337c98de");
    assert_eq!(cf["weekly"]["age_s"], json!(60));
    assert_eq!(cf["weekly"]["stale"], json!(false));
    // the 5h window came only from the team reading, and it reset before the newer weekly reading:
    // the account was used since with no 5h reading, so no source, not the old 99 % (PM 01M70CZ6A0)
    assert!(cf["five_hour"].is_null());
    assert_eq!(cf["five_hour_state"], json!("no_source"));
    let old = a("acct-0000beef");
    assert_eq!(old["state"], json!("ok"));
    assert_eq!(old["weekly"]["age_s"], json!(3 * H));
    assert_eq!(old["weekly"]["stale"], json!(true));
}

/// PM 01M70CZ6A0: the account was used after its 5h window reset (a later weekly reading) but
/// Claude Code passed no `five_hour` since: no source, never the old 99 %.
#[test]
fn used_after_the_5h_reset_with_no_5h_reading_is_no_source_not_the_old_pct() {
    let s = summary(
        "host-aaaaaaaa",
        vec![
            acct_seen(
                "used",
                60,
                Some((0.99, NOW_S - 15 * H, seen_ms(16 * H))),
                Some((0.67, NOW_S + 3 * D, seen_ms(60))),
            ),
            // idle since before the reset: still only reset
            acct_seen(
                "idle",
                60,
                Some((0.99, NOW_S - H, seen_ms(2 * H))),
                Some((0.67, NOW_S + 3 * D, seen_ms(3 * H))),
            ),
        ],
        vec![],
    );
    let f = run(&[("a.json", &s)]);
    let u = row(&f, "used");
    assert_eq!(u.state, State::Ok);
    assert!(u.five_no_source && u.five_hour.is_none() && !u.five_reset);
    let i = row(&f, "idle");
    assert!(!i.five_no_source && i.five_reset && i.five_hour.is_some());
    let js = to_json(&f, NOW_MS);
    let get = |n: &str| {
        js["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["alias"] == n)
            .unwrap()
            .clone()
    };
    let a = get("used");
    assert!(a["five_hour"].is_null());
    assert_eq!(a["five_hour_state"], json!("no_source"));
    assert!(get("idle").get("five_hour_state").is_none());
    let t = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(t.contains("5h 無來源"), "{t}");
    let line = t
        .lines()
        .filter(|l| l.contains("used"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!line.contains("99%"), "{t}");
}
