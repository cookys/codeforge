//! Phase 3a — L1 memory language distribution.
//!
//! Scan the L1 store (`{ctx.project_dir}/store/concepts/*.md` and
//! `{ctx.project_dir}/store/qa/*.md`) and accumulate a per-village count
//! of concepts whose body mentions language-affiliated keywords. The
//! output feeds `game_world.concept_count`, which gates Zone unlock and
//! fuels the `codeforge world` map.
//!
//! Heuristic, not exact: a concept can contribute to multiple villages
//! (e.g. "merge conflict" mentions both Rust and Python). This is
//! intentional — Zone activity is a soft metric, not an exclusive tag.
//!
//! Exact classification would want an LLM pass, but the compile cycle
//! already spends API budget on classification; piggybacking a second
//! round just for Zone display is not worth it today.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Per-village keywords. Case-insensitive match against concept body.
/// Intentionally narrow — generic mentions ("is a language") must not
/// trip all five villages at once. Build-file names (`Cargo.toml`,
/// `go.mod`, `package.json`) are the clearest signals that someone
/// was actually working in that ecosystem.
fn keywords_for_village() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        ("rust", &["rust", "cargo", "rustc", "cargo.toml", "ferris", "tokio", "clippy"]),
        ("python", &["python", "pip", "django", "flask", "requirements.txt", "pyproject", "pytest"]),
        ("typescript", &["typescript", "tsconfig", "tsc", "ts-node"]),
        ("go", &["golang", "go.mod", "goroutine", "gopath", "gofmt"]),
        ("javascript", &["javascript", "npm", "node.js", "package.json", "node_modules"]),
    ]
}

/// Language distribution across the L1 store. Returns a count per known
/// village id; villages with no hits appear as 0. Scanning errors on
/// individual files are logged to stderr and skipped so one corrupt
/// file never blocks the whole walk.
pub fn language_distribution(store_dir: &Path) -> Result<HashMap<String, u32>> {
    let mut counts: HashMap<String, u32> = keywords_for_village()
        .iter()
        .map(|(v, _)| ((*v).to_string(), 0_u32))
        .collect();

    for subdir in ["concepts", "qa", "connections"] {
        let dir = store_dir.join(subdir);
        if !dir.exists() {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("L1 stats: skip {} — {e}", dir.display());
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|e| e == "md") {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("L1 stats: skip {} — {e}", path.display());
                    continue;
                }
            };
            let lowercase = content.to_ascii_lowercase();
            for (village, keywords) in keywords_for_village() {
                if keywords.iter().any(|kw| lowercase.contains(kw)) {
                    if let Some(count) = counts.get_mut(*village) {
                        *count = count.saturating_add(1);
                    }
                }
            }
        }
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_concept(root: &Path, name: &str, body: &str) {
        let concepts = root.join("concepts");
        fs::create_dir_all(&concepts).unwrap();
        fs::write(concepts.join(format!("{name}.md")), body).unwrap();
    }

    #[test]
    fn empty_store_returns_zero_for_all_villages() {
        let dir = TempDir::new().unwrap();
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("rust"), Some(&0));
        assert_eq!(counts.get("python"), Some(&0));
        assert_eq!(counts.get("typescript"), Some(&0));
        assert_eq!(counts.get("go"), Some(&0));
        assert_eq!(counts.get("javascript"), Some(&0));
    }

    #[test]
    fn rust_keyword_counts_toward_rust_village() {
        let dir = TempDir::new().unwrap();
        write_concept(dir.path(), "trait-objects", "Rust trait objects use vtables.");
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("rust"), Some(&1));
        // Python shouldn't be tripped by a Rust-only concept
        assert_eq!(counts.get("python"), Some(&0));
    }

    #[test]
    fn python_cargo_mention_counts_python_only() {
        // "cargo" is a Rust keyword; "pip" is Python. A body about pip
        // alone shouldn't trigger rust.
        let dir = TempDir::new().unwrap();
        write_concept(dir.path(), "venv", "Use pip install to manage venv.");
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("python"), Some(&1));
        assert_eq!(counts.get("rust"), Some(&0));
    }

    #[test]
    fn cross_language_concept_counts_both() {
        // Test the intentional soft-metric behaviour.
        let dir = TempDir::new().unwrap();
        write_concept(
            dir.path(),
            "merge-conflict",
            "Cargo and pip both resolve dependencies differently.",
        );
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("rust"), Some(&1));
        assert_eq!(counts.get("python"), Some(&1));
    }

    #[test]
    fn case_insensitive_matching() {
        let dir = TempDir::new().unwrap();
        write_concept(dir.path(), "upper", "RUST is fine; CARGO too.");
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("rust"), Some(&1));
    }

    #[test]
    fn qa_and_connections_also_contribute() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("qa")).unwrap();
        fs::create_dir_all(dir.path().join("connections")).unwrap();
        fs::write(
            dir.path().join("qa").join("a.md"),
            "Q: python question? A: yes.",
        )
        .unwrap();
        fs::write(
            dir.path().join("connections").join("b.md"),
            "Rust and TypeScript interop via wasm.",
        )
        .unwrap();
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("python"), Some(&1));
        assert_eq!(counts.get("rust"), Some(&1));
        assert_eq!(counts.get("typescript"), Some(&1));
    }

    #[test]
    fn non_md_files_are_skipped() {
        let dir = TempDir::new().unwrap();
        let concepts = dir.path().join("concepts");
        fs::create_dir_all(&concepts).unwrap();
        // .txt should not be counted
        fs::write(concepts.join("rust-notes.txt"), "Rust is fast.").unwrap();
        // .md should be
        fs::write(concepts.join("rust-md.md"), "Rust is safe.").unwrap();
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("rust"), Some(&1));
    }

    #[test]
    fn missing_subdirs_do_not_error() {
        // Only concepts exists — qa/connections absent. Should still work.
        let dir = TempDir::new().unwrap();
        write_concept(dir.path(), "a", "rust");
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("rust"), Some(&1));
    }

    #[test]
    fn multiple_rust_keywords_in_one_file_count_once() {
        // Spec: a concept contributes at most +1 per village, regardless
        // of how many distinct keywords fire. Prevents a copy-paste of
        // the Rust book from inflating the count.
        let dir = TempDir::new().unwrap();
        write_concept(
            dir.path(),
            "all-of-em",
            "Rust, Cargo, rustc, Cargo.toml, tokio, clippy, ferris",
        );
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("rust"), Some(&1));
    }

    #[test]
    fn multiple_files_accumulate() {
        let dir = TempDir::new().unwrap();
        write_concept(dir.path(), "a", "rust");
        write_concept(dir.path(), "b", "rust");
        write_concept(dir.path(), "c", "rust");
        let counts = language_distribution(dir.path()).unwrap();
        assert_eq!(counts.get("rust"), Some(&3));
    }
}
