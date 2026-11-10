use super::quota_view::Tr;
use super::usage_v1::*;

const NOW: i64 = 1_793_700_000_000;
const H: i64 = 3_600_000;

fn fx(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/usage-v1/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn en() -> Tr {
    Tr("en".into())
}
fn lines(name: &str, now: i64) -> Vec<String> {
    render_lines(&parse(&fx(name)).unwrap(), now, &en())
}
fn pool_line<'a>(l: &'a [String], key: &str) -> &'a str {
    l.iter()
        .find(|x| x.starts_with(key))
        .unwrap_or_else(|| panic!("no {key} in {l:?}"))
}

#[test]
fn stale_never_renders_like_fresh() {
    let l = lines("stale.json", NOW);
    let c = pool_line(&l, "cookys");
    assert!(c.contains("5h 42% (old reading, 3h ago)"), "{c}");
    assert!(c.contains("7d 31% (old reading, 3h ago)"), "{c}");
    assert!(!c.contains("resets"), "{c}");
}

#[test]
fn stale_zh_has_old_marker() {
    let l = render_lines(&parse(&fx("stale.json")).unwrap(), NOW, &Tr("zh-TW".into()));
    assert!(pool_line(&l, "cookys").contains("5h 42% (舊讀數，3h 前)"));
}

#[test]
fn reset_passed_renders_no_pct() {
    let l = lines("reset_passed.json", NOW);
    let c = pool_line(&l, "cookys");
    assert!(c.contains("5h reset, waiting for a new reading"), "{c}");
    assert!(c.contains("7d reset, waiting for a new reading"), "{c}");
    assert!(!c.contains('%'), "{c}");
}

#[test]
fn fresh_shows_pct_reset_and_countdown() {
    let l = lines("normal.json", NOW);
    let c = pool_line(&l, "cookys");
    assert!(c.contains("5h 42% resets "), "{c}");
    assert!(c.contains("(in 1h20m)"), "{c}");
    assert!(!c.contains("old reading"));
}

#[test]
fn local_recompute_from_old_fixture() {
    // normal.json read 3h later: 5h resets_at (NOW+80m) has passed, 7d readings are > 2h old.
    let l = lines("normal.json", NOW + 3 * H);
    let c = pool_line(&l, "cookys");
    assert!(c.contains("5h reset, waiting for a new reading"), "{c}");
    assert!(c.contains("7d 31% (old reading, 3h ago)"), "{c}");
}

#[test]
fn hub_stale_flag_is_never_cleared() {
    let s = parse(&fx("stale.json")).unwrap();
    let j = to_json(&s, NOW);
    assert_eq!(j["pools"][0]["five_hour"]["state"], "stale");
    // the 5h reading is only 3h old here but also flagged; with now = seen time it is still stale
    let j = to_json(&s, NOW - 3 * H);
    assert_eq!(j["pools"][0]["five_hour"]["state"], "stale");
    assert_eq!(j["pools"][0]["seven_day"]["state"], "fresh");
}

#[test]
fn missing_field_window_is_unknown_rest_renders() {
    let l = lines("missing_fields.json", NOW);
    let c = pool_line(&l, "cookys");
    assert!(c.contains("5h —"), "{c}");
    assert!(c.contains("7d 31% resets"), "{c}");
    let s = pool_line(&l, "acct:acct-99887766");
    assert!(s.contains("5h —"), "{s}");
    assert!(s.contains("7d 20% resets"), "{s}");
}

#[test]
fn placeholder_pool_shows_reason() {
    let l = lines("normal.json", NOW);
    let c = pool_line(&l, "acct:acct-c0dec0de");
    assert!(c.contains("codex"));
    assert!(c.contains("5h — (no reader for codex yet)"), "{c}");
}

#[test]
fn bad_schema_or_pools_is_error() {
    assert!(parse(br#"{"schema":"usage.v2","pools":[]}"#).is_err());
    assert!(parse(br#"{"pools":[]}"#).is_err());
    assert!(parse(br#"{"schema":"usage.v1","pools":{}}"#).is_err());
    assert!(parse(b"not json").is_err());
    assert!(parse(br#"{"schema":"usage.v1","pools":[]}"#).is_ok());
}

#[test]
fn oversize_input_is_error() {
    let mut b = br#"{"schema":"usage.v1","pools":[],"x":""#.to_vec();
    b.extend(std::iter::repeat_n(b'a', 300 * 1024));
    b.extend_from_slice(br#""}"#);
    assert!(parse(&b).is_err());
}

#[test]
fn unknown_extra_keys_accepted() {
    // normal.json carries a top-level future_field
    assert!(parse(&fx("normal.json")).is_ok());
}

#[test]
fn stale_source_is_printed() {
    let l = lines("stale.json", NOW);
    assert!(
        l.iter()
            .any(|x| x.contains("hangar.claude-usage-digest/1") && x.contains("old")),
        "{l:?}"
    );
    let l = lines("normal.json", NOW);
    assert!(!l.iter().any(|x| x.starts_with("source")), "{l:?}");
}

#[test]
fn json_view_shape_and_no_pct_when_reset() {
    let j = to_json(&parse(&fx("reset_passed.json")).unwrap(), NOW);
    let w = &j["pools"][0]["five_hour"];
    assert_eq!(w["state"], "reset_passed");
    assert!(w["pct"].is_null());
    assert_eq!(w["reset_passed"], true);
    assert_eq!(j["pools"][0]["id"], "cookys");
}

#[test]
fn flag_off_gives_off_message() {
    assert_eq!(configured(None), None);
    assert_eq!(configured(Some("")), None);
    assert_eq!(configured(Some("/x/y.json")).as_deref(), Some("/x/y.json"));
    assert!(off_message(&en()).contains("CODEFORGE_USAGE_V1"));
}

#[test]
fn file_source_loads() {
    let p = format!(
        "{}/tests/fixtures/usage-v1/normal.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let b = fetch(&p, None).unwrap();
    assert!(parse(&b).is_ok());
}

#[test]
fn http_source_sends_bearer_from_token_file() {
    use std::io::{Read, Write};
    let body = fx("normal.json");
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let t = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut req = Vec::new();
        let mut buf = [0u8; 1024];
        while !req.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = s.read(&mut buf).unwrap();
            req.extend_from_slice(&buf[..n]);
        }
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        s.write_all(head.as_bytes()).unwrap();
        s.write_all(&body).unwrap();
        String::from_utf8_lossy(&req).to_string()
    });
    let d = tempfile::tempdir().unwrap();
    let tf = d.path().join("tok");
    std::fs::write(&tf, "s3cret-token\n").unwrap();
    let got = fetch(&format!("http://127.0.0.1:{port}/v1/usage"), Some(&tf)).unwrap();
    let req = t.join().unwrap();
    assert!(req.starts_with("GET /v1/usage "), "{req}");
    assert!(
        req.to_lowercase()
            .contains("authorization: bearer s3cret-token\r\n"),
        "{req}"
    );
    assert!(parse(&got).is_ok());
}
