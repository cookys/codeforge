//! Duplicate code block detection for the Doppelganger MOB (B10).
//!
//! Keywords (for future lookups): duplicate code detection, clone detection
//! heuristic, sliding window line hash. Approach = normalized-line
//! sliding window hash + text verification on hit + O(hits) diagonal
//! chain walk; no AST/token clone detector, no rolling hash (U1 consult,
//! plan §6).
//!
//! A game signal, not a refactoring tool: it is deliberately cheap and
//! conservative. Missing a real duplicate is fine; flagging two `}` lines
//! as "duplicate code" is not. Design + review history:
//! `doc/plans/2026-11-02-doppelganger-split.md` §3.1.
//!
//! Pipeline per file (`DupIndex::add_file`):
//! 1. Classify each line — blank / comment / trivial lines are skipped;
//!    import, declaration and attribute lines (including multi-line
//!    groups) are *boundaries* that cut the window, so unrelated code on
//!    either side is never stitched into one block.
//! 2. Slide a window of [`WINDOW_LINES`] kept lines inside each segment;
//!    windows that are too short or too repetitive are not indexed.
//! 3. Hash each window; a hash keeps at most one (earliest) position per
//!    file and at most [`MAX_OCCURRENCES`] files.
//!
//! `DupIndex::find_pairs` then pairs positions across files, verifies the
//! text on every hash hit, measures the longest aligned run per file pair
//! in O(hits), and returns the pairs sorted deterministically.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Kept lines per window.
pub const WINDOW_LINES: usize = 8;
/// Minimum normalized bytes in one window.
pub const MIN_WINDOW_BYTES: usize = 200;
/// Minimum distinct lines in one window.
pub const MIN_DISTINCT_LINES: usize = 4;
/// Files remembered per window hash (one position each).
pub const MAX_OCCURRENCES: usize = 8;
/// Cumulative normalized bytes indexed per scan.
pub const INDEX_BUDGET_BYTES: usize = 4 * 1024 * 1024;

/// Lines whose alphanumeric-only content is one of these are structural
/// noise (`} else {`, `return;`, `pass`).
const TRIVIAL_WORDS: &[&str] = &[
    "else", "return", "break", "continue", "pass", "default", "try", "finally", "do", "end",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    Script, // ts / js
    Go,
}

impl Lang {
    fn from_ext(ext: &str) -> Option<Self> {
        Some(match ext {
            "rs" => Self::Rust,
            "py" => Self::Python,
            "ts" | "js" => Self::Script,
            "go" => Self::Go,
            _ => return None,
        })
    }
}

/// Multi-line constructs tracked across lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    BlockComment,
    PyString(&'static str),
    /// Inside a multi-line import / declaration / attribute: every line is
    /// a boundary until the terminator shows up.
    Boundary(Terminator),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Terminator {
    Semicolon,    // rust `use …{` … `};`
    CloseParen,   // python `from x import (` … `)`
    ScriptImport, // ts/js `import {` … `} from '…';`
    GoParen,      // go `import (` … `)`
    CloseBracket, // rust `#[…` … `]`
}

#[derive(Debug, PartialEq, Eq)]
enum LineClass {
    Skip,
    Boundary,
    Keep,
}

/// A detected duplicate between two files. `a` < `b` by byte order of the
/// scan-root-relative path; `block_lines` is the longest aligned run in
/// kept lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupPair {
    pub a: String,
    pub b: String,
    pub block_lines: usize,
}

struct FileEntry {
    rel: String,
    lines: Vec<String>,
}

/// Per-scan index. Build with `add_file` in traversal order, then call
/// `find_pairs`.
#[derive(Default)]
pub struct DupIndex {
    files: Vec<FileEntry>,
    buckets: HashMap<u64, Vec<(u32, u32)>>,
    indexed_bytes: usize,
    exhausted: bool,
}

impl DupIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when a file should never take part in duplicate detection:
    /// generated or minified sources.
    pub fn is_excluded(rel: &str, text: &str) -> bool {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if name.ends_with(".min.js")
            || name.ends_with(".d.ts")
            || name.ends_with(".pb.go")
            || name.ends_with("_pb2.py")
        {
            return true;
        }
        text.lines()
            .take(5)
            .any(|l| l.contains("@generated") || l.contains("DO NOT EDIT"))
    }

    /// Index one file. Returns false (and indexes nothing) once the
    /// per-scan byte budget is exhausted or the extension is unknown.
    pub fn add_file(&mut self, rel: &str, ext: &str, text: &str) -> bool {
        if self.exhausted {
            return false;
        }
        let Some(lang) = Lang::from_ext(ext) else {
            return false;
        };
        let (lines, segs) = normalize(text, lang);
        let bytes: usize = lines.iter().map(|l| l.len()).sum();
        if self.indexed_bytes + bytes > INDEX_BUDGET_BYTES {
            self.exhausted = true;
            return false;
        }
        self.indexed_bytes += bytes;

        let file_idx = self.files.len() as u32;
        for start in 0..lines.len().saturating_sub(WINDOW_LINES - 1) {
            let end = start + WINDOW_LINES;
            if segs[start] != segs[end - 1] || !window_is_substantial(&lines[start..end]) {
                continue;
            }
            let h = hash_window(&lines[start..end]);
            let bucket = self.buckets.entry(h).or_default();
            if bucket.len() >= MAX_OCCURRENCES {
                continue;
            }
            if bucket.last().map(|&(f, _)| f) == Some(file_idx) {
                continue; // this file already has its earliest position
            }
            bucket.push((file_idx, start as u32));
        }
        self.files.push(FileEntry {
            rel: rel.to_string(),
            lines,
        });
        true
    }

    /// All file pairs sharing at least one verified window, sorted by
    /// (longest block desc, a, b).
    pub fn find_pairs(&self) -> Vec<DupPair> {
        let mut hits: HashMap<(u32, u32), HashSet<(u32, u32)>> = HashMap::new();
        for bucket in self.buckets.values() {
            for i in 0..bucket.len() {
                for j in (i + 1)..bucket.len() {
                    let (fa, ka) = bucket[i];
                    let (fb, kb) = bucket[j];
                    if fa == fb || !self.windows_equal(fa, ka, fb, kb) {
                        continue;
                    }
                    let (key, point) = if fa < fb {
                        ((fa, fb), (ka, kb))
                    } else {
                        ((fb, fa), (kb, ka))
                    };
                    hits.entry(key).or_default().insert(point);
                }
            }
        }
        let mut out: Vec<DupPair> = hits
            .into_iter()
            .map(|((fa, fb), points)| {
                let ra = &self.files[fa as usize].rel;
                let rb = &self.files[fb as usize].rel;
                let (a, b) = if ra.as_bytes() <= rb.as_bytes() {
                    (ra.clone(), rb.clone())
                } else {
                    (rb.clone(), ra.clone())
                };
                DupPair {
                    a,
                    b,
                    block_lines: WINDOW_LINES + longest_chain(&points) - 1,
                }
            })
            .collect();
        out.sort_by(|x, y| {
            y.block_lines
                .cmp(&x.block_lines)
                .then_with(|| x.a.as_bytes().cmp(y.a.as_bytes()))
                .then_with(|| x.b.as_bytes().cmp(y.b.as_bytes()))
        });
        out
    }

    fn windows_equal(&self, fa: u32, ka: u32, fb: u32, kb: u32) -> bool {
        let la = &self.files[fa as usize].lines;
        let lb = &self.files[fb as usize].lines;
        let (ka, kb) = (ka as usize, kb as usize);
        la[ka..ka + WINDOW_LINES] == lb[kb..kb + WINDOW_LINES]
    }
}

/// Longest run of points advancing by (+1, +1). Each point is visited at
/// most once: walks start only where (ka-1, kb-1) is absent.
pub fn longest_chain(points: &HashSet<(u32, u32)>) -> usize {
    let mut best = 0usize;
    for &(ka, kb) in points {
        if ka > 0 && kb > 0 && points.contains(&(ka - 1, kb - 1)) {
            continue;
        }
        let mut len = 1usize;
        let (mut x, mut y) = (ka, kb);
        while points.contains(&(x + 1, y + 1)) {
            x += 1;
            y += 1;
            len += 1;
        }
        best = best.max(len);
    }
    best
}

fn hash_window(lines: &[String]) -> u64 {
    let mut h = DefaultHasher::new();
    for l in lines {
        l.as_str().hash(&mut h); // `str` hashing appends a terminator
    }
    h.finish()
}

fn window_is_substantial(lines: &[String]) -> bool {
    let bytes: usize = lines.iter().map(|l| l.len()).sum();
    if bytes < MIN_WINDOW_BYTES {
        return false;
    }
    let mut distinct: Vec<&str> = Vec::with_capacity(lines.len());
    for l in lines {
        if !distinct.contains(&l.as_str()) {
            distinct.push(l.as_str());
        }
    }
    distinct.len() >= MIN_DISTINCT_LINES
}

/// Kept (normalized) lines plus a segment id per kept line. A boundary
/// line bumps the segment so no window can span it.
fn normalize(text: &str, lang: Lang) -> (Vec<String>, Vec<u32>) {
    let mut lines = Vec::new();
    let mut segs = Vec::new();
    let mut seg = 0u32;
    let mut state = State::Normal;
    for raw in text.lines() {
        let t = raw.trim();
        let (class, next) = classify(t, lang, state);
        state = next;
        match class {
            LineClass::Skip => {}
            LineClass::Boundary => seg += 1,
            LineClass::Keep => {
                lines.push(t.to_string());
                segs.push(seg);
            }
        }
    }
    (lines, segs)
}

fn classify(t: &str, lang: Lang, state: State) -> (LineClass, State) {
    match state {
        State::BlockComment => {
            let next = if t.contains("*/") {
                State::Normal
            } else {
                state
            };
            return (LineClass::Skip, next);
        }
        State::PyString(q) => {
            let next = if t.contains(q) { State::Normal } else { state };
            return (LineClass::Skip, next);
        }
        State::Boundary(term) => {
            let done = match term {
                Terminator::Semicolon => t.ends_with(';'),
                Terminator::CloseParen => t.contains(')'),
                Terminator::ScriptImport => t.contains("from") || t.ends_with(';'),
                Terminator::GoParen => t.starts_with(')'),
                Terminator::CloseBracket => t.ends_with(']'),
            };
            return (
                LineClass::Boundary,
                if done { State::Normal } else { state },
            );
        }
        State::Normal => {}
    }

    if t.is_empty() {
        return (LineClass::Skip, State::Normal);
    }

    // Comments.
    match lang {
        Lang::Python => {
            if t.starts_with('#') {
                return (LineClass::Skip, State::Normal);
            }
            for q in ["\"\"\"", "'''"] {
                if let Some(rest) = t.strip_prefix(q) {
                    let next = if rest.contains(q) {
                        State::Normal
                    } else {
                        State::PyString(q)
                    };
                    return (LineClass::Skip, next);
                }
            }
        }
        _ => {
            if t.starts_with("//") {
                return (LineClass::Skip, State::Normal);
            }
            if let Some(rest) = t.strip_prefix("/*") {
                let next = if rest.contains("*/") {
                    State::Normal
                } else {
                    State::BlockComment
                };
                return (LineClass::Skip, next);
            }
            if t == "*" || t.starts_with("*/") || t.starts_with("* ") {
                return (LineClass::Skip, State::Normal);
            }
        }
    }

    // Code followed by an unclosed `/*` (review g1): the line itself is
    // classified as usual and the comment body after it is skipped.
    let opens_comment = lang != Lang::Python && opens_unclosed_block_comment(t);

    // Boundaries: imports, declarations, attributes, decorators. Checked
    // before the comment state (review g2): a boundary line stays a
    // boundary, and a multi-line continuation it opens wins over the
    // comment (its lines are boundaries until the terminator anyway).
    if let Some(next) = boundary(t, lang) {
        let next = if opens_comment && next == State::Normal {
            State::BlockComment
        } else {
            next
        };
        return (LineClass::Boundary, next);
    }

    let next = if opens_comment {
        State::BlockComment
    } else {
        State::Normal
    };
    if is_trivial(t) {
        return (LineClass::Skip, next);
    }
    (LineClass::Keep, next)
}

/// `Some(next_state)` when `t` is a boundary line.
fn boundary(t: &str, lang: Lang) -> Option<State> {
    match lang {
        Lang::Rust => {
            if t.starts_with("#[") || t.starts_with("#![") {
                return Some(if t.ends_with(']') {
                    State::Normal
                } else {
                    State::Boundary(Terminator::CloseBracket)
                });
            }
            let s = strip_rust_visibility(t);
            if s.starts_with("use ") {
                return Some(if t.ends_with(';') {
                    State::Normal
                } else {
                    State::Boundary(Terminator::Semicolon)
                });
            }
            if s.starts_with("extern crate") || (s.starts_with("mod ") && t.ends_with(';')) {
                return Some(State::Normal);
            }
            None
        }
        Lang::Python => {
            if t.starts_with('@') {
                return Some(State::Normal);
            }
            if t.starts_with("import ") || t.starts_with("from ") {
                return Some(if t.contains('(') && !t.contains(')') {
                    State::Boundary(Terminator::CloseParen)
                } else {
                    State::Normal
                });
            }
            None
        }
        Lang::Script => {
            if t.starts_with('@') {
                return Some(State::Normal);
            }
            let is_import = t.starts_with("import ")
                || t.starts_with("import{")
                || t.starts_with("export {")
                || t.starts_with("export{")
                || t.starts_with("export * ")
                || (t.starts_with("export ") && t.contains(" from "));
            if is_import {
                let complete =
                    t.contains("from") || t.ends_with(';') || t.contains('\'') || t.contains('"');
                return Some(if complete {
                    State::Normal
                } else {
                    State::Boundary(Terminator::ScriptImport)
                });
            }
            None
        }
        Lang::Go => {
            if t.starts_with("package ") {
                return Some(State::Normal);
            }
            if t.starts_with("import") {
                return Some(if t.contains('(') && !t.contains(')') {
                    State::Boundary(Terminator::GoParen)
                } else {
                    State::Normal
                });
            }
            None
        }
    }
}

/// True when `t` leaves a `/*` block comment open: a `/*` outside a
/// double-quoted string, before any `//`, with no `*/` after it.
fn opens_unclosed_block_comment(t: &str) -> bool {
    let b = t.as_bytes();
    let mut in_string = false;
    let mut in_comment = false;
    let mut i = 0;
    while i < b.len() {
        if in_comment {
            if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                in_comment = false;
                i += 2;
                continue;
            }
        } else if in_string {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            if b[i] == b'"' {
                in_string = false;
            }
        } else if b[i] == b'"' {
            in_string = true;
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            return false;
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            in_comment = true;
            i += 2;
            continue;
        }
        i += 1;
    }
    in_comment
}

/// Drop one leading Rust visibility qualifier: `pub`, `pub(crate)`,
/// `pub(super)`, `pub(self)`, `pub(in path)`.
fn strip_rust_visibility(t: &str) -> &str {
    if let Some(rest) = t.strip_prefix("pub(") {
        if let Some(close) = rest.find(')') {
            return rest[close + 1..].trim_start();
        }
        return t;
    }
    if let Some(rest) = t.strip_prefix("pub ") {
        return rest.trim_start();
    }
    t
}

fn is_trivial(t: &str) -> bool {
    let alnum: String = t.chars().filter(|c| c.is_alphanumeric()).collect();
    if alnum.chars().count() < 3 {
        return true;
    }
    TRIVIAL_WORDS.contains(&alnum.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(points: &[(u32, u32)]) -> HashSet<(u32, u32)> {
        points.iter().copied().collect()
    }

    #[test]
    fn longest_chain_handles_double_alignment() {
        let p = set(&[(0, 0), (0, 100), (1, 1), (1, 101), (2, 2)]);
        assert_eq!(longest_chain(&p), 3);
    }

    #[test]
    fn longest_chain_single_and_empty() {
        assert_eq!(longest_chain(&set(&[(5, 9)])), 1);
        assert_eq!(longest_chain(&HashSet::new()), 0);
    }

    #[test]
    fn trivial_lines_are_skipped() {
        for t in [
            "}", "});", ")", "} else {", "else {", "return;", "pass", "end",
        ] {
            assert!(is_trivial(t), "{t:?} should be trivial");
        }
        assert!(!is_trivial("return Ok(value);"));
    }

    #[test]
    fn rust_visibility_is_stripped_before_boundary_check() {
        for t in [
            "pub use a::b;",
            "pub(crate) use a::b;",
            "pub(super) use a::b;",
            "pub(in crate::x) use a::b;",
            "pub extern crate foo;",
            "pub mod foo;",
        ] {
            assert!(boundary(t, Lang::Rust).is_some(), "{t:?}");
        }
        assert_eq!(
            boundary("pub(crate) use a::{", Lang::Rust),
            Some(State::Boundary(Terminator::Semicolon))
        );
        assert!(boundary("pub fn run() {", Lang::Rust).is_none());
    }

    #[test]
    fn block_comment_body_without_stars_is_skipped() {
        let text =
            "/*\nlong prose line one here\nlong prose line two here\n*/\nlet real_code = 1;\n";
        let (lines, _) = normalize(text, Lang::Rust);
        assert_eq!(lines, vec!["let real_code = 1;".to_string()]);
    }

    #[test]
    fn block_comment_opened_mid_line_is_tracked() {
        // The opener's code part is kept; the comment body is not.
        let text = "let a = 1; /*\nprose inside the comment\n*/\nlet real_code = 2;\n";
        let (lines, _) = normalize(text, Lang::Rust);
        assert_eq!(
            lines,
            vec![
                "let a = 1; /*".to_string(),
                "let real_code = 2;".to_string()
            ]
        );
    }

    #[test]
    fn comment_opener_inside_string_or_line_comment_is_ignored() {
        let text = "let glob = \"src/*.rs\";\nlet next_line = 2;\n// see /* old\nlet third = 3;\n";
        let (lines, _) = normalize(text, Lang::Rust);
        assert_eq!(
            lines,
            vec![
                "let glob = \"src/*.rs\";".to_string(),
                "let next_line = 2;".to_string(),
                "let third = 3;".to_string(),
            ]
        );
    }

    #[test]
    fn python_docstring_and_paren_import_are_handled() {
        let text = "from x import (\n    alpha,\n    beta,\n)\n\"\"\"\ndoc text here\n\"\"\"\nvalue = compute()\n";
        let (lines, _) = normalize(text, Lang::Python);
        assert_eq!(lines, vec!["value = compute()".to_string()]);
    }

    #[test]
    fn boundary_bumps_segment() {
        let text = "let first_line = 1;\nuse a::b;\nlet second_line = 2;\n";
        let (lines, segs) = normalize(text, Lang::Rust);
        assert_eq!(lines.len(), 2);
        assert_ne!(segs[0], segs[1]);
    }

    #[test]
    fn deref_line_is_kept() {
        let (lines, _) = normalize("*counter += step_size;\n", Lang::Rust);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn excluded_files() {
        assert!(DupIndex::is_excluded("web/app.min.js", ""));
        assert!(DupIndex::is_excluded("types/x.d.ts", ""));
        assert!(DupIndex::is_excluded("a.rs", "// @generated\nfn x() {}"));
        assert!(!DupIndex::is_excluded("a.rs", "fn x() {}"));
    }
}
