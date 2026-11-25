//! Display-cell regressions for the terminal statusline.
use std::io::Write;
use std::process::{Command, Stdio};
use unicode_width::UnicodeWidthStr;

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            assert_eq!(chars.next(), Some('['));
            assert!(chars.by_ref().any(|c| ('@'..='~').contains(&c)));
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn unicode_width_rules() {
    assert_eq!(UnicodeWidthStr::width("中Ａ"), 4);
    for c in "●‣✓↑↓·…─│╭╮╰╯".chars() {
        assert_eq!(UnicodeWidthStr::width(c.to_string().as_str()), 1, "{c}");
    }
    assert_eq!(
        UnicodeWidthStr::width(strip_ansi("\x1b[38;2;1;2;3m中\x1b[0m").as_str()),
        2
    );
}

#[test]
fn statusline_cjk_fits_at_requested_widths() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("專案（測試）mixed中文");
    for dir in [
        project.clone(),
        tmp.path().join("home"),
        tmp.path().join("data"),
        tmp.path().join("live"),
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let input = serde_json::json!({"model":{"display_name":"模型中文Ａmixed"}, "cwd":project, "context_window":{"used_percentage":42.0,"context_window_size":1000000}, "rate_limits":{"five_hour":{"used_percentage":81.0,"resets_at":4102444800_i64},"seven_day":{"used_percentage":99.0,"resets_at":4102444800_i64}}}).to_string();
    for width in [40, 60, 80, 120, 200] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codeforge"))
            .arg("statusline")
            .current_dir(&project)
            .env("HOME", tmp.path().join("home"))
            .env("CODEFORGE_DATA_DIR", tmp.path().join("data"))
            .env("AUTOPILOT_LIVE_DIR", tmp.path().join("live"))
            .env("CODEFORGE_USAGE_AUTOCOLLECT", "0")
            .env("CODEFORGE_WIDTH", width.to_string())
            .env_remove("NO_COLOR")
            .env_remove("CODEFORGE_DIR")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = String::from_utf8(output.stdout).unwrap();
        assert_eq!(output.matches('\n').count(), 1);
        assert!(output.ends_with('\n'));
        assert!(!output.contains('\u{fffd}'));
        let line = strip_ansi(output.trim_end_matches('\n'));
        assert!(
            UnicodeWidthStr::width(line.as_str()) <= width,
            "{width}: {line:?}"
        );
    }
}
