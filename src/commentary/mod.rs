//! Phase 3c — AI Commentary system.
//!
//! Pet 在戰鬥 / level-up / 長時間 idle / session 超長時說話。
//! 全域 1/hour、opt-in、strategy-aware 語氣、30 天不重複同一 phrase。
//! 有 API key 走 Haiku，無 key/離線 rule-based。
//!
//! Public surface:
//! * `Kind` — the trigger kinds a commentary can represent.
//! * `Source` — whether a phrase came from the LLM or the rule-based pool.
//! * `FeedRow` — one emitted commentary entry (persisted in `commentary_feed`).
//! * `Tone` — composite `village:strategy` key used for phrase selection.
//! * `normalize_phrase` + `phrase_hash_hex` — the canonical dedup key derivation.
//! * `repo` — SQLite CRUD for the three Phase 3c tables.
//!
//! Phase 3c P2+ modules (trigger, budget, generator) land in sibling files.

// P1 ships the scaffolding only; the daemon tick integration (P4) and CLI
// (P6) wire everything up. Suppress unused-warnings until P2 starts calling
// in — each subsequent phase removes the corresponding item's implicit
// allowance via actual usage.
#![allow(dead_code)]

use sha1::{Digest, Sha1};

pub mod budget;
pub mod dispatch;
pub mod generator;
pub mod repo;
pub mod trigger;

/// Trigger kinds the commentary system reacts to. `as_str` is the canonical
/// on-disk representation — do not change without a migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Boss (or other named large mob) was defeated this tick.
    BossKill,
    /// Pet level increased this tick.
    LevelUp,
    /// Active session has exceeded the "long session" threshold (≥ 4h).
    SessionLong,
    /// Pet has been idle for ≥ 30 min with no interaction.
    LongIdle,
    /// A new zone was unlocked (placeholder — wired up when Phase 3a lands).
    ZoneUnlock,
    /// Manual trigger from `codeforge commentary test` (bypasses rate limit).
    ManualTest,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Self::BossKill,
        Self::LevelUp,
        Self::SessionLong,
        Self::LongIdle,
        Self::ZoneUnlock,
        Self::ManualTest,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::BossKill => "boss_kill",
            Self::LevelUp => "level_up",
            Self::SessionLong => "session_long",
            Self::LongIdle => "long_idle",
            Self::ZoneUnlock => "zone_unlock",
            Self::ManualTest => "manual_test",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "boss_kill" => Self::BossKill,
            "level_up" => Self::LevelUp,
            "session_long" => Self::SessionLong,
            "long_idle" => Self::LongIdle,
            "zone_unlock" => Self::ZoneUnlock,
            "manual_test" => Self::ManualTest,
            _ => return None,
        })
    }
}

/// Where a phrase came from. Stored in `commentary_feed.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Haiku,
    Rule,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Haiku => "haiku",
            Self::Rule => "rule",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "haiku" => Self::Haiku,
            "rule" => Self::Rule,
            _ => return None,
        })
    }
}

/// Composite tone key. The pool selector keys on the exact string; writing a
/// custom village without a registered phrase set just falls back to the
/// village's first registered strategy bucket.
#[derive(Debug, Clone)]
pub struct Tone {
    pub village: String,
    pub strategy: String,
}

impl Tone {
    pub fn new(village: impl Into<String>, strategy: impl Into<String>) -> Self {
        Self {
            village: village.into(),
            strategy: strategy.into(),
        }
    }

    /// Wire-format used in `commentary_feed.tone` and rule-pool key lookups.
    pub fn composite(&self) -> String {
        format!("{}:{}", self.village, self.strategy)
    }
}

/// One row of `commentary_feed`. `id` is `None` before insertion.
#[derive(Debug, Clone)]
pub struct FeedRow {
    pub id: Option<i64>,
    pub created_at: i64,
    pub tick_count: u64,
    pub kind: Kind,
    pub tone: String,
    pub phrase: String,
    pub phrase_hash: String,
    pub source: Source,
    pub displayed_count: u32,
}

/// Canonicalise a phrase for dedup purposes:
/// * collapse runs of whitespace to single spaces
/// * trim leading/trailing whitespace
/// * strip trailing punctuation (`。！？!?.`) — Haiku and rule outputs tend
///   to differ only in terminal punctuation; we don't want "好耶！" and
///   "好耶。" to slip past dedup.
///
/// The result is purely for hashing — we still store the original phrase in
/// `commentary_feed.phrase` for display.
pub fn normalize_phrase(phrase: &str) -> String {
    let collapsed: String = phrase.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_end_matches(|c: char| {
        matches!(c, '。' | '！' | '？' | '!' | '?' | '.' | ',' | '，' | '、' | ' ')
    });
    trimmed.to_string()
}

/// Hex-encoded SHA-1 of the normalized phrase. Stable across runs and Rust
/// versions — safe to persist. 40-char lowercase hex.
pub fn phrase_hash_hex(phrase: &str) -> String {
    let normalized = normalize_phrase(phrase);
    let mut hasher = Sha1::new();
    hasher.update(normalized.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(40);
    for byte in digest {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip_all() {
        for k in Kind::ALL {
            assert_eq!(Kind::from_str(k.as_str()), Some(k));
        }
    }

    #[test]
    fn kind_from_str_rejects_typo() {
        assert_eq!(Kind::from_str("bosskill"), None);
        assert_eq!(Kind::from_str(""), None);
    }

    #[test]
    fn kind_from_str_is_case_insensitive_and_trims() {
        assert_eq!(Kind::from_str("  BOSS_KILL  "), Some(Kind::BossKill));
    }

    #[test]
    fn source_roundtrip() {
        assert_eq!(Source::from_str("haiku"), Some(Source::Haiku));
        assert_eq!(Source::from_str("rule"), Some(Source::Rule));
        assert_eq!(Source::from_str("other"), None);
    }

    #[test]
    fn tone_composite_format() {
        let t = Tone::new("rust", "aggressive");
        assert_eq!(t.composite(), "rust:aggressive");
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(
            normalize_phrase("  hello   world   "),
            "hello world"
        );
    }

    #[test]
    fn normalize_strips_terminal_punctuation() {
        // Both Chinese and ASCII terminal punctuation are removed so
        // "好耶！" and "好耶。" and "好耶" all hash identically.
        let a = normalize_phrase("好耶！");
        let b = normalize_phrase("好耶。");
        let c = normalize_phrase("好耶");
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn phrase_hash_is_40_char_hex() {
        let h = phrase_hash_hex("anything");
        assert_eq!(h.len(), 40);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn phrase_hash_matches_across_punctuation_variants() {
        assert_eq!(phrase_hash_hex("好耶！"), phrase_hash_hex("好耶"));
        assert_eq!(phrase_hash_hex("nice!"), phrase_hash_hex("nice."));
    }

    #[test]
    fn phrase_hash_differs_for_different_phrases() {
        assert_ne!(phrase_hash_hex("one"), phrase_hash_hex("two"));
    }

    #[test]
    fn normalize_cjk_safe() {
        // Verify we never byte-index CJK strings. Passing a CJK string with
        // terminal punctuation must not panic — this test would fail with
        // a byte-slice implementation.
        let s = "寵物擊殺了 Boss！！";
        let normalized = normalize_phrase(s);
        assert!(!normalized.is_empty());
    }
}
