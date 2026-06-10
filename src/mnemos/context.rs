//! Context fetch (`GET /v1/atoms/context`) + markdown formatting for SessionStart
//! injection, and topic derivation from git branch + recent commits.
//!
//! Response shape (Mnemos api.rs `context` handler): `{ "atoms": [ Atom, ... ] }`.
//! We deserialize only the fields we render — Mnemos may carry more.

use serde::Deserialize;

/// A subset of Mnemos `Atom` (store.rs) — only the fields we render.
#[derive(Debug, Clone, Deserialize)]
pub struct ContextAtom {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub citation_count: i64,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextResponse {
    #[serde(default)]
    pub atoms: Vec<ContextAtom>,
}

/// Render fetched atoms as a markdown block suitable for SystemPromptAddition.
/// Empty atom list → a short "no relevant memory" note (still valid markdown).
/// `header` / `empty_note` are localized labels supplied by the CLI layer.
pub fn format_markdown_with(
    header: &str,
    empty_note: &str,
    topic: &str,
    atoms: &[ContextAtom],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("## {header}\n\n"));
    if topic.is_empty() {
        out.push_str("_topic: (derived from session)_\n\n");
    } else {
        out.push_str(&format!("_topic: {topic}_\n\n"));
    }
    if atoms.is_empty() {
        out.push_str(&format!("_{empty_note}_\n"));
        return out;
    }
    for a in atoms {
        let pin = if a.pinned { "📌 " } else { "" };
        let title = if a.title.trim().is_empty() {
            "(untitled)"
        } else {
            a.title.trim()
        };
        out.push_str(&format!("- {pin}**{title}**"));
        if !a.kind.is_empty() {
            out.push_str(&format!(" _({})_", a.kind));
        }
        if a.citation_count > 0 {
            out.push_str(&format!(" — cited {}×", a.citation_count));
        }
        out.push('\n');
        let body = a.body.trim();
        if !body.is_empty() {
            // indent body under the bullet; collapse to a single quoted block
            for line in body.lines() {
                out.push_str(&format!("  > {line}\n"));
            }
        }
        out.push_str(&format!("  <sub>atom: {}</sub>\n", a.id));
    }
    out
}

/// Convenience wrapper with default English labels (used by tests).
#[cfg(test)]
pub fn format_markdown(topic: &str, atoms: &[ContextAtom]) -> String {
    format_markdown_with(
        "Mnemos — relevant memory",
        "No relevant atoms surfaced for this session.",
        topic,
        atoms,
    )
}

/// Derive a context topic from the current git branch + recent commit subjects
/// when the user did not pass `--topic`. Best-effort; returns "" if nothing usable.
pub fn derive_topic(repo_dir: &std::path::Path) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(branch) = git_output(repo_dir, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        let b = branch.trim();
        if !b.is_empty() && b != "HEAD" {
            // strip common prefixes like feature/ fix/ so the topic reads as keywords
            let cleaned = b
                .rsplit('/')
                .next()
                .unwrap_or(b)
                .replace(['-', '_'], " ");
            parts.push(cleaned);
        }
    }

    if let Some(log) = git_output(
        repo_dir,
        &["log", "-3", "--pretty=format:%s"],
    ) {
        for subject in log.lines().take(3) {
            let s = subject.trim();
            if !s.is_empty() {
                parts.push(s.to_string());
            }
        }
    }

    parts.join(" ").trim().to_string()
}

fn git_output(repo_dir: &std::path::Path, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(id: &str, title: &str, body: &str, cites: i64, pinned: bool) -> ContextAtom {
        ContextAtom {
            id: id.to_string(),
            kind: "lesson".to_string(),
            title: title.to_string(),
            body: body.to_string(),
            citation_count: cites,
            pinned,
        }
    }

    #[test]
    fn parses_mnemos_context_response() {
        let json = serde_json::json!({
            "atoms": [
                { "id": "01A", "kind": "lesson", "title": "T", "body": "B", "citation_count": 3, "pinned": true }
            ]
        });
        let resp: ContextResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.atoms.len(), 1);
        assert_eq!(resp.atoms[0].id, "01A");
        assert!(resp.atoms[0].pinned);
        assert_eq!(resp.atoms[0].citation_count, 3);
    }

    #[test]
    fn parses_response_with_unknown_extra_fields() {
        // Mnemos Atom carries more fields (confidence, evidence_refs, ...); must not break.
        let json = serde_json::json!({
            "atoms": [
                { "id": "01A", "title": "T", "body": "B", "confidence": 0.8,
                  "evidence_refs": [], "supersedes": null, "when_hint": "x" }
            ]
        });
        let resp: ContextResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.atoms[0].id, "01A");
    }

    #[test]
    fn empty_atoms_renders_note() {
        let md = format_markdown("rust shutdown", &[]);
        assert!(md.contains("Mnemos"));
        assert!(md.contains("No relevant atoms"));
        assert!(md.contains("rust shutdown"));
    }

    #[test]
    fn formats_atoms_as_markdown() {
        let atoms = vec![
            atom("01A", "Notify drops wakeup", "Use mpsc channel.", 2, true),
            atom("01B", "Untitled lesson", "", 0, false),
        ];
        let md = format_markdown("ipc shutdown", &atoms);
        assert!(md.contains("📌 **Notify drops wakeup**"));
        assert!(md.contains("cited 2×"));
        assert!(md.contains("> Use mpsc channel."));
        assert!(md.contains("atom: 01A"));
        assert!(md.contains("atom: 01B"));
        // no citation suffix when count == 0
        let second_block = md.split("01B").next().unwrap();
        assert!(!second_block.ends_with("cited 0×"));
    }

    #[test]
    fn untitled_atom_handled() {
        let atoms = vec![atom("01C", "   ", "body", 0, false)];
        let md = format_markdown("t", &atoms);
        assert!(md.contains("(untitled)"));
    }
}
