//! `source_evidence` element (ship spec §4.4) — the citation locator that lets a
//! Mnemos atom open back to its raw origin (citations-mandatory).
//!
//! Structure is locked CodeForge-side; Mnemos keeps it as an opaque `serde_json::Value`
//! (filters null only). Each element aligns with RETRIEVAL_SPEC §2.1 ref shape.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One `source_evidence` element: `{ kind, value, locator? }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceEvidence {
    /// Origin type. Sprint 1: `session_jsonl` | `git_commit` | `l1_concept`.
    pub kind: String,
    /// Primary identifier. jsonl path / full sha / repo-relative path.
    pub value: String,
    /// Precise locator; omitted when empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<Value>,
}

impl SourceEvidence {
    /// `l1_concept` evidence — repo-relative markdown path + topic locator.
    pub fn l1_concept(rel_path: impl Into<String>, topic: impl Into<String>) -> Self {
        Self {
            kind: "l1_concept".to_string(),
            value: rel_path.into(),
            locator: Some(serde_json::json!({ "topic": topic.into() })),
        }
    }

    /// `git_commit` evidence — full sha, no locator.
    pub fn git_commit(sha: impl Into<String>) -> Self {
        Self {
            kind: "git_commit".to_string(),
            value: sha.into(),
            locator: None,
        }
    }

    /// `session_jsonl` evidence — jsonl path + `{uuid, ts, line}` locator.
    /// Constructor for Haiku-attributed session evidence; the digest path also
    /// deserializes this shape directly from model output.
    #[allow(dead_code)]
    pub fn session_jsonl(
        path: impl Into<String>,
        uuid: impl Into<String>,
        ts: impl Into<String>,
        line: u64,
    ) -> Self {
        Self {
            kind: "session_jsonl".to_string(),
            value: path.into(),
            locator: Some(serde_json::json!({
                "uuid": uuid.into(),
                "ts": ts.into(),
                "line": line,
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l1_concept_shape() {
        let ev = SourceEvidence::l1_concept(".codeforge/store/concepts/x.md", "x");
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["kind"], "l1_concept");
        assert_eq!(v["value"], ".codeforge/store/concepts/x.md");
        assert_eq!(v["locator"]["topic"], "x");
    }

    #[test]
    fn git_commit_omits_locator() {
        let ev = SourceEvidence::git_commit("64a0f2c1d");
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["kind"], "git_commit");
        assert_eq!(v["value"], "64a0f2c1d");
        assert!(v.get("locator").is_none(), "git_commit must omit locator");
    }

    #[test]
    fn session_jsonl_locator() {
        let ev = SourceEvidence::session_jsonl("~/p/u.jsonl", "4e7c", "2026-05-15T05:41:12Z", 318);
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["kind"], "session_jsonl");
        assert_eq!(v["locator"]["uuid"], "4e7c");
        assert_eq!(v["locator"]["line"], 318);
    }
}
