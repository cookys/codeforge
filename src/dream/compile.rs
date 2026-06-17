use crate::db;
use crate::memory::fts::FtsIndex;
use crate::memory::{l0, l1};
/// Dream Compile：L0 signals → L1 Markdown（增量，使用 Claude Agent SDK）
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub struct CompileResult {
    pub signals_processed: usize,
    pub l1_created: usize,
    pub l1_updated: usize,
    /// mem0-style reconciliation (T2.2): signals whose fact already existed and
    /// were skipped rather than written — keeps the store free of stale dupes.
    pub l1_noop: usize,
}

/// mem0-style reconciliation decision (T2.2) for a freshly-compiled fact against
/// the related existing L1 entries surfaced by FTS. Biased toward `Add` — a wrong
/// `Noop`/`Delete` loses knowledge, so any ambiguity resolves to `Add`.
#[derive(Debug, PartialEq, Eq)]
enum Reconcile {
    /// Genuinely new (or merely complementary) — write a new file.
    Add,
    /// An equivalent fact already exists — write nothing.
    Noop,
    /// `candidate[i]` covers the same point; the new fact refines it — merge in place.
    Update(usize),
    /// The new fact obsoletes/contradicts `candidate[i]` — supersede it, then add.
    Delete(usize),
}

/// What `apply_decision` actually did to the store, for metric counting.
#[derive(Debug, PartialEq, Eq)]
enum Applied {
    Added,
    Updated,
    Noop,
}

pub async fn run(ctx: &db::Context, conn: &Connection) -> Result<CompileResult> {
    let signals = l0::read_uncompiled(ctx, conn)?;
    if signals.is_empty() {
        return Ok(CompileResult {
            signals_processed: 0,
            l1_created: 0,
            l1_updated: 0,
            l1_noop: 0,
        });
    }

    let store_dir = ctx.project_dir.join("store");
    let fts = FtsIndex::new(conn);

    let mut l1_created = 0;
    let mut l1_updated = 0;
    let mut l1_noop = 0;

    for signal in &signals {
        // 使用 LLM 分類並生成 L1 entry
        match compile_signal(ctx, signal).await {
            Ok(Some(mut entry)) => {
                let file_path = l1::L1Entry::path_for(
                    &store_dir,
                    &entry.frontmatter.kind,
                    &l1::L1Entry::slugify(&entry.frontmatter.topic),
                );
                entry.file_path = file_path.clone();

                if file_path.exists() {
                    // 同 slug = 同主題 → in-place 合併（既有行為，視為 update）
                    if let Ok(existing) = l1::L1Entry::from_file(&file_path) {
                        entry = merge_into(&entry, &existing);
                    }
                    entry.save()?;
                    fts_upsert_entry(&fts, &ctx.project_dir, &entry)?;
                    l1_updated += 1;
                } else {
                    // 新 slug：對賬既有相關條目，決定 ADD/UPDATE/DELETE/NOOP（T2.2）
                    let candidates = fts_candidates(conn, &ctx.project_dir, &entry, &file_path, 3);
                    let decision = if candidates.is_empty() {
                        Reconcile::Add
                    } else {
                        reconcile_decision(&entry, &candidates).await
                    };
                    match apply_decision(decision, entry, &candidates, &fts, &ctx.project_dir)? {
                        Applied::Added => l1_created += 1,
                        Applied::Updated => l1_updated += 1,
                        Applied::Noop => l1_noop += 1,
                    }
                }
            }
            Ok(None) => {
                // signal 品質不足，存在 L0 但不升至 L1
            }
            Err(e) => {
                eprintln!(
                    "  警告：compile signal {} 失敗：{}",
                    signal.id.chars().take(8).collect::<String>(),
                    e
                );
            }
        }
    }

    // 更新 signal cursors：為每個被處理的 signal 的 source_date 標記 compiled
    // 避免跨日 signals 重複處理（只更新 today 會漏掉昨天以前的檔案）
    let mut processed_dates = std::collections::HashSet::new();
    for signal in &signals {
        // timestamp 格式：2006-01-02T15:04:05Z，前 10 字元為日期
        if signal.timestamp.len() >= 10 {
            processed_dates.insert(signal.timestamp[..10].to_string());
        }
    }
    for date in processed_dates {
        l0::mark_compiled(ctx, conn, &date)?;
    }

    Ok(CompileResult {
        signals_processed: signals.len(),
        l1_created,
        l1_updated,
        l1_noop,
    })
}

/// Merge a freshly-compiled `new` fact into a `target` entry, in place: take the
/// new body/title but preserve the target's accumulated metadata (path, kind,
/// topic, created date, strength, refs, last_ref) and union sources + links.
/// Shared by the same-slug merge and the reconciliation `Update` path (T2.2).
fn merge_into(new: &l1::L1Entry, target: &l1::L1Entry) -> l1::L1Entry {
    let mut merged = new.clone();
    merged.file_path = target.file_path.clone();
    merged.frontmatter.kind = target.frontmatter.kind.clone();
    merged.frontmatter.topic = target.frontmatter.topic.clone();
    merged.frontmatter.created = target.frontmatter.created.clone();
    merged.frontmatter.strength = target.frontmatter.strength;
    merged.frontmatter.refs = target.frontmatter.refs;
    merged.frontmatter.last_ref = target.frontmatter.last_ref.clone();
    // status is NOT copied: a merge yields a live, current entry. Update targets
    // are already active (no-op); for a same-slug landing on a superseded entry
    // this revives it to `active` so the new knowledge stays visible to recall
    // rather than being written into a dead file.
    merged.frontmatter.status = "active".to_string();

    let mut sources = target.frontmatter.sources.clone();
    for s in &new.frontmatter.sources {
        if !sources.contains(s) {
            sources.push(s.clone());
        }
    }
    merged.frontmatter.sources = sources;

    let mut links = target.frontmatter.links.clone();
    for l in &new.frontmatter.links {
        if !links.contains(l) {
            links.push(l.clone());
        }
    }
    merged.frontmatter.links = links;
    merged
}

/// Upsert an entry into the FTS index using its path relative to `project_dir`
/// (the same relative form `recall`/`search` expect).
fn fts_upsert_entry(fts: &FtsIndex, project_dir: &Path, entry: &l1::L1Entry) -> Result<()> {
    let rel = entry
        .file_path
        .strip_prefix(project_dir)
        .unwrap_or(&entry.file_path)
        .to_string_lossy()
        .to_string();
    let tags = entry.frontmatter.links.join(" ");
    fts.upsert(&rel, &entry.title, &entry.body, &tags)
}

/// Find active L1 entries related to `entry` via FTS (title + topic query), to
/// feed reconciliation. Excludes `exclude` (the new entry's own slug path) and
/// any non-active entry; returns at most `limit`.
fn fts_candidates(
    conn: &Connection,
    project_dir: &Path,
    entry: &l1::L1Entry,
    exclude: &Path,
    limit: usize,
) -> Vec<l1::L1Entry> {
    let fts = FtsIndex::new(conn);
    let query = format!("{} {}", entry.title, entry.frontmatter.topic);
    let results = fts.search(&query, limit * 3).unwrap_or_default();
    let mut out = Vec::new();
    for r in results {
        let path = project_dir.join(&r.file_path);
        if path == *exclude {
            continue;
        }
        if let Ok(e) = l1::L1Entry::from_file(&path) {
            if e.frontmatter.status == "active" {
                out.push(e);
                if out.len() >= limit {
                    break;
                }
            }
        }
    }
    out
}

/// Apply a reconciliation decision to the store (no network). Returns what it did.
/// - `Add`: write the new entry at its own path.
/// - `Noop`: write nothing.
/// - `Update(i)`: merge the new fact into `candidates[i]` in place.
/// - `Delete(i)`: supersede `candidates[i]` (status `superseded-by:<new slug>`,
///   matching `dedup`'s convention) then write the new entry.
fn apply_decision(
    decision: Reconcile,
    entry: l1::L1Entry,
    candidates: &[l1::L1Entry],
    fts: &FtsIndex,
    project_dir: &Path,
) -> Result<Applied> {
    match decision {
        Reconcile::Noop => Ok(Applied::Noop),
        Reconcile::Add => {
            entry.save()?;
            fts_upsert_entry(fts, project_dir, &entry)?;
            Ok(Applied::Added)
        }
        Reconcile::Update(i) => {
            // Guard: out-of-range target degrades to a plain add (never lose the fact).
            let Some(target) = candidates.get(i) else {
                entry.save()?;
                fts_upsert_entry(fts, project_dir, &entry)?;
                return Ok(Applied::Added);
            };
            let mut merged = merge_into(&entry, target);
            merged.frontmatter.updated = chrono::Utc::now().format("%Y-%m-%d").to_string();
            merged.save()?;
            fts_upsert_entry(fts, project_dir, &merged)?;
            Ok(Applied::Updated)
        }
        Reconcile::Delete(i) => {
            if let Some(target) = candidates.get(i) {
                let mut superseded = target.clone();
                superseded.frontmatter.status =
                    format!("superseded-by:{}", l1::L1Entry::slugify(&entry.frontmatter.topic));
                superseded.save()?;
                // Re-sync FTS (status isn't an FTS column, so this is content-only).
                // What actually keeps the superseded entry from resurfacing as a
                // candidate is `fts_candidates`' read-time `status == "active"` filter.
                fts_upsert_entry(fts, project_dir, &superseded)?;
            }
            entry.save()?;
            fts_upsert_entry(fts, project_dir, &entry)?;
            Ok(Applied::Added)
        }
    }
}

/// Ask Haiku how the new fact reconciles against the candidate entries (T2.2).
/// Falls back to `Add` with no API key or on any error — reconciliation must
/// never drop knowledge it can't confidently place.
async fn reconcile_decision(new: &l1::L1Entry, candidates: &[l1::L1Entry]) -> Reconcile {
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => return Reconcile::Add,
    };
    match reconcile_via_llm(&api_key, new, candidates).await {
        Ok(d) => d,
        Err(_) => Reconcile::Add,
    }
}

async fn reconcile_via_llm(
    api_key: &str,
    new: &l1::L1Entry,
    candidates: &[l1::L1Entry],
) -> Result<Reconcile> {
    let body_preview: String = new.body.chars().take(400).collect();
    let mut cand_block = String::new();
    for (i, c) in candidates.iter().enumerate() {
        let snip: String = c
            .body
            .lines()
            .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .unwrap_or("")
            .chars()
            .take(160)
            .collect();
        cand_block.push_str(&format!("{i}. {}: {snip}\n", c.title));
    }

    let prompt = format!(
        r##"你是知識庫的對賬器。有一條「新事實」和若干「既有條目」，判斷新事實該如何併入。

新事實：
標題：{title}
內容：{body_preview}

既有條目（編號 0 起）：
{cand_block}
回傳 JSON（嚴格格式）：{{"op": "add" | "noop" | "update" | "delete", "target": <既有條目編號或 null>}}
- add：與既有條目無關、或互補的新知識 → 新增（target = null）
- noop：某既有條目已表達同一事實 → 不新增（target = 該編號）
- update：某既有條目講同一主題，新事實是更精確/更完整的版本 → 併入該條目（target = 該編號）
- delete：新事實與某既有條目矛盾、使其過時 → 取代該條目（target = 該編號）
不確定時一律選 add（絕不可遺失知識）。只輸出 JSON。"##,
        title = new.title,
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "claude-haiku-4-5-20251001",
            "max_tokens": 256,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("reconcile API 錯誤：{}", resp.status());
    }

    let v: serde_json::Value = resp.json().await?;
    let text = v["content"][0]["text"].as_str().unwrap_or("{}");
    Ok(parse_reconcile_decision(text, candidates.len()))
}

/// Parse the reconciliation LLM's JSON into a [`Reconcile`]. Any malformed output,
/// unknown op, or out-of-range `target` degrades to [`Reconcile::Add`] — the safe
/// default that never drops knowledge.
fn parse_reconcile_decision(text: &str, n_candidates: usize) -> Reconcile {
    let json = match extract_json(text) {
        Ok(j) => j,
        Err(_) => return Reconcile::Add,
    };
    let v: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return Reconcile::Add,
    };
    let target = v["target"].as_u64().map(|t| t as usize);
    let in_range = |t: Option<usize>| t.filter(|&i| i < n_candidates);
    match v["op"].as_str().unwrap_or("add") {
        "noop" => in_range(target).map(|_| Reconcile::Noop).unwrap_or(Reconcile::Add),
        "update" => in_range(target).map(Reconcile::Update).unwrap_or(Reconcile::Add),
        "delete" => in_range(target).map(Reconcile::Delete).unwrap_or(Reconcile::Add),
        _ => Reconcile::Add,
    }
}

async fn compile_signal(_ctx: &db::Context, signal: &l0::Signal) -> Result<Option<l1::L1Entry>> {
    // 嘗試使用 Claude API（若有設定 ANTHROPIC_API_KEY）
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        if !api_key.is_empty() {
            return compile_via_llm(&api_key, signal).await;
        }
    }

    // Fallback：rule-based 分類（不需要 LLM）
    compile_rule_based(signal)
}

async fn compile_via_llm(api_key: &str, signal: &l0::Signal) -> Result<Option<l1::L1Entry>> {
    let prompt = format!(
        r##"你是一個知識編譯器。將下方的 signal 編譯為結構化的 L1 知識條目。

Signal 內容：
{}

輸出 JSON（嚴格遵守格式）：
{{
  "type": "concept" | "connection" | "qa",
  "topic": "簡短主題 3-10 字",
  "title": "完整標題",
  "body": "完整 Markdown 內容，含 wikilink 至少 2 個",
  "links": ["topic1", "topic2"],
  "quality": 0.0-1.0
}}

type 說明：
- concept：獨立知識點（事實、定義、技術要點）
- connection：兩個概念的關係（X 如何影響 Y）
- qa：問答對（問題 + 解答）

若 signal 品質太低（無意義、重複、過短），回傳{{"quality": 0.0}}"##,
        signal.content
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "claude-haiku-4-5-20251001",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("LLM API 錯誤：{}", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    let text = body["content"][0]["text"].as_str().unwrap_or("{}");

    // 從回應中提取 JSON
    let json_str = extract_json(text)?;
    let v: serde_json::Value = serde_json::from_str(&json_str)?;

    let quality = v["quality"].as_f64().unwrap_or(0.0);
    if quality < 0.3 {
        return Ok(None);
    }

    let kind = match v["type"].as_str().unwrap_or("concept") {
        "connection" => l1::L1Type::Connection,
        "qa" => l1::L1Type::Qa,
        _ => l1::L1Type::Concept,
    };

    let topic = v["topic"].as_str().unwrap_or("unknown").to_string();
    let title_line = v["title"].as_str().unwrap_or("# Unknown").to_string();
    let body_text = v["body"].as_str().unwrap_or("").to_string();
    let links: Vec<String> = v["links"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let fm = l1::L1Frontmatter {
        kind,
        topic,
        created: now.clone(),
        updated: now,
        sources: vec![signal.id.clone()],
        links,
        refs: 0,
        last_ref: None,
        strength: 1.0,
        status: "active".to_string(),
    };

    Ok(Some(l1::L1Entry {
        frontmatter: fm,
        title: title_line.trim_start_matches("# ").to_string(),
        body: body_text,
        file_path: std::path::PathBuf::new(), // 會在 caller 設定
    }))
}

fn compile_rule_based(signal: &l0::Signal) -> Result<Option<l1::L1Entry>> {
    let content = signal.content.trim();
    if content.len() < 10 {
        return Ok(None);
    }

    // 簡單的 rule-based 分類
    let kind = if content.contains('?')
        || content.to_lowercase().contains("如何")
        || content.to_lowercase().contains("怎麼")
    {
        l1::L1Type::Qa
    } else if content.contains(" vs ") || content.contains(" 和 ") || content.contains(" 與 ") {
        l1::L1Type::Connection
    } else {
        l1::L1Type::Concept
    };

    // 取前 50 字作為 topic
    let topic = content
        .chars()
        .take(50)
        .collect::<String>()
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");

    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let fm = l1::L1Frontmatter {
        kind,
        topic: topic.clone(),
        created: now.clone(),
        updated: now,
        sources: vec![signal.id.clone()],
        links: vec![], // rule-based 無 wikilinks（LLM 才能生成）
        refs: 0,
        last_ref: None,
        strength: 0.7, // rule-based 品質較低
        status: "active".to_string(),
    };

    Ok(Some(l1::L1Entry {
        frontmatter: fm,
        title: topic,
        body: format!(
            "# {}\n\n{}",
            content.chars().take(50).collect::<String>(),
            content
        ),
        file_path: std::path::PathBuf::new(),
    }))
}

fn extract_json(text: &str) -> Result<String> {
    // 尋找 JSON block
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Ok(text[start..=end].to_string());
        }
    }
    anyhow::bail!("LLM 回應中找不到 JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::l1::{L1Entry, L1Frontmatter, L1Type};
    use tempfile::TempDir;

    fn entry(store: &Path, topic: &str, body: &str) -> L1Entry {
        let slug = L1Entry::slugify(topic);
        let file_path = L1Entry::path_for(store, &L1Type::Concept, &slug);
        L1Entry {
            frontmatter: L1Frontmatter {
                kind: L1Type::Concept,
                topic: topic.to_string(),
                created: "2026-01-01".to_string(),
                updated: "2026-01-01".to_string(),
                sources: vec!["sig-old".to_string()],
                links: vec![],
                refs: 3,
                last_ref: Some("2026-02-01".to_string()),
                strength: 0.8,
                status: "active".to_string(),
            },
            title: format!("title for {topic}"),
            body: body.to_string(),
            file_path,
        }
    }

    fn fts_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    // ---- parse_reconcile_decision (pure) ----

    #[test]
    fn parse_add_when_op_add_or_unknown() {
        assert_eq!(parse_reconcile_decision(r#"{"op":"add","target":null}"#, 2), Reconcile::Add);
        assert_eq!(parse_reconcile_decision(r#"{"op":"bogus","target":0}"#, 2), Reconcile::Add);
    }

    #[test]
    fn parse_update_and_delete_and_noop_with_valid_target() {
        assert_eq!(parse_reconcile_decision(r#"{"op":"update","target":1}"#, 2), Reconcile::Update(1));
        assert_eq!(parse_reconcile_decision(r#"{"op":"delete","target":0}"#, 2), Reconcile::Delete(0));
        assert_eq!(parse_reconcile_decision(r#"{"op":"noop","target":0}"#, 2), Reconcile::Noop);
    }

    #[test]
    fn parse_out_of_range_or_missing_target_degrades_to_add() {
        // target ≥ n_candidates, or null, on a non-add op → Add (never drop the fact).
        assert_eq!(parse_reconcile_decision(r#"{"op":"update","target":5}"#, 2), Reconcile::Add);
        assert_eq!(parse_reconcile_decision(r#"{"op":"noop","target":null}"#, 2), Reconcile::Add);
        assert_eq!(parse_reconcile_decision("not json at all", 2), Reconcile::Add);
    }

    // ---- apply_decision (file effects, no network) ----

    #[test]
    fn apply_noop_writes_nothing() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let conn = fts_conn();
        let fts = FtsIndex::new(&conn);
        let new = entry(&store, "fresh fact", "# t\n\nnew body");
        let cand = entry(&store, "existing fact", "# t\n\nold body");

        let applied = apply_decision(Reconcile::Noop, new.clone(), &[cand], &fts, dir.path()).unwrap();
        assert_eq!(applied, Applied::Noop);
        assert!(!new.file_path.exists(), "NOOP must not write the new entry");
    }

    #[test]
    fn apply_add_writes_new_file() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let conn = fts_conn();
        let fts = FtsIndex::new(&conn);
        let new = entry(&store, "fresh fact", "# t\n\nnew body");

        let applied = apply_decision(Reconcile::Add, new.clone(), &[], &fts, dir.path()).unwrap();
        assert_eq!(applied, Applied::Added);
        assert!(new.file_path.exists());
    }

    #[test]
    fn apply_update_merges_into_target_and_preserves_metadata() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let conn = fts_conn();
        let fts = FtsIndex::new(&conn);

        let mut target = entry(&store, "shutdown signal", "# t\n\nold explanation");
        target.frontmatter.created = "2025-12-01".to_string();
        target.save().unwrap();

        let mut new = entry(&store, "tokio shutdown", "# t\n\nrefined explanation");
        new.frontmatter.sources = vec!["sig-new".to_string()];

        let applied =
            apply_decision(Reconcile::Update(0), new.clone(), std::slice::from_ref(&target), &fts, dir.path())
                .unwrap();
        assert_eq!(applied, Applied::Updated);

        // The new entry's *own* path is never written — the merge lands on target's path.
        assert!(!new.file_path.exists(), "update must not create a second file");
        let merged = L1Entry::from_file(&target.file_path).unwrap();
        assert!(merged.body.contains("refined explanation"), "body refreshed");
        assert_eq!(merged.frontmatter.created, "2025-12-01", "created preserved");
        assert!(merged.frontmatter.sources.contains(&"sig-old".to_string()), "old source kept");
        assert!(merged.frontmatter.sources.contains(&"sig-new".to_string()), "new source unioned");
        assert_ne!(merged.frontmatter.updated, "2026-01-01", "updated bumped to today");
    }

    #[test]
    fn apply_delete_supersedes_target_and_adds_new() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let conn = fts_conn();
        let fts = FtsIndex::new(&conn);

        let target = entry(&store, "old wrong fact", "# t\n\nuse Notify::notify_waiters");
        target.save().unwrap();
        let new = entry(&store, "corrected shutdown fact", "# t\n\nuse mpsc::channel(1)");

        let applied =
            apply_decision(Reconcile::Delete(0), new.clone(), std::slice::from_ref(&target), &fts, dir.path())
                .unwrap();
        assert_eq!(applied, Applied::Added);

        let old = L1Entry::from_file(&target.file_path).unwrap();
        assert!(
            old.frontmatter.status.starts_with("superseded-by:"),
            "old fact superseded, got status: {}",
            old.frontmatter.status
        );
        assert!(new.file_path.exists(), "the corrected fact is written");
    }

    #[test]
    fn merge_into_unions_links_and_keeps_target_strength() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let mut target = entry(&store, "t", "old");
        target.frontmatter.strength = 0.95;
        target.frontmatter.links = vec!["a".to_string()];
        let mut new = entry(&store, "t2", "new");
        new.frontmatter.strength = 0.5;
        new.frontmatter.links = vec!["b".to_string()];

        let merged = merge_into(&new, &target);
        assert_eq!(merged.frontmatter.strength, 0.95, "target strength preserved");
        assert_eq!(merged.file_path, target.file_path);
        assert!(merged.frontmatter.links.contains(&"a".to_string()));
        assert!(merged.frontmatter.links.contains(&"b".to_string()));
    }

    #[test]
    fn merge_into_revives_a_superseded_target_to_active() {
        // A new signal landing on a superseded same-slug entry must revive it —
        // otherwise the fresh knowledge is written into a dead (recall-invisible) file.
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let mut target = entry(&store, "t", "old");
        target.frontmatter.status = "superseded-by:something".to_string();
        let new = entry(&store, "t", "new");

        let merged = merge_into(&new, &target);
        assert_eq!(merged.frontmatter.status, "active", "merge revives to active");
    }

    #[test]
    fn apply_update_out_of_range_target_degrades_to_add() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let conn = fts_conn();
        let fts = FtsIndex::new(&conn);
        let new = entry(&store, "fresh fact", "# t\n\nbody");

        // No candidates but Update(0) → must not panic / drop; writes the new fact.
        let applied = apply_decision(Reconcile::Update(0), new.clone(), &[], &fts, dir.path()).unwrap();
        assert_eq!(applied, Applied::Added);
        assert!(new.file_path.exists());
    }

    #[test]
    fn apply_delete_out_of_range_target_still_adds_new() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let conn = fts_conn();
        let fts = FtsIndex::new(&conn);
        let new = entry(&store, "fresh fact", "# t\n\nbody");

        let applied = apply_decision(Reconcile::Delete(0), new.clone(), &[], &fts, dir.path()).unwrap();
        assert_eq!(applied, Applied::Added);
        assert!(new.file_path.exists());
    }
}
