//! `codeforge bootstrap` — one-command per-machine setup (BACKLOG B14).
//!
//! Convergence of the multi-step "make a fresh machine codeforge-ready" flow into
//! a single idempotent command. Thin orchestrator over existing pieces:
//!   1. Claude Code wiring  → `install --all` (statusline + global hooks)
//!   2. fmt pin toolchain   → `scripts/fmt.sh --check` (self-installs pinned rustfmt; B19)
//!   3. Mnemos opt-in       → report-only status (never auto-creates the env file)
//!
//! NOT in scope: installing the codeforge binary itself (you must already have it
//! to run this), auto-creating `~/.config/mnemos.env` (opt-in is deliberate), or
//! driving remote machines (run this on each machine, or follow the BACKLOG runbook).

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::install::{self, InstallOpts};
use crate::mnemos::config::MnemosConfig;

pub struct BootstrapOpts {
    pub dry_run: bool,
    pub quiet: bool,
}

fn say(quiet: bool, msg: &str) {
    if !quiet {
        println!("{}", msg);
    }
}

/// Walk up from `start` looking for `scripts/fmt.sh` — the codeforge-clone marker.
/// Returns the script path if this machine is running inside a clone, else `None`
/// (e.g. a machine that only has the installed binary, where fmt pin is irrelevant).
fn find_fmt_script(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        let candidate = cur.join("scripts").join("fmt.sh");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// The Mnemos opt-in status line(s) — pure so it's unit-testable without env mutation.
fn mnemos_status_lines(opted_in: bool, env_path: &str) -> Vec<String> {
    if opted_in {
        vec!["   ✓ 已 opt-in（dream→ship 會 POST 到 Mnemos）".to_string()]
    } else {
        vec![
            "   – 未 opt-in（codeforge-only：dream 照常蒸餾、ship 乾淨 no-op）".to_string(),
            format!("     要啟用：建立 {env_path} 或設 MNEMOS_INGEST_URL"),
        ]
    }
}

/// `codeforge bootstrap` — see module docs.
///
/// Best-effort: each step is error-isolated and reported; a failure in one step
/// (e.g. a pre-existing foreign statusLine that codeforge won't clobber) does NOT
/// abort the independent steps. A final summary lists anything needing attention.
/// Always returns `Ok` — the report, not the exit code, is the actionable output.
pub fn run(opts: BootstrapOpts) -> Result<()> {
    say(opts.quiet, "🔨 codeforge bootstrap — 多機部署一鍵設定");
    if opts.dry_run {
        say(opts.quiet, "   (--dry-run：只預覽，不寫入)");
    }

    let mut warnings: Vec<String> = Vec::new();

    // Step 1: Claude Code wiring (statusline + global hooks) via install --all.
    say(
        opts.quiet,
        "\n── [1/3] Claude Code wiring (install --all) ──",
    );
    let install_res = install::run(InstallOpts {
        hooks: false,
        all: true,
        project_hooks: false,
        dry_run: opts.dry_run,
        force: false,
        yes: true,
        settings_path: None,
        quiet: opts.quiet,
    });
    if let Err(e) = install_res {
        // Most common benign case: a statusLine set by something else — codeforge
        // refuses to clobber without --force. Report + continue, don't abort.
        say(opts.quiet, &format!("   ⚠ 跳過：{e}"));
        say(
            opts.quiet,
            "     （若要讓 codeforge 接管現有 statusLine：codeforge install --all --force）",
        );
        warnings.push(format!("Claude Code wiring: {e}"));
    }

    // Step 2: pinned fmt toolchain — only meaningful inside a codeforge clone.
    say(opts.quiet, "\n── [2/3] fmt pin toolchain (B19) ──");
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match find_fmt_script(&cwd) {
        Some(script) if opts.dry_run => {
            say(
                opts.quiet,
                &format!(
                    "   would run: {} --check  (self-installs pinned toolchain)",
                    script.display()
                ),
            );
        }
        Some(script) => {
            say(
                opts.quiet,
                &format!("   running {} --check ...", script.display()),
            );
            match std::process::Command::new(&script).arg("--check").status() {
                Ok(status) if status.success() => {
                    say(opts.quiet, "   ✓ fmt pin toolchain ready, formatting clean")
                }
                Ok(_) => {
                    say(
                        opts.quiet,
                        "   ⚠ fmt --check 未通過 —— 跑 `./scripts/fmt.sh` 收乾後再 commit",
                    );
                    warnings.push("fmt: 格式未對齊（跑 ./scripts/fmt.sh）".to_string());
                }
                Err(e) => {
                    say(opts.quiet, &format!("   ⚠ 無法執行 fmt.sh：{e}"));
                    warnings.push(format!("fmt: {e}"));
                }
            }
        }
        None => {
            say(
                opts.quiet,
                "   – 非 codeforge clone（找不到 scripts/fmt.sh）→ fmt pin 不適用，跳過",
            );
        }
    }

    // Step 3: Mnemos opt-in status (report-only — never auto-create the env file).
    say(opts.quiet, "\n── [3/3] Mnemos opt-in ──");
    let env_path = MnemosConfig::env_file_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.config/mnemos.env".to_string());
    for line in mnemos_status_lines(MnemosConfig::opted_in(), &env_path) {
        say(opts.quiet, &line);
    }

    // Summary.
    say(opts.quiet, "\n── 完成 ──");
    if warnings.is_empty() {
        say(opts.quiet, "   ✓ 所有步驟就緒");
    } else {
        say(opts.quiet, &format!("   ⚠ {} 步需注意：", warnings.len()));
        for w in &warnings {
            say(opts.quiet, &format!("     - {w}"));
        }
    }
    say(
        opts.quiet,
        "   其他機器：在各自的 codeforge clone 跑 `git pull && codeforge bootstrap`",
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn find_fmt_script_detects_clone_from_nested_cwd() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(root.join("scripts").join("fmt.sh"), "#!/usr/bin/env bash\n").unwrap();
        let nested = root.join("src").join("cli");
        std::fs::create_dir_all(&nested).unwrap();

        let found = find_fmt_script(&nested).expect("should walk up to scripts/fmt.sh");
        assert_eq!(found, root.join("scripts").join("fmt.sh"));
    }

    #[test]
    fn find_fmt_script_returns_none_outside_clone() {
        let temp = TempDir::new().unwrap();
        // No scripts/fmt.sh anywhere up the tree under this temp root.
        let sub = temp.path().join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        // A directory with scripts/ but no fmt.sh must NOT match.
        std::fs::create_dir_all(sub.join("scripts")).unwrap();
        assert!(find_fmt_script(&sub).is_none());
    }

    #[test]
    fn mnemos_status_reports_opted_in() {
        let lines = mnemos_status_lines(true, "/home/u/.config/mnemos.env");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("已 opt-in"));
    }

    #[test]
    fn mnemos_status_hints_when_not_opted_in() {
        let lines = mnemos_status_lines(false, "/home/u/.config/mnemos.env");
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("/home/u/.config/mnemos.env"));
        assert!(lines[1].contains("MNEMOS_INGEST_URL"));
    }
}
