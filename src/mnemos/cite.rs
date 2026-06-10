//! Citation write-back envelope (§11.1) + POST helper.
//!
//! `POST /v1/atoms/:atom_id/cite` reports that an atom was referenced; Mnemos bumps
//! `citation_count` + `last_cited_at`. ULID `cite_id` is the idempotency key.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Client identifier sent in every cite envelope (§11.1 `client` field).
pub fn client_id() -> String {
    format!("codeforge_ship/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiteEnvelope {
    /// ULID idempotency key (§11.1).
    pub cite_id: String,
    /// Client detection time (RFC3339; Mnemos clamps to min(cited_at, received_at), §11.2 D4).
    pub cited_at: String,
    /// `codeforge_ship/<version>`.
    pub client: String,
    /// Which session referenced the atom (debug/audit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_ref: Option<Value>,
    pub evidence: CiteEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiteEvidence {
    /// Sprint 1: `fulltext_match`. Mnemos strict-validates against a known set.
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

impl CiteEnvelope {
    /// Build a `fulltext_match` cite envelope for a heuristic detection.
    pub fn fulltext_match(
        cite_id: String,
        cited_at: String,
        matched_text: impl Into<String>,
        confidence: f64,
        session_ref: Option<Value>,
    ) -> Self {
        Self {
            cite_id,
            cited_at,
            client: client_id(),
            session_ref,
            evidence: CiteEvidence {
                method: "fulltext_match".to_string(),
                matched_text: Some(matched_text.into()),
                confidence: Some(confidence),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fulltext_envelope_shape() {
        let env = CiteEnvelope::fulltext_match(
            "01HQXYZABCDEFGHIJKLMNOPQRT".to_string(),
            "2026-05-15T22:34:56Z".to_string(),
            "Notify::notify_waiters",
            0.7,
            Some(serde_json::json!({ "kind": "session_jsonl", "value": "~/p/u.jsonl" })),
        );
        let v = serde_json::to_value(&env).unwrap();
        assert_eq!(v["cite_id"], "01HQXYZABCDEFGHIJKLMNOPQRT");
        assert_eq!(v["cited_at"], "2026-05-15T22:34:56Z");
        assert!(v["client"].as_str().unwrap().starts_with("codeforge_ship/"));
        assert_eq!(v["evidence"]["method"], "fulltext_match");
        assert_eq!(v["evidence"]["matched_text"], "Notify::notify_waiters");
        assert_eq!(v["evidence"]["confidence"], 0.7);
        assert_eq!(v["session_ref"]["kind"], "session_jsonl");
    }

    #[test]
    fn session_ref_omitted_when_none() {
        let env = CiteEnvelope::fulltext_match(
            "01HQXYZABCDEFGHIJKLMNOPQRT".to_string(),
            "2026-05-15T22:34:56Z".to_string(),
            "x",
            0.5,
            None,
        );
        let v = serde_json::to_value(&env).unwrap();
        assert!(v.get("session_ref").is_none());
    }

    #[test]
    fn client_id_carries_version() {
        assert!(client_id().starts_with("codeforge_ship/"));
        assert!(client_id().contains(env!("CARGO_PKG_VERSION")));
    }
}
