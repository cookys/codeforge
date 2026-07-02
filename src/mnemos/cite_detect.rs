//! cite-detect heuristic (ship spec §9, source-contract §11.6 `fulltext_match`).
//!
//! At SessionEnd, scan a transcript for occurrences of each atom's title.
//! A hit → that atom is considered cited. High false-positive rate is acceptable
//! for Sprint 1; Sprint 5+ upgrades to Haiku judgement.
//!
//! **Feedback-loop guard (RISKS ③)**: the transcript must be scanned over genuine
//! conversation ONLY. A Claude session `.jsonl` also embeds the SessionStart-hook
//! `additionalContext` — including the P1.2 central-recall downlink, which injects
//! atom *titles* straight from the brain. Scanning that would make every surfaced
//! atom self-cite (citation↑ → rank↑ → surfaced more → cited more), a deterministic
//! pollution loop that poisons `recall::rank`. [`citable_text_from_transcript`]
//! strips it by keeping only `user` / `assistant` turns (injected context lives in
//! `type:"attachment"` records).

use super::context::ContextAtom;

/// One detected citation: which atom + the matched text.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedCite {
    pub atom_id: String,
    pub matched_text: String,
}

/// Minimum atom-title "weight" to consider for matching — guards against trivial
/// short titles producing pervasive false positives. Weight (not raw char count)
/// so CJK isn't systematically under-cited: a CJK char is information-dense, so it
/// counts double. A 3-char Chinese title (weight 6) qualifies; a 6-char ASCII one
/// also qualifies; a 2-char CJK title (weight 4) is still rejected as too generic.
const MIN_TITLE_WEIGHT: usize = 6;

/// True for CJK / Kana / Hangul scalar values (dense scripts where a few chars
/// already carry a distinctive title — see [`MIN_TITLE_WEIGHT`]).
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF |   // Hiragana + Katakana
        0x3400..=0x4DBF |   // CJK Unified Ext A
        0x4E00..=0x9FFF |   // CJK Unified
        0xAC00..=0xD7AF |   // Hangul syllables
        0xF900..=0xFAFF |   // CJK Compatibility Ideographs
        0x20000..=0x2FA1F) // CJK Ext B+ / Compatibility Supplement
}

/// Information weight of a title: CJK/Kana/Hangul chars count 2, others 1.
fn title_weight(title: &str) -> usize {
    title.chars().map(|c| if is_cjk(c) { 2 } else { 1 }).sum()
}

/// Detect cited atoms by case-insensitive substring match of atom title in transcript.
/// Each atom yields at most one DetectedCite (dedup by atom_id).
///
/// The caller MUST pass conversation-only text (see
/// [`citable_text_from_transcript`]); this function does no filtering itself.
pub fn detect(transcript: &str, atoms: &[ContextAtom]) -> Vec<DetectedCite> {
    let haystack = transcript.to_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut hits = Vec::new();
    for a in atoms {
        let title = a.title.trim();
        if title_weight(title) < MIN_TITLE_WEIGHT {
            continue;
        }
        if haystack.contains(&title.to_lowercase()) && seen.insert(a.id.clone()) {
            hits.push(DetectedCite {
                atom_id: a.id.clone(),
                matched_text: title.to_string(),
            });
        }
    }
    hits
}

/// Extract only genuine conversation (human + assistant turns) from a Claude Code
/// session transcript, EXCLUDING injected context (SessionStart-hook
/// `additionalContext`, file attachments, tool results — all carried in
/// `type:"attachment"` and other non-conversational records).
///
/// This is the feedback-loop guard: without it, the central-recall downlink's own
/// injected atom titles would be substring-matched and self-cited every SessionEnd.
///
/// If the input is not JSONL (e.g. a plain-text transcript handed to the manual
/// `cite-detect` subcommand), it's returned unchanged.
pub fn citable_text_from_transcript(raw: &str) -> String {
    let mut saw_json = false;
    let mut out = String::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        saw_json = true;
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        // ONLY real conversation turns. `attachment` (hook additionalContext, file
        // pastes), `last-prompt`, `queue-operation`, `system`, … are excluded.
        if ty != "user" && ty != "assistant" {
            continue;
        }
        push_message_text(&v, &mut out);
    }
    if saw_json {
        out
    } else {
        // Not a JSONL transcript — treat as plain text (manual cite-detect path).
        raw.to_string()
    }
}

/// Append the textual content of a user/assistant record's `message.content`.
/// Handles both the string form (simple user turn) and the block-array form
/// (assistant / rich user turns) — only `text` blocks are taken; `tool_use` /
/// `tool_result` / `thinking` blocks are ignored (they can echo injected data).
fn push_message_text(record: &serde_json::Value, out: &mut String) {
    let content = record.get("message").and_then(|m| m.get("content"));
    match content {
        Some(serde_json::Value::String(s)) => {
            out.push_str(s);
            out.push('\n');
        }
        Some(serde_json::Value::Array(blocks)) => {
            for b in blocks {
                let is_text = b.get("type").and_then(|t| t.as_str()) == Some("text");
                if is_text {
                    if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                        out.push_str(t);
                        out.push('\n');
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(id: &str, title: &str) -> ContextAtom {
        ContextAtom {
            id: id.to_string(),
            kind: "lesson".to_string(),
            title: title.to_string(),
            body: String::new(),
            citation_count: 0,
            pinned: false,
        }
    }

    #[test]
    fn matches_title_in_transcript() {
        let atoms = vec![
            atom("01A", "Notify drops wakeup signals"),
            atom("01B", "Unrelated lesson about caching"),
        ];
        let transcript = "we discovered that Notify drops wakeup signals when no waiter exists";
        let hits = detect(transcript, &atoms);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].atom_id, "01A");
        assert_eq!(hits[0].matched_text, "Notify drops wakeup signals");
    }

    #[test]
    fn case_insensitive() {
        let atoms = vec![atom("01A", "Tokio Shutdown Race")];
        let hits = detect("fixed the tokio shutdown race today", &atoms);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn no_match_returns_empty() {
        let atoms = vec![atom("01A", "Something never mentioned here")];
        let hits = detect("totally different content", &atoms);
        assert!(hits.is_empty());
    }

    #[test]
    fn short_titles_skipped() {
        let atoms = vec![atom("01A", "ABC")]; // weight 3 < 6
        let hits = detect("ABC appears here", &atoms);
        assert!(hits.is_empty());
    }

    #[test]
    fn cjk_titles_are_citable_not_under_cited() {
        // C3: a 4-5 char Chinese atom title (weight ≥ 8) must be citable — the old
        // 6-char rule silently dropped the operator's Chinese knowledge.
        let atoms = vec![atom("01A", "記憶體管理")]; // 5 CJK → weight 10
        let hits = detect("今天在討論 記憶體管理 的策略", &atoms);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].atom_id, "01A");
    }

    #[test]
    fn two_char_cjk_title_still_rejected_as_generic() {
        let atoms = vec![atom("01A", "記憶")]; // weight 4 < 6
        let hits = detect("記憶 出現在這裡", &atoms);
        assert!(hits.is_empty());
    }

    #[test]
    fn citable_text_excludes_injected_recall_block() {
        // C1 (CRITICAL): a transcript that only contains the SessionStart-hook
        // injected recall block (an `attachment` record carrying the atom title,
        // exactly what the P1.2 downlink injects) and NO genuine use → 0 cites.
        let atom_title = "Notify drops wakeup signals";
        let transcript = format!(
            "{}\n{}\n",
            serde_json::json!({
                "type": "attachment",
                "attachment": {
                    "hookSpecificOutput": {
                        "hookEventName": "SessionStart",
                        "additionalContext": format!("## Mnemos — relevant memory\n- **{atom_title}**")
                    }
                }
            }),
            serde_json::json!({
                "type": "user",
                "message": { "role": "user", "content": "let's refactor the config loader" }
            }),
        );
        let citable = citable_text_from_transcript(&transcript);
        assert!(
            !citable.contains(atom_title),
            "injected recall must be stripped: {citable:?}"
        );
        let atoms = vec![atom("01A", atom_title)];
        let hits = detect(&citable, &atoms);
        assert!(hits.is_empty(), "injection-only transcript must cite 0");
    }

    #[test]
    fn citable_text_keeps_genuine_use_in_user_and_assistant_turns() {
        // Positive: the atom title genuinely referenced in a real turn IS detected.
        let atom_title = "Notify drops wakeup signals";
        let transcript = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({
                "type": "attachment",
                "attachment": { "additionalContext": format!("- **{atom_title}**") }
            }),
            serde_json::json!({
                "type": "user",
                "message": { "role": "user", "content":
                    format!("apply the lesson: {atom_title}, use an mpsc channel") }
            }),
            serde_json::json!({
                "type": "assistant",
                "message": { "role": "assistant", "content": [
                    { "type": "text", "text": "done" }
                ]}
            }),
        );
        let citable = citable_text_from_transcript(&transcript);
        let atoms = vec![atom("01A", atom_title)];
        let hits = detect(&citable, &atoms);
        assert_eq!(
            hits.len(),
            1,
            "genuine reference in a user turn must be cited"
        );
    }

    #[test]
    fn citable_text_plain_text_passthrough() {
        // Non-JSONL input (manual cite-detect on a plain file) is returned as-is.
        let raw = "just some plain notes mentioning Tokio Shutdown Race here";
        assert_eq!(citable_text_from_transcript(raw), raw);
    }

    #[test]
    fn dedups_by_atom_id() {
        let atoms = vec![atom("01A", "repeated phrase here")];
        let hits = detect(
            "repeated phrase here ... repeated phrase here again",
            &atoms,
        );
        assert_eq!(hits.len(), 1);
    }
}
