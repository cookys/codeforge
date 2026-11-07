use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, BufRead, Write as _};
use std::path::{Path, PathBuf};

/// Options for `codeforge install`.
pub struct InstallOpts {
    pub hooks: bool,
    pub all: bool,
    pub subagent_statusline: bool,
    pub project_hooks: bool,
    pub dry_run: bool,
    pub force: bool,
    /// `--replace-statusline` (B28): take over a statusLine / subagentStatusLine
    /// another tool owns without touching any hook (unlike `--force`).
    pub replace_statusline: bool,
    pub yes: bool,
    pub settings_path: Option<PathBuf>,
    pub quiet: bool,
    /// `--cleanup-period-days N`: write this retention even into a shared settings.json.
    pub cleanup_period_days: Option<u64>,
}

/// Options for `codeforge uninstall`.
pub struct UninstallOpts {
    pub statusline: bool,
    pub hooks: bool,
    pub settings_path: Option<PathBuf>,
    pub quiet: bool,
}

const MARKER_KEY: &str = "_installed_by";
const MARKER_PREFIX: &str = "codeforge@";
/// Marker for everything the global install writes (hooks, statusLine,
/// subagentStatusLine). An install-contract version, not the package version, so
/// re-running install after an upgrade leaves a shared settings.json byte-identical
/// (B35). `--project-hooks` keeps [`current_marker`].
const GLOBAL_MARKER: &str = "codeforge@1";

/// Claude Code env-var placeholder. Claude Code expands `${CLAUDE_PROJECT_DIR}`
/// to the absolute project root before spawning hook processes
/// (https://code.claude.com/docs/en/hooks.md). Using this in committed
/// settings.json makes the file portable across every clone.
const PROJECT_DIR_ENV_PLACEHOLDER: &str = "${CLAUDE_PROJECT_DIR}";

/// `codeforge install` — wire Claude Code integration.
///
/// Default (no flags): patches `~/.claude/settings.json` with a
/// `statusLine` block pointing to the binary's absolute path.
///
/// Flags:
/// - `--hooks`: install global hooks that run in every project — `emit-session`,
///   `session-digest`, the SessionStart recall injectors (local `memory context
///   --hook` + central `mnemos-cli context --hook`, the latter = P1.2 fleet recall
///   downlink), plus the `codeforge dream → codeforge ship --no-hook`
///   memory-pipeline SessionEnd chain.
/// - `--all`:   statusLine + global hooks.
/// - `--project-hooks`: wire the 2 codeforge-clone-only DEV scripts
///   (check-improvements, check-dev-flow) into `$CWD/.claude/settings.json`.
///   Requires CWD to be a codeforge clone.
/// - `--dry-run`: print resulting JSON + extraction plan, no writes.
/// - `--force`:  clear all hook entries (including non-codeforge) before
///   writing. Requires `--yes` (or interactive confirmation).
/// - `--yes`:    skip confirmation prompts.
/// - `--settings-path P`: override target settings.json.
/// - `--quiet`:  no stdout (exit code reflects result).
pub fn run(opts: InstallOpts) -> Result<()> {
    // --project-hooks is mutually exclusive with --hooks/--all (different scope target)
    if opts.project_hooks && (opts.hooks || opts.all) {
        bail!("--project-hooks 不能跟 --hooks / --all 同時使用（前者寫 $CWD/.claude，後者寫 ~/.claude）");
    }

    let install_statusline = !opts.hooks && !opts.project_hooks || opts.all;
    let install_hooks = opts.hooks || opts.all;
    let install_project_hooks = opts.project_hooks;

    if opts.force && !opts.yes {
        say(
            opts.quiet,
            "⚠ --force 會清除 settings.json 內所有 hook entries（含非 codeforge）。",
        );
        if !prompt_confirm("確定要繼續？輸入 'yes' 確認: ")? {
            bail!("使用者取消 --force 操作");
        }
    }

    if install_project_hooks {
        run_install_project_hooks(&opts)?;
        return Ok(());
    }

    let exe = std::env::current_exe().context("讀取 codeforge binary 路徑失敗")?;
    exe.to_str()
        .ok_or_else(|| anyhow!("binary 路徑含非 UTF-8 字元：{}", exe.display()))?;

    let settings_path = opts
        .settings_path
        .clone()
        .map(Ok)
        .unwrap_or_else(default_global_settings_path)?;
    say(
        opts.quiet,
        &format!("  目標 settings.json: {}", settings_path.display()),
    );
    check_settings_path(&settings_path)?;
    let shared = shared_settings(&settings_path);
    for line in shared_warning_lines(&settings_path, &shared) {
        say(opts.quiet, &line);
    }

    let mut settings = load_settings(&settings_path)?;

    if install_statusline {
        let path_hit = codeforge_on_path();
        let on_path = same_binary(path_hit.as_deref(), &exe);
        if shared.is_shared() && !on_path {
            say(
                opts.quiet,
                &format!(
                    "⚠ PATH 上的 codeforge 不是目前這個 binary（{}）；statusLine 會執行 PATH 上的那一個",
                    exe.display()
                ),
            );
        }
        let cmd = statusline_command(&exe, on_path, shared.is_shared(), "statusline");
        let action = patch_statusline(&mut settings, &cmd, opts.force || opts.replace_statusline)?;
        say(opts.quiet, &format!("✓ statusLine {}", action));
        say(opts.quiet, &format!("  command: {}", cmd));

        if opts.subagent_statusline {
            let sub_cmd =
                statusline_command(&exe, on_path, shared.is_shared(), "subagent-statusline");
            let sub_action = patch_subagent_statusline(
                &mut settings,
                &sub_cmd,
                opts.force || opts.replace_statusline,
            )?;
            say(opts.quiet, &format!("✓ subagentStatusLine {}", sub_action));
            say(opts.quiet, &format!("  command: {}", sub_cmd));
        }
    }

    if install_hooks {
        // The hook commands are `codeforge hook <name>`; the scripts live in a
        // private per-version cache. Warm it now (not in dry-run) so an unwritable
        // data dir fails here rather than inside a hook later.
        let cache = if opts.dry_run {
            super::hook::cache_dir()?
        } else {
            super::hook::materialize_all()?
        };
        let action = patch_hooks(&mut settings, None, opts.force)?;
        say(opts.quiet, &format!("✓ hooks {}", action));
        say(
            opts.quiet,
            &format!(
                "  scripts: codeforge hook <name>（快取 {}）",
                cache.display()
            ),
        );
    }

    // The memory pipeline (dream/ship) can only distill session transcripts that
    // still exist. Claude Code's default cleanupPeriodDays is 30 — older sessions
    // are deleted before they're ever digested. Bump retention so raw material
    // survives — but a shared settings.json would carry it to every machine, so
    // there it takes an explicit --cleanup-period-days. An explicit flag applies
    // whatever else is being installed (a bare `install` included).
    if install_hooks || opts.cleanup_period_days.is_some() {
        for line in apply_cleanup_period(
            &mut settings,
            opts.cleanup_period_days,
            shared.is_shared(),
            opts.force,
        ) {
            say(opts.quiet, &line);
        }
    }

    if opts.dry_run {
        say(opts.quiet, "");
        say(opts.quiet, "─── dry-run: 將寫入下列 settings.json 內容 ───");
        if !opts.quiet {
            println!("{}", serde_json::to_string_pretty(&settings)?);
        }
        say(opts.quiet, "─── 結束（無檔案被修改） ───");
        return Ok(());
    }

    write_settings_atomic(&settings_path, &settings)?;
    say(opts.quiet, "");
    say(
        opts.quiet,
        "生效方式：下個 user message 或 /clear 之後 Claude Code 會 pick up 新設定。",
    );
    Ok(())
}

/// `codeforge uninstall` — remove codeforge-tagged entries.
pub fn run_uninstall(opts: UninstallOpts) -> Result<()> {
    // If neither flag set, default to "everything"
    let neither = !opts.statusline && !opts.hooks;
    let remove_statusline = neither || opts.statusline;
    let remove_hooks = neither || opts.hooks;

    let settings_path = opts
        .settings_path
        .clone()
        .map(Ok)
        .unwrap_or_else(default_global_settings_path)?;

    check_settings_path(&settings_path)?;
    if !settings_path.exists() {
        say(
            opts.quiet,
            &format!(
                "settings.json 不存在：{}（沒事可做）",
                settings_path.display()
            ),
        );
        return Ok(());
    }

    say(
        opts.quiet,
        &format!("  目標 settings.json: {}", settings_path.display()),
    );
    let mut settings = load_settings(&settings_path)?;

    if remove_statusline {
        let action = unpatch_statusline(&mut settings)?;
        say(opts.quiet, &format!("✓ statusLine {}", action));
    }

    if remove_hooks {
        let action = unpatch_hooks(&mut settings)?;
        say(opts.quiet, &format!("✓ hooks {}", action));

        // Also remove the script caches (every version)
        let base = super::hook::hooks_root()?;
        if base.exists() {
            fs::remove_dir_all(&base).with_context(|| format!("移除 {} 失敗", base.display()))?;
            say(opts.quiet, &format!("✓ removed {}", base.display()));
        }
    }

    write_settings_atomic(&settings_path, &settings)?;
    Ok(())
}

fn run_install_project_hooks(opts: &InstallOpts) -> Result<()> {
    let cwd = std::env::current_dir().context("讀取 CWD 失敗")?;
    let cwd_str = cwd
        .to_str()
        .ok_or_else(|| anyhow!("CWD 含非 UTF-8 字元：{}", cwd.display()))?
        .to_string();

    ensure_in_codeforge_repo(&cwd)?;

    let settings_path = opts
        .settings_path
        .clone()
        .unwrap_or_else(|| cwd.join(".claude").join("settings.json"));
    say(
        opts.quiet,
        &format!("  目標 settings.json: {}", settings_path.display()),
    );

    // Existence check uses the real CWD path (resolves $CLAUDE_PROJECT_DIR for us locally).
    let scripts_dir_real = cwd.join(".claude").join("scripts");
    for name in PROJECT_HOOK_SCRIPT_NAMES {
        let p = scripts_dir_real.join(name);
        if !p.exists() {
            bail!(
                "找不到必要 script：{}（執行位置不像 codeforge clone）",
                p.display()
            );
        }
    }

    // Written into command strings — uses Claude Code's documented env var so the
    // committed settings.json is portable across every clone. Claude Code expands
    // `${CLAUDE_PROJECT_DIR}` before spawning the hook process.
    let scripts_dir_template = Path::new(PROJECT_DIR_ENV_PLACEHOLDER)
        .join(".claude")
        .join("scripts");

    let mut settings = load_settings(&settings_path)?;
    let action = patch_hooks(&mut settings, Some(&scripts_dir_template), opts.force)?;
    say(opts.quiet, &format!("✓ project hooks {}", action));
    say(
        opts.quiet,
        &format!("  scripts: {}", scripts_dir_template.display()),
    );
    say(opts.quiet, &format!("  clone root: {}", cwd_str));

    if opts.dry_run {
        say(opts.quiet, "");
        say(opts.quiet, "─── dry-run: 將寫入下列 settings.json 內容 ───");
        if !opts.quiet {
            println!("{}", serde_json::to_string_pretty(&settings)?);
        }
        say(opts.quiet, "─── 結束（無檔案被修改） ───");
        return Ok(());
    }

    write_settings_atomic(&settings_path, &settings)?;
    Ok(())
}

fn ensure_in_codeforge_repo(cwd: &Path) -> Result<()> {
    let cargo_toml = cwd.join("Cargo.toml");
    if !cargo_toml.exists() {
        bail!(
            "--project-hooks 必須在 codeforge clone root 執行（缺 Cargo.toml）：{}",
            cwd.display()
        );
    }
    let content = fs::read_to_string(&cargo_toml)
        .with_context(|| format!("讀取 {} 失敗", cargo_toml.display()))?;
    // Look for `name = "codeforge"` in the [package] section (simple substring is OK)
    if !content.contains("name = \"codeforge\"") {
        bail!(
            "Cargo.toml 的 package name 不是 codeforge：{}",
            cargo_toml.display()
        );
    }
    Ok(())
}

// ─── settings.json IO ─────────────────────────────────────────────────────

fn load_settings(path: &Path) -> Result<Value> {
    if path.exists() {
        let raw =
            fs::read_to_string(path).with_context(|| format!("讀取 {} 失敗", path.display()))?;
        if raw.trim().is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_str(&raw)
            .with_context(|| format!("解析 {} 失敗（不是合法 JSON）", path.display()))
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("建立目錄 {} 失敗", parent.display()))?;
        }
        Ok(json!({}))
    }
}

/// Refuse a settings path that is a dangling symlink. `Path::exists()` is false
/// for one, so without this check `load_settings` would read it as an empty file
/// (dry-run prints a fake success) and `run_uninstall` would report "nothing to do".
fn check_settings_path(path: &Path) -> Result<()> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(()); // missing entirely: load_settings creates it
    };
    if meta.file_type().is_symlink() && fs::canonicalize(path).is_err() {
        let dest = fs::read_link(path)
            .map(|d| d.display().to_string())
            .unwrap_or_else(|_| "?".to_string());
        bail!(
            "settings.json 是指向不存在檔案的 symlink：{} → {}（請先修好連結或它指向的檔案）",
            path.display(),
            dest
        );
    }
    Ok(())
}

/// The file a write to `path` must land in: the final target when `path` is a
/// symlink (relative links and chains resolved), else `path` itself. Renaming
/// over the link would replace the link with a regular file and detach it from
/// e.g. a dotfiles repo.
fn resolve_write_target(path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            check_settings_path(path)?;
            fs::canonicalize(path).with_context(|| format!("解析 symlink {} 失敗", path.display()))
        }
        _ => Ok(path.to_path_buf()),
    }
}

fn write_settings_atomic(path: &Path, settings: &Value) -> Result<()> {
    let target = resolve_write_target(path)?;
    let file_name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "settings.json".to_string());
    let tmp = target.with_file_name(format!("{file_name}.tmp.{}", std::process::id()));
    let pretty = serde_json::to_string_pretty(settings)? + "\n";

    // A change keeps the previous text in `<path>.bak` (operator card d-f6e451d2), a plain file beside the
    // path install was given: never inside a symlink's target dir, which may be a git-tracked dotfiles repo.
    // A no-op re-write leaves an earlier backup alone.
    if let Ok(before) = fs::read_to_string(&target) {
        if before != pretty {
            let bak_name = format!(
                "{}.bak",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "settings.json".to_string())
            );
            let bak = path.with_file_name(bak_name);
            fs::write(&bak, &before).with_context(|| format!("備份 {} 失敗", bak.display()))?;
        }
    }

    let result = (|| -> Result<()> {
        let mut f = fs::File::create(&tmp)
            .with_context(|| format!("寫入 {} 失敗（目錄不可寫？）", target.display()))?;
        f.write_all(pretty.as_bytes())
            .with_context(|| format!("寫入 {} 失敗", tmp.display()))?;
        f.sync_all()
            .with_context(|| format!("寫入 {} 失敗", tmp.display()))?;
        drop(f);
        if let Ok(meta) = fs::metadata(&target) {
            let _ = fs::set_permissions(&tmp, meta.permissions());
        }
        fs::rename(&tmp, &target).with_context(|| format!("rename 到 {} 失敗", target.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Where a settings.json is shared with other machines: a symlink (typically into a
/// dotfiles repo) and/or a file tracked by git. Both mean whatever install writes
/// spreads beyond this machine.
#[derive(Debug, Default, PartialEq)]
struct SharedSettings {
    symlink_target: Option<PathBuf>,
    git_repo: Option<PathBuf>,
}

impl SharedSettings {
    fn is_shared(&self) -> bool {
        self.symlink_target.is_some() || self.git_repo.is_some()
    }
}

fn shared_settings(path: &Path) -> SharedSettings {
    let is_link = fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    let canonical = fs::canonicalize(path).ok();
    let symlink_target = if is_link { canonical.clone() } else { None };
    let git_repo = canonical.as_deref().and_then(git_tracking_repo);
    SharedSettings {
        symlink_target,
        git_repo,
    }
}

/// The repo root when `file` is tracked by git; `None` when it isn't, git is
/// missing, or anything fails. git's own output never reaches the user (it is
/// English, and `ls-files --error-unmatch` prints a `fatal:` for every untracked
/// file), and inherited `GIT_*` variables must not redirect `-C` to another repo.
fn git_tracking_repo(file: &Path) -> Option<PathBuf> {
    use std::process::{Command, Stdio};
    let parent = file.parent()?;
    let name = file.file_name()?;
    let git = |args: &[&std::ffi::OsStr]| {
        let mut c = Command::new("git");
        c.arg("-C")
            .arg(parent)
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        c
    };
    let tracked = git(&[
        "ls-files".as_ref(),
        "--error-unmatch".as_ref(),
        "--".as_ref(),
        name,
    ])
    .stdout(Stdio::null())
    .status()
    .ok()?
    .success();
    if !tracked {
        return None;
    }
    let out = git(&["rev-parse".as_ref(), "--show-toplevel".as_ref()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let root = String::from_utf8(out.stdout).ok()?;
    let root = root.trim();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

fn shared_warning_lines(path: &Path, shared: &SharedSettings) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(t) = &shared.symlink_target {
        lines.push(format!("⚠ settings.json 是 symlink → {}", t.display()));
    }
    if let Some(repo) = &shared.git_repo {
        let shown = shared.symlink_target.as_deref().unwrap_or(path);
        lines.push(format!(
            "⚠ {} 在 git 版控中（repo：{}）：寫入的內容會隨 commit 擴散到其他機器",
            shown.display(),
            repo.display()
        ));
    }
    if shared.is_shared() {
        lines.push("  寫入會直接改 target 檔案；hook 指令不含本機路徑，可以安全共用".to_string());
    }
    lines
}

fn default_global_settings_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CODEFORGE_CLAUDE_SETTINGS") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().context("找不到使用者 home 目錄")?;
    Ok(home.join(".claude").join("settings.json"))
}

// ─── statusLine block ─────────────────────────────────────────────────────

/// The first `codeforge` executable on `$PATH`, if any.
fn codeforge_on_path() -> Option<PathBuf> {
    let exe_name = format!("codeforge{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&exe_name))
        .find(|p| p.is_file())
}

fn same_binary(a: Option<&Path>, b: &Path) -> bool {
    match (
        a.and_then(|a| fs::canonicalize(a).ok()),
        fs::canonicalize(b).ok(),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// The statusLine / subagentStatusLine command. A shared settings.json always gets
/// the PATH form so every machine writes the same bytes; a local file gets the PATH
/// form when PATH resolves to this very binary, else the absolute path (it does not
/// spread, and it keeps working when PATH is missing ~/.cargo/bin).
fn statusline_command(exe: &Path, on_path: bool, shared: bool, sub: &str) -> String {
    if shared || on_path {
        format!("codeforge {sub}")
    } else {
        format!("{} {sub}", crate::platform::shell_path(exe))
    }
}

fn current_marker() -> String {
    format!("{}{}", MARKER_PREFIX, env!("CARGO_PKG_VERSION"))
}

fn statusline_is_codeforge(v: &Value) -> bool {
    v.get(MARKER_KEY)
        .and_then(|x| x.as_str())
        .is_some_and(|s| s.starts_with(MARKER_PREFIX))
}

/// Seconds between idle re-runs of the statusLine command. Claude Code re-runs it only on events
/// (a new assistant message, /compact, a mode change…) unless `refreshInterval` is set
/// (code.claude.com/docs/en/statusline, "When the status line updates"). Without it an idle
/// session keeps the first render: forge-stats' heartbeat (`band_seen`, B34) can change and the
/// statusline never compacts or expands. 10 matches the heartbeat's 10 s freshness window.
/// Operator card d-f6e451d2.
const STATUSLINE_REFRESH_S: u64 = 10;

fn patch_statusline(settings: &mut Value, cmd: &str, force: bool) -> Result<&'static str> {
    require_object_root(settings)?;
    let obj = settings.as_object_mut().expect("checked");
    let prior = obj.get("statusLine").cloned();
    if let Some(p) = &prior {
        if !statusline_is_codeforge(p) && !force {
            bail!("settings.json.statusLine 已被其他程式設定（不是 codeforge）；使用 --replace-statusline 覆蓋（只換這個欄位，hooks 不動）");
        }
    }
    // a refreshInterval already there (ours, or one the user set) is kept as it is
    let refresh = prior
        .as_ref()
        .and_then(|p| p.get("refreshInterval"))
        .cloned()
        .unwrap_or_else(|| json!(STATUSLINE_REFRESH_S));
    let new_block = json!({
        "type": "command",
        "command": cmd,
        "refreshInterval": refresh,
        MARKER_KEY: GLOBAL_MARKER,
    });
    obj.insert("statusLine".to_string(), new_block.clone());
    Ok(match prior {
        None => "新增",
        Some(p) if p == new_block => "已是最新（無變動）",
        Some(_) => "更新",
    })
}

/// Idempotently add `settings.json.subagentStatusLine` pointing at
/// `<binary> subagent-statusline`. Opt-in only (`--subagent-statusline`),
/// and only called when `statusLine.command` already ends with
/// `codeforge statusline` (checked by the caller, which just wrote it).
/// Mirrors `patch_statusline`'s merge/force semantics.
fn patch_subagent_statusline(settings: &mut Value, cmd: &str, force: bool) -> Result<&'static str> {
    require_object_root(settings)?;
    let new_block = json!({
        "type": "command",
        "command": cmd,
        MARKER_KEY: GLOBAL_MARKER,
    });
    let obj = settings.as_object_mut().expect("checked");
    let prior = obj.get("subagentStatusLine").cloned();
    if let Some(p) = &prior {
        if !statusline_is_codeforge(p) && !force {
            bail!("settings.json.subagentStatusLine 已被其他程式設定（不是 codeforge）；使用 --replace-statusline 覆蓋（只換這個欄位，hooks 不動）");
        }
    }
    obj.insert("subagentStatusLine".to_string(), new_block.clone());
    Ok(match prior {
        None => "新增",
        Some(p) if p == new_block => "已是最新（無變動）",
        Some(_) => "更新",
    })
}

fn unpatch_statusline(settings: &mut Value) -> Result<&'static str> {
    require_object_root(settings)?;
    let obj = settings.as_object_mut().expect("checked");
    match obj.get("statusLine").cloned() {
        None => Ok("不存在（無變動）"),
        Some(v) if statusline_is_codeforge(&v) => {
            obj.remove("statusLine");
            Ok("已移除")
        }
        Some(_) => Ok("保留（非 codeforge 寫入）"),
    }
}

// ─── hooks block ──────────────────────────────────────────────────────────

// The global hook scripts are embedded in `hook.rs` and run via `codeforge hook`.

/// Codeforge-clone-only DEV scripts (used by `--project-hooks`). These already
/// live in the codeforge clone at `.claude/scripts/`, so we only need the
/// names — no embedding required.
///
/// Note: emit-session.js / session-digest.js (and the dream→ship memory
/// pipeline) are product-wide and live in the global `--hooks` install path
/// (~/.claude/settings.json) — installing them into project settings too would
/// cause dual-fire. See commit 2648b34.
const PROJECT_HOOK_SCRIPT_NAMES: &[&str] = &["check-improvements.js", "check-dev-flow.js"];

/// `project_scripts_dir`: `Some(dir)` for `--project-hooks` (node scripts under the
/// codeforge clone), `None` for the global install (`codeforge hook <name>`).
fn patch_hooks(
    settings: &mut Value,
    project_scripts_dir: Option<&Path>,
    force: bool,
) -> Result<&'static str> {
    require_object_root(settings)?;

    // Build the entries we want to insert. The two install modes carry
    // strictly disjoint hooks to avoid dual-fire when both are installed
    // (commit 2648b34):
    //
    //   --hooks         (global ~/.claude/settings.json):
    //       product-wide hooks that should run in EVERY project —
    //       emit-session, session-digest; the SessionStart local-recall injector
    //       (`codeforge memory context --hook`); and the memory pipeline
    //       `codeforge dream --quiet` → `codeforge ship --no-hook` →
    //       `codeforge usage collect --quiet` (G36) SessionEnd chain.
    //       dream distills L0→L1 per-project (hook CWD = project root); ship then
    //       forwards the day's L1 to Mnemos. ship --no-hook self-gates on Mnemos
    //       opt-in (see MnemosConfig::opted_in), so codeforge-only users keep
    //       distilling with dream while ship is a clean no-op for them. The
    //       SessionStart injector is the no-mnemos READ path: it surfaces a lean
    //       ranked L1 index as additionalContext (no-op when no active L1).
    //
    //   --project-hooks (project .claude/settings.json):
    //       codeforge-clone-only DEV hooks — check-improvements (SessionStart),
    //       check-dev-flow (PreToolUse). dream/ship moved to the global path so
    //       they run across all projects, not just the codeforge clone.
    let entries: Vec<(&str, Option<&str>, Vec<Value>)> = if let Some(scripts_dir) =
        project_scripts_dir
    {
        let marker = current_marker();
        let check_improvements = scripts_dir.join("check-improvements.js");
        let check_dev_flow = scripts_dir.join("check-dev-flow.js");
        vec![
            (
                "SessionStart",
                None,
                vec![hook_entry(
                    &format!("node {}", crate::platform::shell_path(&check_improvements)),
                    10000,
                    &marker,
                )],
            ),
            (
                "PreToolUse",
                Some("Edit|Write|Bash"),
                vec![hook_entry(
                    &format!("node {}", crate::platform::shell_path(&check_dev_flow)),
                    5000,
                    &marker,
                )],
            ),
        ]
    } else {
        let marker = GLOBAL_MARKER;
        vec![
            (
                "SessionStart",
                None,
                vec![
                    hook_entry("codeforge hook emit-session session_start", 3000, marker),
                    // Local recall (no-mnemos READ path): inject a lean ranked L1
                    // index as additionalContext. No-op when the project has no
                    // active L1. Symmetric to the mnemos-cli context central path.
                    hook_entry(
                        "codeforge memory context --hook 2>/dev/null || true",
                        10000,
                        marker,
                    ),
                    // Central recall (fleet recall DOWNLINK, P1.2): pull cross-source
                    // atoms from the Mnemos brain and inject them at SessionStart —
                    // the READ half that makes tunnel-connected fleet boxes bidirectional
                    // instead of WRITE-only. `--hook` self-gates on Mnemos opt-in
                    // (clean no-op for codeforge-only machines) and injects nothing on
                    // empty/unreachable, so this line is safe to ship to every machine.
                    hook_entry(
                        "codeforge mnemos-cli context --hook --max 5 --with-themes --max-sensitivity work 2>/dev/null || true",
                        10000,
                        marker,
                    ),
                ],
            ),
            (
                "SessionEnd",
                None,
                vec![
                    hook_entry("codeforge hook emit-session session_end", 3000, marker),
                    hook_entry("codeforge hook session-digest", 30000, marker),
                    // Memory pipeline: dream distills L0→L1, ship forwards to Mnemos.
                    // Order matters — ship reads the L1 that dream just produced.
                    hook_entry(
                        "codeforge dream --quiet 2>/dev/null || true",
                        30000,
                        marker,
                    ),
                    hook_entry(
                        "codeforge ship --no-hook 2>/dev/null || true",
                        30000,
                        marker,
                    ),
                    // G36: incremental usage collect (numbers only) + summary.json.
                    // A first collect over a large history can outlive the hook
                    // timeout; each file commits on its own, so the next session
                    // continues where this one stopped.
                    hook_entry(
                        "codeforge usage collect --quiet 2>/dev/null || true",
                        30000,
                        marker,
                    ),
                ],
            ),
            (
                "PreCompact",
                None,
                vec![hook_entry("codeforge hook session-digest", 30000, marker)],
            ),
        ]
    };

    let obj = settings.as_object_mut().expect("checked");
    let hooks_root = obj.entry("hooks".to_string()).or_insert_with(|| json!({}));
    let hooks_root = hooks_root
        .as_object_mut()
        .ok_or_else(|| anyhow!("settings.hooks 不是 object"))?;

    if force {
        // Clear everything under hooks/ before re-installing
        hooks_root.clear();
    }

    // Sweep ALL existing codeforge-owned groups across every hook_type FIRST,
    // then add our current entries. Sweeping every type (not just the ones we're
    // about to write) is what lets an entry relocate between hook_types without
    // orphaning its old group — e.g. dream/ship moving from the project SessionEnd
    // to the global SessionEnd. The previous per-type retain left a stale group
    // behind in any type we no longer model (the known --project-hooks
    // drop/duplicate bug for the unmarked dream entry).
    let mut had_prior = false;
    let existing_types: Vec<String> = hooks_root.keys().cloned().collect();
    for ty in &existing_types {
        if let Some(arr) = hooks_root.get_mut(ty).and_then(|v| v.as_array_mut()) {
            let before = arr.len();
            arr.retain(|group| !group_is_codeforge(group));
            if arr.len() != before {
                had_prior = true;
            }
        }
    }

    for (hook_type, matcher, our_entries) in entries.iter() {
        let arr = hooks_root
            .entry(hook_type.to_string())
            .or_insert_with(|| json!([]));
        let arr = arr
            .as_array_mut()
            .ok_or_else(|| anyhow!("settings.hooks.{} 不是 array", hook_type))?;
        arr.push(json!({
            "matcher": matcher.unwrap_or(""),
            "hooks": our_entries,
        }));
    }

    // Collapse any arrays the sweep emptied (a hook_type we no longer write to,
    // e.g. project SessionEnd after dream relocated to global).
    hooks_root.retain(|_, v| match v.as_array() {
        Some(a) => !a.is_empty(),
        None => true,
    });

    // We always (re)write the current entry set, so there's no "no-op" outcome to
    // report without a full pre/post diff. `had_prior` distinguishes a first-time
    // install from a re-install that swept a prior codeforge group.
    Ok(match (force, had_prior) {
        (true, _) => "重置（--force）",
        (false, false) => "新增",
        (false, true) => "更新",
    })
}

fn unpatch_hooks(settings: &mut Value) -> Result<&'static str> {
    require_object_root(settings)?;
    let obj = settings.as_object_mut().expect("checked");
    let Some(hooks_root) = obj.get_mut("hooks") else {
        return Ok("不存在（無變動）");
    };
    let Some(hooks_obj) = hooks_root.as_object_mut() else {
        return Ok("hooks 不是 object，跳過");
    };
    let mut removed = 0usize;
    let type_names: Vec<String> = hooks_obj.keys().cloned().collect();
    for ty in &type_names {
        if let Some(arr) = hooks_obj.get_mut(ty).and_then(|v| v.as_array_mut()) {
            let before = arr.len();
            arr.retain(|group| !group_is_codeforge(group));
            removed += before - arr.len();
        }
    }
    // Collapse empty arrays
    hooks_obj.retain(|_, v| match v.as_array() {
        Some(a) => !a.is_empty(),
        None => true,
    });
    // Collapse empty hooks object
    if hooks_obj.is_empty() {
        obj.remove("hooks");
    }
    Ok(if removed > 0 {
        "已移除"
    } else {
        "無 codeforge entries（無變動）"
    })
}

/// Transcript retention (days) the global install writes into settings.json.
/// Far above Claude Code's 30-day default so the dream/ship memory pipeline has
/// time to digest sessions before they're cleaned up. ~10 years ≈ "keep".
const DEFAULT_CLEANUP_PERIOD_DAYS: u64 = 3650;

/// The cleanupPeriodDays policy (B35), as the lines install prints:
/// - `--cleanup-period-days N`: write N (an existing value only with `--force`).
/// - no flag, local file: write the default 3650 when unset (unchanged behaviour).
/// - no flag, shared file: never add, change or remove it, `--force` included.
fn apply_cleanup_period(
    settings: &mut Value,
    flag: Option<u64>,
    shared: bool,
    force: bool,
) -> Vec<String> {
    if flag.is_none() && shared {
        let existing = settings.get("cleanupPeriodDays").filter(|v| !v.is_null());
        return vec![match existing {
            Some(v) => format!(
                "ℹ 共用檔裡已有 cleanupPeriodDays = {v}，保留不動；它會套用到每台機器"
            ),
            None => format!(
                "ℹ cleanupPeriodDays 未寫入（共用檔會套用到每台機器）；要保留 transcript 給 dream/ship，可加 --cleanup-period-days {DEFAULT_CLEANUP_PERIOD_DAYS}"
            ),
        }];
    }
    let days = flag.unwrap_or(DEFAULT_CLEANUP_PERIOD_DAYS);
    let action = patch_cleanup_period(settings, days, force);
    let mut lines = vec![format!("✓ cleanupPeriodDays {action}")];
    if action != "已設定（保留現值）" {
        lines.push(format!(
            "  ↳ 保留 session transcript {days} 天（供 dream/ship 萃取）；~/.claude/projects/ 會隨之長期累積"
        ));
    }
    lines
}

/// Set `cleanupPeriodDays` to `days` unless the user already chose a value (then
/// leave it, unless `force`). Returns a human action string.
fn patch_cleanup_period(settings: &mut Value, days: u64, force: bool) -> &'static str {
    let Some(obj) = settings.as_object_mut() else {
        return "跳過（settings 非 object）";
    };
    let present = obj
        .get("cleanupPeriodDays")
        .map(|v| !v.is_null())
        .unwrap_or(false);
    if present && !force {
        return "已設定（保留現值）";
    }
    obj.insert("cleanupPeriodDays".to_string(), json!(days));
    if present {
        "重置（--force）"
    } else {
        "新增"
    }
}

fn hook_entry(command: &str, timeout_ms: u64, marker: &str) -> Value {
    json!({
        "type": "command",
        "command": command,
        "timeout": timeout_ms,
        MARKER_KEY: marker,
    })
}

/// A "hook group" is codeforge-owned when every entry in its `hooks`
/// array carries our marker — OR is a known legacy codeforge-owned entry
/// from before the marker was introduced (see [`hook_is_codeforge`]).
fn group_is_codeforge(group: &Value) -> bool {
    let Some(inner) = group.get("hooks").and_then(|h| h.as_array()) else {
        return false;
    };
    if inner.is_empty() {
        return false;
    }
    inner.iter().all(hook_is_codeforge)
}

/// A single hook entry is recognized as codeforge-owned if either:
/// - It carries our `_installed_by: codeforge@...` marker (V2.2+ install path), or
/// - Its `command` matches a known legacy pattern that codeforge shipped
///   before the marker existed (currently: `codeforge dream`, which was
///   manually placed in project settings.json since Phase 1 / commit b1634ad).
///
/// Migration intent: a fresh `codeforge install --project-hooks` on a repo
/// with the pre-2.2 un-marker dream entry should REPLACE it (not duplicate).
fn hook_is_codeforge(h: &Value) -> bool {
    if h.get(MARKER_KEY)
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.starts_with(MARKER_PREFIX))
    {
        return true;
    }
    h.get("command")
        .and_then(|c| c.as_str())
        .is_some_and(is_legacy_codeforge_command)
}

/// Known codeforge-owned hook commands shipped before the marker was added.
/// Keep this list tight — broadening it risks clobbering user hooks.
///
/// Two families of pre-marker entries exist in the field:
///   1. the inline `codeforge dream` command (hand-placed since Phase 1), and
///   2. node hook-script commands written by pre-marker installs — recognized by
///      the codeforge-owned scripts path (`/codeforge/hooks/` for global,
///      `/.claude/scripts/` for project) plus a known script basename.
///
/// Recognizing (2) lets a re-install SWEEP the old versioned copy (e.g. the
/// `hooks/0.0.3/…` entries) instead of duplicating it alongside the new version
/// — the dual-fire that bit the dream→ship relocation upgrade.
///
/// TODO: remove this once all field installs have cycled through a marker-aware
/// install. Safe to delete when no production settings.json carries an un-marker
/// codeforge entry.
fn is_legacy_codeforge_command(cmd: &str) -> bool {
    // Pre-marker inline subcommands codeforge shipped in the SessionEnd /
    // SessionStart chains: `dream`, `ship`, `memory context`. `ship` + `memory
    // context` were chained in after `dream` but before every field install cycled
    // through the marker. While they were unrecognized, a pre-marker group
    // containing one of them failed `group_is_codeforge`'s ALL-hooks-ours gate, so
    // the WHOLE group escaped the sweep and the new marked group stacked beside it
    // → dual-fire. Recognizing them lets a re-install reclaim the full pre-marker
    // group. Match on a token boundary (exact, or subcommand + space) so a
    // prefix collision like `codeforge shipment` / `memory contextualize` (or any
    // future codeforge subcommand) is NOT mistaken for one of these.
    let is_premarker_sub = |sub: &str| {
        let exact = format!("codeforge {sub}");
        cmd == exact || cmd.starts_with(&format!("{exact} "))
    };
    if is_premarker_sub("dream")
        || is_premarker_sub("ship")
        || is_premarker_sub("memory context")
        // Central recall (P1.2 fleet downlink) was hand-added to some SessionStart
        // groups before it shipped in `install`; recognize it so a re-install sweeps
        // the un-marked entry instead of stacking a second (dual context injection).
        || is_premarker_sub("mnemos-cli context")
    {
        return true;
    }
    let on_codeforge_path = cmd.contains("/codeforge/hooks/") || cmd.contains("/.claude/scripts/");
    on_codeforge_path
        && [
            "emit-session.js",
            "session-digest.js",
            "check-improvements.js",
            "check-dev-flow.js",
        ]
        .iter()
        .any(|basename| cmd.contains(basename))
}

// ─── shared helpers ───────────────────────────────────────────────────────

fn require_object_root(settings: &Value) -> Result<()> {
    if settings.is_object() {
        return Ok(());
    }
    Err(anyhow!(
        "settings.json 根節點不是 object（type={}）",
        value_type_name(settings)
    ))
}

fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn say(quiet: bool, msg: &str) {
    if !quiet {
        println!("{}", msg);
    }
}

fn prompt_confirm(prompt: &str) -> Result<bool> {
    print!("{}", prompt);
    io::stdout().flush().ok();
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    Ok(line.trim().eq_ignore_ascii_case("yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statusline_cmd(s: &Value) -> &str {
        s["statusLine"]["command"].as_str().unwrap()
    }

    #[test]
    fn statusline_merge_into_empty() {
        let mut s = json!({});
        let action = patch_statusline(&mut s, "/abs/codeforge statusline", false).unwrap();
        assert_eq!(action, "新增");
        assert_eq!(statusline_cmd(&s), "/abs/codeforge statusline");
        // V2.2 adds marker
        assert!(s["statusLine"][MARKER_KEY]
            .as_str()
            .unwrap()
            .starts_with("codeforge@"));
    }

    #[test]
    fn statusline_merge_preserves_unrelated() {
        let mut s = json!({"theme": "auto"});
        patch_statusline(&mut s, "/x statusline", false).unwrap();
        assert_eq!(s["theme"], "auto");
        assert_eq!(statusline_cmd(&s), "/x statusline");
    }

    #[test]
    fn statusline_idempotent_on_own() {
        let mut s = json!({});
        patch_statusline(&mut s, "/x statusline", false).unwrap();
        let action = patch_statusline(&mut s, "/x statusline", false).unwrap();
        assert_eq!(action, "已是最新（無變動）");
    }

    #[test]
    fn statusline_overwrites_codeforge_marker() {
        let mut s = json!({});
        patch_statusline(&mut s, "/old codeforge statusline", false).unwrap();
        let action = patch_statusline(&mut s, "/new codeforge statusline", false).unwrap();
        assert_eq!(action, "更新");
        assert_eq!(statusline_cmd(&s), "/new codeforge statusline");
    }

    #[test]
    fn statusline_sets_refresh_interval_and_keeps_one_already_there() {
        // a fresh install: 10 s
        let mut s = json!({});
        patch_statusline(&mut s, "/x statusline", false).unwrap();
        assert_eq!(s["statusLine"]["refreshInterval"], 10);
        // an install from before the field: re-install adds it (an update, not "no change")
        let mut old = json!({"statusLine": {"type": "command", "command": "/x statusline", MARKER_KEY: GLOBAL_MARKER}});
        assert_eq!(
            patch_statusline(&mut old, "/x statusline", false).unwrap(),
            "更新"
        );
        assert_eq!(old["statusLine"]["refreshInterval"], 10);
        // the user changed ours to 3: kept, and re-install is a no-op
        let mut own = json!({});
        patch_statusline(&mut own, "/x statusline", false).unwrap();
        own["statusLine"]["refreshInterval"] = json!(3);
        assert_eq!(
            patch_statusline(&mut own, "/x statusline", false).unwrap(),
            "已是最新（無變動）"
        );
        assert_eq!(own["statusLine"]["refreshInterval"], 3);
        // --replace-statusline over the user's own block keeps the user's interval
        let mut user = json!({"statusLine": {"type": "command", "command": "/user/script", "refreshInterval": 30}});
        patch_statusline(&mut user, "/x statusline", true).unwrap();
        assert_eq!(user["statusLine"]["command"], "/x statusline");
        assert_eq!(user["statusLine"]["refreshInterval"], 30);
    }

    #[test]
    fn statusline_refuses_user_owned_without_force() {
        // User has their own statusLine (no marker)
        let mut s = json!({"statusLine": {"type": "command", "command": "/user/script"}});
        let err = patch_statusline(&mut s, "/new statusline", false).unwrap_err();
        // B28: the error names the narrow override, not --force
        assert!(err.to_string().contains("--replace-statusline"));
        // The user's statusLine is untouched
        assert_eq!(s["statusLine"]["command"], "/user/script");
    }

    #[test]
    fn statusline_overwrites_user_owned_with_force() {
        let mut s = json!({"statusLine": {"type": "command", "command": "/user/script"}});
        let action = patch_statusline(&mut s, "/new statusline", /*force=*/ true).unwrap();
        assert_eq!(action, "更新");
        assert_eq!(statusline_cmd(&s), "/new statusline");
    }

    #[test]
    fn statusline_rejects_array_root() {
        let mut s = json!(["arr"]);
        let err = patch_statusline(&mut s, "/x", false).unwrap_err();
        assert!(err.to_string().contains("array"));
    }

    fn subagent_statusline_cmd(s: &Value) -> &str {
        s["subagentStatusLine"]["command"].as_str().unwrap()
    }

    #[test]
    fn subagent_statusline_merge_into_empty() {
        let mut s = json!({});
        let action =
            patch_subagent_statusline(&mut s, "/abs/codeforge subagent-statusline", false).unwrap();
        assert_eq!(action, "新增");
        assert_eq!(
            subagent_statusline_cmd(&s),
            "/abs/codeforge subagent-statusline"
        );
        assert!(s["subagentStatusLine"][MARKER_KEY]
            .as_str()
            .unwrap()
            .starts_with("codeforge@"));
    }

    #[test]
    fn subagent_statusline_idempotent_on_own() {
        let mut s = json!({});
        patch_subagent_statusline(&mut s, "/x subagent-statusline", false).unwrap();
        let action = patch_subagent_statusline(&mut s, "/x subagent-statusline", false).unwrap();
        assert_eq!(action, "已是最新（無變動）");
    }

    #[test]
    fn subagent_statusline_refuses_user_owned_without_force() {
        let mut s = json!({"subagentStatusLine": {"type": "command", "command": "/user/script"}});
        let err = patch_subagent_statusline(&mut s, "/x subagent-statusline", false).unwrap_err();
        assert!(err.to_string().contains("subagentStatusLine"));
        assert!(err.to_string().contains("--replace-statusline"));
    }

    #[test]
    fn install_only_adds_subagent_statusline_when_flag_passed() {
        let mut s = json!({});
        // Simulate the caller: statusLine patched first (always, when
        // install_statusline is true), subagentStatusLine only when the
        // opt-in flag is set.
        patch_statusline(&mut s, "/abs/codeforge statusline", false).unwrap();
        assert!(s.get("subagentStatusLine").is_none());

        patch_subagent_statusline(&mut s, "/abs/codeforge subagent-statusline", false).unwrap();
        assert_eq!(
            subagent_statusline_cmd(&s),
            "/abs/codeforge subagent-statusline"
        );
    }

    #[test]
    fn group_is_codeforge_detects_marker() {
        let ours = json!({
            "matcher": "",
            "hooks": [{"type": "command", "command": "x", MARKER_KEY: "codeforge@0.0.2"}]
        });
        assert!(group_is_codeforge(&ours));

        let theirs = json!({
            "matcher": "",
            "hooks": [{"type": "command", "command": "x"}]
        });
        assert!(!group_is_codeforge(&theirs));

        let mixed = json!({
            "matcher": "",
            "hooks": [
                {"type": "command", "command": "x", MARKER_KEY: "codeforge@0.0.2"},
                {"type": "command", "command": "y"},
            ]
        });
        assert!(!group_is_codeforge(&mixed));
    }

    #[test]
    fn patch_hooks_creates_block() {
        let mut s = json!({});
        let action = patch_hooks(&mut s, None, false).unwrap();
        assert_eq!(action, "新增");
        assert!(s["hooks"]["SessionStart"].is_array());
        assert!(s["hooks"]["SessionEnd"].is_array());
        assert!(s["hooks"]["PreCompact"].is_array());
        // Global SessionStart: emit-session + local-recall + central-recall (3).
        let session_start = &s["hooks"]["SessionStart"][0]["hooks"];
        assert_eq!(session_start.as_array().unwrap().len(), 3);
        let ss_cmds = serde_json::to_string(session_start).unwrap();
        assert!(ss_cmds.contains("emit-session"), "missing emit-session");
        assert!(
            ss_cmds.contains("codeforge memory context --hook"),
            "missing local-recall injector"
        );
        assert!(
            ss_cmds.contains("codeforge mnemos-cli context --hook"),
            "missing central-recall downlink (P1.2)"
        );
        // Global SessionEnd chain: emit-session, session-digest, dream, ship, usage collect (5).
        let session_end = &s["hooks"]["SessionEnd"][0]["hooks"];
        assert_eq!(session_end.as_array().unwrap().len(), 5);
        let last = session_end.as_array().unwrap().last().unwrap();
        assert!(
            last["command"]
                .as_str()
                .unwrap()
                .contains("codeforge usage collect --quiet"),
            "usage collect runs last in SessionEnd (G36)"
        );
        let cmds = serde_json::to_string(session_end).unwrap();
        assert!(cmds.contains("emit-session"), "missing emit-session");
        assert!(cmds.contains("session-digest"), "missing session-digest");
        assert!(cmds.contains("codeforge dream --quiet"), "missing dream");
        assert!(cmds.contains("codeforge ship --no-hook"), "missing ship");
        // dream must come before ship (ship reads the L1 dream produces).
        let se = session_end.as_array().unwrap();
        let dream_idx = se
            .iter()
            .position(|h| h["command"].as_str().unwrap().contains("dream"))
            .unwrap();
        let ship_idx = se
            .iter()
            .position(|h| h["command"].as_str().unwrap().contains("ship"))
            .unwrap();
        assert!(dream_idx < ship_idx, "dream must precede ship");
    }

    #[test]
    fn patch_hooks_project_layout_is_codeforge_clone_only() {
        let mut s = json!({});
        let dir = PathBuf::from("/tmp/cf-test");
        patch_hooks(&mut s, Some(&dir), false).unwrap();
        // 2 hook types present: SessionStart + PreToolUse. dream/ship moved to
        // the global path, so project mode no longer writes SessionEnd; PreCompact
        // is global-only too.
        assert!(s["hooks"]["SessionStart"].is_array());
        assert!(s["hooks"]["PreToolUse"].is_array());
        assert!(
            s["hooks"].get("SessionEnd").is_none(),
            "dream/ship are global-only now"
        );
        assert!(
            s["hooks"].get("PreCompact").is_none(),
            "PreCompact must be global-only"
        );
        // PreToolUse has the Edit|Write|Bash matcher
        assert_eq!(s["hooks"]["PreToolUse"][0]["matcher"], "Edit|Write|Bash");
        // Each present hook type has exactly 1 entry (the codeforge-clone-only one)
        assert_eq!(
            s["hooks"]["SessionStart"][0]["hooks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            s["hooks"]["PreToolUse"][0]["hooks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // SessionStart is check-improvements
        let ss_cmd = s["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(ss_cmd.contains("check-improvements.js"), "got: {}", ss_cmd);
        // No product-wide hooks leaked into project hooks (dual-fire prevention)
        let all_commands = serde_json::to_string(&s["hooks"]).unwrap();
        assert!(
            !all_commands.contains("emit-session"),
            "emit-session leaked into project hooks"
        );
        assert!(
            !all_commands.contains("session-digest"),
            "session-digest leaked into project hooks"
        );
        assert!(
            !all_commands.contains("codeforge dream"),
            "dream leaked into project hooks"
        );
        assert!(
            !all_commands.contains("codeforge ship"),
            "ship leaked into project hooks"
        );
    }

    #[test]
    fn project_mode_sweeps_legacy_dream_entry() {
        // Migration: pre-relocation project settings had a hand-written dream
        // entry in SessionEnd (often un-markered). dream now lives in the global
        // path, so a `--project-hooks` re-run must SWEEP that stale entry (not
        // leave it orphaned). The full-sweep + collapse handles this.
        let mut s = json!({
            "hooks": {
                "SessionEnd": [{
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": "codeforge dream --quiet 2>/dev/null || true",
                        "timeout": 30000
                    }]
                }]
            }
        });
        let action = patch_hooks(&mut s, Some(Path::new("/tmp/cf-test")), false).unwrap();
        assert_eq!(
            action, "更新",
            "legacy dream entry recognized as ours and swept"
        );
        // SessionEnd collapsed away — project mode no longer writes it, and the
        // stale dream group was removed rather than orphaned.
        assert!(
            s["hooks"].get("SessionEnd").is_none(),
            "stale project dream must be swept, SessionEnd collapsed"
        );
        // The dev hooks project mode does own are present.
        assert!(s["hooks"]["SessionStart"].is_array());
        assert!(s["hooks"]["PreToolUse"].is_array());
    }

    #[test]
    fn legacy_recognizes_unmarkered_node_hook_scripts() {
        // Global pre-marker install paths.
        assert!(is_legacy_codeforge_command(
            "node /home/u/.local/share/codeforge/hooks/0.0.3/emit-session.js session_end"
        ));
        assert!(is_legacy_codeforge_command(
            "node /home/u/.local/share/codeforge/hooks/0.0.3/session-digest.js"
        ));
        // Project script path.
        assert!(is_legacy_codeforge_command(
            "node ${CLAUDE_PROJECT_DIR}/.claude/scripts/check-dev-flow.js"
        ));
        // Pre-marker inline subcommands. ship + memory-context were the dual-fire
        // root cause: previously unrecognized, so a pre-marker group containing
        // them failed the all()-codeforge gate and escaped the sweep.
        assert!(is_legacy_codeforge_command(
            "codeforge dream --quiet 2>/dev/null || true"
        ));
        assert!(is_legacy_codeforge_command(
            "codeforge ship --no-hook 2>/dev/null || true"
        ));
        assert!(is_legacy_codeforge_command(
            "codeforge memory context --hook 2>/dev/null || true"
        ));
        // Not ours — must not clobber.
        assert!(!is_legacy_codeforge_command("node /home/u/myscript.js"));
        assert!(!is_legacy_codeforge_command("prettier --write x.ts"));
        // Tight: a different codeforge subcommand we never auto-installed must NOT
        // be swept (guards against over-broad `codeforge ` prefix matching).
        assert!(!is_legacy_codeforge_command("codeforge pet"));
        assert!(!is_legacy_codeforge_command("codeforge statusline"));
        // Token-boundary: prefix collisions must NOT be mistaken for the real subs.
        assert!(!is_legacy_codeforge_command("codeforge shipment --foo"));
        assert!(!is_legacy_codeforge_command(
            "codeforge memory contextualize"
        ));
        assert!(!is_legacy_codeforge_command("codeforge dreamboat"));
    }

    #[test]
    fn premarker_group_with_ship_and_memory_context_is_swept() {
        // Regression for the field dual-fire: a pre-marker SessionEnd group whose
        // entries include the unmarked `codeforge ship` (and SessionStart's
        // `codeforge memory context`) must be recognized as wholly codeforge-owned
        // so a re-install sweeps it instead of stacking a second marked group.
        let session_end_group = json!({
            "matcher": "",
            "hooks": [
                { "command": "node /home/u/.local/share/codeforge/hooks/0.0.5/emit-session.js session_end" },
                { "command": "node /home/u/.local/share/codeforge/hooks/0.0.5/session-digest.js" },
                { "command": "codeforge dream --quiet 2>/dev/null || true" },
                { "command": "codeforge ship --no-hook 2>/dev/null || true" }
            ]
        });
        assert!(
            group_is_codeforge(&session_end_group),
            "pre-marker SessionEnd group containing `codeforge ship` must be swept"
        );

        let session_start_group = json!({
            "matcher": "",
            "hooks": [
                { "command": "node /home/u/.local/share/codeforge/hooks/0.0.5/emit-session.js session_start" },
                { "command": "codeforge memory context --hook 2>/dev/null || true" }
            ]
        });
        assert!(
            group_is_codeforge(&session_start_group),
            "pre-marker SessionStart group containing `codeforge memory context` must be swept"
        );
    }

    #[test]
    fn global_install_sweeps_unmarkered_legacy_node_hooks() {
        // Mirrors the live upgrade case: a global settings.json whose codeforge
        // node hooks were installed by a pre-marker version (no _installed_by).
        // Re-installing must sweep the old 0.0.3 entries, not stack 0.0.4 beside
        // them (which would dual-fire the digest pipeline).
        let mut s = json!({
            "hooks": {
                "SessionStart": [{ "matcher": "", "hooks": [
                    {"type":"command","command":"node /x/codeforge/hooks/0.0.3/emit-session.js session_start","timeout":3000}
                ]}],
                "SessionEnd": [{ "matcher": "", "hooks": [
                    {"type":"command","command":"node /x/codeforge/hooks/0.0.3/emit-session.js session_end","timeout":3000},
                    {"type":"command","command":"node /x/codeforge/hooks/0.0.3/session-digest.js","timeout":30000}
                ]}],
                "PreCompact": [{ "matcher": "", "hooks": [
                    {"type":"command","command":"node /x/codeforge/hooks/0.0.3/session-digest.js","timeout":30000}
                ]}]
            }
        });
        patch_hooks(&mut s, None, false).unwrap();
        // Exactly one SessionEnd group, with the new 4-entry chain — no 0.0.3 dupes.
        let se = s["hooks"]["SessionEnd"].as_array().unwrap();
        assert_eq!(
            se.len(),
            1,
            "old un-markered group must be swept, not kept as sibling"
        );
        let inner = se[0]["hooks"].as_array().unwrap();
        assert_eq!(
            inner.len(),
            5,
            "emit-session + session-digest + dream + ship + usage collect"
        );
        let all = serde_json::to_string(&s["hooks"]).unwrap();
        assert!(
            !all.contains("0.0.3"),
            "stale 0.0.3 entries must be gone: {all}"
        );
    }

    #[test]
    fn global_mode_relocates_legacy_project_dream_into_chain() {
        // Migration the other direction: a settings.json that still carries a
        // legacy dream SessionEnd group gets it swept and replaced by the full
        // global chain (no duplicate dream) when `--hooks` runs.
        let mut s = json!({
            "hooks": {
                "SessionEnd": [{
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": "codeforge dream --quiet 2>/dev/null || true",
                        "timeout": 30000
                    }]
                }]
            }
        });
        patch_hooks(&mut s, None, false).unwrap();
        let se = s["hooks"]["SessionEnd"].as_array().unwrap();
        assert_eq!(
            se.len(),
            1,
            "one codeforge group, legacy not left as a sibling"
        );
        let inner = se[0]["hooks"].as_array().unwrap();
        // emit-session, session-digest, dream, ship — exactly one dream.
        let dream_count = inner
            .iter()
            .filter(|h| h["command"].as_str().unwrap().contains("codeforge dream"))
            .count();
        assert_eq!(dream_count, 1, "no duplicate dream after relocation");
        assert!(inner
            .iter()
            .any(|h| h["command"].as_str().unwrap().contains("codeforge ship")));
    }

    #[test]
    fn cleanup_period_fills_when_unset() {
        let mut s = json!({});
        let action = patch_cleanup_period(&mut s, DEFAULT_CLEANUP_PERIOD_DAYS, false);
        assert_eq!(action, "新增");
        assert_eq!(s["cleanupPeriodDays"], json!(DEFAULT_CLEANUP_PERIOD_DAYS));
    }

    #[test]
    fn cleanup_period_respects_existing_user_value() {
        let mut s = json!({ "cleanupPeriodDays": 90 });
        let action = patch_cleanup_period(&mut s, DEFAULT_CLEANUP_PERIOD_DAYS, false);
        assert_eq!(action, "已設定（保留現值）");
        assert_eq!(
            s["cleanupPeriodDays"],
            json!(90),
            "must not clobber user's value"
        );
    }

    #[test]
    fn cleanup_period_force_overwrites() {
        let mut s = json!({ "cleanupPeriodDays": 90 });
        let action = patch_cleanup_period(&mut s, DEFAULT_CLEANUP_PERIOD_DAYS, true);
        assert_eq!(action, "重置（--force）");
        assert_eq!(s["cleanupPeriodDays"], json!(DEFAULT_CLEANUP_PERIOD_DAYS));
    }

    #[test]
    fn cleanup_policy_flag_writes_even_when_shared() {
        let mut s = json!({});
        let lines = apply_cleanup_period(&mut s, Some(90), true, false);
        assert_eq!(s["cleanupPeriodDays"], json!(90));
        assert!(lines[0].contains("新增"));
        // an existing value still needs --force
        let mut s = json!({"cleanupPeriodDays": 30});
        apply_cleanup_period(&mut s, Some(90), true, false);
        assert_eq!(s["cleanupPeriodDays"], json!(30));
        apply_cleanup_period(&mut s, Some(90), true, true);
        assert_eq!(s["cleanupPeriodDays"], json!(90));
    }

    #[test]
    fn cleanup_policy_local_without_flag_keeps_default() {
        let mut s = json!({});
        apply_cleanup_period(&mut s, None, false, false);
        assert_eq!(s["cleanupPeriodDays"], json!(DEFAULT_CLEANUP_PERIOD_DAYS));
    }

    #[test]
    fn cleanup_policy_shared_without_flag_never_touches_it() {
        let mut s = json!({});
        let lines = apply_cleanup_period(&mut s, None, true, false);
        assert!(s.get("cleanupPeriodDays").is_none());
        assert!(lines[0].contains("--cleanup-period-days 3650"), "{lines:?}");

        for force in [false, true] {
            let mut s = json!({"cleanupPeriodDays": 3650});
            let lines = apply_cleanup_period(&mut s, None, true, force);
            assert_eq!(s["cleanupPeriodDays"], json!(3650), "force={force}");
            assert!(lines[0].contains("保留不動"), "{lines:?}");
        }
    }

    #[test]
    fn project_mode_uses_claude_project_dir_placeholder() {
        // Mimics run_install_project_hooks: pass the env-var template path,
        // not a real on-disk path. Verifies the commands written into
        // settings.json are portable (no per-machine absolute paths).
        let mut s = json!({});
        let template = Path::new(PROJECT_DIR_ENV_PLACEHOLDER)
            .join(".claude")
            .join("scripts");
        patch_hooks(&mut s, Some(&template), false).unwrap();
        let cmd = s["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(
            cmd.contains("${CLAUDE_PROJECT_DIR}/.claude/scripts/check-improvements.js"),
            "expected env-var placeholder in command, got: {}",
            cmd
        );
        // No absolute /home/, /Users/, or C:\ paths should leak
        assert!(
            !cmd.contains("/home/"),
            "unexpected absolute /home/ path: {}",
            cmd
        );
        assert!(
            !cmd.contains("/Users/"),
            "unexpected absolute /Users/ path: {}",
            cmd
        );
    }

    #[test]
    fn patch_hooks_preserves_user_entries() {
        let mut s = json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "",
                    "hooks": [{"type": "command", "command": "user-script.sh"}]
                }]
            }
        });
        patch_hooks(&mut s, None, false).unwrap();
        let session_start = s["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 2);
        assert_eq!(session_start[0]["hooks"][0]["command"], "user-script.sh");
        assert!(group_is_codeforge(&session_start[1]));
    }

    #[test]
    fn patch_hooks_force_clears_user_entries() {
        let mut s = json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "",
                    "hooks": [{"type": "command", "command": "user-script.sh"}]
                }],
                "PreToolUse": [{
                    "matcher": "*",
                    "hooks": [{"type": "command", "command": "user-other"}]
                }]
            }
        });
        let action = patch_hooks(&mut s, None, /*force=*/ true).unwrap();
        assert_eq!(action, "重置（--force）");
        // User's PreToolUse should be gone entirely (we don't write to it in non-project mode)
        assert!(s["hooks"].get("PreToolUse").is_none());
        // User's SessionStart should be gone — replaced by our entry only
        let ss = s["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(ss.len(), 1, "force should leave only codeforge entry");
        assert!(group_is_codeforge(&ss[0]));
    }

    #[test]
    fn patch_hooks_replaces_own_prior_entries() {
        let mut s = json!({});
        patch_hooks(&mut s, None, false).unwrap();
        let len_after_first = s["hooks"]["SessionStart"].as_array().unwrap().len();
        let action = patch_hooks(&mut s, None, false).unwrap();
        assert_eq!(action, "更新");
        let len_after_second = s["hooks"]["SessionStart"].as_array().unwrap().len();
        assert_eq!(len_after_first, len_after_second);
    }

    #[test]
    fn unpatch_hooks_removes_only_codeforge() {
        let mut s = json!({});
        patch_hooks(&mut s, None, false).unwrap();
        // Add a user hook alongside
        let user = json!({
            "matcher": "",
            "hooks": [{"type": "command", "command": "user.sh"}]
        });
        s["hooks"]["SessionStart"]
            .as_array_mut()
            .unwrap()
            .push(user.clone());
        // Now uninstall
        let action = unpatch_hooks(&mut s).unwrap();
        assert_eq!(action, "已移除");
        // User entry preserved
        let ss = s["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(ss.len(), 1);
        assert_eq!(ss[0]["hooks"][0]["command"], "user.sh");
        // SessionEnd/PreCompact entries (only codeforge ones) → removed → arrays collapsed
        assert!(s["hooks"].get("SessionEnd").is_none());
        assert!(s["hooks"].get("PreCompact").is_none());
    }

    #[test]
    fn unpatch_hooks_removes_legacy_unmarker_dream_entry() {
        // Pre-2.2 .claude/settings.json shape: dream entry hand-written, no marker.
        // unpatch_hooks shares group_is_codeforge with patch_hooks, so the
        // legacy detection must propagate to uninstall too — otherwise a
        // user running `codeforge uninstall` before ever cycling through the
        // new --project-hooks would leave the legacy dream entry behind.
        let mut s = json!({
            "hooks": {
                "SessionEnd": [{
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": "codeforge dream --quiet 2>/dev/null || true",
                        "timeout": 30000
                    }]
                }]
            }
        });
        let action = unpatch_hooks(&mut s).unwrap();
        assert_eq!(action, "已移除");
        // Empty SessionEnd array collapsed → entire hooks key removed
        assert!(s.as_object().unwrap().get("hooks").is_none());
    }

    #[test]
    fn unpatch_hooks_collapses_empty_root() {
        let mut s = json!({});
        patch_hooks(&mut s, None, false).unwrap();
        unpatch_hooks(&mut s).unwrap();
        // No user entries anywhere → the entire `hooks` key should be removed
        assert!(s.as_object().unwrap().get("hooks").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn write_through_symlink_keeps_link() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dot = tmp.path().join("dot");
        let home = tmp.path().join("home");
        fs::create_dir_all(&dot).unwrap();
        fs::create_dir_all(&home).unwrap();
        let target = dot.join("settings.json");
        fs::write(&target, "{}\n").unwrap();
        let link = home.join("settings.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_settings_atomic(&link, &json!({"a": 1})).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the link must stay a symlink"
        );
        let written: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(
            written,
            json!({"a": 1}),
            "the content lands in the link target"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_through_relative_symlink_chain() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dot = tmp.path().join("dot");
        fs::create_dir_all(&dot).unwrap();
        fs::write(dot.join("settings.json"), "{}\n").unwrap();
        // home/settings.json -> mid.json -> ../dot/settings.json (relative, chained)
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        std::os::unix::fs::symlink("../dot/settings.json", home.join("mid.json")).unwrap();
        std::os::unix::fs::symlink("mid.json", home.join("settings.json")).unwrap();

        write_settings_atomic(&home.join("settings.json"), &json!({"b": 2})).unwrap();

        assert!(fs::symlink_metadata(home.join("settings.json"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(fs::symlink_metadata(home.join("mid.json"))
            .unwrap()
            .file_type()
            .is_symlink());
        let written: Value =
            serde_json::from_str(&fs::read_to_string(dot.join("settings.json")).unwrap()).unwrap();
        assert_eq!(written, json!({"b": 2}));
        // no tmp file left behind next to the target
        let leftovers: Vec<_> = fs::read_dir(&dot)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn write_plain_file_unchanged_behaviour() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        fs::write(&path, "{}\n").unwrap();
        write_settings_atomic(&path, &json!({"c": 3})).unwrap();
        assert!(!fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\n  \"c\": 3\n}\n");
    }

    /// The d-f6e451d2 rollout: re-running install over an older codeforge install adds
    /// `statusLine.refreshInterval` and changes nothing else; the old text is in `.bak`.
    #[test]
    fn reinstall_over_an_older_install_adds_only_refresh_interval() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        let opts = || InstallOpts {
            hooks: false,
            all: true,
            subagent_statusline: false,
            project_hooks: false,
            dry_run: false,
            force: false,
            replace_statusline: false,
            yes: true,
            settings_path: Some(path.clone()),
            quiet: true,
            cleanup_period_days: None,
        };
        run(opts()).unwrap();
        // an install from before the field, with the user's own settings around it
        let mut older: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        older["statusLine"]
            .as_object_mut()
            .unwrap()
            .remove("refreshInterval");
        older["theme"] = json!("dark");
        older["env"] = json!({"X": "1"});
        let older_text = serde_json::to_string_pretty(&older).unwrap() + "\n";
        fs::write(&path, &older_text).unwrap();

        run(opts()).unwrap();
        let after: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let mut want = older.clone();
        want["statusLine"]["refreshInterval"] = json!(STATUSLINE_REFRESH_S);
        assert_eq!(after, want, "only statusLine.refreshInterval may change");
        assert_eq!(
            fs::read_to_string(tmp.path().join("settings.json.bak")).unwrap(),
            older_text
        );
    }

    #[test]
    fn write_backs_up_a_changed_file_to_bak() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        let bak = tmp.path().join("settings.json.bak");
        // no file before: nothing to back up
        write_settings_atomic(&path, &json!({"a": 1})).unwrap();
        assert!(!bak.exists());
        // a change: the previous text is kept in .bak
        write_settings_atomic(&path, &json!({"a": 2})).unwrap();
        assert_eq!(fs::read_to_string(&bak).unwrap(), "{\n  \"a\": 1\n}\n");
        // a no-op re-write leaves the earlier backup as it was
        write_settings_atomic(&path, &json!({"a": 2})).unwrap();
        assert_eq!(fs::read_to_string(&bak).unwrap(), "{\n  \"a\": 1\n}\n");
    }

    #[cfg(unix)]
    #[test]
    fn write_backs_up_a_linked_settings_next_to_the_link() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dots = tmp.path().join("dotfiles");
        fs::create_dir(&dots).unwrap();
        fs::write(dots.join("settings.json"), "{}\n").unwrap();
        let link = tmp.path().join("settings.json");
        std::os::unix::fs::symlink(dots.join("settings.json"), &link).unwrap();
        write_settings_atomic(&link, &json!({"b": 1})).unwrap();
        // the backup is a plain file beside the link, never inside the (possibly git-tracked) target dir
        let bak = tmp.path().join("settings.json.bak");
        assert!(!fs::symlink_metadata(&bak).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&bak).unwrap(), "{}\n");
        assert!(!dots.join("settings.json.bak").exists());
    }

    #[cfg(unix)]
    fn dangling_link(tmp: &Path) -> PathBuf {
        let link = tmp.join("settings.json");
        std::os::unix::fs::symlink(tmp.join("gone/settings.json"), &link).unwrap();
        link
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_refused_by_install_and_write() {
        let tmp = tempfile::TempDir::new().unwrap();
        let link = dangling_link(tmp.path());
        let err = check_settings_path(&link).unwrap_err().to_string();
        assert!(err.contains("不存在") && err.contains("gone"), "{err}");
        assert!(write_settings_atomic(&link, &json!({})).is_err());
        // the link itself is untouched
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());

        let opts = InstallOpts {
            hooks: true,
            all: false,
            subagent_statusline: false,
            project_hooks: false,
            dry_run: true,
            force: false,
            replace_statusline: false,
            yes: false,
            settings_path: Some(link.clone()),
            quiet: true,
            cleanup_period_days: None,
        };
        assert!(
            run(opts).is_err(),
            "dry-run must not read a dangling link as empty"
        );
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_refused_by_uninstall() {
        let tmp = tempfile::TempDir::new().unwrap();
        let link = dangling_link(tmp.path());
        let res = run_uninstall(UninstallOpts {
            statusline: true,
            hooks: false,
            settings_path: Some(link.clone()),
            quiet: true,
        });
        assert!(
            res.is_err(),
            "uninstall must not report nothing-to-do on a dangling link"
        );
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn shared_settings_plain_file_is_not_shared() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        fs::write(&path, "{}").unwrap();
        let s = shared_settings(&path);
        assert_eq!(s, SharedSettings::default());
        assert!(shared_warning_lines(&path, &s).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn shared_settings_detects_symlink() {
        let tmp = tempfile::TempDir::new().unwrap();
        let target = tmp.path().join("real.json");
        fs::write(&target, "{}").unwrap();
        let link = tmp.path().join("settings.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let s = shared_settings(&link);
        assert_eq!(s.symlink_target, Some(fs::canonicalize(&target).unwrap()));
        assert!(s.is_shared());
        let lines = shared_warning_lines(&link, &s);
        assert!(lines[0].contains("symlink"));
    }

    #[test]
    fn shared_settings_detects_git_tracked_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = tmp.path();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(args)
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_INDEX_FILE")
                .output()
        };
        match git(&["init", "-q"]) {
            Ok(o) if o.status.success() => {}
            _ => {
                eprintln!("skip: git not available");
                return;
            }
        }
        let path = repo.join("settings.json");
        fs::write(&path, "{}").unwrap();
        // untracked → not shared
        assert_eq!(shared_settings(&path).git_repo, None);
        git(&["add", "settings.json"]).unwrap();
        let s = shared_settings(&path);
        assert_eq!(
            s.git_repo.map(|p| fs::canonicalize(p).unwrap()),
            Some(fs::canonicalize(repo).unwrap())
        );
        assert!(shared_warning_lines(&path, &shared_settings(&path))
            .iter()
            .any(|l| l.contains("git 版控")));
    }

    fn all_hook_entries(s: &Value) -> Vec<Value> {
        s["hooks"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|groups| groups.as_array().unwrap().clone())
            .flat_map(|g| g["hooks"].as_array().unwrap().clone())
            .collect()
    }

    #[test]
    fn global_hook_commands_carry_no_machine_path_or_version() {
        let mut s = json!({});
        patch_hooks(&mut s, None, false).unwrap();
        let version = env!("CARGO_PKG_VERSION");
        let entries = all_hook_entries(&s);
        assert!(!entries.is_empty());
        for e in &entries {
            let cmd = e["command"].as_str().unwrap();
            assert!(cmd.starts_with("codeforge "), "{cmd}");
            for bad in [
                "$HOME",
                "~",
                "/home/",
                "/Users/",
                "/codeforge/hooks/",
                "node ",
            ] {
                assert!(!cmd.contains(bad), "{cmd} contains {bad}");
            }
            assert!(!cmd.contains(version), "{cmd} contains the package version");
            // The only '/' allowed is the existing `2>/dev/null` redirect.
            assert!(
                cmd.replace("2>/dev/null", "").find('/').is_none(),
                "absolute path in {cmd}"
            );
            assert_eq!(e[MARKER_KEY], GLOBAL_MARKER);
        }
        let cmds: Vec<&str> = entries
            .iter()
            .map(|e| e["command"].as_str().unwrap())
            .collect();
        assert!(cmds.contains(&"codeforge hook emit-session session_start"));
        assert!(cmds.contains(&"codeforge hook emit-session session_end"));
        assert_eq!(
            cmds.iter()
                .filter(|c| **c == "codeforge hook session-digest")
                .count(),
            2,
            "SessionEnd + PreCompact"
        );
    }

    #[test]
    fn global_install_is_byte_identical_across_runs() {
        let run_once = |s: &mut Value| {
            patch_hooks(s, None, false).unwrap();
            patch_statusline(s, "codeforge statusline", false).unwrap();
            patch_subagent_statusline(s, "codeforge subagent-statusline", false).unwrap();
        };
        let mut a = json!({"model": "x"});
        run_once(&mut a);
        let first = serde_json::to_string_pretty(&a).unwrap();
        run_once(&mut a);
        assert_eq!(first, serde_json::to_string_pretty(&a).unwrap());
        assert_eq!(a["statusLine"][MARKER_KEY], GLOBAL_MARKER);
        assert_eq!(a["subagentStatusLine"][MARKER_KEY], GLOBAL_MARKER);
        assert!(!first.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn project_hooks_output_unchanged() {
        // Byte-for-byte what --project-hooks wrote before B35 (marker keeps the
        // package version; commands use ${CLAUDE_PROJECT_DIR}).
        let mut s = json!({});
        let template = Path::new(PROJECT_DIR_ENV_PLACEHOLDER)
            .join(".claude")
            .join("scripts");
        patch_hooks(&mut s, Some(&template), false).unwrap();
        let marker = format!("codeforge@{}", env!("CARGO_PKG_VERSION"));
        let expected = json!({
            "hooks": {
                "SessionStart": [{"matcher": "", "hooks": [{
                    "type": "command",
                    "command": "node ${CLAUDE_PROJECT_DIR}/.claude/scripts/check-improvements.js",
                    "timeout": 10000,
                    MARKER_KEY: marker,
                }]}],
                "PreToolUse": [{"matcher": "Edit|Write|Bash", "hooks": [{
                    "type": "command",
                    "command": "node ${CLAUDE_PROJECT_DIR}/.claude/scripts/check-dev-flow.js",
                    "timeout": 5000,
                    MARKER_KEY: marker,
                }]}],
            }
        });
        assert_eq!(s, expected);
    }

    #[test]
    fn upgrade_from_versioned_install_leaves_only_new_form() {
        // What a v0.0.6 `install --hooks` wrote: marked, versioned node paths.
        let old = "codeforge@0.0.6";
        let p = "/home/u/.local/share/codeforge/hooks/0.0.6";
        let mut s = json!({
            "statusLine": {"type": "command", "command": "/home/u/.cargo/bin/codeforge statusline", MARKER_KEY: old},
            "hooks": {
                "SessionStart": [
                    {"matcher": "", "hooks": [{"type": "command", "command": "user.sh"}]},
                    {"matcher": "", "hooks": [
                        {"type": "command", "command": format!("node {p}/emit-session.js session_start"), "timeout": 3000, MARKER_KEY: old},
                        {"type": "command", "command": "codeforge memory context --hook 2>/dev/null || true", "timeout": 10000, MARKER_KEY: old}
                    ]}
                ],
                "SessionEnd": [{"matcher": "", "hooks": [
                    {"type": "command", "command": format!("node {p}/emit-session.js session_end"), "timeout": 3000, MARKER_KEY: old},
                    {"type": "command", "command": format!("node {p}/session-digest.js"), "timeout": 30000, MARKER_KEY: old}
                ]}],
                "PreCompact": [{"matcher": "", "hooks": [
                    {"type": "command", "command": format!("node {p}/session-digest.js"), "timeout": 30000, MARKER_KEY: old}
                ]}]
            }
        });
        patch_hooks(&mut s, None, false).unwrap();
        patch_statusline(&mut s, "codeforge statusline", false).unwrap();
        let all = serde_json::to_string(&s).unwrap();
        assert!(!all.contains("0.0.6"), "old entries must be swept: {all}");
        assert!(!all.contains("node "), "{all}");
        // one codeforge group per hook type (no stacking), user hook kept
        for ty in ["SessionStart", "SessionEnd", "PreCompact"] {
            let ours = s["hooks"][ty]
                .as_array()
                .unwrap()
                .iter()
                .filter(|g| group_is_codeforge(g))
                .count();
            assert_eq!(ours, 1, "{ty}");
        }
        assert!(all.contains("user.sh"));
        assert_eq!(s["statusLine"]["command"], "codeforge statusline");
    }

    #[test]
    fn statusline_command_cases() {
        let exe = Path::new("/opt/cf/codeforge");
        // shared file: always the PATH form, whatever PATH says
        assert_eq!(
            statusline_command(exe, false, true, "statusline"),
            "codeforge statusline"
        );
        assert_eq!(
            statusline_command(exe, true, true, "subagent-statusline"),
            "codeforge subagent-statusline"
        );
        // local file, PATH resolves to this binary: PATH form
        assert_eq!(
            statusline_command(exe, true, false, "statusline"),
            "codeforge statusline"
        );
        // local file, PATH does not: absolute path (current behaviour)
        assert_eq!(
            statusline_command(exe, false, false, "statusline"),
            "/opt/cf/codeforge statusline"
        );
    }

    #[test]
    fn same_binary_compares_canonical_paths() {
        let tmp = tempfile::TempDir::new().unwrap();
        let a = tmp.path().join("codeforge");
        fs::write(&a, "").unwrap();
        assert!(same_binary(Some(&a), &a));
        assert!(same_binary(
            Some(&tmp.path().join(".").join("codeforge")),
            &a
        ));
        assert!(!same_binary(None, &a));
        let b = tmp.path().join("other");
        fs::write(&b, "").unwrap();
        assert!(!same_binary(Some(&b), &a));
    }

    #[test]
    fn unpatch_statusline_removes_only_codeforge() {
        // Codeforge-tagged → removed
        let mut s = json!({});
        patch_statusline(&mut s, "/x statusline", false).unwrap();
        let action = unpatch_statusline(&mut s).unwrap();
        assert_eq!(action, "已移除");
        assert!(s.as_object().unwrap().get("statusLine").is_none());

        // User-owned → preserved
        let mut s2 = json!({"statusLine": {"type": "command", "command": "/user"}});
        let action = unpatch_statusline(&mut s2).unwrap();
        assert_eq!(action, "保留（非 codeforge 寫入）");
        assert_eq!(s2["statusLine"]["command"], "/user");
    }
}
