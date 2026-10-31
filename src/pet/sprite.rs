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

/// Sprite width and height in pixels (one cell = two vertical pixels, so 24x12 cells).
pub const SIZE: usize = 24;
/// The small sprite (`village.sprite_mini`): 16x12 pixels = 16 columns x 6 rows, for a band
/// with 6 to 11 rows (`doc/plans/2026-10-31-forge-tabs.md` §2.5).
pub const MINI_WIDTH: usize = 16;
pub const MINI_HEIGHT: usize = 12;
/// The character that means "no pixel".
pub const TRANSPARENT: char = '.';

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

// Ferris, 16×12 (the band's small tier): same palette, claws up / eyes blink.
const FERRIS_MINI_IDLE: &[&str] = &[
    ".hh..........hh.",
    "hoo.o......o.ooh",
    ".ooo........ooo.",
    "..o..ww..ww..o..",
    "...o.wk..kw.o...",
    "....oooooooo....",
    "..oooooooooooo..",
    ".oohoooookoooho.",
    ".oooooooooooooo.",
    "..dooooooooood..",
    "..o.o.o..o.o.o..",
    ".o.o.o....o.o.o.",
];

const FERRIS_MINI_BLINK: &[&str] = &[
    ".hh..........hh.",
    ".ooo........ooo.",
    "..oo........oo..",
    "..o..........o..",
    "...o.kk..kk.o...",
    "....oooooooo....",
    "..oooooooooooo..",
    ".oohoooookoooho.",
    ".oooooooooooooo.",
    "..dooooooooood..",
    "..o.o.o..o.o.o..",
    ".o.o.o....o.o.o.",
];

pub static FERRIS_MINI: Sprite = Sprite {
    width: MINI_WIDTH,
    height: MINI_HEIGHT,
    transparent: '.',
    palette: FERRIS.palette,
    frames: &[FERRIS_MINI_IDLE, FERRIS_MINI_BLINK],
    frame_ms: 500,
};

/// The sprite for a village, if it has one. Villages without one draw ASCII art.
pub fn for_village(id: &str) -> Option<&'static Sprite> {
    match id {
        "rust" => Some(&FERRIS),
        _ => None,
    }
}

/// The small sprite for a village, if it has one (`village.sprite_mini`).
pub fn mini_for_village(id: &str) -> Option<&'static Sprite> {
    match id {
        "rust" => Some(&FERRIS_MINI),
        _ => None,
    }
}

/// Check a `village.sprite` against the contract (24x24); the error names the first problem.
#[cfg(test)]
pub fn validate(s: &Sprite) -> Result<(), String> {
    validate_size(s, SIZE, SIZE)
}

/// Check a sprite against the contract with a fixed size (readers lay out a
/// `width`-column, `height / 2`-row Raster).
pub fn validate_size(s: &Sprite, width: usize, height: usize) -> Result<(), String> {
    if s.width != width || s.height != height {
        return Err(format!(
            "size {}x{} must be {width}x{height}",
            s.width, s.height
        ));
    }
    if s.transparent != TRANSPARENT {
        return Err(format!(
            "transparent must be {TRANSPARENT:?}, got {:?}",
            s.transparent
        ));
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
    to_json_sized(sprite, SIZE, SIZE)
}

/// JSON for `village.sprite_mini` (16x12), `null` when absent or off-contract.
pub fn mini_to_json(sprite: Option<&Sprite>) -> Value {
    to_json_sized(sprite, MINI_WIDTH, MINI_HEIGHT)
}

fn to_json_sized(sprite: Option<&Sprite>, width: usize, height: usize) -> Value {
    let Some(s) = sprite else {
        return Value::Null;
    };
    if validate_size(s, width, height).is_err() {
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
        assert_ne!(FERRIS_MINI.frames[0], FERRIS_MINI.frames[1]);
    }

    /// The forge-stats mod tests draw from `.ts` copies of these sprites (a mod test can
    /// import neither JSON nor files outside the mod); keep them equal to what codeforge writes.
    #[test]
    fn forge_stats_sprite_fixtures_match() {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.claude/skills/forge-stats/tests"
        );
        let read = |name: &str| -> Value {
            let text = std::fs::read_to_string(format!("{dir}/{name}")).unwrap();
            let start = text.find(" = {").expect("fixture has `= {`") + 3;
            serde_json::from_str(text[start..].trim()).unwrap()
        };
        assert_eq!(read("ferris-sprite.ts"), to_json(Some(&FERRIS)));
        assert_eq!(read("ferris-mini.ts"), mini_to_json(Some(&FERRIS_MINI)));
    }

    #[test]
    fn ferris_mini_meets_the_mini_contract() {
        assert_eq!(validate_size(&FERRIS_MINI, MINI_WIDTH, MINI_HEIGHT), Ok(()));
        assert!(validate(&FERRIS_MINI).unwrap_err().contains("24x24"));
        for v in VILLAGES {
            assert_eq!(mini_for_village(v.id).is_some(), v.id == "rust", "{}", v.id);
        }
        let j = mini_to_json(Some(&FERRIS_MINI));
        assert_eq!(j["width"], 16);
        assert_eq!(j["height"], 12);
        assert_eq!(j["frames"][0].as_array().unwrap().len(), 12);
        // a full-size sprite is not a mini one
        assert!(mini_to_json(Some(&FERRIS)).is_null());
        assert!(mini_to_json(None).is_null());
    }

    /// Ferris with one change applied; the result of validating it.
    fn broken(f: impl FnOnce(&mut Sprite)) -> Result<(), String> {
        let mut s = Sprite { ..FERRIS };
        f(&mut s);
        validate(&s)
    }

    /// Ferris's frames with row `row` of frame 1 replaced.
    fn frames_with(row: usize, text: &str) -> &'static [&'static [&'static str]] {
        let mut f1: Vec<&'static str> = FERRIS.frames[1].to_vec();
        f1[row] = Box::leak(text.to_string().into_boxed_str());
        let f1: &'static [&'static str] = Box::leak(f1.into_boxed_slice());
        Box::leak(vec![FERRIS.frames[0], f1].into_boxed_slice())
    }

    #[test]
    fn validate_rejects_each_contract_break() {
        let err = |f: fn(&mut Sprite)| broken(f).unwrap_err();
        assert!(err(|s| s.height = 22).contains("24x24"));
        assert!(err(|s| s.width = 16).contains("24x24"));
        assert!(err(|s| s.transparent = ' ').contains("transparent"));
        assert!(err(|s| s.frames = &FERRIS.frames[..1]).contains("2 frames"));
        assert!(err(|s| s.frame_ms = 100).contains("500"));
        assert!(err(|s| s.palette = &[('o', [200, 95, 0])]).contains("cube"));
        assert!(err(|s| s.palette = &[('é', [215, 95, 0])]).contains("ASCII"));
        assert!(err(|s| s.palette = &[('.', [215, 95, 0])]).contains("ASCII"));
        let mut extra: Vec<(char, [u8; 3])> = FERRIS.palette.to_vec();
        extra.push(('z', [0, 0, 0]));
        let extra: &'static [(char, [u8; 3])] = Box::leak(extra.into_boxed_slice());
        assert!(broken(|s| s.palette = extra)
            .unwrap_err()
            .contains("never used"));
        let short = frames_with(3, &".".repeat(23));
        assert!(broken(|s| s.frames = short).unwrap_err().contains("chars"));
        let alien = frames_with(3, &format!("x{}", ".".repeat(23)));
        assert!(broken(|s| s.frames = alien)
            .unwrap_err()
            .contains("not in the palette"));
        let few = &FERRIS.frames[1][..23];
        let few: &'static [&'static [&'static str]] =
            Box::leak(vec![FERRIS.frames[0], few].into_boxed_slice());
        assert!(broken(|s| s.frames = few).unwrap_err().contains("rows"));
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
