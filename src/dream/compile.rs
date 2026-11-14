use crate::db;
use crate::memory::fts::FtsIndex;
use crate::memory::{l0, l1};
/// Dream Compile：L0 signals → L1 Markdown（增量、批次 LLM 編譯與去重）
use anyhow::Result;
use rusqlite::Connection;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;

const DEFAULT_COMPILE_MAX_ITEMS: usize = 10;

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

struct LlmHook<'a> {
    fake: Option<&'a mut (dyn FnMut(&str, &str) -> Result<crate::llm::LlmOutput> + Send)>,
}

impl LlmHook<'_> {
    fn is_fake(&self) -> bool {
        self.fake.is_some()
    }

    fn call_if_fake(&mut self, prompt: &str, model: &str) -> Option<Result<crate::llm::LlmOutput>> {
        self.fake.as_mut().map(|f| f(prompt, model))
    }
}

fn compile_max_items() -> usize {
    std::env::var("CODEFORGE_COMPILE_MAX_ITEMS")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_COMPILE_MAX_ITEMS)
}

pub async fn run(ctx: &db::Context, conn: &Connection) -> Result<CompileResult> {
    let mut hook = LlmHook { fake: None };
    run_with(ctx, conn, compile_max_items(), &mut hook).await
}

async fn run_with(
    ctx: &db::Context,
    conn: &Connection,
    max_items: usize,
    hook: &mut LlmHook<'_>,
) -> Result<CompileResult> {
    let located = l0::read_uncompiled_located(ctx, conn)?;
    if located.is_empty() {
        return Ok(CompileResult {
            signals_processed: 0,
            l1_created: 0,
            l1_updated: 0,
            l1_noop: 0,
        });
    }

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut absorbed_items: Vec<l0::Signal> = Vec::new();
    let mut llm_batch: Vec<l0::Signal> = Vec::new();
    let mut llm_sigs: Vec<String> = Vec::new();
    let mut batch_sigs: HashSet<String> = HashSet::new();
    let mut blocked_files: HashSet<String> = HashSet::new();
    let mut cursor_advances: HashMap<String, i64> = HashMap::new();
    let mut skipped_dupes = 0usize;
    let mut deferred = 0usize;

    for loc in located {
        // 一旦某個檔案出現超額未處理的 signal，該檔其餘 signals 全數延後，
        // 避免 single-offset cursor 跳過中間未編譯的條目。
        if blocked_files.contains(&loc.source_file) {
            deferred += 1;
            continue;
        }
        if origin_for(&loc.signal.source) == "absorbed" {
            absorbed_items.push(loc.signal);
            cursor_advances.insert(loc.source_file, loc.end_offset);
            continue;
        }
        let sig = signal_signature(&loc.signal);
        if batch_sigs.contains(&sig) || is_signature_compiled_on_date(conn, &sig, &today)? {
            skipped_dupes += 1;
            cursor_advances.insert(loc.source_file, loc.end_offset);
            continue;
        }
        if llm_batch.len() >= max_items {
            deferred += 1;
            blocked_files.insert(loc.source_file);
            continue;
        }
        batch_sigs.insert(sig.clone());
        llm_sigs.push(sig);
        llm_batch.push(loc.signal);
        cursor_advances.insert(loc.source_file, loc.end_offset);
    }

    let model = crate::llm::compile_model();
    let mut compiled_entries: Vec<l1::L1Entry> = Vec::new();

    for signal in &absorbed_items {
        match compile_rule_based(signal) {
            Ok(Some(entry)) => compiled_entries.push(entry),
            Ok(None) => {}
            Err(e) => eprintln!(
                "  警告：compile absorbed signal {} 失敗：{}",
                signal.id.chars().take(8).collect::<String>(),
                e
            ),
        }
    }

    let mut compile_engine: Option<&'static str> = None;
    let mut compile_usage: Option<crate::llm::TokenUsage> = None;
    if !llm_batch.is_empty() {
        let (batch_results, engine, usage) = compile_batch(&llm_batch, &model, hook).await;
        compile_engine = Some(engine);
        compile_usage = usage;
        for entry in batch_results.into_iter().flatten() {
            compiled_entries.push(entry);
        }
        for sig in &llm_sigs {
            record_compiled_signature(conn, sig, &today)?;
        }
    }

    let store_dir = ctx.project_dir.join("store");
    let fts = FtsIndex::new(conn);

    struct PendingItem {
        entry: l1::L1Entry,
        candidates: Vec<l1::L1Entry>,
        existed_before_run: bool,
    }

    let mut pending: Vec<PendingItem> = Vec::with_capacity(compiled_entries.len());
    for mut entry in compiled_entries {
        let file_path = l1::L1Entry::path_for(
            &store_dir,
            &entry.frontmatter.kind,
            &l1::L1Entry::slugify(&entry.frontmatter.topic),
        );
        entry.file_path = file_path.clone();
        let existed_before_run = file_path.exists();
        let candidates = if existed_before_run {
            Vec::new()
        } else {
            fts_candidates(conn, &ctx.project_dir, &entry, &file_path, 3)
        };
        pending.push(PendingItem {
            entry,
            candidates,
            existed_before_run,
        });
    }

    let reconcile_targets: Vec<usize> = pending
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            if !item.existed_before_run && !item.candidates.is_empty() {
                Some(idx)
            } else {
                None
            }
        })
        .collect();

    let mut reconcile_map: HashMap<usize, Reconcile> = HashMap::new();
    let mut reconcile_engine: Option<&'static str> = None;
    let mut reconcile_usage: Option<crate::llm::TokenUsage> = None;
    if !reconcile_targets.is_empty() {
        let batch_input: Vec<(&l1::L1Entry, &[l1::L1Entry])> = reconcile_targets
            .iter()
            .map(|&idx| (&pending[idx].entry, pending[idx].candidates.as_slice()))
            .collect();
        let (decisions, engine, usage) = reconcile_batch(&batch_input, &model, hook).await;
        reconcile_engine = engine;
        reconcile_usage = usage;
        for (idx, decision) in reconcile_targets.into_iter().zip(decisions) {
            reconcile_map.insert(idx, decision);
        }
    }

    let mut l1_created = 0;
    let mut l1_updated = 0;
    let mut l1_noop = 0;

    for (idx, item) in pending.into_iter().enumerate() {
        let mut entry = item.entry;
        if entry.file_path.exists() {
            // 同 slug（包含同批次稍早已寫入的新檔）→ in-place 合併
            if let Ok(existing) = l1::L1Entry::from_file(&entry.file_path) {
                entry = merge_into(&entry, &existing);
            }
            entry.save()?;
            fts_upsert_entry(&fts, &ctx.project_dir, &entry)?;
            l1_updated += 1;
        } else {
            let decision = reconcile_map.remove(&idx).unwrap_or(Reconcile::Add);
            match apply_decision(decision, entry, &item.candidates, &fts, &ctx.project_dir)? {
                Applied::Added => l1_created += 1,
                Applied::Updated => l1_updated += 1,
                Applied::Noop => l1_noop += 1,
            }
        }
    }

    for (source_file, offset) in cursor_advances {
        l0::advance_cursor(conn, &source_file, offset)?;
    }

    let reconcile_called = reconcile_engine.is_some();
    if !llm_batch.is_empty() || reconcile_called {
        let engine = compile_engine
            .or(reconcile_engine)
            .unwrap_or("rule");
        let total_usage = merge_usage(compile_usage, reconcile_usage);
        append_usage_log(
            &ctx.data_dir(),
            llm_batch.len(),
            skipped_dupes,
            deferred,
            &model,
            engine,
            total_usage.as_ref(),
        );
    }

    Ok(CompileResult {
        signals_processed: absorbed_items.len() + llm_batch.len() + skipped_dupes,
        l1_created,
        l1_updated,
        l1_noop,
    })
}

/// Dry-run:對 dry-ingest 出的 signals 跑 compile_signal(LLM)看會編出什麼 L1,
/// 印出預覽但**不 save、不 mark_compiled、不對賬寫入**。回 (會升L1, 低品質跳過, 失敗)。
pub async fn run_dry(ctx: &db::Context, signals: &[l0::Signal]) -> Result<(usize, usize, usize)> {
    let (mut created, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    for signal in signals {
        match compile_signal(ctx, signal).await {
            Ok(Some(entry)) => {
                created += 1;
                let body_head: String = entry
                    .body
                    .lines()
                    .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect();
                println!(
                    "  ✓ 會升 L1 [{}] {} — {}",
                    entry.frontmatter.topic, entry.title, body_head
                );
            }
            Ok(None) => {
                skipped += 1;
                println!(
                    "  · 低品質跳過:{}",
                    signal.content.chars().take(60).collect::<String>()
                );
            }
            Err(e) => {
                failed += 1;
                println!("  ⚠ compile 失敗:{e}");
            }
        }
    }
    Ok((created, skipped, failed))
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
    // origin 不降級:既有為 dev/session(乾淨來源)就保留,別被後到的 absorbed 蓋掉
    // (對齊 origin-purity 規則 — ship 用 origin 排除 absorbed 跨專案 memory)。
    if matches!(target.frontmatter.origin.as_str(), "dev" | "session") {
        merged.frontmatter.origin = target.frontmatter.origin.clone();
    }

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
                superseded.frontmatter.status = format!(
                    "superseded-by:{}",
                    l1::L1Entry::slugify(&entry.frontmatter.topic)
                );
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

/// Ask the LLM how the new fact reconciles against the candidate entries (T2.2).
/// Mirrors `compile_signal`'s backend chain: claude -p headless → `ANTHROPIC_API_KEY`
/// (Haiku) → `Add`. Any unavailable backend, error, or unparseable reply degrades to
/// `Add` — reconciliation must never drop knowledge it can't confidently place.
/// 對單一 run 中所有需要對賬的新條目，發起至多一次批次對賬呼叫（PM 01M7C8NM）。
/// 沿用 compile backend 鏈：headless 三引擎 → ANTHROPIC_API_KEY → 預設 Add。
/// 任一元素無法解析或失敗皆降級為 `Reconcile::Add`（絕不遺失知識）。
async fn reconcile_batch(
    items: &[(&l1::L1Entry, &[l1::L1Entry])],
    model: &str,
    hook: &mut LlmHook<'_>,
) -> (
    Vec<Reconcile>,
    Option<&'static str>,
    Option<crate::llm::TokenUsage>,
) {
    let prompt = build_batch_reconcile_prompt(items);
    let candidate_lens: Vec<usize> = items.iter().map(|(_, cands)| cands.len()).collect();

    if let Some(res) = hook.call_if_fake(&prompt, model) {
        return match res {
            Ok(out) => (
                parse_batch_reconcile_decisions(&out.text, &candidate_lens),
                Some(out.engine),
                out.usage,
            ),
            Err(_) => (vec![Reconcile::Add; items.len()], Some("rule"), None),
        };
    }

    if let Ok(out) = crate::llm::headless_digest_with_usage(&prompt, model) {
        return (
            parse_batch_reconcile_decisions(&out.text, &candidate_lens),
            Some(out.engine),
            out.usage,
        );
    }
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        if !api_key.is_empty() {
            if let Ok((text, usage)) = call_haiku_compile_with_usage(&api_key, model, &prompt).await
            {
                return (
                    parse_batch_reconcile_decisions(&text, &candidate_lens),
                    Some("api"),
                    usage,
                );
            }
        }
    }
    (vec![Reconcile::Add; items.len()], None, None)
}

fn build_batch_reconcile_prompt(items: &[(&l1::L1Entry, &[l1::L1Entry])]) -> String {
    let mut blocks = String::new();
    for (idx, (new, candidates)) in items.iter().enumerate() {
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
        blocks.push_str(&format!(
            "\n=== 項目 index {idx} ===\n新事實標題：{}\n新事實內容：{body_preview}\n既有條目（編號 0 起）：\n{cand_block}",
            new.title
        ));
    }

    format!(
        r##"你是知識庫的對賬器。下方有 {n} 個「新事實」待對賬條目，請為每個項目判斷如何併入對應的既有條目。
{blocks}
回傳 JSON 陣列（嚴格格式，共 {n} 個元素）：
[{{"index": 0, "op": "add" | "noop" | "update" | "delete", "target": <既有條目編號或 null>}}]
- add：與既有條目無關、或互補的新知識 → 新增（target = null）
- noop：某既有條目已表達同一事實 → 不新增（target = 該編號）
- update：某既有條目講同一主題，新事實是更精確/更完整的版本 → 併入該條目（target = 該編號）
- delete：新事實與某既有條目矛盾、使其過時 → 取代該條目（target = 該編號）
不確定時一律選 add（絕不可遺失知識）。只輸出 JSON 陣列。"##,
        n = items.len()
    )
}

fn reconcile_from_value(v: &serde_json::Value, n_candidates: usize) -> Reconcile {
    let target = v["target"].as_u64().map(|t| t as usize);
    let in_range = |t: Option<usize>| t.filter(|&i| i < n_candidates);
    match v["op"].as_str().unwrap_or("add") {
        "noop" => in_range(target)
            .map(|_| Reconcile::Noop)
            .unwrap_or(Reconcile::Add),
        "update" => in_range(target)
            .map(Reconcile::Update)
            .unwrap_or(Reconcile::Add),
        "delete" => in_range(target)
            .map(Reconcile::Delete)
            .unwrap_or(Reconcile::Add),
        _ => Reconcile::Add,
    }
}

fn parse_batch_reconcile_decisions(text: &str, candidate_lens: &[usize]) -> Vec<Reconcile> {
    let mut decisions = vec![Reconcile::Add; candidate_lens.len()];
    let arr = match extract_json_array(text) {
        Ok(arr) => arr,
        Err(_) => {
            if candidate_lens.len() == 1 {
                decisions[0] = parse_reconcile_decision(text, candidate_lens[0]);
            }
            return decisions;
        }
    };
    for (pos, val) in arr.into_iter().enumerate() {
        if !val.is_object() {
            continue;
        }
        let slot = val["index"]
            .as_u64()
            .map(|u| u as usize)
            .unwrap_or(pos);
        if slot >= candidate_lens.len() {
            continue;
        }
        decisions[slot] = reconcile_from_value(&val, candidate_lens[slot]);
    }
    decisions
}

/// Parse a single reconciliation JSON object into [`Reconcile`]. Any malformed
/// output, unknown op, or out-of-range `target` degrades to [`Reconcile::Add`].
fn parse_reconcile_decision(text: &str, n_candidates: usize) -> Reconcile {
    let json = match extract_json(text) {
        Ok(j) => j,
        Err(_) => return Reconcile::Add,
    };
    let v: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return Reconcile::Add,
    };
    reconcile_from_value(&v, n_candidates)
}

/// L1 origin 純度標記:absorbed(跨專案 memory,ship 排除)/ session(session-digest 萃取)/ dev(其餘本 repo)。
fn origin_for(source: &l0::SignalSource) -> String {
    match source {
        // AbsorbedMemory(收嚴後)與 ClaudeCodeSession(收嚴前 absorb 的唯一歷史生產者,
        // 現已無其他生產者)皆視為跨專案 absorbed → ship 排除。後者確保收嚴前的「在途」
        // backlog signal(尚未編譯的舊 absorb,仍標 ClaudeCodeSession)一編譯即被擋,
        // 不需逐 repo 手清 signals(review Major 1)。
        l0::SignalSource::AbsorbedMemory | l0::SignalSource::ClaudeCodeSession => "absorbed",
        l0::SignalSource::SessionDigest => "session",
        _ => "dev",
    }
    .to_string()
}

async fn compile_signal(_ctx: &db::Context, signal: &l0::Signal) -> Result<Option<l1::L1Entry>> {
    // ship 一定排除的來源(origin=="absorbed":AbsorbedMemory + 在途 ClaudeCodeSession backlog)
    // 不值得花 LLM;走 rule-based 快速路徑。用 origin_for 當單一真相,避免漏掉 ClaudeCodeSession。
    if origin_for(&signal.source) == "absorbed" {
        return compile_rule_based(signal);
    }

    let prompt = build_compile_prompt(signal);

    // backend fallback 鏈:headless 三引擎(claude -p → agy → codex)→ ANTHROPIC_API_KEY(Haiku)→ rule-based。
    // 注意:parse 出 Ok(None)(quality 太低)是有效「跳過此 signal」,直接回,不降級到 rule-based。
    // 每個 fallback transition 都出聲(對齊 ship.rs;否則 compile 靜默降級無法診斷品質下降)。
    match crate::llm::headless_digest(&prompt, &crate::llm::digest_model()) {
        Ok(text) => match parse_compile_response(&text, signal) {
            Ok(opt) => return Ok(opt),
            Err(e) => eprintln!("  ⚠ compile: headless digest 回應 parse 失敗（{e}）— fallback"),
        },
        Err(e) => eprintln!("  ℹ compile: headless 三引擎不可用（{e}）— fallback API/rule"),
    }
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        if !api_key.is_empty() {
            match call_haiku_compile(&api_key, &prompt).await {
                Ok(text) => match parse_compile_response(&text, signal) {
                    Ok(opt) => return Ok(opt),
                    Err(e) => eprintln!("  ⚠ compile: Haiku 回應 parse 失敗（{e}）— rule-based"),
                },
                Err(e) => eprintln!("  ⚠ compile: Haiku 失敗（{e}）— rule-based"),
            }
        }
    }
    compile_rule_based(signal)
}

fn build_compile_prompt(signal: &l0::Signal) -> String {
    format!(
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
    )
}

/// Haiku API fallback(僅當 claude -p 不可用且有 ANTHROPIC_API_KEY)。回原始回應文字。
async fn call_haiku_compile(api_key: &str, prompt: &str) -> Result<String> {
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
    Ok(body["content"][0]["text"]
        .as_str()
        .unwrap_or("{}")
        .to_string())
}

/// 解析 LLM 回應 → L1Entry。quality<0.3 回 Ok(None)(有效跳過);JSON 壞回 Err(呼叫端 fallback)。
fn parse_compile_response(text: &str, signal: &l0::Signal) -> Result<Option<l1::L1Entry>> {
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
        origin: origin_for(&signal.source),
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
        origin: origin_for(&signal.source),
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
                origin: "dev".to_string(),
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
        assert_eq!(
            parse_reconcile_decision(r#"{"op":"add","target":null}"#, 2),
            Reconcile::Add
        );
        assert_eq!(
            parse_reconcile_decision(r#"{"op":"bogus","target":0}"#, 2),
            Reconcile::Add
        );
    }

    #[test]
    fn parse_update_and_delete_and_noop_with_valid_target() {
        assert_eq!(
            parse_reconcile_decision(r#"{"op":"update","target":1}"#, 2),
            Reconcile::Update(1)
        );
        assert_eq!(
            parse_reconcile_decision(r#"{"op":"delete","target":0}"#, 2),
            Reconcile::Delete(0)
        );
        assert_eq!(
            parse_reconcile_decision(r#"{"op":"noop","target":0}"#, 2),
            Reconcile::Noop
        );
    }

    #[test]
    fn parse_out_of_range_or_missing_target_degrades_to_add() {
        // target ≥ n_candidates, or null, on a non-add op → Add (never drop the fact).
        assert_eq!(
            parse_reconcile_decision(r#"{"op":"update","target":5}"#, 2),
            Reconcile::Add
        );
        assert_eq!(
            parse_reconcile_decision(r#"{"op":"noop","target":null}"#, 2),
            Reconcile::Add
        );
        assert_eq!(
            parse_reconcile_decision("not json at all", 2),
            Reconcile::Add
        );
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

        let applied =
            apply_decision(Reconcile::Noop, new.clone(), &[cand], &fts, dir.path()).unwrap();
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

        let applied = apply_decision(
            Reconcile::Update(0),
            new.clone(),
            std::slice::from_ref(&target),
            &fts,
            dir.path(),
        )
        .unwrap();
        assert_eq!(applied, Applied::Updated);

        // The new entry's *own* path is never written — the merge lands on target's path.
        assert!(
            !new.file_path.exists(),
            "update must not create a second file"
        );
        let merged = L1Entry::from_file(&target.file_path).unwrap();
        assert!(
            merged.body.contains("refined explanation"),
            "body refreshed"
        );
        assert_eq!(
            merged.frontmatter.created, "2025-12-01",
            "created preserved"
        );
        assert!(
            merged.frontmatter.sources.contains(&"sig-old".to_string()),
            "old source kept"
        );
        assert!(
            merged.frontmatter.sources.contains(&"sig-new".to_string()),
            "new source unioned"
        );
        assert_ne!(
            merged.frontmatter.updated, "2026-01-01",
            "updated bumped to today"
        );
    }

    #[test]
    fn apply_delete_supersedes_target_and_adds_new() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let conn = fts_conn();
        let fts = FtsIndex::new(&conn);

        let target = entry(
            &store,
            "old wrong fact",
            "# t\n\nuse Notify::notify_waiters",
        );
        target.save().unwrap();
        let new = entry(
            &store,
            "corrected shutdown fact",
            "# t\n\nuse mpsc::channel(1)",
        );

        let applied = apply_decision(
            Reconcile::Delete(0),
            new.clone(),
            std::slice::from_ref(&target),
            &fts,
            dir.path(),
        )
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
        assert_eq!(
            merged.frontmatter.strength, 0.95,
            "target strength preserved"
        );
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
        assert_eq!(
            merged.frontmatter.status, "active",
            "merge revives to active"
        );
    }

    #[test]
    fn apply_update_out_of_range_target_degrades_to_add() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().join("store");
        let conn = fts_conn();
        let fts = FtsIndex::new(&conn);
        let new = entry(&store, "fresh fact", "# t\n\nbody");

        // No candidates but Update(0) → must not panic / drop; writes the new fact.
        let applied =
            apply_decision(Reconcile::Update(0), new.clone(), &[], &fts, dir.path()).unwrap();
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

        let applied =
            apply_decision(Reconcile::Delete(0), new.clone(), &[], &fts, dir.path()).unwrap();
        assert_eq!(applied, Applied::Added);
        assert!(new.file_path.exists());
    }
}
