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

/// Releases ship prebuilt Linux x86_64 and aarch64 binaries only (operator decision, v0.0.9):
/// on any other OS there is nothing to download, so say so instead of failing on a missing asset.
fn no_prebuilt_hint(os: &str) -> Option<String> {
    if os == "linux" {
        return None;
    }
    Some(format!(
        "{os} 沒有預編譯 binary：release 只提供 Linux x86_64／aarch64（自 v0.0.9 起不再發 macOS）。\n\
         請改用原始碼更新：cargo install --git https://github.com/{REPO_OWNER}/{REPO_NAME} --locked --force"
    ))
}

/// `codeforge self-update` — see module docs.
pub fn run(opts: SelfUpdateOpts) -> Result<()> {
    if let Some(hint) = no_prebuilt_hint(std::env::consts::OS) {
        anyhow::bail!(hint);
    }
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
    "無法取得 release 或本平台的檔案 —— 確認 https://github.com/cookys/codeforge/releases \
     已發布版本（首發 v0.0.5）；預編譯檔只有 Linux x86_64／aarch64，其他平台請用 \
     cargo install --git https://github.com/cookys/codeforge --locked --force";

#[cfg(test)]
mod tests {
    use super::no_prebuilt_hint;

    #[test]
    fn only_linux_has_prebuilt_binaries() {
        assert!(no_prebuilt_hint("linux").is_none());
        for os in ["macos", "windows", "freebsd"] {
            let hint = no_prebuilt_hint(os).expect("non-linux gets a hint");
            assert!(hint.starts_with(os), "{hint}");
            assert!(hint.contains(
                "cargo install --git https://github.com/cookys/codeforge --locked --force"
            ));
        }
    }
}
