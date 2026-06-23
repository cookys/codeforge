//! `codeforge self-update` — replace the running binary in place from the latest
//! GitHub release (BACKLOG B20). Modern updater à la rustup/gh/starship: works
//! wherever the binary lives (`~/.local/bin`, `~/.cargo/bin`, …) because it
//! rewrites `current_exe()`, so it sidesteps the "which copy is on PATH" problem.
//!
//! Pairs with `install.sh` (first install on a bare machine). Both pull the
//! per-platform tarballs that `.github/workflows/release.yml` publishes:
//!   `codeforge-<version>-<target>.tar.gz`  containing  `codeforge-<version>-<target>/codeforge`
//! (hence `bin_path_in_archive` below points into that subdir).

use anyhow::{Context, Result};

pub struct SelfUpdateOpts {
    /// Only report whether a newer release exists; do not download/replace.
    pub check: bool,
    pub quiet: bool,
}

const REPO_OWNER: &str = "cookys";
const REPO_NAME: &str = "codeforge";
const BIN_NAME: &str = "codeforge";

fn say(quiet: bool, msg: &str) {
    if !quiet {
        println!("{msg}");
    }
}

/// `codeforge self-update` — see module docs.
pub fn run(opts: SelfUpdateOpts) -> Result<()> {
    let current = self_update::cargo_crate_version!();

    let updater = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .current_version(current)
        // release.yml tarball layout: codeforge-<ver>-<target>/codeforge
        .bin_path_in_archive("codeforge-{{ version }}-{{ target }}/{{ bin }}")
        .no_confirm(true)
        .show_download_progress(!opts.quiet)
        .build()
        .context("建立 self-update 設定失敗")?;

    if opts.check {
        let latest = updater.get_latest_release().context(NO_RELEASE_HINT)?;
        // A malformed remote tag (semver parse error) is intentionally treated as
        // "up to date" — never claim an update we can't verify is actually newer.
        if self_update::version::bump_is_greater(current, &latest.version).unwrap_or(false) {
            say(
                opts.quiet,
                &format!(
                    "有新版可用：{current} → {}（跑 `codeforge self-update` 更新）",
                    latest.version
                ),
            );
        } else {
            say(opts.quiet, &format!("已是最新版（{current}）"));
        }
        return Ok(());
    }

    say(opts.quiet, &format!("目前版本 {current}，檢查更新中 …"));
    let status = updater.update().context(NO_RELEASE_HINT)?;
    if status.updated() {
        say(
            opts.quiet,
            &format!("✓ 已更新到 {}（重開 shell 或重跑即生效）", status.version()),
        );
    } else {
        say(opts.quiet, &format!("已是最新版（{}）", status.version()));
    }
    Ok(())
}

const NO_RELEASE_HINT: &str =
    "無法取得 release —— 確認 https://github.com/cookys/codeforge/releases 已發布版本（首發 v0.0.5）";
