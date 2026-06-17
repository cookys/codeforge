use crate::db;
use crate::memory::fts::FtsIndex;
use crate::memory::{l0, l1};
/// Dream Compile：L0 signals → L1 Markdown（增量，使用 Claude Agent SDK）
use anyhow::Result;
use rusqlite::Connection;

pub struct CompileResult {
    pub signals_processed: usize,
    pub l1_created: usize,
    pub l1_updated: usize,
}

pub async fn run(ctx: &db::Context, conn: &Connection) -> Result<CompileResult> {
    let signals = l0::read_uncompiled(ctx, conn)?;
    if signals.is_empty() {
        return Ok(CompileResult {
            signals_processed: 0,
            l1_created: 0,
            l1_updated: 0,
        });
    }

    let store_dir = ctx.project_dir.join("store");
    let fts = FtsIndex::new(conn);

    let mut l1_created = 0;
    let mut l1_updated = 0;

    for signal in &signals {
        // 使用 LLM 分類並生成 L1 entry
        match compile_signal(ctx, signal).await {
            Ok(Some(entry)) => {
                let file_path = l1::L1Entry::path_for(
                    &store_dir,
                    &entry.frontmatter.kind,
                    &l1::L1Entry::slugify(&entry.frontmatter.topic),
                );

                let existed = file_path.exists();
                let mut final_entry = entry;
                final_entry.file_path = file_path.clone();

                // 若已存在，合併（更新 body 並保留舊 sources）
                if existed {
                    if let Ok(existing) = l1::L1Entry::from_file(&file_path) {
                        let mut merged_sources = existing.frontmatter.sources.clone();
                        for s in &final_entry.frontmatter.sources {
                            if !merged_sources.contains(s) {
                                merged_sources.push(s.clone());
                            }
                        }
                        final_entry.frontmatter.sources = merged_sources;
                        final_entry.frontmatter.created = existing.frontmatter.created;
                        final_entry.frontmatter.refs = existing.frontmatter.refs;
                        final_entry.frontmatter.strength = existing.frontmatter.strength;
                        // origin 不降級:既有為 dev/session(乾淨)就保留,別被後到的 absorbed 蓋掉。
                        if matches!(existing.frontmatter.origin.as_str(), "dev" | "session") {
                            final_entry.frontmatter.origin = existing.frontmatter.origin.clone();
                        }
                    }
                    l1_updated += 1;
                } else {
                    l1_created += 1;
                }

                final_entry.save()?;

                // 更新 FTS index
                let rel = file_path
                    .strip_prefix(&ctx.project_dir)
                    .unwrap_or(&file_path)
                    .to_string_lossy()
                    .to_string();
                let tags = final_entry.frontmatter.links.join(" ");
                fts.upsert(&rel, &final_entry.title, &final_entry.body, &tags)?;
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
    })
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
    // absorbed(跨專案 memory)ship 一定排除(origin=="absorbed"),不值得花 LLM;走 rule-based
    // 快速路徑,避免 dream 對一堆跨專案 memory 跑昂貴 claude -p。
    if matches!(signal.source, l0::SignalSource::AbsorbedMemory) {
        return compile_rule_based(signal);
    }

    let prompt = build_compile_prompt(signal);

    // backend fallback 鏈:claude -p headless(免 key,品質最高)→ ANTHROPIC_API_KEY(Haiku)→ rule-based。
    // 注意:parse 出 Ok(None)(quality 太低)是有效「跳過此 signal」,直接回,不降級到 rule-based。
    if let Ok(text) = crate::llm::claude_p(&prompt, &crate::llm::digest_model()) {
        if let Ok(opt) = parse_compile_response(&text, signal) {
            return Ok(opt);
        }
    }
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        if !api_key.is_empty() {
            if let Ok(text) = call_haiku_compile(&api_key, &prompt).await {
                if let Ok(opt) = parse_compile_response(&text, signal) {
                    return Ok(opt);
                }
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
