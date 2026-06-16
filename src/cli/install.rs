use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, BufRead, Write as _};
use std::path::{Path, PathBuf};

/// Options for `codeforge install`.
pub struct InstallOpts {
    pub hooks: bool,
    pub all: bool,
    pub project_hooks: bool,
    pub dry_run: bool,
    pub force: bool,
    pub yes: bool,
    pub settings_path: Option<PathBuf>,
    pub quiet: bool,
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
///   `session-digest`, plus the `codeforge dream → codeforge ship --no-hook`
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
        say(opts.quiet, "⚠ --force 會清除 settings.json 內所有 hook entries（含非 codeforge）。");
        if !prompt_confirm("確定要繼續？輸入 'yes' 確認: ")? {
            bail!("使用者取消 --force 操作");
        }
    }

    if install_project_hooks {
        run_install_project_hooks(&opts)?;
        return Ok(());
    }

    let exe = std::env::current_exe().context("讀取 codeforge binary 路徑失敗")?;
    let exe_str = exe
        .to_str()
        .ok_or_else(|| anyhow!("binary 路徑含非 UTF-8 字元：{}", exe.display()))?
        .to_string();

    let settings_path = opts
        .settings_path
        .clone()
        .map(Ok)
        .unwrap_or_else(default_global_settings_path)?;
    say(opts.quiet, &format!("  目標 settings.json: {}", settings_path.display()));

    let mut settings = load_settings(&settings_path)?;

    if install_statusline {
        let cmd = format!("{} statusline", exe_str);
        let action = patch_statusline(&mut settings, &cmd, opts.force)?;
        say(opts.quiet, &format!("✓ statusLine {}", action));
        say(opts.quiet, &format!("  command: {}", cmd));
    }

    if install_hooks {
        let hooks_dir = if opts.dry_run {
            // Don't actually extract files in dry-run; show the path that would be used
            default_data_local_dir()?
                .join("codeforge")
                .join("hooks")
                .join(env!("CARGO_PKG_VERSION"))
        } else {
            extract_hook_scripts()?
        };
        let action = patch_hooks(&mut settings, &hooks_dir, opts.force, /*project=*/ false)?;
        say(opts.quiet, &format!("✓ hooks {}", action));
        say(opts.quiet, &format!("  scripts: {}", hooks_dir.display()));

        // The memory pipeline (dream/ship) can only distill session transcripts
        // that still exist. Claude Code's default cleanupPeriodDays is 30 — older
        // sessions are deleted before they're ever digested. Bump retention so
        // raw material survives. Fills only when unset (respects a user's explicit
        // value unless --force).
        let action = patch_cleanup_period(&mut settings, opts.force);
        say(opts.quiet, &format!("✓ cleanupPeriodDays {}", action));
        if action != "已設定（保留現值）" {
            say(
                opts.quiet,
                &format!(
                    "  ↳ 保留 session transcript {} 天（供 dream/ship 萃取）；~/.claude/projects/ 會隨之長期累積",
                    DEFAULT_CLEANUP_PERIOD_DAYS
                ),
            );
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
    say(opts.quiet, "生效方式：下個 user message 或 /clear 之後 Claude Code 會 pick up 新設定。");
    Ok(())
}

/// `codeforge uninstall` — remove codeforge-tagged entries.
pub fn run_uninstall(opts: UninstallOpts) -> Result<()> {
    // If neither flag set, default to "everything"
    let remove_statusline = !opts.statusline && !opts.hooks || opts.statusline;
    let remove_hooks = !opts.statusline && !opts.hooks || opts.hooks;

    let settings_path = opts
        .settings_path
        .clone()
        .map(Ok)
        .unwrap_or_else(default_global_settings_path)?;

    if !settings_path.exists() {
        say(opts.quiet, &format!("settings.json 不存在：{}（沒事可做）", settings_path.display()));
        return Ok(());
    }

    say(opts.quiet, &format!("  目標 settings.json: {}", settings_path.display()));
    let mut settings = load_settings(&settings_path)?;

    if remove_statusline {
        let action = unpatch_statusline(&mut settings)?;
        say(opts.quiet, &format!("✓ statusLine {}", action));
    }

    if remove_hooks {
        let action = unpatch_hooks(&mut settings)?;
        say(opts.quiet, &format!("✓ hooks {}", action));

        // Also remove the extracted scripts dir
        let base = default_data_local_dir()?.join("codeforge").join("hooks");
        if base.exists() {
            fs::remove_dir_all(&base)
                .with_context(|| format!("移除 {} 失敗", base.display()))?;
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
    say(opts.quiet, &format!("  目標 settings.json: {}", settings_path.display()));

    // Existence check uses the real CWD path (resolves $CLAUDE_PROJECT_DIR for us locally).
    let scripts_dir_real = cwd.join(".claude").join("scripts");
    for name in PROJECT_HOOK_SCRIPT_NAMES {
        let p = scripts_dir_real.join(name);
        if !p.exists() {
            bail!("找不到必要 script：{}（執行位置不像 codeforge clone）", p.display());
        }
    }

    // Written into command strings — uses Claude Code's documented env var so the
    // committed settings.json is portable across every clone. Claude Code expands
    // `${CLAUDE_PROJECT_DIR}` before spawning the hook process.
    let scripts_dir_template = Path::new(PROJECT_DIR_ENV_PLACEHOLDER).join(".claude").join("scripts");

    let mut settings = load_settings(&settings_path)?;
    let action = patch_hooks(&mut settings, &scripts_dir_template, opts.force, /*project=*/ true)?;
    say(opts.quiet, &format!("✓ project hooks {}", action));
    say(opts.quiet, &format!("  scripts: {}", scripts_dir_template.display()));
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
        bail!("--project-hooks 必須在 codeforge clone root 執行（缺 Cargo.toml）：{}", cwd.display());
    }
    let content = fs::read_to_string(&cargo_toml)
        .with_context(|| format!("讀取 {} 失敗", cargo_toml.display()))?;
    // Look for `name = "codeforge"` in the [package] section (simple substring is OK)
    if !content.contains("name = \"codeforge\"") {
        bail!("Cargo.toml 的 package name 不是 codeforge：{}", cargo_toml.display());
    }
    Ok(())
}

// ─── settings.json IO ─────────────────────────────────────────────────────

fn load_settings(path: &Path) -> Result<Value> {
    if path.exists() {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("讀取 {} 失敗", path.display()))?;
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

fn write_settings_atomic(path: &Path, settings: &Value) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let pretty = serde_json::to_string_pretty(settings)? + "\n";
    fs::write(&tmp, pretty).with_context(|| format!("寫入 {} 失敗", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("rename 到 {} 失敗", path.display()))
}

fn default_global_settings_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CODEFORGE_CLAUDE_SETTINGS") {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var("HOME").context("環境變數 HOME 未設定")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

fn default_data_local_dir() -> Result<PathBuf> {
    if let Some(p) = dirs::data_local_dir() {
        return Ok(p);
    }
    let home = std::env::var("HOME").context("HOME 未設定且無 data_local_dir")?;
    Ok(PathBuf::from(home).join(".local").join("share"))
}

// ─── statusLine block ─────────────────────────────────────────────────────

fn current_marker() -> String {
    format!("{}{}", MARKER_PREFIX, env!("CARGO_PKG_VERSION"))
}

fn statusline_is_codeforge(v: &Value) -> bool {
    v.get(MARKER_KEY)
        .and_then(|x| x.as_str())
        .is_some_and(|s| s.starts_with(MARKER_PREFIX))
}

fn patch_statusline(settings: &mut Value, cmd: &str, force: bool) -> Result<&'static str> {
    require_object_root(settings)?;
    let new_block = json!({
        "type": "command",
        "command": cmd,
        MARKER_KEY: current_marker(),
    });
    let obj = settings.as_object_mut().expect("checked");
    let prior = obj.get("statusLine").cloned();
    if let Some(p) = &prior {
        if !statusline_is_codeforge(p) && !force {
            bail!(
                "settings.json.statusLine 已被其他程式設定（不是 codeforge）；使用 --force 覆蓋"
            );
        }
    }
    obj.insert("statusLine".to_string(), new_block.clone());
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

/// The 2 global-safe scripts. Project-specific scripts
/// (check-improvements, check-dev-flow) are NOT installed by `--hooks`.
const HOOK_SCRIPTS: &[(&str, &str)] = &[
    ("emit-session.js", include_str!("../../.claude/scripts/emit-session.js")),
    ("session-digest.js", include_str!("../../.claude/scripts/session-digest.js")),
];

/// Codeforge-clone-only DEV scripts (used by `--project-hooks`). These already
/// live in the codeforge clone at `.claude/scripts/`, so we only need the
/// names — no embedding required.
///
/// Note: emit-session.js / session-digest.js (and the dream→ship memory
/// pipeline) are product-wide and live in the global `--hooks` install path
/// (~/.claude/settings.json) — installing them into project settings too would
/// cause dual-fire. See commit 2648b34.
const PROJECT_HOOK_SCRIPT_NAMES: &[&str] = &[
    "check-improvements.js",
    "check-dev-flow.js",
];

fn extract_hook_scripts() -> Result<PathBuf> {
    let version = env!("CARGO_PKG_VERSION");
    let base = default_data_local_dir()?
        .join("codeforge")
        .join("hooks")
        .join(version);
    fs::create_dir_all(&base)
        .with_context(|| format!("建立 hooks 目錄 {} 失敗", base.display()))?;
    for (name, content) in HOOK_SCRIPTS {
        let dest = base.join(name);
        fs::write(&dest, content)
            .with_context(|| format!("寫入 {} 失敗", dest.display()))?;
    }
    Ok(base)
}

fn patch_hooks(
    settings: &mut Value,
    scripts_dir: &Path,
    force: bool,
    project: bool,
) -> Result<&'static str> {
    require_object_root(settings)?;
    let marker = current_marker();

    let emit_path = scripts_dir.join("emit-session.js");
    let digest_path = scripts_dir.join("session-digest.js");

    // Build the entries we want to insert. The two install modes carry
    // strictly disjoint hooks to avoid dual-fire when both are installed
    // (commit 2648b34):
    //
    //   --hooks         (global ~/.claude/settings.json):
    //       product-wide hooks that should run in EVERY project —
    //       emit-session, session-digest; the SessionStart local-recall injector
    //       (`codeforge memory context --hook`); and the memory pipeline
    //       `codeforge dream --quiet` → `codeforge ship --no-hook` SessionEnd chain.
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
    let entries: Vec<(&str, Option<&str>, Vec<Value>)> = if project {
        let check_improvements = scripts_dir.join("check-improvements.js");
        let check_dev_flow = scripts_dir.join("check-dev-flow.js");
        vec![
            ("SessionStart", None, vec![
                hook_entry(&format!("node {}", check_improvements.display()), 10000, &marker),
            ]),
            ("PreToolUse", Some("Edit|Write|Bash"), vec![
                hook_entry(&format!("node {}", check_dev_flow.display()), 5000, &marker),
            ]),
        ]
    } else {
        vec![
            ("SessionStart", None, vec![
                hook_entry(
                    &format!("node {} session_start", emit_path.display()),
                    3000,
                    &marker,
                ),
                // Local recall (no-mnemos READ path): inject a lean ranked L1
                // index as additionalContext. No-op when the project has no
                // active L1. Symmetric to the mnemos-cli context central path.
                hook_entry("codeforge memory context --hook 2>/dev/null || true", 10000, &marker),
            ]),
            ("SessionEnd", None, vec![
                hook_entry(&format!("node {} session_end", emit_path.display()), 3000, &marker),
                hook_entry(&format!("node {}", digest_path.display()), 30000, &marker),
                // Memory pipeline: dream distills L0→L1, ship forwards to Mnemos.
                // Order matters — ship reads the L1 that dream just produced.
                hook_entry("codeforge dream --quiet 2>/dev/null || true", 30000, &marker),
                hook_entry("codeforge ship --no-hook 2>/dev/null || true", 30000, &marker),
            ]),
            ("PreCompact", None, vec![hook_entry(
                &format!("node {}", digest_path.display()),
                30000,
                &marker,
            )]),
        ]
    };

    let obj = settings.as_object_mut().expect("checked");
    let hooks_root = obj
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}));
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

/// Set `cleanupPeriodDays` on global settings unless the user already chose a
/// value (then leave it, unless `force`). Returns a human action string.
fn patch_cleanup_period(settings: &mut Value, force: bool) -> &'static str {
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
    obj.insert(
        "cleanupPeriodDays".to_string(),
        json!(DEFAULT_CLEANUP_PERIOD_DAYS),
    );
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
    if cmd.starts_with("codeforge dream") {
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
    fn statusline_refuses_user_owned_without_force() {
        // User has their own statusLine (no marker)
        let mut s = json!({"statusLine": {"type": "command", "command": "/user/script"}});
        let err = patch_statusline(&mut s, "/new statusline", false).unwrap_err();
        assert!(err.to_string().contains("force"));
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
        let dir = PathBuf::from("/tmp/cf-test");
        let action = patch_hooks(&mut s, &dir, false, false).unwrap();
        assert_eq!(action, "新增");
        assert!(s["hooks"]["SessionStart"].is_array());
        assert!(s["hooks"]["SessionEnd"].is_array());
        assert!(s["hooks"]["PreCompact"].is_array());
        // Global SessionStart: emit-session + local-recall injector (2).
        let session_start = &s["hooks"]["SessionStart"][0]["hooks"];
        assert_eq!(session_start.as_array().unwrap().len(), 2);
        let ss_cmds = serde_json::to_string(session_start).unwrap();
        assert!(ss_cmds.contains("emit-session"), "missing emit-session");
        assert!(ss_cmds.contains("codeforge memory context --hook"), "missing local-recall injector");
        // Global SessionEnd chain: emit-session, session-digest, dream, ship (4).
        let session_end = &s["hooks"]["SessionEnd"][0]["hooks"];
        assert_eq!(session_end.as_array().unwrap().len(), 4);
        let cmds = serde_json::to_string(session_end).unwrap();
        assert!(cmds.contains("emit-session"), "missing emit-session");
        assert!(cmds.contains("session-digest"), "missing session-digest");
        assert!(cmds.contains("codeforge dream --quiet"), "missing dream");
        assert!(cmds.contains("codeforge ship --no-hook"), "missing ship");
        // dream must come before ship (ship reads the L1 dream produces).
        let se = session_end.as_array().unwrap();
        let dream_idx = se.iter().position(|h| h["command"].as_str().unwrap().contains("dream")).unwrap();
        let ship_idx = se.iter().position(|h| h["command"].as_str().unwrap().contains("ship")).unwrap();
        assert!(dream_idx < ship_idx, "dream must precede ship");
    }

    #[test]
    fn patch_hooks_project_layout_is_codeforge_clone_only() {
        let mut s = json!({});
        let dir = PathBuf::from("/tmp/cf-test");
        patch_hooks(&mut s, &dir, false, /*project=*/ true).unwrap();
        // 2 hook types present: SessionStart + PreToolUse. dream/ship moved to
        // the global path, so project mode no longer writes SessionEnd; PreCompact
        // is global-only too.
        assert!(s["hooks"]["SessionStart"].is_array());
        assert!(s["hooks"]["PreToolUse"].is_array());
        assert!(s["hooks"].get("SessionEnd").is_none(), "dream/ship are global-only now");
        assert!(s["hooks"].get("PreCompact").is_none(), "PreCompact must be global-only");
        // PreToolUse has the Edit|Write|Bash matcher
        assert_eq!(s["hooks"]["PreToolUse"][0]["matcher"], "Edit|Write|Bash");
        // Each present hook type has exactly 1 entry (the codeforge-clone-only one)
        assert_eq!(s["hooks"]["SessionStart"][0]["hooks"].as_array().unwrap().len(), 1);
        assert_eq!(s["hooks"]["PreToolUse"][0]["hooks"].as_array().unwrap().len(), 1);
        // SessionStart is check-improvements
        let ss_cmd = s["hooks"]["SessionStart"][0]["hooks"][0]["command"].as_str().unwrap();
        assert!(ss_cmd.contains("check-improvements.js"), "got: {}", ss_cmd);
        // No product-wide hooks leaked into project hooks (dual-fire prevention)
        let all_commands = serde_json::to_string(&s["hooks"]).unwrap();
        assert!(!all_commands.contains("emit-session"), "emit-session leaked into project hooks");
        assert!(!all_commands.contains("session-digest"), "session-digest leaked into project hooks");
        assert!(!all_commands.contains("codeforge dream"), "dream leaked into project hooks");
        assert!(!all_commands.contains("codeforge ship"), "ship leaked into project hooks");
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
        let action = patch_hooks(&mut s, Path::new("/tmp/cf-test"), false, /*project=*/ true)
            .unwrap();
        assert_eq!(action, "更新", "legacy dream entry recognized as ours and swept");
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
        // Not ours — must not clobber.
        assert!(!is_legacy_codeforge_command("node /home/u/myscript.js"));
        assert!(!is_legacy_codeforge_command("prettier --write x.ts"));
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
        patch_hooks(&mut s, Path::new("/tmp/cf-test"), false, /*project=*/ false).unwrap();
        // Exactly one SessionEnd group, with the new 4-entry chain — no 0.0.3 dupes.
        let se = s["hooks"]["SessionEnd"].as_array().unwrap();
        assert_eq!(se.len(), 1, "old un-markered group must be swept, not kept as sibling");
        let inner = se[0]["hooks"].as_array().unwrap();
        assert_eq!(inner.len(), 4, "emit-session + session-digest + dream + ship");
        let all = serde_json::to_string(&s["hooks"]).unwrap();
        assert!(!all.contains("0.0.3"), "stale 0.0.3 entries must be gone: {all}");
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
        patch_hooks(&mut s, Path::new("/tmp/cf-test"), false, /*project=*/ false).unwrap();
        let se = s["hooks"]["SessionEnd"].as_array().unwrap();
        assert_eq!(se.len(), 1, "one codeforge group, legacy not left as a sibling");
        let inner = se[0]["hooks"].as_array().unwrap();
        // emit-session, session-digest, dream, ship — exactly one dream.
        let dream_count = inner
            .iter()
            .filter(|h| h["command"].as_str().unwrap().contains("codeforge dream"))
            .count();
        assert_eq!(dream_count, 1, "no duplicate dream after relocation");
        assert!(inner.iter().any(|h| h["command"].as_str().unwrap().contains("codeforge ship")));
    }

    #[test]
    fn cleanup_period_fills_when_unset() {
        let mut s = json!({});
        let action = patch_cleanup_period(&mut s, false);
        assert_eq!(action, "新增");
        assert_eq!(s["cleanupPeriodDays"], json!(DEFAULT_CLEANUP_PERIOD_DAYS));
    }

    #[test]
    fn cleanup_period_respects_existing_user_value() {
        let mut s = json!({ "cleanupPeriodDays": 90 });
        let action = patch_cleanup_period(&mut s, false);
        assert_eq!(action, "已設定（保留現值）");
        assert_eq!(s["cleanupPeriodDays"], json!(90), "must not clobber user's value");
    }

    #[test]
    fn cleanup_period_force_overwrites() {
        let mut s = json!({ "cleanupPeriodDays": 90 });
        let action = patch_cleanup_period(&mut s, true);
        assert_eq!(action, "重置（--force）");
        assert_eq!(s["cleanupPeriodDays"], json!(DEFAULT_CLEANUP_PERIOD_DAYS));
    }

    #[test]
    fn project_mode_uses_claude_project_dir_placeholder() {
        // Mimics run_install_project_hooks: pass the env-var template path,
        // not a real on-disk path. Verifies the commands written into
        // settings.json are portable (no per-machine absolute paths).
        let mut s = json!({});
        let template = Path::new(PROJECT_DIR_ENV_PLACEHOLDER).join(".claude").join("scripts");
        patch_hooks(&mut s, &template, false, /*project=*/ true).unwrap();
        let cmd = s["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(
            cmd.contains("${CLAUDE_PROJECT_DIR}/.claude/scripts/check-improvements.js"),
            "expected env-var placeholder in command, got: {}",
            cmd
        );
        // No absolute /home/, /Users/, or C:\ paths should leak
        assert!(!cmd.contains("/home/"), "unexpected absolute /home/ path: {}", cmd);
        assert!(!cmd.contains("/Users/"), "unexpected absolute /Users/ path: {}", cmd);
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
        patch_hooks(&mut s, Path::new("/tmp"), false, false).unwrap();
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
        let action = patch_hooks(&mut s, Path::new("/tmp"), /*force=*/ true, false).unwrap();
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
        patch_hooks(&mut s, Path::new("/tmp/v1"), false, false).unwrap();
        let len_after_first = s["hooks"]["SessionStart"].as_array().unwrap().len();
        let action = patch_hooks(&mut s, Path::new("/tmp/v2"), false, false).unwrap();
        assert_eq!(action, "更新");
        let len_after_second = s["hooks"]["SessionStart"].as_array().unwrap().len();
        assert_eq!(len_after_first, len_after_second);
    }

    #[test]
    fn unpatch_hooks_removes_only_codeforge() {
        let mut s = json!({});
        patch_hooks(&mut s, Path::new("/tmp"), false, false).unwrap();
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
        patch_hooks(&mut s, Path::new("/tmp"), false, false).unwrap();
        unpatch_hooks(&mut s).unwrap();
        // No user entries anywhere → the entire `hooks` key should be removed
        assert!(s.as_object().unwrap().get("hooks").is_none());
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
