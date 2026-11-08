use super::fleet_quota::*;
use super::quota_view::Tr;
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
    merge(&ok, skipped, NOW_MS, DEFAULT_USE_TARGET)
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
    let f = merge(&ok, skipped, NOW_MS, DEFAULT_USE_TARGET);
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
    assert!(h.five_hour.is_none() && h.weekly.is_some());
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("已重置，等新觀察"), "{txt}");
}

#[test]
fn codex_only_alias_and_fixed_providers_are_no_source() {
    let s = summary("host-aaaaaaaa", vec![], vec![day("cx", "codex")]);
    let f = run(&[("box.json", &s)]);
    let r = row(&f, "cx");
    assert_eq!(r.state, State::NoSource);
    assert_eq!(r.provider, "codex");
    assert_eq!(r.hosts, vec!["box"]);
    for p in NO_SOURCE_PROVIDERS {
        assert_eq!(row(&f, p).state, State::NoSource);
    }
    let txt = text(&f, 100, NOW_MS, &Tr("zh-TW".into()), false);
    assert!(txt.contains("無來源"), "{txt}");
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
    let f = merge(&ok, skipped, NOW_MS, DEFAULT_USE_TARGET);
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
    for (tr, personal, more) in [("zh-TW", "個人", "等 "), ("en", "personal", "more")] {
        let txt = text(&f, 80, NOW_MS, &Tr(tr.into()), false);
        let head = txt.lines().find(|l| l.contains("acct-ae88ee5d")).unwrap();
        assert!(UnicodeWidthStr::width(head) <= 80, "{head:?}");
        assert!(head.contains(personal), "{head:?}");
        assert!(
            head.contains("4124gs") && head.contains(more),
            "{tr}: {head:?}"
        );
        let pos = |s: &str| head.find(s).unwrap();
        assert!(
            pos("acct-ae88ee5d") < pos(personal)
                && pos(personal) < pos("claude")
                && pos("claude") < pos("4124gs")
        );
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
