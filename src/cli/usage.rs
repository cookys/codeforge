//! `codeforge usage` (G36 plan, phase 3): collect, write `summary.json`, print a table.

use crate::usage::{collect, price, quota, store, summary};
use anyhow::{Context as _, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub enum Action {
    Show {
        days: u32,
        by: String,
        json: bool,
    },
    Collect {
        quiet: bool,
    },
    Push,
    Accounts {
        json: bool,
    },
    Fleet {
        dir: Option<PathBuf>,
        json: bool,
        out: Option<PathBuf>,
        use_target: f64,
    },
    AliasList,
    AliasSet {
        who: String,
        name: String,
    },
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Collect every root, write the summary; returns the connection for the caller's report.
/// `wait` false: when another collect is running, return `None` at once (the background
/// statusline trigger and the SessionEnd hook); otherwise wait for it.
fn collect_and_summarize(
    quiet: bool,
    wait: bool,
) -> Result<Option<(rusqlite::Connection, PathBuf)>> {
    let data = store::data_dir(std::env::var("CODEFORGE_DATA_DIR").ok().as_deref());
    let dir = store::usage_dir(&data);
    let (mut conn, notes) = store::open(&dir)?;
    let Some(_lock) = store::collect_lock(&dir, wait)? else {
        return Ok(None);
    };
    if !notes.is_empty() {
        eprintln!("codeforge usage: 已收緊檔案權限（{}）", notes.join("、"));
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut claude_dirs = Vec::new();
    if let Ok(d) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !d.is_empty() {
            claude_dirs.push(PathBuf::from(d));
        }
    }
    claude_dirs.push(home.join(".claude"));
    let codex = std::env::var("CODEX_HOME")
        .ok()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let mut roots = collect::roots(&claude_dirs, Some(&codex));
    let (live_base, _) = crate::live::resolve_live_base();
    for r in roots.iter_mut().filter(|r| r.kind == collect::Kind::Claude) {
        r.account = crate::usage::alias::login_identity(
            &crate::usage::alias::login_path_for_root(&r.dir, &home),
            &crate::usage::alias::identity_cache_path(&live_base, &r.dir),
        )
        .map(|i| i.alias);
    }
    let snaps = quota::read_all(&quota::quota_dir(&live_base));
    let now = now_ms();
    let rep = collect::collect(&mut conn, &roots, &snaps, now)?;
    let since = (chrono::Utc::now() - chrono::Duration::days(6))
        .format("%Y-%m-%d")
        .to_string();
    let rows = collect::rows_since(&conn, &since)?;
    let fresh: Vec<_> = snaps
        .into_iter()
        .filter(|s| (0..quota::MAX_AGE_MS).contains(&(now - s.written_at_ms)))
        .collect();
    let v = summary::build(
        &rows,
        &fresh,
        &summary::host_alias(&summary::hostname()),
        now,
    );
    summary::validate(&v)?;
    let sdir = summary::summary_dir(&live_base);
    let path = summary::write(&sdir, &v)?;
    let fleet_env = std::env::var("CODEFORGE_FLEET_USAGE_DIR").ok();
    if let Err(e) = write_fleet_quota(&sdir, fleet_env.as_deref(), now) {
        if !quiet {
            eprintln!("codeforge usage: 全帳號配額表沒有更新（{e:#}）");
        }
    }
    let dropped = summary::dropped_count(&v);
    if !quiet && dropped > 0 {
        eprintln!("codeforge usage: 有 {dropped} 份本機配額快照不合規格，已略過（見 summary.json 的 dropped_accounts）");
    }
    if !quiet {
        eprintln!(
            "codeforge usage: 讀了 {} 個轉錄檔（略過 {}）",
            rep.files, rep.skipped
        );
    }
    Ok(Some((conn, path)))
}

/// File next to `summary.json` that the forge-stats「全部帳號」tab reads.
pub const FLEET_FILE: &str = "fleet-quota.json";

/// When `fleet_dir` (the value of `CODEFORGE_FLEET_USAGE_DIR`) is set and non-empty, merge the
/// hosts' summaries in it and write `fleet-quota.json` into `out_dir` (0600, atomic). `None` or
/// empty: nothing happens, and an older file is left alone. The env value is a parameter so tests
/// do not touch the process env.
fn write_fleet_quota(
    out_dir: &std::path::Path,
    fleet_dir: Option<&str>,
    now: i64,
) -> Result<Option<PathBuf>> {
    use crate::usage::fleet_quota as fq;
    let Some(d) = fleet_dir.filter(|d| !d.is_empty()) else {
        return Ok(None);
    };
    let (files, skipped) = fq::load(std::path::Path::new(d))?;
    let f = fq::merge(
        &files,
        skipped,
        now,
        fq::DEFAULT_USE_TARGET,
        &crate::usage::names::load_default(),
    );
    let v = fq::to_json(&f, now);
    crate::live::write_live_json(out_dir, FLEET_FILE, &v)
        .map_err(|e| anyhow::anyhow!("無法寫入 {FLEET_FILE}（{:?}）", e.kind()))?;
    Ok(Some(out_dir.join(FLEET_FILE)))
}

fn collected(quiet: bool) -> Result<(rusqlite::Connection, PathBuf)> {
    collect_and_summarize(quiet, true)?.context("collect 沒有結果")
}

fn compact(n: u64) -> String {
    match n {
        0..=9_999 => n.to_string(),
        10_000..=999_999 => format!("{:.1}K", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

fn money(c: Option<f64>) -> String {
    c.map_or_else(|| "—".to_string(), |c| format!("${c:.2}"))
}

/// Column widths of the `usage` table; the 2nd (key) is left-aligned, the rest right-aligned
/// after the date.
const COLS: [usize; 8] = [10, 24, 8, 8, 8, 8, 6, 10];

/// `s` cut to at most `cols` display columns (whole chars, CJK-safe).
fn clip_cols(s: &str, cols: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > cols {
            break;
        }
        used += w;
        out.push(ch);
    }
    out
}

/// One table line. Cells are clipped and padded by display width, not char count: a CJK char
/// is two columns, so `format!("{:<10}")` would misalign the CJK headers.
fn table_line(cells: &[&str; 8]) -> String {
    let mut out = Vec::new();
    for (i, c) in cells.iter().enumerate() {
        let c = clip_cols(c, COLS[i]);
        let pad = " ".repeat(COLS[i].saturating_sub(UnicodeWidthStr::width(c.as_str())));
        out.push(if i < 2 {
            format!("{c}{pad}")
        } else {
            format!("{pad}{c}")
        });
    }
    out.join("  ")
}

pub fn run(action: Action) -> Result<()> {
    match action {
        Action::Collect { quiet } => {
            collect_and_summarize(quiet, !quiet)?;
            Ok(())
        }
        Action::Accounts { json } => accounts(json),
        Action::AliasList => alias_list(),
        Action::AliasSet { who, name } => alias_set(&who, &name),
        Action::Fleet {
            dir,
            json,
            out,
            use_target,
        } => {
            let dir = dir
                .or_else(|| {
                    std::env::var("CODEFORGE_FLEET_USAGE_DIR")
                        .ok()
                        .filter(|d| !d.is_empty())
                        .map(PathBuf::from)
                })
                .context("請用 --dir 指定資料夾，或設環境變數 CODEFORGE_FLEET_USAGE_DIR")?;
            fleet(&dir, json, out.as_deref(), use_target)
        }
        Action::Push => {
            let (_conn, path) = collected(true)?;
            let dir = path.parent().map(PathBuf::from).unwrap_or_default();
            let sha = summary::push(&dir, "fleet")?;
            println!("{sha}");
            Ok(())
        }
        Action::Show { days, by, json } => {
            let (conn, _) = collected(true)?;
            let days = days.clamp(1, collect::HORIZON_DAYS as u32);
            let since = (chrono::Utc::now() - chrono::Duration::days(days as i64 - 1))
                .format("%Y-%m-%d")
                .to_string();
            let rows = collect::rows_since(&conn, &since)?;
            // (day, key) → tokens, requests, costs
            type Acc = (price::Tokens, u64, Vec<Option<f64>>);
            let mut agg: BTreeMap<(String, String), Acc> = BTreeMap::new();
            for r in &rows {
                let key = match by.as_str() {
                    "project" => r.project.clone(),
                    "account" => r.account.clone(),
                    _ => r.model.clone(),
                };
                let e = agg.entry((r.day.clone(), key)).or_default();
                e.0.add(&r.tokens);
                e.1 = e.1.saturating_add(r.requests);
                e.2.push(price::cost(&r.model, r.speed == "fast", &r.tokens));
            }
            if json {
                let out: Vec<_> = agg
                    .iter()
                    .map(|((day, key), (t, req, costs))| {
                        let (cost, unpriced) = price::total(costs.iter().copied());
                        serde_json::json!({
                            "day": day, "key": key, "input": t.input, "output": t.output,
                            "cache_write_5m": t.cache_write_5m, "cache_write_1h": t.cache_write_1h,
                            "cache_read": t.cache_read, "requests": req,
                            "cost_usd": cost, "unpriced_rows": unpriced,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
                return Ok(());
            }
            println!(
                "{}",
                table_line(&[
                    "日期",
                    &by,
                    "輸入",
                    "輸出",
                    "快取寫",
                    "快取讀",
                    "請求",
                    "API 等值"
                ])
            );
            let mut all = Vec::new();
            for ((day, key), (t, req, costs)) in &agg {
                let (cost, unpriced) = price::total(costs.iter().copied());
                let cost_s = match (cost, unpriced) {
                    (Some(_), n) if n > 0 => format!("≥{}", money(cost)),
                    _ => money(cost),
                };
                all.extend(costs.iter().copied());
                println!(
                    "{}",
                    table_line(&[
                        day,
                        key,
                        &compact(t.input),
                        &compact(t.output),
                        &compact(t.cache_write_5m + t.cache_write_1h),
                        &compact(t.cache_read),
                        &req.to_string(),
                        &cost_s,
                    ])
                );
            }
            let (total, unpriced) = price::total(all);
            match (total, unpriced) {
                (Some(_), 0) => println!(
                    "合計 {}（API 等值，估算；訂閱方案不按 token 計費）",
                    money(total)
                ),
                (Some(_), n) => println!("合計 ≥{}（{n} 列無定價；API 等值，估算）", money(total)),
                (None, _) => println!("合計 —（沒有可定價的資料）"),
            }
            Ok(())
        }
    }
}

/// Terminal width for `usage accounts`: the terminal, else `COLUMNS`, else 100; never wider than 100 (design 6.2).
/// No lower clamp beyond 1: every line is clipped to this width, so a narrow terminal never wraps.
fn accounts_width() -> usize {
    let w = crossterm::terminal::size()
        .ok()
        .map(|(c, _)| c as usize)
        .or_else(|| std::env::var("COLUMNS").ok().and_then(|v| v.parse().ok()))
        .unwrap_or(100);
    w.clamp(1, 100)
}

fn fleet(
    dir: &std::path::Path,
    json: bool,
    out: Option<&std::path::Path>,
    target: f64,
) -> Result<()> {
    use crate::usage::{fleet_quota as fq, quota_view as qv};
    anyhow::ensure!(target > 0.0 && target <= 1.0, "--use-target 要在 (0, 1]");
    let now = now_ms();
    let (files, skipped) = fq::load(dir)?;
    let f = fq::merge(
        &files,
        skipped,
        now,
        target,
        &crate::usage::names::load_default(),
    );
    let v = fq::to_json(&f, now);
    if let Some(p) = out {
        fq::write_atomic(p, &v)?;
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    use std::io::IsTerminal;
    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    print!(
        "{}",
        fq::text(&f, accounts_width(), now, &qv::Tr::current(), color)
    );
    Ok(())
}

/// Every account this host knows: its local quota snapshots, then the rows of the fleet table
/// next to `summary.json`: (key, org, email), one per key (the first that has an email wins).
fn known_accounts() -> Vec<(String, Option<String>, Option<String>)> {
    let (live_base, _) = crate::live::resolve_live_base();
    let mut out: Vec<(String, Option<String>, Option<String>)> =
        quota::read_all(&quota::quota_dir(&live_base))
            .into_iter()
            .map(|s| (s.alias, s.org, s.email))
            .collect();
    let fleet = summary::summary_dir(&live_base).join(FLEET_FILE);
    if let Some(v) = std::fs::read(&fleet)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
    {
        let s = |a: &serde_json::Value, k: &str| a[k].as_str().map(str::to_string);
        for a in v["accounts"].as_array().into_iter().flatten() {
            if let Some(k) = s(a, "alias") {
                out.push((k, s(a, "org"), s(a, "email")));
            }
        }
    }
    let mut seen: std::collections::BTreeMap<String, usize> = Default::default();
    let mut merged: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for (k, org, email) in out {
        match seen.get(&k) {
            Some(&i) => {
                let m = &mut merged[i];
                m.1 = m.1.take().or(org);
                m.2 = m.2.take().or(email);
            }
            None => {
                seen.insert(k.clone(), merged.len());
                merged.push((k, org, email));
            }
        }
    }
    merged
}

fn who_of(a: &[(String, Option<String>, Option<String>)]) -> Vec<crate::usage::names::Who<'_>> {
    a.iter()
        .map(|(k, o, e)| crate::usage::names::Who {
            key: k,
            org: o.as_deref(),
            email: e.as_deref(),
        })
        .collect()
}

fn alias_list() -> Result<()> {
    use crate::usage::names;
    let names = names::load_default();
    let known = known_accounts();
    let who = who_of(&known);
    for ((k, org, email), n) in known.iter().zip(names::assign(&who, &names)) {
        let Some(n) = n else { continue };
        let set = if names.0.contains_key(k) {
            "（自訂）"
        } else {
            ""
        };
        println!(
            "{n}{set}\t{}\t{}",
            org.as_deref().unwrap_or("—"),
            email.as_deref().unwrap_or("—")
        );
    }
    Ok(())
}

fn alias_set(who: &str, name: &str) -> Result<()> {
    use crate::usage::names;
    anyhow::ensure!(
        names::valid_name(name),
        "名字要 1 到 24 個字，不能有 @、換行或其他控制字元，也不能以 acct- 開頭"
    );
    let path = names::default_path().context("找不到設定資料夾（HOME 沒有設）")?;
    let mut set = names::load(&path);
    let known = known_accounts();
    let who_list = who_of(&known);
    let key = names::resolve(&who_list, &set, who)?;
    let shown = names::assign(&who_list, &set);
    anyhow::ensure!(
        !known
            .iter()
            .zip(&shown)
            .any(|((k, _, _), n)| *k != key && n.as_deref() == Some(name)),
        "「{name}」已經是另一個帳號的名字"
    );
    set.0.insert(key, name.to_string());
    names::save(&path, &set)?;
    println!("已設定：{who} → {name}");
    Ok(())
}

fn accounts(json: bool) -> Result<()> {
    use crate::usage::quota_mirror_read as rd;
    use crate::usage::quota_view as qv;
    let st = rd::resolve_live_dir()
        .map(|d| rd::read_mirror(&d))
        .unwrap_or_default();
    let now_s = chrono::Utc::now().timestamp();
    let entries = rd::quota_entries(&st, now_s);
    let tr = qv::Tr::current();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&qv::accounts_json(&st, &entries, now_s, &tr))?
        );
        return Ok(());
    }
    use std::io::IsTerminal;
    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    print!(
        "{}",
        qv::accounts_text(&st, &entries, accounts_width(), now_s, &tr, color)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_rows_have_one_display_width_with_cjk_cells() {
        let header = table_line(&[
            "日期",
            "model",
            "輸入",
            "輸出",
            "快取寫",
            "快取讀",
            "請求",
            "API 等值",
        ]);
        let rows = [
            table_line(&[
                "2026-11-03",
                "claude-opus-5-5",
                "1.0M",
                "2.5K",
                "10",
                "9",
                "3",
                "$1.50",
            ]),
            // a CJK project name must not push the later columns out
            table_line(&[
                "2026-11-03",
                "專案鍛造間",
                "1.0M",
                "2.5K",
                "10",
                "9",
                "3",
                "≥$1.50",
            ]),
            table_line(&["2026-11-03", &"專".repeat(40), "1", "2", "3", "4", "5", "—"]),
        ];
        let w = UnicodeWidthStr::width(header.as_str());
        for r in &rows {
            assert_eq!(
                UnicodeWidthStr::width(r.as_str()),
                w,
                "{r:?} vs header {header:?}"
            );
        }
    }

    fn fleet_dir_with_host(root: &std::path::Path) -> PathBuf {
        let d = root.join("hosts");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn fleet_file_written_when_env_set() {
        let t = tempfile::tempdir().unwrap();
        let hosts = fleet_dir_with_host(t.path());
        let out = t.path().join("usage");
        let p = write_fleet_quota(&out, hosts.to_str(), 1_000_000)
            .unwrap()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        assert_eq!(v["schema"], "codeforge.fleet-quota/1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn no_fleet_file_when_env_unset_or_empty_and_old_one_stays() {
        let t = tempfile::tempdir().unwrap();
        let out = t.path().join("usage");
        assert!(write_fleet_quota(&out, None, 1).unwrap().is_none());
        assert!(write_fleet_quota(&out, Some(""), 1).unwrap().is_none());
        assert!(!out.join(FLEET_FILE).exists());
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join(FLEET_FILE), "old").unwrap();
        assert!(write_fleet_quota(&out, None, 1).unwrap().is_none());
        assert_eq!(
            std::fs::read_to_string(out.join(FLEET_FILE)).unwrap(),
            "old"
        );
    }

    #[test]
    fn bad_fleet_dir_is_an_error_for_the_caller_to_swallow() {
        let t = tempfile::tempdir().unwrap();
        let out = t.path().join("usage");
        let bad = t.path().join("nope");
        assert!(write_fleet_quota(&out, bad.to_str(), 1).is_err());
        assert!(!out.join(FLEET_FILE).exists());
    }
}
