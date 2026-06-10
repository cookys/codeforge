//! Ledger envelope + payload (ship spec §4) — the body POSTed to
//! `/v1/ingest/ledger`. Field names are aligned 1:1 with Mnemos
//! `LedgerEnvelope` / `LedgerPayload` / `LedgerLesson` structs (contract truth, §4.1).
//!
//! `source` and `machine_id` are sent but Mnemos drops them today (§4.1/§4.2);
//! kept for spec-convention readability + forward-compat.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::evidence::SourceEvidence;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEnvelope {
    /// ULID idempotency key. Same payload always reuses the same ship_id (§7.1).
    pub ship_id: String,
    /// Cosmetic — endpoint path decides source. Sent per §3 convention.
    pub source: String,
    /// ISO 8601 UTC pack time. Mnemos parses-but-ignores; used for ship-failed/ resend ordering.
    pub ship_at: String,
    /// Cosmetic / forward-compat (§4.2). Mnemos drops it today.
    pub machine_id: String,
    pub payload: LedgerPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerPayload {
    /// `YYYY-MM-DD`. Required by Mnemos (empty → 400 payload_validation).
    pub ledger_date: String,
    /// Repo name; part of Mnemos source_id, used as atom entity.
    pub repo: String,
    /// Core payload — each non-empty lesson → 1 lesson atom.
    pub lessons: Vec<LedgerLesson>,
    /// Audit bundle, kept verbatim by Mnemos in document ref_locator_json.provenance (§4.3).
    pub provenance: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerLesson {
    /// Atom title (≤ ~40 chars). Empty → Mnemos falls back to detail prefix.
    pub title: String,
    /// Atom body.
    pub detail: String,
    /// Atom kind hint. CodeForge sends `"lesson"` (Sprint 0 Mnemos only has lesson kind).
    pub candidate_atom_kind: String,
    /// Citation locators (§4.4) — at least one element required (citations-mandatory).
    pub source_evidence: Vec<SourceEvidence>,
}

impl LedgerLesson {
    /// True when both title and detail are empty — Mnemos would `continue` (no atom).
    /// CodeForge must not emit such a lesson.
    pub fn is_empty(&self) -> bool {
        self.title.trim().is_empty() && self.detail.trim().is_empty()
    }

    /// True when the lesson has no citation locator — violates citations-mandatory; drop it.
    pub fn has_evidence(&self) -> bool {
        !self.source_evidence.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_lesson() -> LedgerLesson {
        LedgerLesson {
            title: "Notify::notify_waiters 會丟 wakeup".to_string(),
            detail: "用 mpsc::channel(1) 而非 Notify::notify_waiters。".to_string(),
            candidate_atom_kind: "lesson".to_string(),
            source_evidence: vec![SourceEvidence::l1_concept(
                ".codeforge/store/concepts/ipc-shutdown.md",
                "ipc-shutdown",
            )],
        }
    }

    #[test]
    fn envelope_field_names_match_mnemos_contract() {
        let env = LedgerEnvelope {
            ship_id: "01J0ZX9K8R7QABCDEF0123456".to_string(),
            source: "codeforge_ledger".to_string(),
            ship_at: "2026-05-15T08:23:01Z".to_string(),
            machine_id: "main-linux-blackwell".to_string(),
            payload: LedgerPayload {
                ledger_date: "2026-05-15".to_string(),
                repo: "codeforge".to_string(),
                lessons: vec![sample_lesson()],
                provenance: serde_json::json!({ "raw_signal_count": 142 }),
            },
        };
        let v = serde_json::to_value(&env).unwrap();
        // Envelope layer (§4.1)
        assert!(v.get("ship_id").is_some());
        assert_eq!(v["source"], "codeforge_ledger");
        assert!(v.get("ship_at").is_some());
        assert!(v.get("machine_id").is_some());
        // Payload layer
        assert_eq!(v["payload"]["ledger_date"], "2026-05-15");
        assert_eq!(v["payload"]["repo"], "codeforge");
        assert_eq!(v["payload"]["provenance"]["raw_signal_count"], 142);
        // Lesson layer
        let lesson = &v["payload"]["lessons"][0];
        assert!(lesson.get("title").is_some());
        assert!(lesson.get("detail").is_some());
        assert_eq!(lesson["candidate_atom_kind"], "lesson");
        assert!(lesson["source_evidence"].is_array());
        assert_eq!(lesson["source_evidence"][0]["kind"], "l1_concept");
    }

    #[test]
    fn empty_lesson_detection() {
        let mut l = sample_lesson();
        assert!(!l.is_empty());
        l.title = "  ".to_string();
        l.detail = String::new();
        assert!(l.is_empty());
    }

    #[test]
    fn evidence_presence() {
        let mut l = sample_lesson();
        assert!(l.has_evidence());
        l.source_evidence.clear();
        assert!(!l.has_evidence());
    }

    #[test]
    fn roundtrips_through_mnemos_shape() {
        // Mnemos deserializes with serde(default); ensure our output parses back cleanly.
        let env = LedgerEnvelope {
            ship_id: "01J0ZX9K8R7QABCDEF0123456".to_string(),
            source: "codeforge_ledger".to_string(),
            ship_at: "2026-05-15T08:23:01Z".to_string(),
            machine_id: "box".to_string(),
            payload: LedgerPayload {
                ledger_date: "2026-05-15".to_string(),
                repo: "codeforge".to_string(),
                lessons: vec![sample_lesson()],
                provenance: serde_json::json!({}),
            },
        };
        let json = serde_json::to_string(&env).unwrap();
        let back: LedgerEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ship_id, env.ship_id);
        assert_eq!(back.payload.lessons.len(), 1);
    }
}
