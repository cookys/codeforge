//! Statusline colours follow the terminal theme (PM 01M88BD7K5): the compiled binary in a
//! sandboxed HOME / data dir / live dir, never the real ones. Every truecolor foreground must
//! reach 3:1 against the theme's background (4.5:1 for text roles is the unit test's job).

use std::io::Write;
use std::process::{Command, Stdio};

fn contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let l = |(r, g, b): (u8, u8, u8)| {
        let f = |c: u8| {
            let x = c as f64 / 255.0;
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
    };
    let (x, y) = (l(a), l(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn foregrounds(out: &str) -> Vec<(u8, u8, u8)> {
    out.split("38;2;")
        .skip(1)
        .filter_map(|p| {
            let n: Vec<u8> = p
                .split([';', 'm'])
                .take(3)
                .filter_map(|x| x.parse().ok())
                .collect();
            (n.len() == 3).then(|| (n[0], n[1], n[2]))
        })
        .collect()
}

fn run(envs: &[(&str, &str)]) -> String {
    let tmp = tempfile::TempDir::new().unwrap();
    let p = tmp.path();
    for d in ["home", "proj", "data", "live"] {
        std::fs::create_dir_all(p.join(d)).unwrap();
    }
    let mut c = Command::new(env!("CARGO_BIN_EXE_codeforge"));
    c.arg("statusline")
        .current_dir(p.join("proj"))
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("CODEFORGE_DIR")
        .env_remove("CODEFORGE_ACCOUNT_ALIAS")
        .env_remove("CODEFORGE_THEME")
        .env_remove("COLORFGBG")
        .env_remove("NO_COLOR")
        .env("HOME", p.join("home"))
        .env("CODEFORGE_DATA_DIR", p.join("data"))
        .env("AUTOPILOT_LIVE_DIR", p.join("live"))
        .env("CODEFORGE_USAGE_AUTOCOLLECT", "0")
        .env("CODEFORGE_WIDTH", "200")
        .env("CODEFORGE_LOCALE", "zh-TW");
    for (k, v) in envs {
        c.env(k, v);
    }
    let stdin = serde_json::json!({
        "session_id": "7a0c1a52-25cb-47ca-821c-cec391832eed",
        "model": {"display_name": "Opus 5.5"},
        "cwd": "/tmp/專案/子目錄",
        "context_window": {"used_percentage": 85.0, "context_window_size": 1_000_000},
        "rate_limits": {
            "five_hour": {"used_percentage": 40.0, "resets_at": 4_102_444_800i64},
            "seven_day": {"used_percentage": 101.0, "resets_at": 4_102_444_800i64},
        },
    })
    .to_string();
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn assert_all_pass(out: &str, bg: (u8, u8, u8), case: &str) {
    let fg = foregrounds(out);
    assert!(fg.len() >= 5, "{case}: too few colours to judge: {out:?}");
    let bad: Vec<_> = fg.iter().filter(|c| contrast(**c, bg) < 3.0).collect();
    assert!(bad.is_empty(), "{case}: below 3:1 on {bg:?}: {bad:?}");
}

#[test]
fn light_terminal_via_colorfgbg_passes_on_white() {
    assert_all_pass(
        &run(&[("COLORFGBG", "0;15")]),
        (255, 255, 255),
        "COLORFGBG=0;15",
    );
}

#[test]
fn light_terminal_via_settings_switch_passes_on_white() {
    assert_all_pass(
        &run(&[("CODEFORGE_THEME", "light")]),
        (255, 255, 255),
        "CODEFORGE_THEME=light",
    );
}

#[test]
fn no_theme_env_draws_dark_and_passes_on_black() {
    let out = run(&[]);
    assert_all_pass(&out, (0, 0, 0), "no theme env");
    // reverse control at binary level: the dark line fails on white
    assert!(foregrounds(&out)
        .iter()
        .any(|c| contrast(*c, (255, 255, 255)) < 3.0));
}
