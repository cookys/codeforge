//! G40 reader + `usage accounts` + chips tests (temp dirs; `now` is fixed).

use super::quota_mirror_read::*;
use super::quota_view::*;
use serde_json::{json, Value};
use std::path::Path;

const N: i64 = 1_793_700_000;
const DAY: i64 = 86400;

fn tr() -> Tr {
    Tr("zh-TW".to_string())
}

fn win(kind: &str, util: Value, observed_at: i64, o: Value) -> Value {
    let mut w = json!({"kind": kind, "utilization": util, "used": null, "limit": null, "unit": null,
        "resets_at": N + 7500, "source": "passive", "observed_at": observed_at});
    for (k, v) in o.as_object().unwrap() {
        w[k.as_str()] = v.clone();
    }
    w
}
fn rec(provider: &str, alias: &str, vis: &str, o: Value) -> Value {
    let mut r = json!({"schema": "fleet.quota/1", "provider": provider, "alias": alias, "visibility": vis,
        "windows": [], "refusal": null, "token_state": "unknown", "token_state_at": null, "host_role": "agent",
        "token_issued_at": null, "token_expires_at": null});
    for (k, v) in o.as_object().unwrap() {
        r[k.as_str()] = v.clone();
    }
    r
}
fn local_row(record: Value) -> Value {
    json!({"writer_courier": "local", "host_role": "agent", "observed_at_hub": null, "skewed": false, "record": record})
}
fn view_text(rows: Vec<Value>) -> String {
    json!({"schema": "fleet.quota-view/1", "namespace": "cookys", "hub_offset": 0, "rows": rows})
        .to_string()
}
fn put(live: &Path, provider: &str, name: &str, text: &str) {
    let d = live.join("quota").join(provider);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(name), text).unwrap();
}
fn put_view(live: &Path, provider: &str, alias: &str, record: Value) {
    put(
        live,
        provider,
        &format!("{alias}.json"),
        &view_text(vec![local_row(record)]),
    );
}

/// One alias per state.
fn multi_state(live: &Path) {
    let ob = N - 60;
    put_view(
        live,
        "claude",
        "work",
        rec(
            "claude",
            "work",
            "L3",
            json!({
                "windows": [win("rolling_5h", json!(0.62), ob, json!({"resets_at": N + 7500})), win("weekly", json!(0.31), ob, json!({"resets_at": N + 3 * DAY + 4 * 3600}))],
                "token_state": "ok", "token_state_at": N - 1000, "token_expires_at": N + 214 * DAY
            }),
        ),
    );
    put_view(
        live,
        "claude",
        "home",
        rec(
            "claude",
            "home",
            "L3",
            json!({
                "windows": [win("rolling_5h", json!(0.12), ob, json!({"resets_at": N + 4 * 3600 + 2400})), win("weekly", json!(0.91), ob, json!({"resets_at": N + DAY + 2 * 3600}))],
                "token_state": "ok", "token_state_at": N - 1000, "token_expires_at": N + 6 * DAY
            }),
        ),
    );
    put_view(
        live,
        "codex",
        "main",
        rec(
            "codex",
            "main",
            "L3",
            json!({
                "windows": [win("weekly", json!(0.17), N - 7800, json!({"resets_at": N + 3 * DAY})),
                    win("credits", Value::Null, ob, json!({"remaining": 12.5, "unit": "credits", "resets_at": null}))]
            }),
        ),
    );
    put_view(
        live,
        "glm",
        "main",
        rec(
            "glm",
            "main",
            "L2",
            json!({
                "windows": [win("daily", json!(0.41), ob, json!({"resets_at": null, "source": "estimated"}))]
            }),
        ),
    );
    put_view(
        live,
        "qoder",
        "main",
        rec(
            "qoder",
            "main",
            "L2",
            json!({
                "windows": [win("credits", json!(0.38), ob, json!({"resets_at": null, "used": 1900.0, "limit": 5000.0, "unit": "credits"}))]
            }),
        ),
    );
    put_view(
        live,
        "kimi",
        "main",
        rec(
            "kimi",
            "main",
            "L1",
            json!({
                "refusal": {"observed_at": N - 600, "retry_at": N + 4800, "reason": "rate_limit"}
            }),
        ),
    );
    put_view(
        live,
        "agy",
        "main",
        rec("agy", "main", "L0", json!({"source": "registry"})),
    );
    put_view(
        live,
        "minimax",
        "main",
        rec(
            "minimax",
            "main",
            "L3",
            json!({
                "windows": [win("weekly", json!(0.5), N - 7 * 3600, json!({"resets_at": N + DAY}))]
            }),
        ),
    );
    put_view(
        live,
        "gone",
        "tok",
        rec(
            "gone",
            "tok",
            "L0",
            json!({"token_state": "invalid", "token_state_at": N - 100}),
        ),
    );
    put(live, "claude", "broken.json", "{not json");
}

fn state_of(live: &Path) -> (QuotaState, Vec<QEntry>) {
    let st = read_mirror(live);
    let e = quota_entries(&st, N);
    (st, e)
}

// ---- resolver ----

fn env(over: Option<&str>, xdg: Option<&str>, home: Option<&str>) -> QuotaEnv {
    QuotaEnv {
        autopilot_live_dir: over.map(String::from),
        xdg_runtime_dir: xdg.map(String::from),
        home: home.map(String::from),
    }
}
fn dirs(e: &QuotaEnv, uid: Option<u32>) -> Vec<String> {
    live_candidates(e, uid)
        .into_iter()
        .map(|(d, _)| d.display().to_string())
        .collect()
}

#[test]
fn resolver_order_matches_the_mod() {
    assert_eq!(
        dirs(&env(Some("/o/"), Some("/x"), Some("/h")), Some(7)),
        ["/o"]
    );
    assert_eq!(
        dirs(&env(None, Some("/x/"), Some("/h/")), Some(7)),
        ["/x/autopilot", "/dev/shm/autopilot-7", "/h/.autopilot"]
    );
    assert_eq!(
        dirs(&env(None, None, Some("/h")), Some(7)),
        [
            "/run/user/7/autopilot",
            "/dev/shm/autopilot-7",
            "/h/.autopilot"
        ]
    );
    assert_eq!(dirs(&env(None, None, Some("/h")), None), ["/h/.autopilot"]);
    // never a /tmp candidate
    assert!(!dirs(&env(None, None, None), Some(1))
        .iter()
        .any(|d| d.starts_with("/tmp")));
}

#[test]
fn resolver_picks_the_first_existing_but_never_checks_override_or_fallback() {
    let e = env(None, None, Some("/h"));
    let got = live_dir(&e, Some(7), |d| d == Path::new("/dev/shm/autopilot-7"));
    assert_eq!(got.unwrap(), Path::new("/dev/shm/autopilot-7"));
    let got = live_dir(&e, Some(7), |_| false);
    assert_eq!(got.unwrap(), Path::new("/h/.autopilot"));
    assert_eq!(live_dir(&env(None, None, None), None, |_| true), None);
    let o = env(Some("/nowhere"), None, None);
    assert_eq!(
        live_dir(&o, Some(7), |_| false).unwrap(),
        Path::new("/nowhere")
    );
}

// ---- reader ----

#[test]
fn no_quota_dir_is_not_present_and_loads_none() {
    let t = tempfile::tempdir().unwrap();
    assert!(!read_mirror(t.path()).present);
    assert!(load_from(t.path(), N).is_none());
    assert!(load_from(&t.path().join("missing"), N).is_none());
    std::fs::create_dir(t.path().join("quota")).unwrap();
    assert_eq!(load_from(t.path(), N), Some(vec![]));
}

#[cfg(unix)]
#[test]
fn symlinked_view_and_symlinked_quota_dir_are_rejected() {
    let t = tempfile::tempdir().unwrap();
    let live = t.path().join("live");
    put_view(
        &live,
        "claude",
        "real",
        rec("claude", "real", "L0", json!({"source": "registry"})),
    );
    let d = live.join("quota/claude");
    std::os::unix::fs::symlink(d.join("real.json"), d.join("link.json")).unwrap();
    let st = read_mirror(&live);
    assert_eq!(st.raw.len(), 1);
    assert_eq!(st.bad, 1);
    // the quota dir itself a link: absent
    let other = t.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::os::unix::fs::symlink(live.join("quota"), other.join("quota")).unwrap();
    assert!(!read_mirror(&other).present);
}

#[cfg(unix)]
#[test]
fn open_nofollow_rejects_a_link_even_when_listed_as_a_file() {
    // read_capped is what closes the race: a link is refused at open time
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("a"), "x").unwrap();
    std::os::unix::fs::symlink(t.path().join("a"), t.path().join("b")).unwrap();
    assert!(matches!(
        super::quota_mirror_read::read_capped_for_test(&t.path().join("b"), 10),
        Err(true)
    ));
    assert!(super::quota_mirror_read::read_capped_for_test(&t.path().join("a"), 10).is_ok());
    assert!(matches!(
        super::quota_mirror_read::read_capped_for_test(&t.path().join("a"), 0),
        Err(true)
    ));
    assert!(matches!(
        super::quota_mirror_read::read_capped_for_test(&t.path().join("zz"), 10),
        Err(false)
    ));
}

#[test]
fn oversize_view_is_rejected_and_counted() {
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "claude", "big.json", &" ".repeat(32 * 1024 + 1));
    let st = read_mirror(t.path());
    assert_eq!((st.raw.len(), st.bad), (0, 1));
}

#[test]
fn skip_rules_lock_dot_non_json_and_bad_names() {
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "claude", ".work.json.tmp", "x");
    put(t.path(), "claude", ".hidden.json", "x");
    put(t.path(), "claude", "work.json.lock", "x");
    put(t.path(), "claude", "notes.txt", "x");
    put(t.path(), "claude", "Bad_Alias.json", "x");
    put(t.path(), "Bad_Provider", "a.json", "x");
    let st = read_mirror(t.path());
    assert_eq!((st.raw.len(), st.bad, st.skipped), (0, 1, 0));
}

#[test]
fn namespace_json_is_metadata_only_at_the_top() {
    let t = tempfile::tempdir().unwrap();
    // inside a provider dir, `namespace` is an alias
    put_view(
        t.path(),
        "claude",
        "namespace",
        rec("claude", "namespace", "L0", json!({"source": "registry"})),
    );
    let st = read_mirror(t.path());
    assert_eq!(st.raw.len(), 1);
    assert_eq!(st.raw[0].alias, "namespace");
}

#[test]
fn namespace_guard_peer_rows_only_with_equal_namespaces() {
    let t = tempfile::tempdir().unwrap();
    let r = rec(
        "claude",
        "work",
        "L3",
        json!({"windows": [win("weekly", json!(0.4), N - 60, json!({"resets_at": N + 3600}))]}),
    );
    let peer = json!({"writer_courier": "cookys/desk/peer", "host_role": "desk", "observed_at_hub": N, "skewed": false, "record": r});
    put(t.path(), "claude", "work.json", &view_text(vec![peer]));
    // no namespace.json: the peer row is not used, the view is no entry
    assert!(quota_entries(&read_mirror(t.path()), N).is_empty());
    std::fs::write(
        t.path().join("quota/namespace.json"),
        json!({"schema": "fleet.quota-ns/1", "namespace": "other"}).to_string(),
    )
    .unwrap();
    assert!(quota_entries(&read_mirror(t.path()), N).is_empty());
    std::fs::write(
        t.path().join("quota/namespace.json"),
        json!({"schema": "fleet.quota-ns/1", "namespace": "cookys"}).to_string(),
    )
    .unwrap();
    let st = read_mirror(t.path());
    assert_eq!(st.ns.as_deref(), Some("cookys"));
    assert_eq!(quota_entries(&st, N).len(), 1);
    // a wrong-schema namespace file is no namespace
    std::fs::write(
        t.path().join("quota/namespace.json"),
        json!({"schema": "x", "namespace": "cookys"}).to_string(),
    )
    .unwrap();
    assert_eq!(read_mirror(t.path()).ns, None);
}

#[test]
fn caps_64_providers_and_256_views() {
    let t = tempfile::tempdir().unwrap();
    for i in 0..66 {
        std::fs::create_dir_all(t.path().join(format!("quota/p{i:02}"))).unwrap();
    }
    assert_eq!(read_mirror(t.path()).skipped, 2);
    let u = tempfile::tempdir().unwrap();
    // 6 providers x 45 views = 270 -> 256 read, 14 counted as not read
    for p in 0..6 {
        for a in 0..45 {
            let (pn, an) = (format!("p{p}"), format!("a{a:02}"));
            put_view(
                u.path(),
                &pn,
                &an,
                rec(&pn, &an, "L0", json!({"source": "registry"})),
            );
        }
    }
    let st = read_mirror(u.path());
    assert_eq!((st.raw.len(), st.skipped, st.bad), (256, 14, 0));
}

#[test]
fn a_bad_row_in_a_good_view_is_counted() {
    let t = tempfile::tempdir().unwrap();
    let good = local_row(rec("claude", "work", "L0", json!({"source": "registry"})));
    put(
        t.path(),
        "claude",
        "work.json",
        &view_text(vec![good, json!({"junk": 1})]),
    );
    let st = read_mirror(t.path());
    assert_eq!((st.raw.len(), st.bad), (1, 1));
}

// ---- usage accounts text ----

fn text_at(live: &Path, cols: usize) -> String {
    let (st, e) = state_of(live);
    accounts_text(&st, &e, cols, N, &tr(), false)
}

#[test]
fn accounts_text_width_100_every_state() {
    let t = tempfile::tempdir().unwrap();
    multi_state(t.path());
    let s = text_at(t.path(), 100);
    eprintln!("{s}");
    for l in s.lines() {
        assert!(
            unicode_width::UnicodeWidthStr::width(l) <= 100,
            "too wide: {l}"
        );
    }
    let has = |x: &str| assert!(s.contains(x), "missing {x:?} in\n{s}");
    has("配額檢視");
    has("claude:work  L3");
    has("5h    ▰▰▰▰▰▰▱▱▱▱  62%  ◷ 2h5m 後重置");
    has("週    ▰▰▰▱▱▱▱▱▱▱  31%  ◷ 3d4h 後重置");
    has("token 到期 214 天");
    has("▲ token 到期 6 天");
    has("91% 緊");
    has("額度  剩 12.50 credits"); // remaining-only credits: no meter, no %
    has("估 41%");
    has("重置時間未知");
    has("剩 3,100 credits");
    has("429 冷卻");
    has("後重試");
    has("agy:main  L0");
    has("從未觀察");
    has("過期或已重置，等新觀察");
    has("gone:tok");
    has("token 已失效");
    has("1 筆無法讀取");
    has("估 = 本機計數，不是供應商回報；緊 = 已用 ≥ 90%");
    // block order: token flag first, then tight, 429, rest, never/dropped last
    let pos = |k: &str| s.find(k).unwrap();
    assert!(pos("claude:home") < pos("gone:tok"));
    assert!(pos("gone:tok") < pos("kimi:main"));
    assert!(pos("kimi:main") < pos("claude:work"));
    assert!(pos("claude:work") < pos("agy:main"));
    // the age of the stale codex window shows (> 30 min), the fresh ones do not
    assert!(s.contains("2h10m 前"));
    assert_eq!(s.matches(" 前").count(), 1);
    // credits remaining-only draws no gauge
    let credit_line = s.lines().find(|l| l.contains("額度  剩 12.50")).unwrap();
    assert!(!credit_line.contains('▰') && !credit_line.contains('%'));
}

#[test]
fn accounts_text_width_70_drops_age_then_reset_word() {
    let t = tempfile::tempdir().unwrap();
    multi_state(t.path());
    let s = text_at(t.path(), 70);
    for l in s.lines() {
        assert!(
            unicode_width::UnicodeWidthStr::width(l) <= 70,
            "too wide: {l}"
        );
    }
    assert!(s.contains("claude:work"));
    assert!(s.contains("62%"));
}

#[test]
fn accounts_text_narrow_rules_by_width() {
    // one codex weekly window, 2h10m old: age shows at >= 80 columns only
    let t = tempfile::tempdir().unwrap();
    put_view(
        t.path(),
        "codex",
        "main",
        rec(
            "codex",
            "main",
            "L3",
            json!({"windows": [win("weekly", json!(0.17), N - 7800, json!({"resets_at": N + 3 * DAY}))]}),
        ),
    );
    let line_of = |cols: usize| {
        text_at(t.path(), cols)
            .lines()
            .find(|l| l.contains("週"))
            .unwrap()
            .to_string()
    };
    for cols in [100, 80] {
        let l = line_of(cols);
        assert!(
            l.contains("2h10m 前") && l.contains("後重置") && l.contains('▰'),
            "{l}"
        );
    }
    for cols in [79, 70, 60] {
        let l = line_of(cols);
        assert!(
            !l.contains("前") && l.contains("後重置") && l.contains('▰'),
            "{l}"
        );
    }
    // below 60 a block is one line
    let s = text_at(t.path(), 59);
    assert_eq!(s.lines().filter(|l| l.contains("codex:main")).count(), 1);
    assert!(!s.contains("  週"));
}

#[test]
fn accounts_text_width_50_is_one_line_per_alias() {
    let t = tempfile::tempdir().unwrap();
    multi_state(t.path());
    let s = text_at(t.path(), 50);
    for l in s.lines() {
        assert!(
            unicode_width::UnicodeWidthStr::width(l) <= 50,
            "too wide: {l}"
        );
    }
    let body: Vec<&str> = s.lines().skip(2).collect();
    for key in [
        "claude:work",
        "claude:home",
        "codex:main",
        "glm:main",
        "kimi:main",
        "agy:main",
        "minimax:main",
        "gone:tok",
    ] {
        assert_eq!(body.iter().filter(|l| l.contains(key)).count(), 1, "{key}");
    }
    let l = |k: &str| body.iter().find(|l| l.contains(k)).unwrap().to_string();
    assert!(l("claude:home").contains("▲") && l("claude:home").contains("週 91% 緊"));
    assert!(l("kimi:main").contains("429 冷卻"));
    assert!(l("agy:main").contains("無資料"));
    assert!(l("glm:main").contains("估 41%"));
}

#[test]
fn accounts_empty_and_missing_mirror_say_so() {
    let t = tempfile::tempdir().unwrap();
    let s = text_at(t.path(), 100);
    assert!(
        s.contains("尚未收到配額資料（由 autopilot 的 reporter／wrapper 寫入）"),
        "{s}"
    );
    let st = QuotaState::default();
    let s = accounts_text(&st, &[], 100, N, &tr(), false);
    assert!(s.contains("尚未收到配額資料"));
}

#[test]
fn accounts_english_words_come_from_the_locale_file() {
    let t = tempfile::tempdir().unwrap();
    multi_state(t.path());
    let (st, e) = state_of(t.path());
    let s = accounts_text(&st, &e, 100, N, &Tr("en".into()), false);
    assert!(
        s.contains("429 cooling") && s.contains("tight") && s.contains("token expires in 6 d"),
        "{s}"
    );
}

#[test]
fn accounts_color_only_warns_the_flag() {
    let t = tempfile::tempdir().unwrap();
    multi_state(t.path());
    let (st, e) = state_of(t.path());
    let s = accounts_text(&st, &e, 100, N, &tr(), true);
    assert!(s.contains("\x1b[33m▲\x1b[0m"));
    // the tight word carries no colour of its own
    assert!(!s.contains("\x1b[33m緊"));
    assert_eq!(s.matches("\x1b[33m").count(), 2); // claude:home token + gone:tok
}

#[test]
fn accounts_json_shape() {
    let t = tempfile::tempdir().unwrap();
    multi_state(t.path());
    let (st, e) = state_of(t.path());
    let v = accounts_json(&st, &e, N, &tr());
    assert_eq!(v["schema"], "codeforge.usage-accounts/1");
    assert_eq!(v["unreadable"], 1);
    assert_eq!(v["skipped"], 0);
    let list = v["entries"].as_array().unwrap();
    assert_eq!(list.len(), 9);
    assert_eq!(list[0]["alias"], "home"); // display order: token flag first, by key
    let gone = list.iter().find(|x| x["provider"] == "gone").unwrap();
    assert_eq!(gone["state"], "token_only");
    assert_eq!(gone["token_state"], "invalid");
    let home = list.iter().find(|x| x["alias"] == "home").unwrap();
    assert_eq!(home["level"], "L3");
    assert_eq!(home["tight"], true);
    assert_eq!(home["token_warn"]["level"], "urgent");
    let w = &home["windows"][1];
    assert_eq!(w["kind"], "weekly");
    assert!((w["utilization"].as_f64().unwrap() - 0.91).abs() < 1e-9);
    assert!(w["resets_at_local"].is_number() && w["age_s"].is_number());
    assert_eq!(w["estimated"], false);
    let glm = list.iter().find(|x| x["provider"] == "glm").unwrap();
    assert_eq!(glm["windows"][0]["estimated"], true);
    assert_eq!(glm["level"], "L2");
    let kimi = list.iter().find(|x| x["provider"] == "kimi").unwrap();
    assert_eq!(kimi["state"], "refusal");
    assert_eq!(kimi["refusal"]["reason"], "rate_limit");
    // the dropped alias is listed as such, not as a window
    let mm = list.iter().find(|x| x["provider"] == "minimax").unwrap();
    assert_eq!(mm["state"], "dropped");
    assert_eq!(mm["windows"].as_array().unwrap().len(), 0);
}

// ---- chips ----

#[test]
fn band_chips_order_and_words() {
    let t = tempfile::tempdir().unwrap();
    multi_state(t.path());
    let (_, e) = state_of(t.path());
    let b = quota_band(&e, N, &tr());
    let heads: Vec<String> = b
        .chips
        .iter()
        .map(|c| format!("{} {}", c.key, c.head))
        .collect();
    assert_eq!(heads[0], "claude:home token 到期 6 天");
    assert_eq!(heads[1], "gone:tok token 已失效");
    assert_eq!(heads[2], "claude:home 週 91% 緊");
    assert!(heads.iter().any(|h| h == "kimi:main 429 冷卻"));
    assert!(heads.iter().any(|h| h == "claude:work 5h 62%"));
    assert!(heads.iter().any(|h| h == "glm:main 日 估 41%"));
    assert!(heads.iter().any(|h| h == "qoder:main 額度 剩 3,100"));
    assert!(heads.iter().any(|h| h == "codex:main 週 17%"));
    assert_eq!(b.no_data, 2); // agy (never) + minimax (dropped)
    let forms = band_forms(&b);
    let first = plain(&forms[0]);
    assert!(first.ends_with("無資料 2"), "{first}");
    assert!(first.contains("▲ gone:tok token 已失效") && first.starts_with("▲ claude:home"));
    assert!(first.contains("claude:work 5h 62% ◷ 2h5m"));
    // widest first, strictly narrowing in the tail
    let widths: Vec<usize> = forms.iter().map(|f| width(f)).collect();
    assert!(widths.windows(2).all(|w| w[0] >= w[1]), "{widths:?}");
    // the last form is the single most severe chip, alias only
    assert_eq!(plain(forms.last().unwrap()), "▲ home token 到期 6 天");
}

#[test]
fn band_alias_collision_keeps_provider_prefix() {
    let t = tempfile::tempdir().unwrap();
    let w = |u: f64| json!({"windows": [win("weekly", json!(u), N - 60, json!({"resets_at": N + 3600}))]});
    put_view(
        t.path(),
        "claude",
        "main",
        rec("claude", "main", "L3", w(0.3)),
    );
    put_view(
        t.path(),
        "codex",
        "main",
        rec("codex", "main", "L3", w(0.2)),
    );
    put_view(t.path(), "kimi", "solo", rec("kimi", "solo", "L3", w(0.1)));
    let (_, e) = state_of(t.path());
    let forms = band_forms(&quota_band(&e, N, &tr()));
    let all: Vec<String> = forms.iter().map(|f| plain(f)).collect();
    assert!(all
        .iter()
        .any(|f| f.contains("solo 週 10%") && !f.contains("kimi:solo")));
    assert!(all
        .iter()
        .all(|f| !f.contains("main 週") || f.contains("claude:main") || f.contains("codex:main")));
}

#[test]
fn no_chips_for_an_empty_mirror() {
    let b = quota_band(&[], N, &tr());
    assert!(b.chips.is_empty() && band_forms(&b).is_empty());
}

/// review g1 5f5a8af7: below AGE_MIN_COLS the compact one-line form never shows the age, even where it would fit.
#[test]
fn compact_form_below_60_never_shows_the_age() {
    let t = tempfile::tempdir().unwrap();
    put_view(
        t.path(),
        "codex",
        "main",
        rec(
            "codex",
            "main",
            "L3",
            json!({"windows": [win("weekly", json!(0.17), N - 7800, json!({"resets_at": N + 3 * DAY}))]}),
        ),
    );
    for cols in [59, 55, 50] {
        let s = text_at(t.path(), cols);
        let l = s.lines().find(|l| l.contains("codex:main")).unwrap();
        assert!(!l.contains("前"), "age at {cols}: {l}");
    }
}

/// review g1 6d419bf9: no rendered line is ever wider than the terminal, at any width down to 1, including
/// the title, the empty-mirror message, the footers and an alias longer than the terminal.
#[test]
fn no_line_is_wider_than_the_terminal_at_any_width() {
    let multi = tempfile::tempdir().unwrap();
    multi_state(multi.path());
    // a 24-character provider and alias: `provider:alias` is 49 columns
    let long = "a23456789012345678901234";
    put_view(
        multi.path(),
        long,
        long,
        rec(
            long,
            long,
            "L3",
            json!({"windows": [win("rolling_5h", json!(0.5), N - 60, json!({"resets_at": N + 3600}))]}),
        ),
    );
    let empty = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(empty.path().join("quota")).unwrap();
    for dir in [multi.path(), empty.path()] {
        for cols in 1..=100 {
            for l in text_at(dir, cols).lines() {
                let w = unicode_width::UnicodeWidthStr::width(l);
                assert!(w <= cols, "{w} > {cols}: {l:?}");
            }
        }
    }
}

#[test]
fn clip_cuts_whole_characters_and_marks_the_cut() {
    use crate::usage::quota_view::{clip, plain, Part, Style};
    let l = vec![Part {
        text: "配額檢視 abc".into(),
        style: Style::Plain,
    }];
    assert_eq!(plain(&clip(&l, 100)), "配額檢視 abc");
    // 5 columns: 4 for content (two CJK characters), 1 for the ellipsis
    assert_eq!(plain(&clip(&l, 5)), "配額…");
    // an odd budget never splits a wide character
    assert_eq!(plain(&clip(&l, 4)), "配…");
    assert_eq!(plain(&clip(&l, 1)), "…");
    assert!(clip(&l, 0).is_empty());
}

// ---- B45: shape c (token state only) is "no usage data", not "expired or reset" ----

#[test]
fn shape_c_record_says_no_usage_data_while_an_aged_out_record_keeps_dropped() {
    let t = tempfile::tempdir().unwrap();
    put_view(
        t.path(),
        "tok",
        "only",
        rec(
            "tok",
            "only",
            "L0",
            json!({"token_state": "ok", "token_state_at": N - 100}),
        ),
    );
    put_view(
        t.path(),
        "aged",
        "out",
        rec(
            "aged",
            "out",
            "L3",
            json!({"windows": [win("rolling_5h", json!(0.4), N - 25200, json!({"resets_at": N + 3000}))]}),
        ),
    );
    let (st, e) = state_of(t.path());
    let s = accounts_text(&st, &e, 100, N, &tr(), false);
    let line_after = |k: &str| {
        let mut it = s.lines().skip_while(|l| !l.starts_with(k));
        it.next();
        it.next().unwrap().to_string()
    };
    assert_eq!(line_after("tok:only"), "  沒有用量資料");
    assert_eq!(line_after("aged:out"), "  過期或已重置，等新觀察");
    let en = accounts_text(&st, &e, 100, N, &Tr("en".to_string()), false);
    assert!(en.contains("no usage data"), "{en}");
    // compact line and the band both stay on the short neutral word
    let narrow = accounts_text(&st, &e, 40, N, &tr(), false);
    assert!(narrow
        .lines()
        .any(|l| l.starts_with("tok:only") && l.contains("無資料")));
    assert_eq!(quota_band(&e, N, &tr()).no_data, 2);
    let v = accounts_json(&st, &e, N, &tr());
    let list = v["entries"].as_array().unwrap();
    let state_of_provider = |p: &str| {
        list.iter().find(|x| x["provider"] == p).unwrap()["state"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(state_of_provider("tok"), "no_usage");
    assert_eq!(state_of_provider("aged"), "dropped");
}
