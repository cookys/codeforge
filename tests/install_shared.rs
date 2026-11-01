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
        .env("CODEFORGE_DATA_DIR", tmp.path().join("data"))
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
    assert!(
        all.contains("\"cleanupPeriodDays\": 3650"),
        "a local file keeps the default retention: {all}"
    );
}

#[test]
fn hook_usage_errors_exit_one_never_two() {
    let tmp = tempfile::TempDir::new().unwrap();
    for args in [vec!["hook"], vec!["hook", "no-such-hook"]] {
        let out = codeforge()
            .args(&args)
            .env("CODEFORGE_DATA_DIR", tmp.path())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "{args:?}: {:?}", out);
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("emit-session"), "lists the hooks: {err}");
    }
    // --help is not an error
    let out = codeforge().args(["hook", "--help"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn hook_runs_embedded_script_from_private_cache() {
    let node_ok = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !node_ok {
        eprintln!("skip: node not available");
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let mut child = codeforge()
        .args(["hook", "session-digest"])
        .env("CODEFORGE_DATA_DIR", tmp.path().join("data"))
        .env("HOME", &home)
        .current_dir(tmp.path())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"{}").unwrap();
    }
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    let cached = tmp
        .path()
        .join("data/hooks")
        .join(env!("CARGO_PKG_VERSION"))
        .join("session-digest.js");
    assert!(
        cached.is_file(),
        "script materialized at {}",
        cached.display()
    );
}

#[cfg(unix)]
#[test]
fn dry_run_on_symlinked_settings_warns_and_writes_portable_commands() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dot = tmp.path().join("dotfiles");
    std::fs::create_dir_all(&dot).unwrap();
    std::fs::write(dot.join("settings.json"), "{}").unwrap();
    let link = tmp.path().join("settings.json");
    std::os::unix::fs::symlink(dot.join("settings.json"), &link).unwrap();
    let out = codeforge()
        .args(["install", "--all", "--dry-run"])
        .env("CODEFORGE_CLAUDE_SETTINGS", &link)
        .env("CODEFORGE_DATA_DIR", tmp.path().join("data"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("⚠ settings.json 是 symlink"), "{stdout}");
    let json_start = stdout
        .find("\n{")
        .expect("dry-run prints the settings JSON");
    let json_end = stdout.rfind('}').unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout[json_start..=json_end]).unwrap();
    assert_eq!(v["statusLine"]["command"], "codeforge statusline");
    assert!(
        v.get("cleanupPeriodDays").is_none(),
        "shared file gets no cleanupPeriodDays without the flag"
    );
    assert!(stdout.contains("--cleanup-period-days 3650"), "{stdout}");
    for groups in v["hooks"].as_object().unwrap().values() {
        for g in groups.as_array().unwrap() {
            for h in g["hooks"].as_array().unwrap() {
                let cmd = h["command"].as_str().unwrap();
                assert!(cmd.starts_with("codeforge "), "{cmd}");
                assert!(!cmd.contains(env!("CARGO_PKG_VERSION")), "{cmd}");
            }
        }
    }
    // the link and its target are untouched by a dry-run
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read_to_string(dot.join("settings.json")).unwrap(),
        "{}"
    );
}
