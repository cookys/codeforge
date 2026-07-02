//! ship-state (§7.6) + ship-failed/ queue (§7.4) persistence.
//!
//! - ship-state.json: `{ "<repo>": { "<date>": "<ship_id>" } }` — guards against
//!   re-shipping the same day.
//! - ship-failed/<ship_id>.json: the full envelope, replayable verbatim.

use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::ledger::LedgerEnvelope;

/// Root dir for codeforge ship artifacts (`~/.codeforge/`).
pub fn ship_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codeforge")
}

pub fn ship_state_path(root: &Path) -> PathBuf {
    root.join("ship-state.json")
}

pub fn ship_failed_dir(root: &Path) -> PathBuf {
    root.join("ship-failed")
}

/// `{ repo: { date: ship_id } }`
pub type ShipState = BTreeMap<String, BTreeMap<String, String>>;

pub fn load_state(root: &Path) -> ShipState {
    let path = ship_state_path(root);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_state(root: &Path, state: &ShipState) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(ship_state_path(root), json)?;
    Ok(())
}

// --- B18 auto-cite dedup (C2): one cite per (repo, date, atom) across sessions ---

/// `{ repo: { date: [atom_id, …] } }` — atoms already auto-cited per repo+day.
/// Seeds the skip list so a later same-day session's SessionEnd ship (which
/// re-scans the whole day's transcripts) never double-cites an atom.
pub type AutoCiteState = BTreeMap<String, BTreeMap<String, Vec<String>>>;

pub fn autocite_state_path(root: &Path) -> PathBuf {
    root.join("autocite-state.json")
}

pub fn load_autocite(root: &Path) -> AutoCiteState {
    std::fs::read_to_string(autocite_state_path(root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_autocite(root: &Path, state: &AutoCiteState) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(autocite_state_path(root), json)?;
    Ok(())
}

/// Has this repo+date already been shipped successfully?
pub fn already_shipped(state: &ShipState, repo: &str, date: &str) -> bool {
    state
        .get(repo)
        .map(|m| m.contains_key(date))
        .unwrap_or(false)
}

/// Record a successful ship.
pub fn mark_shipped(state: &mut ShipState, repo: &str, date: &str, ship_id: &str) {
    state
        .entry(repo.to_string())
        .or_default()
        .insert(date.to_string(), ship_id.to_string());
}

/// Write a failed envelope to ship-failed/<ship_id>.json.
pub fn write_failed(root: &Path, env: &LedgerEnvelope) -> Result<PathBuf> {
    let dir = ship_failed_dir(root);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", env.ship_id));
    std::fs::write(&path, serde_json::to_string_pretty(env)?)?;
    Ok(path)
}

/// Load all queued failed envelopes, sorted by `ship_at` ascending (§7.4).
pub fn load_failed(root: &Path) -> Vec<(PathBuf, LedgerEnvelope)> {
    let dir = ship_failed_dir(root);
    let mut out: Vec<(PathBuf, LedgerEnvelope)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().map(|x| x == "json").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(env) = serde_json::from_str::<LedgerEnvelope>(&content) {
                    out.push((path, env));
                }
            }
        }
    }
    out.sort_by(|a, b| a.1.ship_at.cmp(&b.1.ship_at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mnemos::ledger::LedgerPayload;

    fn env(ship_id: &str, ship_at: &str) -> LedgerEnvelope {
        LedgerEnvelope {
            ship_id: ship_id.to_string(),
            source: "codeforge_ledger".to_string(),
            ship_at: ship_at.to_string(),
            machine_id: "box".to_string(),
            payload: LedgerPayload {
                ledger_date: "2026-05-15".to_string(),
                repo: "codeforge".to_string(),
                lessons: vec![],
                provenance: serde_json::json!({}),
            },
        }
    }

    #[test]
    fn state_roundtrip_and_guards() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = load_state(root);
        assert!(!already_shipped(&state, "codeforge", "2026-05-15"));
        mark_shipped(&mut state, "codeforge", "2026-05-15", "01ABC");
        save_state(root, &state).unwrap();

        let reloaded = load_state(root);
        assert!(already_shipped(&reloaded, "codeforge", "2026-05-15"));
        assert!(!already_shipped(&reloaded, "codeforge", "2026-05-16"));
        assert!(!already_shipped(&reloaded, "other", "2026-05-15"));
    }

    #[test]
    fn failed_queue_roundtrip_sorted_by_ship_at() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_failed(root, &env("01B", "2026-05-15T10:00:00Z")).unwrap();
        write_failed(root, &env("01A", "2026-05-15T08:00:00Z")).unwrap();
        let loaded = load_failed(root);
        assert_eq!(loaded.len(), 2);
        // sorted ascending by ship_at → 08:00 first
        assert_eq!(loaded[0].1.ship_id, "01A");
        assert_eq!(loaded[1].1.ship_id, "01B");
    }

    #[test]
    fn load_failed_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_failed(dir.path());
        assert!(loaded.is_empty());
    }
}
