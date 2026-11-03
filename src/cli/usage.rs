//! `codeforge usage` (G36 plan, phase 3): collect, write `summary.json`, print a table.

use crate::usage::{collect, price, quota, store, summary};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub enum Action {
    Show { days: u32, by: String, json: bool },
    Collect { quiet: bool },
    Push,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Collect every root, write the summary; returns the connection for the caller's report.
fn collect_and_summarize(quiet: bool) -> Result<(rusqlite::Connection, PathBuf)> {
    let data = store::data_dir(std::env::var("CODEFORGE_DATA_DIR").ok().as_deref());
    let (mut conn, notes) = store::open(&store::usage_dir(&data))?;
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
    let roots = collect::roots(&claude_dirs, Some(&codex));
    let (live_base, _) = crate::live::resolve_live_base();
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
    let path = summary::write(&summary::summary_dir(&live_base), &v)?;
    if !quiet {
        eprintln!(
            "codeforge usage: 讀了 {} 個轉錄檔（略過 {}）",
            rep.files, rep.skipped
        );
    }
    Ok((conn, path))
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

pub fn run(action: Action) -> Result<()> {
    match action {
        Action::Collect { quiet } => {
            collect_and_summarize(quiet)?;
            Ok(())
        }
        Action::Push => {
            let (_conn, path) = collect_and_summarize(true)?;
            let dir = path.parent().map(PathBuf::from).unwrap_or_default();
            let sha = summary::push(&dir, "fleet")?;
            println!("{sha}");
            Ok(())
        }
        Action::Show { days, by, json } => {
            let (conn, _) = collect_and_summarize(true)?;
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
                "{:<10}  {:<24}  {:>8}  {:>8}  {:>8}  {:>8}  {:>6}  {:>10}",
                "日期", by, "輸入", "輸出", "快取寫", "快取讀", "請求", "API 等值"
            );
            let mut all = Vec::new();
            for ((day, key), (t, req, costs)) in &agg {
                let (cost, unpriced) = price::total(costs.iter().copied());
                let cost_s = match (cost, unpriced) {
                    (Some(_), n) if n > 0 => format!("≥{}", money(cost)),
                    _ => money(cost),
                };
                all.extend(costs.iter().copied());
                let key_s: String = key.chars().take(24).collect();
                println!(
                    "{:<10}  {:<24}  {:>8}  {:>8}  {:>8}  {:>8}  {:>6}  {:>10}",
                    day,
                    key_s,
                    compact(t.input),
                    compact(t.output),
                    compact(t.cache_write_5m + t.cache_write_1h),
                    compact(t.cache_read),
                    req,
                    cost_s
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
