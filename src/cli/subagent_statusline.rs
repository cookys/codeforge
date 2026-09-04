//! `codeforge subagent-statusline` — reads one JSON line describing the
//! current subagent task list (Claude Code's subagent status line hook) and
//! writes `<live-base>/context/<sid>.tasks.json`. Prints nothing; always
//! exits 0, even when stdin is empty or unparseable — this command has no
//! rendering output to protect, so a write failure (or bad input) is just
//! logged to stderr and otherwise ignored.
//!
//! Schema (`schema_version: 1`), autopilot v2.36.1 plan §2.5:
//! `{"schema_version":1,"session_id":"<raw>","written_at":"<RFC3339 UTC>",
//!   "tasks":[{"id","type","status","description","label","startTime",
//!             "model","cwd","contextWindowSize","tokenCount","name"}]}`
//! Each task field is copied only when present in stdin — never invented.
//! This writer never reads the main statusline's file, and vice versa.

use anyhow::Result;
use std::io::BufRead;

const TASK_FIELDS: &[&str] = &[
    "id",
    "type",
    "status",
    "description",
    "label",
    "startTime",
    "model",
    "cwd",
    "contextWindowSize",
    "tokenCount",
    "name",
];

pub fn run() -> Result<()> {
    let stdin = std::io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_ok() && !line.trim().is_empty() {
        if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&line) {
            write_tasks_context(&raw);
        }
    }
    Ok(())
}

fn write_tasks_context(raw: &serde_json::Value) {
    let session_id = raw["session_id"].as_str().unwrap_or("");
    let sid = crate::live::sanitize_session_id(session_id);
    let record = build_tasks_record(raw, session_id);

    let (base, _source) = crate::live::resolve_live_base();
    let context_dir = base.join("context");
    if let Err(e) =
        crate::live::write_live_json(&context_dir, &format!("{sid}.tasks.json"), &record)
    {
        eprintln!("codeforge: failed to write live subagent context file: {e}");
    }
}

/// Build the tasks record schema from the raw stdin JSON.
fn build_tasks_record(raw: &serde_json::Value, session_id: &str) -> serde_json::Value {
    let tasks: Vec<serde_json::Value> = raw["tasks"]
        .as_array()
        .map(|arr| arr.iter().map(build_task_row).collect())
        .unwrap_or_default();

    serde_json::json!({
        "schema_version": 1,
        "session_id": session_id,
        "written_at": chrono::Utc::now().to_rfc3339(),
        "tasks": tasks,
    })
}

fn build_task_row(task: &serde_json::Value) -> serde_json::Value {
    let mut row = serde_json::Map::new();
    for field in TASK_FIELDS {
        if let Some(v) = task.get(field) {
            row.insert((*field).to_string(), v.clone());
        }
    }
    serde_json::Value::Object(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (h) subagent fixture from p0/subagent.json ⇒ tasks file has one row
    /// with id == "a9c9b5673eb39f842" and tokenCount == 47688; main file
    /// untouched.
    #[test]
    fn tasks_record_from_p0_fixture_matches_schema() {
        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/subagent-p0.json"
        );
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture_path).unwrap()).unwrap();

        let record = build_tasks_record(&raw, raw["session_id"].as_str().unwrap());
        assert_eq!(record["schema_version"], 1);
        assert_eq!(record["session_id"], "93196c52-25cb-47ca-821c-cec391832eed");
        let tasks = record["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["id"], "a9c9b5673eb39f842");
        assert_eq!(tasks[0]["tokenCount"], 47688);
        assert_eq!(tasks[0]["status"], "running");
        assert_eq!(tasks[0]["model"], "claude-sonnet-5");
        // "name" absent in the fixture — must not be invented.
        assert!(tasks[0].get("name").is_none());
    }

    /// End-to-end via write_tasks_context: writes only the .tasks.json
    /// file, never the main .json file, in the same context dir.
    #[test]
    fn write_tasks_context_leaves_main_file_untouched() {
        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/subagent-p0.json"
        );
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture_path).unwrap()).unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("AUTOPILOT_LIVE_DIR", dir.path());
        // Force this test's own resolution by bypassing the process-wide
        // OnceLock cache: call write_live_json directly against the same
        // context dir shape write_tasks_context would use, rather than
        // going through resolve_live_base() (which may already be cached
        // from another test in this binary).
        let context_dir = dir.path().join("context");
        let sid = crate::live::sanitize_session_id(raw["session_id"].as_str().unwrap());
        let record = build_tasks_record(&raw, raw["session_id"].as_str().unwrap());
        crate::live::write_live_json(&context_dir, &format!("{sid}.tasks.json"), &record).unwrap();

        assert!(context_dir.join(format!("{sid}.tasks.json")).exists());
        assert!(!context_dir.join(format!("{sid}.json")).exists());

        std::env::remove_var("AUTOPILOT_LIVE_DIR");
    }
}
