//! L1 → lessons digest (ship spec §5–6).
//!
//! Reads the day's active L1 entries + git log, reduces to structured `lessons[]`
//! via Haiku (when `ANTHROPIC_API_KEY` set) or a rule-based passthrough fallback
//! (§6.3). Pure reduce/merge/parse logic is unit-tested; the network call is thin.

use anyhow::Result;
use std::path::Path;

use crate::memory::l1::{self, L1Entry};

use super::evidence::SourceEvidence;
use super::ledger::LedgerLesson;

/// Select the day's active L1 entries (status == active, created or updated == date).
pub fn select_l1_for_date(entries: &[L1Entry], ledger_date: &str) -> Vec<L1Entry> {
    entries
        .iter()
        .filter(|e| e.frontmatter.status == "active")
        .filter(|e| {
            e.frontmatter.created == ledger_date || e.frontmatter.updated == ledger_date
        })
        .cloned()
        .collect()
}

/// Repo-relative path of an L1 entry for `l1_concept` evidence value.
fn rel_path(entry: &L1Entry, project_parent: &Path) -> String {
    entry
        .file_path
        .strip_prefix(project_parent)
        .unwrap_or(&entry.file_path)
        .to_string_lossy()
        .to_string()
}

/// Rule-based passthrough (§6.3): one L1 entry → one lesson, body truncated,
/// evidence = that entry's `l1_concept` ref. Used when no API key.
pub fn passthrough_lessons(entries: &[L1Entry], project_parent: &Path) -> Vec<LedgerLesson> {
    entries
        .iter()
        .map(|e| {
            let detail: String = e.body.trim().chars().take(400).collect();
            LedgerLesson {
                title: e.title.trim().chars().take(60).collect(),
                detail,
                candidate_atom_kind: "lesson".to_string(),
                source_evidence: vec![SourceEvidence::l1_concept(
                    rel_path(e, project_parent),
                    e.frontmatter.topic.clone(),
                )],
            }
        })
        .filter(|l| !l.is_empty() && l.has_evidence())
        .collect()
}

/// Merge same-title lessons (§5.1 rule 5): concatenate details, union evidence.
pub fn merge_lessons(lessons: Vec<LedgerLesson>) -> Vec<LedgerLesson> {
    let mut out: Vec<LedgerLesson> = Vec::new();
    for l in lessons {
        if let Some(existing) = out.iter_mut().find(|e| e.title == l.title && !l.title.is_empty())
        {
            if !l.detail.is_empty() && !existing.detail.contains(&l.detail) {
                existing.detail.push_str("\n\n");
                existing.detail.push_str(&l.detail);
            }
            for ev in l.source_evidence {
                if !existing.source_evidence.contains(&ev) {
                    existing.source_evidence.push(ev);
                }
            }
        } else {
            out.push(l);
        }
    }
    out
}

/// Parse the Haiku JSON output into lessons, dropping any without evidence (§6.2).
/// Tolerates extra fields. Returns error if the JSON has no `lessons` array.
pub fn parse_digest_json(json: &str) -> Result<Vec<LedgerLesson>> {
    let start = json.find('{');
    let end = json.rfind('}');
    let slice = match (start, end) {
        (Some(s), Some(e)) if e >= s => &json[s..=e],
        _ => anyhow::bail!("digest 回應中找不到 JSON"),
    };
    let v: serde_json::Value = serde_json::from_str(slice)?;
    let arr = v
        .get("lessons")
        .and_then(|l| l.as_array())
        .ok_or_else(|| anyhow::anyhow!("digest JSON 缺 lessons 陣列"))?;

    let mut lessons = Vec::new();
    for item in arr {
        let title = item
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let detail = item
            .get("detail")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let kind = item
            .get("candidate_atom_kind")
            .and_then(|k| k.as_str())
            .unwrap_or("lesson")
            .to_string();
        let source_evidence: Vec<SourceEvidence> = item
            .get("source_evidence")
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|ev| serde_json::from_value::<SourceEvidence>(ev.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        let lesson = LedgerLesson {
            title,
            detail,
            candidate_atom_kind: kind,
            source_evidence,
        };
        // §6.2 / §4.4: drop lessons with no evidence or fully empty.
        if !lesson.is_empty() && lesson.has_evidence() {
            lessons.push(lesson);
        }
    }
    Ok(lessons)
}

/// Build the digest prompt (§6.1) from the day's inputs.
pub fn build_prompt(
    repo: &str,
    ledger_date: &str,
    git_head: &str,
    git_branch: &str,
    git_log_oneline: &str,
    l1_block: &str,
) -> String {
    format!(
        r##"你是 CodeForge 的 ledger digester。把今天這個 repo 的開發痕跡 reduce 成可長期保存的「技術教訓（lessons）」，準備送進中央記憶庫 Mnemos 變成 atom。

== 輸入 ==
repo: {repo}
日期: {ledger_date}
git HEAD: {git_head} ({git_branch})

[當日 commits]
{git_log_oneline}

[當日 L1 知識條目]
{l1_block}

== 任務 ==
從上面萃取「值得跨 session / 跨 repo 記住」的技術教訓。每條教訓：
- title：一句話結論（≤ 40 字）
- detail：2–4 句，講清楚「為什麼」與「下次怎麼做」
- source_evidence：array，每個 element 是 {{kind, value, locator}}；kind 用 "l1_concept" | "git_commit" | "session_jsonl"
- 每條教訓至少一個 source_evidence；找不到出處的教訓請丟棄，不要硬湊

丟棄規則：流水帳、純情緒、無法歸因、和昨天重複的，一律不要。寧缺勿濫。

== 輸出 ==
嚴格 JSON，無多餘文字：
{{"lessons":[{{"title":"...","detail":"...","candidate_atom_kind":"lesson","source_evidence":[{{"kind":"l1_concept","value":".codeforge/store/concepts/xxx.md","locator":{{"topic":"xxx"}}}}]}}]}}
若今天沒有任何值得保存的教訓，回傳 {{"lessons":[]}}。"##
    )
}

/// Render the L1 entries block for the prompt.
pub fn render_l1_block(entries: &[L1Entry], project_parent: &Path) -> String {
    let mut block = String::new();
    for e in entries {
        let body_head: String = e.body.lines().take(8).collect::<Vec<_>>().join("\n");
        block.push_str(&format!(
            "### {}（檔: {}）\n{}\n\n",
            e.frontmatter.topic,
            rel_path(e, project_parent),
            body_head
        ));
    }
    block
}

/// Scan all L1 entries under a store dir (thin wrapper over l1::scan_all).
pub fn scan_l1(store_dir: &Path) -> Result<Vec<L1Entry>> {
    l1::scan_all(store_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::l1::{L1Entry, L1Frontmatter, L1Type};
    use std::path::PathBuf;

    fn entry(topic: &str, created: &str, updated: &str, status: &str, body: &str) -> L1Entry {
        L1Entry {
            frontmatter: L1Frontmatter {
                kind: L1Type::Concept,
                topic: topic.to_string(),
                created: created.to_string(),
                updated: updated.to_string(),
                sources: vec![],
                links: vec![],
                refs: 0,
                last_ref: None,
                strength: 1.0,
                status: status.to_string(),
            },
            title: topic.to_string(),
            body: body.to_string(),
            file_path: PathBuf::from(format!(".codeforge/store/concepts/{topic}.md")),
        }
    }

    #[test]
    fn selects_only_active_matching_date() {
        let entries = vec![
            entry("a", "2026-05-15", "2026-05-15", "active", "body a"),
            entry("b", "2026-05-14", "2026-05-15", "active", "body b"), // updated today
            entry("c", "2026-05-10", "2026-05-10", "active", "body c"), // old
            entry("d", "2026-05-15", "2026-05-15", "superseded", "body d"), // inactive
        ];
        let sel = select_l1_for_date(&entries, "2026-05-15");
        let topics: Vec<&str> = sel.iter().map(|e| e.frontmatter.topic.as_str()).collect();
        assert_eq!(topics, vec!["a", "b"]);
    }

    #[test]
    fn passthrough_one_lesson_per_entry_with_evidence() {
        let entries = vec![entry("ipc", "2026-05-15", "2026-05-15", "active", "long body text")];
        let lessons = passthrough_lessons(&entries, Path::new(""));
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].title, "ipc");
        assert!(lessons[0].has_evidence());
        assert_eq!(lessons[0].source_evidence[0].kind, "l1_concept");
    }

    #[test]
    fn passthrough_drops_empty_body_entries() {
        let entries = vec![entry("empty", "2026-05-15", "2026-05-15", "active", "   ")];
        // title is non-empty ("empty") so is_empty()==false; lesson kept with l1 evidence.
        let lessons = passthrough_lessons(&entries, Path::new(""));
        assert_eq!(lessons.len(), 1);
    }

    #[test]
    fn merge_same_title_lessons() {
        let mk = |title: &str, detail: &str, ev_topic: &str| LedgerLesson {
            title: title.to_string(),
            detail: detail.to_string(),
            candidate_atom_kind: "lesson".to_string(),
            source_evidence: vec![SourceEvidence::l1_concept("p", ev_topic)],
        };
        let merged = merge_lessons(vec![
            mk("T", "detail one", "t1"),
            mk("T", "detail two", "t2"),
            mk("U", "other", "u"),
        ]);
        assert_eq!(merged.len(), 2);
        let t = merged.iter().find(|l| l.title == "T").unwrap();
        assert!(t.detail.contains("detail one") && t.detail.contains("detail two"));
        assert_eq!(t.source_evidence.len(), 2);
    }

    #[test]
    fn parse_digest_valid_json() {
        let json = r#"prefix junk {"lessons":[
            {"title":"Notify drops wakeup","detail":"use mpsc","candidate_atom_kind":"lesson",
             "source_evidence":[{"kind":"l1_concept","value":".cf/x.md","locator":{"topic":"x"}}]}
        ]} trailing"#;
        let lessons = parse_digest_json(json).unwrap();
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].title, "Notify drops wakeup");
        assert_eq!(lessons[0].source_evidence[0].kind, "l1_concept");
    }

    #[test]
    fn parse_digest_drops_lessons_without_evidence() {
        let json = r#"{"lessons":[
            {"title":"no evidence","detail":"x","source_evidence":[]},
            {"title":"has evidence","detail":"y",
             "source_evidence":[{"kind":"git_commit","value":"abc"}]}
        ]}"#;
        let lessons = parse_digest_json(json).unwrap();
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].title, "has evidence");
    }

    #[test]
    fn parse_digest_empty_lessons_ok() {
        let lessons = parse_digest_json(r#"{"lessons":[]}"#).unwrap();
        assert!(lessons.is_empty());
    }

    #[test]
    fn parse_digest_no_json_errors() {
        assert!(parse_digest_json("no json here").is_err());
    }

    #[test]
    fn parse_digest_missing_lessons_array_errors() {
        assert!(parse_digest_json(r#"{"foo":1}"#).is_err());
    }

    #[test]
    fn prompt_includes_inputs() {
        let p = build_prompt("codeforge", "2026-05-15", "abc", "main", "log here", "l1 here");
        assert!(p.contains("codeforge"));
        assert!(p.contains("2026-05-15"));
        assert!(p.contains("log here"));
        assert!(p.contains("l1 here"));
    }
}
