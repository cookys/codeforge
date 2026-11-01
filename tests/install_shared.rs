//! Binary-level checks for `codeforge install` on shared settings files (B35).
//! They run the compiled binary against a temp settings.json via
//! CODEFORGE_CLAUDE_SETTINGS, never the real ~/.claude/settings.json.

use std::process::Command;

fn codeforge() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_codeforge"));
    c.env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    c
}

#[test]
fn dry_run_on_untracked_file_leaks_no_git_output() {
    let tmp = tempfile::TempDir::new().unwrap();
    let settings = tmp.path().join("settings.json");
    std::fs::write(&settings, "{}").unwrap();
    let out = codeforge()
        .args(["install", "--hooks", "--dry-run"])
        .env("CODEFORGE_CLAUDE_SETTINGS", &settings)
        .env("CODEFORGE_DATA_LOCAL_DIR", tmp.path().join("data"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!all.contains("fatal:"), "git output leaked: {all}");
    assert!(!all.contains("error: pathspec"), "git output leaked: {all}");
    assert!(
        !all.contains("⚠ settings.json"),
        "plain file must not warn: {all}"
    );
}
