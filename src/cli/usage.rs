//! `codeforge usage` (G36 plan, phase 3): collect, write `summary.json`, print a table.

use crate::usage::reset_rule::Kind;
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
        claude_dirs: Vec<PathBuf>,
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
    NoteSet {
        target: String,
        text: String,
        new: bool,
        /// at most one rule, of the kind its flag named
        reset: Option<(crate::usage::reset_rule::Kind, String)>,
    },
    NoteReset {
        target: String,
        rule: String,
    },
    NoteClear {
        target: String,
    },
    PlanSet {
        target: String,
        plan: String,
    },
    PlanClear {
        target: String,
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
    extra_claude_dirs: &[PathBuf],
) -> Result<Option<(rusqlite::Connection, PathBuf)>> {
    for dir in extra_claude_dirs {
        if !dir.is_absolute() {
            anyhow::bail!("--claude-dir 必須是絕對路徑：「{}」", dir.display());
        }
    }
    let configured_dirs = crate::usage::names::default_path()
        .map(|p| crate::usage::names::load_claude_dirs(&p))
        .transpose()?
        .unwrap_or_default();
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
    claude_dirs.extend(configured_dirs);
    claude_dirs.extend(collect::remembered_claude_dirs(&conn)?);
    claude_dirs.extend_from_slice(extra_claude_dirs);
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
    let mut fresh: Vec<_> = snaps
        .into_iter()
        .filter(|s| (0..quota::MAX_AGE_MS).contains(&(now - s.written_at_ms)))
        .collect();
    // codex accounts carry the windows of their newest rollout `rate_limits` the same way
    fresh.extend(collect::codex_snapshots(&conn, now)?);
    let v = summary::with_codex_sessions(
        summary::build(
            &rows,
            &fresh,
            &summary::host_alias(&summary::hostname()),
            now,
            &crate::usage::names::load_default(),
        ),
        &collect::codex_sessions_since(&conn, &since)?,
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
    let mut f = fq::merge(
        &files,
        skipped,
        now,
        fq::DEFAULT_USE_TARGET,
        &crate::usage::names::load_default(),
    );
    fq::attach_notes(&mut f, &crate::usage::names::load_notes_default());
    fq::attach_resets(&mut f, &crate::usage::names::load_resets_default(), now);
    fq::attach_plans(&mut f, &crate::usage::names::load_plans_default());
    let v = fq::to_json(&f, now);
    crate::live::write_live_json(out_dir, FLEET_FILE, &v)
        .map_err(|e| anyhow::anyhow!("無法寫入 {FLEET_FILE}（{:?}）", e.kind()))?;
    Ok(Some(out_dir.join(FLEET_FILE)))
}

fn collected(quiet: bool) -> Result<(rusqlite::Connection, PathBuf)> {
    collect_and_summarize(quiet, true, &[])?.context("collect 沒有結果")
}

fn compact(n: u64) -> String {
    match n {
        0..=9_999 => n.to_string(),
        10_000..=999_999 => format!("{:.1}K", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

/// The requests cell: compacted like the token cells so a big count is not clipped by its 6-wide column.
fn request_cell(n: u64) -> String {
    compact(n)
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
        Action::Collect { quiet, claude_dirs } => {
            collect_and_summarize(quiet, !quiet, &claude_dirs)?;
            Ok(())
        }
        Action::Accounts { json } => accounts(json),
        Action::AliasList => alias_list(),
        Action::AliasSet { who, name } => alias_set(&who, &name),
        Action::NoteSet {
            target,
            text,
            new,
            reset,
        } => note_set(
            &target,
            Some(&text),
            new,
            reset.as_ref().map(|(k, r)| (*k, r.as_str())),
        ),
        Action::NoteReset { target, rule } => note_reset(&target, &rule),
        Action::NoteClear { target } => note_set(&target, None, false, None),
        Action::PlanSet { target, plan } => plan_set(&target, Some(&plan)),
        Action::PlanClear { target } => plan_set(&target, None),
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
                        // no login: no account, null (one line apart, PM 01M7052H79)
                        let key = Some(key).filter(|k| by != "account" || !k.is_empty());
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
            // by account: the display name, never the `acct-` key (--json keeps the key)
            let shown = if by == "account" {
                account_names(agg.keys().map(|(_, k)| k.as_str()))
            } else {
                BTreeMap::new()
            };
            let mut all = Vec::new();
            for ((day, key), (t, req, costs)) in &agg {
                let key = if by == "account" && key == collect::UNATTRIBUTED {
                    UNATTRIBUTED_SHOWN
                } else {
                    shown.get(key).map_or(key.as_str(), String::as_str)
                };
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
                        &request_cell(*req),
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
    let mut f = fq::merge(
        &files,
        skipped,
        now,
        target,
        &crate::usage::names::load_default(),
    );
    fq::attach_notes(&mut f, &crate::usage::names::load_notes_default());
    fq::attach_resets(&mut f, &crate::usage::names::load_resets_default(), now);
    fq::attach_plans(&mut f, &crate::usage::names::load_plans_default());
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
            // a no-data note row (`note set --new`) is a note, not an account
            let note_row = a["provider"] == "claude"
                && a["state"] == "no_source"
                && a["hosts"].as_array().is_some_and(Vec::is_empty);
            if let (Some(k), false) = (s(a, "alias"), note_row) {
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

/// How `usage --by account` shows the rows of sessions with no login (no account).
const UNATTRIBUTED_SHOWN: &str = "未歸屬（未登入）";

/// Each of `keys` → the name it is shown by: the accounts this host knows, plus any other key
/// (a codex account shows as itself); an `acct-` key nobody can name shows as 未命名帳號.
fn account_names<'a>(keys: impl Iterator<Item = &'a str>) -> BTreeMap<String, String> {
    use crate::usage::names;
    let mut known = known_accounts();
    for k in keys.filter(|k| *k != crate::usage::collect::UNATTRIBUTED) {
        if !known.iter().any(|(kk, _, _)| kk == k) {
            known.push((k.to_string(), None, None));
        }
    }
    let who = who_of(&known);
    known
        .iter()
        .zip(names::assign(&who, &names::load_default()))
        .map(|((k, _, _), n)| (k.clone(), n.unwrap_or_else(|| "未命名帳號".into())))
        .collect()
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
    let shown = names::assign(&who, &names);
    let plans = names::load_plans_default();
    for ((k, org, email), n) in known.iter().zip(&shown) {
        let Some(n) = n else { continue };
        let set = if names.0.contains_key(k) {
            "（自訂）"
        } else {
            ""
        };
        let tr = crate::usage::quota_view::Tr::current();
        let plan = plans
            .for_row(k, names::key_provider(k).unwrap_or("claude"))
            .map_or_else(
                || tr.t("quota.plan_unknown", &[]),
                |id| names::plan_display(id).to_string(),
            );
        println!(
            "{n}{set}\t{}\t{}\t{plan}",
            org.as_deref().unwrap_or("—"),
            email.as_deref().unwrap_or("—")
        );
    }
    let notes = names::load_notes_default();
    // accounts with no data, only a note (`usage note set --new`)
    for (k, _) in
        crate::usage::fleet_quota::note_only(&notes, |k| shown.iter().flatten().any(|n| n == k))
    {
        println!("{k}（無資料）\t—\t—");
    }
    let resets = names::load_resets_default();
    let keys: std::collections::BTreeSet<&String> = notes.0.keys().chain(resets.0.keys()).collect();
    if !keys.is_empty() {
        // an account key is shown by its name, never itself; a provider label is shown as is
        let shown = account_names(keys.iter().map(|k| k.as_str()).filter(|k| !is_provider(k)));
        println!("備註：");
        for k in keys {
            let who = if is_provider(k) {
                k.as_str()
            } else {
                shown.get(k).map_or("未命名帳號", String::as_str)
            };
            let text = notes.0.get(k).map_or("—", String::as_str);
            match resets.0.get(k) {
                Some(rule) => println!("  {who}\t{text}\t{} {rule}", rule_label(rule)),
                None => println!("  {who}\t{text}"),
            }
        }
    }
    Ok(())
}

/// A provider label a note may be keyed by: `claude` or a [`fleet_quota::PROVIDERS`] name.
fn is_provider(s: &str) -> bool {
    crate::usage::fleet_quota::is_provider_label(s)
}

/// The notes-table key `target` names: a provider label as is, else the one account whose email
/// or shown name it is (the same lookup as `alias set`).
fn note_key(
    target: &str,
    known: &[crate::usage::names::Who],
    names: &crate::usage::names::Names,
    notes: &crate::usage::names::Notes,
) -> Result<String> {
    // a provider label, or the key of a no-data account (`note set --new`), is its own key
    if is_provider(target)
        || (notes.0.contains_key(target) && crate::usage::names::valid_name(target))
    {
        return Ok(target.to_string());
    }
    crate::usage::names::resolve(known, names, target)
}

/// `note set --new <name>`: the notes key for an account with no data. The name must be a valid
/// display name nobody else has: not a provider label, not an existing note, not a known
/// account's key or shown name.
fn new_note_key(
    name: &str,
    known: &[crate::usage::names::Who],
    names: &crate::usage::names::Names,
    notes: &crate::usage::names::Notes,
) -> Result<String> {
    use crate::usage::names;
    anyhow::ensure!(
        names::valid_name(name),
        "--new 的名字最寬 {} 欄，不能是 email 或 acct-／codex- key，不能有 @、換行或其他控制字元，前後不能有空白",
        names::NAME_MAX
    );
    anyhow::ensure!(
        !is_provider(name),
        "「{name}」是 provider 名：去掉 --new，直接設定 provider 的備註"
    );
    anyhow::ensure!(
        !notes.0.contains_key(name),
        "「{name}」已經有備註：去掉 --new 就能改它"
    );
    let taken = known.iter().any(|w| w.key == name)
        || names::assign(known, names)
            .iter()
            .flatten()
            .any(|n| n == name);
    anyhow::ensure!(
        !taken,
        "「{name}」已是一個有資料的帳號：去掉 --new，直接用它的名字設定備註"
    );
    Ok(name.to_string())
}

/// A rule the operator typed: valid (of `kind` when a flag named one), else an error with the form.
fn check_rule(rule: &str, kind: Option<Kind>) -> Result<()> {
    use crate::usage::reset_rule::{valid, valid_kind, EXAMPLE, EXAMPLE_AT, EXAMPLE_MONTHLY};
    let weekly = format!("每週 <星期>@<HH:MM><±HH:MM>，星期為 mon tue wed thu fri sat sun（小寫），例如 {EXAMPLE}（每週五 17:00，UTC+8）");
    let monthly = format!("每月 <日>@<HH:MM><±HH:MM>，日為 1 到 31（不補 0），例如 {EXAMPLE_MONTHLY}（每月 17 日 00:00，UTC+8）");
    let at = format!("一次性 YYYY-MM-DDTHH:MM:SS±HH:MM（或結尾 Z），例如 {EXAMPLE_AT}");
    let (ok, form) = match kind {
        Some(k) => (
            valid_kind(rule, k),
            match k {
                Kind::Weekly => weekly,
                Kind::Monthly => monthly,
                Kind::At => at,
            },
        ),
        None => (valid(rule), format!("{weekly}；{monthly}；或{at}")),
    };
    anyhow::ensure!(ok, "重置規則格式是{form}；時區要寫明");
    Ok(())
}

/// The label for a stored rule.
fn rule_label(rule: &str) -> &'static str {
    match crate::usage::reset_rule::parse(rule).map(|r| r.kind()) {
        Some(Kind::Monthly) => "每月重置",
        Some(Kind::At) => "一次重置",
        _ => "每週重置",
    }
}

/// `usage note set` (`Some(text)`, with an optional weekly rule) / `usage note clear` (`None`:
/// the note and the rule).
fn note_set(target: &str, text: Option<&str>, new: bool, rule: Option<(Kind, &str)>) -> Result<()> {
    use crate::usage::names;
    if let Some(t) = text {
        anyhow::ensure!(
            names::valid_note(t),
            "備註最寬 {} 欄（中文字算 2 欄），不能有 @、換行或其他控制字元，前後不能有空白",
            names::NOTE_MAX
        );
    }
    if let Some((k, r)) = rule {
        check_rule(r, Some(k))?;
    }
    let rule = rule.map(|(_, r)| r);
    let path = names::default_path().context("找不到設定資料夾（HOME 沒有設）")?;
    let set = names::load(&path);
    let known = known_accounts();
    let notes = names::load_notes(&path);
    let who = who_of(&known);
    let key = if new {
        new_note_key(target, &who, &set, &notes)?
    } else {
        note_key(target, &who, &set, &notes)?
    };
    let target = if target.contains('@') {
        "該 email"
    } else {
        target
    };
    match text {
        Some(t) => {
            names::set_note_and_reset(&path, &key, t, rule)?;
            println!("已設定備註：{target} → {t}");
            if let Some(r) = rule {
                println!("已設定{}：{target} → {r}", rule_label(r));
            }
        }
        None if names::clear_note_and_reset(&path, &key)? => {
            println!("已清除備註與重置規則：{target}")
        }
        None => println!("{target} 沒有備註"),
    }
    Ok(())
}

/// `usage plan set <target> <plan>` (`Some`) / `usage plan clear <target>` (`None`). The target is
/// resolved as `usage note set` resolves it, so a plan can sit on a provider label too.
fn plan_set(target: &str, plan: Option<&str>) -> Result<()> {
    use crate::usage::names;
    let id = plan
        .map(|p| {
            names::parse_plan(p).with_context(|| {
                format!(
                    "不認得方案「{p}」；可用：{}（5x、20x 是 max5x、max20x 的簡寫）",
                    names::PLANS.join("、")
                )
            })
        })
        .transpose()?;
    let path = names::default_path().context("找不到設定資料夾（HOME 沒有設）")?;
    let set = names::load(&path);
    let known = known_accounts();
    let who = who_of(&known);
    let key = note_key(target, &who, &set, &names::load_notes(&path))?;
    let target = if target.contains('@') {
        "該 email"
    } else {
        target
    };
    match id {
        Some(id) => {
            names::set_plan(&path, &key, id)?;
            println!("已設定方案：{target} → {}", names::plan_display(id));
        }
        None if names::clear_plan(&path, &key)? => println!("已清除方案：{target}"),
        None => println!("{target} 沒有方案"),
    }
    Ok(())
}

/// `usage note reset <target> <rule>`: only the rule, for a target that already has a note or is a
/// known account or provider.
fn note_reset(target: &str, rule: &str) -> Result<()> {
    use crate::usage::names;
    check_rule(rule, None)?;
    let path = names::default_path().context("找不到設定資料夾（HOME 沒有設）")?;
    let set = names::load(&path);
    let known = known_accounts();
    let who = who_of(&known);
    let key = note_key(target, &who, &set, &names::load_notes(&path))?;
    names::set_reset(&path, &key, rule)?;
    let target = if target.contains('@') {
        "該 email"
    } else {
        target
    };
    println!("已設定{}：{target} → {rule}", rule_label(rule));
    Ok(())
}

fn alias_set(who: &str, name: &str) -> Result<()> {
    use crate::usage::names;
    anyhow::ensure!(
        names::valid_name(name),
        "名字最寬 24 欄（中文字算 2 欄），不能有 @、換行或其他控制字元，也不能以 acct- 開頭"
    );
    let path = names::default_path().context("找不到設定資料夾（HOME 沒有設）")?;
    let set = names::load(&path);
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
    names::set(&path, &key, name)?;
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
    fn request_cell_is_compacted_to_fit_its_column() {
        assert_eq!(compact(1_000_000), "1.0M");
        assert!(compact(1_000_000).chars().count() <= COLS[6]);
        assert_eq!(request_cell(1_234_567), "1.2M");
        assert_eq!(request_cell(42), "42");
    }

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
    fn a_note_target_is_a_provider_label_else_one_account() {
        use crate::usage::names::{Names, Notes, Who};
        let known = [
            Who {
                key: "acct-11111111",
                org: Some("Stranity"),
                email: Some("me@example.com"),
            },
            Who {
                key: "codex-1a2b3c4d",
                org: None,
                email: None,
            },
        ];
        let n = Names::default();
        let no = Notes::default();
        let note_key = |t: &str, k: &[Who], n: &Names| note_key(t, k, n, &no);
        assert_eq!(note_key("kimi", &known, &n).unwrap(), "kimi");
        // `codex` is the provider label (every codex row), not the one account shown as codex
        assert_eq!(note_key("codex", &known, &n).unwrap(), "codex");
        assert_eq!(note_key("stranity", &known, &n).unwrap(), "acct-11111111");
        assert_eq!(
            note_key("ME@example.com", &known, &n).unwrap(),
            "acct-11111111"
        );
        assert!(note_key("nobody", &known, &n).is_err());
        assert!(
            note_key("MiniMax", &known, &n).is_err(),
            "labels are lowercase"
        );
    }

    #[test]
    fn a_reset_rule_is_checked_with_a_chinese_error_that_shows_the_form() {
        assert!(check_rule("fri@17:00+08:00", Some(Kind::Weekly)).is_ok());
        assert!(check_rule("17@00:00+08:00", Some(Kind::Monthly)).is_ok());
        assert!(check_rule("2026-11-21T00:00:00+08:00", Some(Kind::At)).is_ok());
        for any in ["fri@17:00+08:00", "17@00:00+08:00", "2026-11-21T00:00:00Z"] {
            assert!(check_rule(any, None).is_ok(), "{any}");
        }
        for bad in ["fri", "Fri@17:00+08:00", "fri@17:00", "fri@25:00+08:00", ""] {
            let e = check_rule(bad, Some(Kind::Weekly)).unwrap_err().to_string();
            assert!(
                e.contains("fri@17:00+08:00") && e.contains("重置規則"),
                "{bad:?}: {e}"
            );
        }
        // a flag takes only its own form, and the error shows that form
        let e = check_rule("fri@17:00+08:00", Some(Kind::Monthly))
            .unwrap_err()
            .to_string();
        assert!(e.contains("17@00:00+08:00") && !e.contains("fri@"), "{e}");
        let e = check_rule("17@00:00+08:00", Some(Kind::At))
            .unwrap_err()
            .to_string();
        assert!(e.contains("2026-11-21T00:00:00+08:00"), "{e}");
        // `note reset` takes any form and shows all three
        let e = check_rule("17@00", None).unwrap_err().to_string();
        assert!(
            e.contains("fri@17:00+08:00")
                && e.contains("17@00:00+08:00")
                && e.contains("T00:00:00"),
            "{e}"
        );
        assert_eq!(rule_label("fri@17:00+08:00"), "每週重置");
        assert_eq!(rule_label("17@00:00+08:00"), "每月重置");
        assert_eq!(rule_label("2026-11-21T00:00:00Z"), "一次重置");
    }

    #[test]
    fn note_set_new_takes_only_a_free_valid_name_and_clear_finds_it() {
        use crate::usage::names::{Names, Notes, Who};
        let known = [
            Who {
                key: "acct-11111111",
                org: Some("Stranity"),
                email: Some("me@example.com"),
            },
            Who {
                key: "work",
                org: None,
                email: None,
            },
        ];
        let n = Names::default();
        let mut notes = Notes::default();
        assert_eq!(
            new_note_key("twgame", &known, &n, &notes).unwrap(),
            "twgame"
        );
        for bad in [
            "acct-22222222",
            "codex-1a2b3c4d",
            "a@b.com",
            "kimi",
            "claude",
            "stranity",
            "work",
            "",
            " x",
        ] {
            assert!(new_note_key(bad, &known, &n, &notes).is_err(), "{bad:?}");
        }
        notes.0.insert("twgame".into(), "週四重置".into());
        let e = new_note_key("twgame", &known, &n, &notes).unwrap_err();
        assert!(e.to_string().contains("去掉 --new"), "{e}");
        // without --new the entry is found by its name (set again, or clear)
        assert_eq!(note_key("twgame", &known, &n, &notes).unwrap(), "twgame");
        assert!(note_key("other", &known, &n, &notes).is_err());
    }

    #[test]
    fn fleet_file_carries_the_no_source_reasons() {
        let t = tempfile::tempdir().unwrap();
        let hosts = fleet_dir_with_host(t.path());
        let out = t.path().join("usage");
        let p = write_fleet_quota(&out, hosts.to_str(), 1_000_000)
            .unwrap()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        let kimi = v["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["provider"] == "kimi")
            .unwrap();
        assert_eq!(kimi["source_note"], "web_only");
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
