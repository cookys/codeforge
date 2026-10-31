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
            "sprite": crate::pet::sprite::to_json(crate::pet::sprite::for_village(v.id)),
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

/// Schema of the per-session view (`<live_base>/codeforge/session/<sid>.json`).
pub const SESSION_SCHEMA: &str = "codeforge.session-view/1";
/// Directory under [`DIR`] that holds one session view per session id.
pub const SESSION_DIR: &str = "session";

/// The per-session view: memory and brain lights for UI readers of this session only
/// (`pet.json` is shared by every session, so per-project state must not go there).
pub fn build_session_view(
    raw_sid: &str,
    memory_light: &str,
    active: usize,
    brain_light: &str,
    written_at: &str,
) -> Value {
    json!({
        "schema": SESSION_SCHEMA,
        "session_id": raw_sid,
        "written_at": written_at,
        "memory": { "light": memory_light, "active": active },
        "brain": { "light": brain_light },
    })
}

/// Write the session view atomically; the file name never contains a slash.
pub fn write_session_view_to(base: &Path, value: &Value) -> io::Result<()> {
    let raw_sid = value["session_id"].as_str().unwrap_or("");
    let name = format!("{}.json", crate::live::sanitize_session_id(raw_sid));
    crate::live::write_live_json(&base.join(DIR).join(SESSION_DIR), &name, value)
}

/// Best-effort: failure prints one stderr line, like [`publish`].
pub fn write_session_view(raw_sid: &str, memory_light: &str, active: usize, brain_light: &str) {
    let (base, _source) = crate::live::resolve_live_base();
    let value = build_session_view(
        raw_sid,
        memory_light,
        active,
        brain_light,
        &chrono::Utc::now().to_rfc3339(),
    );
    if let Err(e) = write_session_view_to(&base, &value) {
        eprintln!("codeforge: failed to write session view: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::village::VILLAGES;

    #[test]
    fn session_view_carries_lights_and_raw_sid() {
        let v = build_session_view("a/b:c d", "active", 12, "ok", "2026-10-31T00:00:00Z");
        assert_eq!(v["schema"], SESSION_SCHEMA);
        assert_eq!(v["session_id"], "a/b:c d");
        assert_eq!(v["written_at"], "2026-10-31T00:00:00Z");
        assert_eq!(v["memory"]["light"], "active");
        assert_eq!(v["memory"]["active"], 12);
        assert_eq!(v["brain"]["light"], "ok");
    }

    #[test]
    fn session_view_lands_under_sanitized_name() {
        let dir = tempfile::tempdir().unwrap();
        let v = build_session_view("a/b:c d", "empty", 0, "hidden", "2026-10-31T00:00:00Z");
        write_session_view_to(dir.path(), &v).unwrap();
        let path = dir.path().join(DIR).join(SESSION_DIR).join("a_b_c_d.json");
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["memory"]["light"], "empty");
    }

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
    fn rust_village_carries_a_valid_sprite() {
        let rust = VILLAGES.iter().find(|v| v.id == "rust").unwrap();
        let v = build(
            Some((&ferris(), rust)),
            Extras::default(),
            "2026-10-31T00:00:00Z",
        );
        let s = &v["village"]["sprite"];
        assert!(s.is_object(), "rust village should carry a sprite, got {s}");
        assert_eq!(s["width"], 24);
        assert_eq!(s["height"], 24);
        assert_eq!(s["transparent"], ".");
        assert_eq!(s["frame_ms"], 500);
        assert_eq!(s["palette"]["o"], json!([215, 95, 0]));
        let frames = s["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 2);
        for f in frames {
            let rows = f.as_array().unwrap();
            assert_eq!(rows.len(), 24);
            assert!(rows
                .iter()
                .all(|r| r.as_str().unwrap().chars().count() == 24));
        }
    }

    #[test]
    fn village_without_sprite_writes_null_sprite_key() {
        let python = VILLAGES.iter().find(|v| v.id == "python").unwrap();
        let p = PetState {
            village: "python".into(),
            ..ferris()
        };
        let v = build(
            Some((&p, python)),
            Extras::default(),
            "2026-10-31T00:00:00Z",
        );
        let village = v["village"].as_object().unwrap();
        assert!(
            village.contains_key("sprite"),
            "sprite key must always be written"
        );
        assert!(v["village"]["sprite"].is_null());
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
