//! Pixel sprites for the pet view snapshot (`village.sprite` in `pet.json`).
//!
//! A sprite is a palette plus frames of fixed-width rows of palette characters
//! (`transparent` = no pixel). UI readers draw it with terminal half blocks: one
//! cell is two vertical pixels (`▀` fg = top, bg = bottom), so `height` is even.
//! Colours stay inside the xterm-256 cube so a 256-colour terminal keeps them.
//! Contract and drawing rules: `doc/plans/2026-10-31-pixel-pet.md` §2.5.

use serde_json::{json, Map, Value};

pub struct Sprite {
    pub width: usize,
    pub height: usize,
    pub transparent: char,
    pub palette: &'static [(char, [u8; 3])],
    pub frames: &'static [&'static [&'static str]],
    pub frame_ms: u32,
}

/// xterm-256 colour cube channel values.
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

// Ferris, 24×24. `o` shell, `h` highlight, `d` shade, `w` eye white, `k` pupil/mouth.
const FERRIS_IDLE: &[&str] = &[
    "........................",
    "........................",
    "..hh................hh..",
    ".hoo................ooh.",
    ".oo.o..............o.oo.",
    ".ooooo............ooooo.",
    "..ooo..............ooo..",
    "...o....www..www....o...",
    "....o...wkw..wkw...o....",
    "....o...www..www...o....",
    ".....o...o....o...o.....",
    "......o..o....o..o......",
    "......oooooooooooo......",
    "....oooooooooooooooo....",
    "...oooooooooooooooooo...",
    "..oohoooooooooooooohoo..",
    "..oooooooookkooooooooo..",
    "..oooooooooooooooooooo..",
    "...dooooooooooooooood...",
    "....dddooooooooooddd....",
    "...o..o..o....o..o..o...",
    "..o..o..o......o..o..o..",
    "........................",
    "........................",
];

// Blink: eyes shut, pincers closed.
const FERRIS_BLINK: &[&str] = &[
    "........................",
    "........................",
    "..hh................hh..",
    ".hoo................ooh.",
    ".oooo..............oooo.",
    ".ooooo............ooooo.",
    "..ooo..............ooo..",
    "...o................o...",
    "....o...kkk..kkk...o....",
    "....o..............o....",
    ".....o...o....o...o.....",
    "......o..o....o..o......",
    "......oooooooooooo......",
    "....oooooooooooooooo....",
    "...oooooooooooooooooo...",
    "..oohoooooooooooooohoo..",
    "..oooooooookkooooooooo..",
    "..oooooooooooooooooooo..",
    "...dooooooooooooooood...",
    "....dddooooooooooddd....",
    "...o..o..o....o..o..o...",
    "..o..o..o......o..o..o..",
    "........................",
    "........................",
];

pub static FERRIS: Sprite = Sprite {
    width: 24,
    height: 24,
    transparent: '.',
    palette: &[
        ('o', [215, 95, 0]),
        ('h', [255, 135, 0]),
        ('d', [175, 95, 0]),
        ('w', [255, 255, 255]),
        ('k', [0, 0, 0]),
    ],
    frames: &[FERRIS_IDLE, FERRIS_BLINK],
    frame_ms: 500,
};

/// The sprite for a village, if it has one. Villages without one draw ASCII art.
pub fn for_village(id: &str) -> Option<&'static Sprite> {
    match id {
        "rust" => Some(&FERRIS),
        _ => None,
    }
}

/// Check a sprite against the contract; the error names the first problem.
pub fn validate(s: &Sprite) -> Result<(), String> {
    if s.width == 0 || s.height == 0 {
        return Err("width and height must be > 0".into());
    }
    // `is_multiple_of` needs Rust 1.87; MSRV is 1.85.
    if s.height & 1 != 0 {
        return Err(format!("height {} must be even", s.height));
    }
    if !s.transparent.is_ascii() {
        return Err("transparent must be one ASCII char".into());
    }
    for (c, rgb) in s.palette {
        if !c.is_ascii() || *c == s.transparent {
            return Err(format!(
                "palette key {c:?} must be ASCII and not the transparent char"
            ));
        }
        if let Some(v) = rgb.iter().find(|v| !CUBE.contains(v)) {
            return Err(format!(
                "palette {c:?} channel {v} is not an xterm-256 cube value"
            ));
        }
    }
    if s.frames.len() < 2 {
        return Err(format!("need at least 2 frames, got {}", s.frames.len()));
    }
    if s.frame_ms != 500 {
        return Err(format!("frame_ms must be 500, got {}", s.frame_ms));
    }
    for (fi, frame) in s.frames.iter().enumerate() {
        if frame.len() != s.height {
            return Err(format!(
                "frame {fi} has {} rows, want {}",
                frame.len(),
                s.height
            ));
        }
        for (ri, row) in frame.iter().enumerate() {
            if row.chars().count() != s.width {
                return Err(format!(
                    "frame {fi} row {ri} has {} chars, want {}",
                    row.chars().count(),
                    s.width
                ));
            }
            if let Some(c) = row
                .chars()
                .find(|c| *c != s.transparent && !s.palette.iter().any(|(k, _)| k == c))
            {
                return Err(format!(
                    "frame {fi} row {ri} uses {c:?}, not in the palette"
                ));
            }
        }
    }
    for (c, _) in s.palette {
        if !s.frames.iter().any(|f| f.iter().any(|r| r.contains(*c))) {
            return Err(format!("palette {c:?} is never used"));
        }
    }
    Ok(())
}

/// JSON for `village.sprite`, or `null` when the village has none — or when
/// its sprite breaks the contract, so a bad sprite never reaches a reader.
pub fn to_json(sprite: Option<&Sprite>) -> Value {
    let Some(s) = sprite else {
        return Value::Null;
    };
    if validate(s).is_err() {
        return Value::Null;
    }
    let mut palette = Map::new();
    for (c, rgb) in s.palette {
        palette.insert(c.to_string(), json!(rgb));
    }
    json!({
        "width": s.width,
        "height": s.height,
        "transparent": s.transparent.to_string(),
        "palette": palette,
        "frames": s.frames,
        "frame_ms": s.frame_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::village::VILLAGES;

    #[test]
    fn ferris_meets_the_contract() {
        assert_eq!(validate(&FERRIS), Ok(()));
    }

    #[test]
    fn only_rust_has_a_sprite_for_now() {
        for v in VILLAGES {
            assert_eq!(for_village(v.id).is_some(), v.id == "rust", "{}", v.id);
        }
    }

    #[test]
    fn frames_differ() {
        assert_ne!(FERRIS.frames[0], FERRIS.frames[1]);
    }

    fn broken(f: impl FnOnce(&mut Sprite)) -> Result<(), String> {
        let mut s = Sprite {
            width: 2,
            height: 2,
            transparent: '.',
            palette: &[('o', [215, 95, 0])],
            frames: &[&["o.", ".o"], &[".o", "o."]],
            frame_ms: 500,
        };
        assert_eq!(validate(&s), Ok(()), "baseline sprite must be valid");
        f(&mut s);
        validate(&s)
    }

    #[test]
    fn validate_rejects_each_contract_break() {
        assert!(broken(|s| s.height = 3).unwrap_err().contains("even"));
        assert!(broken(|s| s.width = 3).unwrap_err().contains("chars"));
        assert!(broken(|s| s.frames = &[&["o.", ".o"]])
            .unwrap_err()
            .contains("2 frames"));
        assert!(broken(|s| s.frames = &[&["ox", ".o"], &[".o", "o."]])
            .unwrap_err()
            .contains("not in the palette"));
        assert!(broken(|s| s.palette = &[('o', [200, 95, 0])])
            .unwrap_err()
            .contains("cube"));
        assert!(
            broken(|s| s.palette = &[('o', [215, 95, 0]), ('h', [255, 135, 0])])
                .unwrap_err()
                .contains("never used")
        );
        assert!(broken(|s| s.frame_ms = 100).unwrap_err().contains("500"));
        assert!(broken(|s| s.transparent = 'o')
            .unwrap_err()
            .contains("transparent"));
    }

    #[test]
    fn to_json_null_without_sprite_or_when_invalid() {
        assert!(to_json(None).is_null());
        let bad = Sprite {
            width: 2,
            height: 3,
            transparent: '.',
            palette: &[('o', [215, 95, 0])],
            frames: &[&["o.", ".o", ".."], &[".o", "o.", ".."]],
            frame_ms: 500,
        };
        assert!(to_json(Some(&bad)).is_null());
        let v = to_json(Some(&FERRIS));
        assert_eq!(v["frames"][1][8].as_str().unwrap().chars().count(), 24);
    }
}
