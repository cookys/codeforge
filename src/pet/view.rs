//! Pet view snapshot — `<live_base>/codeforge/pet.json`.
//!
//! A small, display-ready JSON copy of the pet (state + village + ASCII art)
//! written next to the statusline's live context files, so UI readers
//! (e.g. a Claude Code mod pane) can draw the pet without spawning
//! `codeforge` or depending on the reader's cwd having a `.codeforge/`.
//!
//! Writers: `codeforge statusline` after every render (it runs in every
//! session, any cwd) and `codeforge adopt` right after a pet is created.
//! Readers only read; the file is replaced atomically (same-dir temp +
//! rename, mode 0600) by `crate::live::write_live_json`.
//!
//! Schema `codeforge.pet-view/1`:
//! ```json
//! {
//!   "schema": "codeforge.pet-view/1",
//!   "written_at": "<RFC3339 UTC>",
//!   "pet": null | {
//!     "name": "Ferris", "level": 1, "xp": 0, "xp_to_next": 100,
//!     "hp": 16, "atk": 19, "def": 11, "sup": 18, "ver": 13,
//!     "mood": 80 | null, "strategy": "explorer" | null, "message": "..." | null
//!   },
//!   "village": null | {
//!     "id": "rust", "display_name": "The Forge-Ruins", "language": "Rust",
//!     "tagline": "...", "rgb": [215, 95, 0],
//!     "art_small": ["..."], "art_full": ["..."]
//!   }
//! }
//! ```
//! `pet` and `village` are both `null` when no pet has been adopted yet.

use crate::pet::state::PetState;
use crate::pet::village::Village;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

pub const SCHEMA: &str = "codeforge.pet-view/1";
/// Purpose segment under the live base (siblings: `context/`).
pub const DIR: &str = "codeforge";
pub const FILE: &str = "pet.json";

/// Optional live extras the statusline knows but `adopt` does not.
#[derive(Debug, Default, Clone, Copy)]
pub struct Extras<'a> {
    pub mood: Option<u32>,
    pub strategy: Option<&'static str>,
    pub message: Option<&'a str>,
}

/// Build the snapshot. `pet = None` ⇒ the "no pet yet" record.
pub fn build(pet: Option<(&PetState, &Village)>, extras: Extras<'_>, written_at: &str) -> Value {
    let Some((p, v)) = pet else {
        return json!({ "schema": SCHEMA, "written_at": written_at, "pet": null, "village": null });
    };
    let (r, g, b) = v.rgb();
    json!({
        "schema": SCHEMA,
        "written_at": written_at,
        "pet": {
            "name": p.name,
            "level": p.level,
            "xp": p.xp,
            "xp_to_next": p.xp_to_next,
            "hp": p.hp,
            "atk": p.atk,
            "def": p.def,
            "sup": p.sup,
            "ver": p.ver,
            "mood": extras.mood,
            "strategy": extras.strategy,
            "message": extras.message,
        },
        "village": {
            "id": v.id,
            "display_name": v.display_name,
            "language": v.language,
            "tagline": v.tagline,
            "rgb": [r, g, b],
            "art_small": v.ascii_small.lines().collect::<Vec<_>>(),
            "art_full": v.ascii_full.lines().collect::<Vec<_>>(),
        },
    })
}

/// Write `value` to `<base>/codeforge/pet.json`.
pub fn write_to(base: &Path, value: &Value) -> io::Result<()> {
    crate::live::write_live_json(&base.join(DIR), FILE, value)
}

/// Build and write to the resolved live base. Best-effort: a failure is one
/// stderr line, never an error for the caller (statusline must still render).
pub fn publish(pet: Option<(&PetState, &Village)>, extras: Extras<'_>) {
    let (base, _source) = crate::live::resolve_live_base();
    let value = build(pet, extras, &chrono::Utc::now().to_rfc3339());
    if let Err(e) = write_to(&base, &value) {
        eprintln!("codeforge: failed to write pet view snapshot: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::village::VILLAGES;

    fn ferris() -> PetState {
        PetState {
            village: "rust".into(),
            name: "Ferris".into(),
            level: 3,
            xp: 25,
            xp_to_next: 100,
            atk: 19,
            hp: 16,
            def: 11,
            sup: 18,
            ver: 13,
        }
    }

    #[test]
    fn no_pet_record_has_null_pet_and_village() {
        let v = build(None, Extras::default(), "2026-10-31T00:00:00Z");
        assert_eq!(v["schema"], SCHEMA);
        assert!(v["pet"].is_null());
        assert!(v["village"].is_null());
    }

    #[test]
    fn pet_record_carries_state_village_and_indented_art() {
        let rust = VILLAGES.iter().find(|v| v.id == "rust").unwrap();
        let p = ferris();
        let extras = Extras {
            mood: Some(80),
            strategy: Some("explorer"),
            message: Some("hi"),
        };
        let v = build(Some((&p, rust)), extras, "2026-10-31T00:00:00Z");
        assert_eq!(v["pet"]["name"], "Ferris");
        assert_eq!(v["pet"]["level"], 3);
        assert_eq!(v["pet"]["xp_to_next"], 100);
        assert_eq!(v["pet"]["mood"], 80);
        assert_eq!(v["pet"]["strategy"], "explorer");
        assert_eq!(v["village"]["id"], "rust");
        assert_eq!(v["village"]["rgb"], json!([0xD7, 0x5F, 0x00]));
        assert_eq!(v["village"]["art_full"][0], "   _~^~^~_");
        assert_eq!(v["village"]["art_small"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn write_to_lands_under_codeforge_dir() {
        let dir = tempfile::tempdir().unwrap();
        let value = build(None, Extras::default(), "2026-10-31T00:00:00Z");
        write_to(dir.path(), &value).unwrap();
        let text = std::fs::read_to_string(dir.path().join(DIR).join(FILE)).unwrap();
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["schema"], SCHEMA);
    }
}
