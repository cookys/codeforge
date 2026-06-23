//! cite-detect heuristic (ship spec §9, source-contract §11.6 `fulltext_match`).
//!
//! At SessionEnd, scan a transcript for occurrences of each atom's title.
//! A hit → that atom is considered cited. High false-positive rate is acceptable
//! for Sprint 1; Sprint 5+ upgrades to Haiku judgement.

use super::context::ContextAtom;

/// One detected citation: which atom + the matched text.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedCite {
    pub atom_id: String,
    pub matched_text: String,
}

/// Minimum atom-title length to consider for matching — guards against trivial
/// 1-2 char titles producing pervasive false positives.
const MIN_TITLE_LEN: usize = 6;

/// Detect cited atoms by case-insensitive substring match of atom title in transcript.
/// Each atom yields at most one DetectedCite (dedup by atom_id).
pub fn detect(transcript: &str, atoms: &[ContextAtom]) -> Vec<DetectedCite> {
    let haystack = transcript.to_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut hits = Vec::new();
    for a in atoms {
        let title = a.title.trim();
        if title.chars().count() < MIN_TITLE_LEN {
            continue;
        }
        if haystack.contains(&title.to_lowercase()) && seen.insert(a.id.clone()) {
            hits.push(DetectedCite {
                atom_id: a.id.clone(),
                matched_text: title.to_string(),
            });
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(id: &str, title: &str) -> ContextAtom {
        ContextAtom {
            id: id.to_string(),
            kind: "lesson".to_string(),
            title: title.to_string(),
            body: String::new(),
            citation_count: 0,
            pinned: false,
        }
    }

    #[test]
    fn matches_title_in_transcript() {
        let atoms = vec![
            atom("01A", "Notify drops wakeup signals"),
            atom("01B", "Unrelated lesson about caching"),
        ];
        let transcript = "we discovered that Notify drops wakeup signals when no waiter exists";
        let hits = detect(transcript, &atoms);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].atom_id, "01A");
        assert_eq!(hits[0].matched_text, "Notify drops wakeup signals");
    }

    #[test]
    fn case_insensitive() {
        let atoms = vec![atom("01A", "Tokio Shutdown Race")];
        let hits = detect("fixed the tokio shutdown race today", &atoms);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn no_match_returns_empty() {
        let atoms = vec![atom("01A", "Something never mentioned here")];
        let hits = detect("totally different content", &atoms);
        assert!(hits.is_empty());
    }

    #[test]
    fn short_titles_skipped() {
        let atoms = vec![atom("01A", "ABC")]; // < MIN_TITLE_LEN
        let hits = detect("ABC appears here", &atoms);
        assert!(hits.is_empty());
    }

    #[test]
    fn dedups_by_atom_id() {
        let atoms = vec![atom("01A", "repeated phrase here")];
        let hits = detect(
            "repeated phrase here ... repeated phrase here again",
            &atoms,
        );
        assert_eq!(hits.len(), 1);
    }
}
