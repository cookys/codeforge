use crate::db;
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// `codeforge install` — wire Claude Code integration.
///
/// Default (no flags): patches `~/.claude/settings.json` with a
/// `statusLine` block pointing to the binary's absolute path.
///
/// `--hooks`: also installs global-safe Claude Code hooks
/// (`emit-session` + `session-digest`). Scripts are embedded into the
/// binary at compile time (`include_str!`) and extracted to
/// `~/.local/share/codeforge/hooks/<version>/` at install time. Hook
/// entries are tagged with `_installed_by: "codeforge@<version>"` so
/// re-running replaces our entries in place without disturbing
/// user-owned hooks.
///
/// `--all`: equivalent to default + `--hooks`.
pub fn run(_ctx: &db::Context, hooks: bool, all: bool) -> Result<()> {
    let install_statusline = !hooks || all;
    let install_hooks = hooks || all;

    let exe = std::env::current_exe().context("讀取 codeforge binary 路徑失敗")?;
    let exe_str = exe
        .to_str()
        .ok_or_else(|| anyhow!("binary 路徑含非 UTF-8 字元：{}", exe.display()))?
        .to_string();

    let settings_path = settings_json_path()?;
    println!("  目標 settings.json: {}", settings_path.display());

    let mut settings = load_settings(&settings_path)?;

    if install_statusline {
        let cmd = format!("{} statusline", exe_str);
        let action = patch_statusline(&mut settings, &cmd)?;
        println!("✓ statusLine {}", action);
        println!("  command: {}", cmd);
    }

    if install_hooks {
        let hooks_dir = extract_hook_scripts()?;
        let action = patch_hooks(&mut settings, &hooks_dir)?;
        println!("✓ hooks {}", action);
        println!("  scripts: {}", hooks_dir.display());
    }

    write_settings_atomic(&settings_path, &settings)?;

    println!();
    println!("生效方式：下個 user message 或 /clear 之後 Claude Code 會 pick up 新設定。");
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

fn settings_json_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CODEFORGE_CLAUDE_SETTINGS") {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var("HOME").context("環境變數 HOME 未設定")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

// ─── statusLine block ─────────────────────────────────────────────────────

fn patch_statusline(settings: &mut Value, cmd: &str) -> Result<&'static str> {
    require_object_root(settings)?;
    let new_block = json!({
        "type": "command",
        "command": cmd,
    });
    let obj = settings.as_object_mut().expect("checked");
    let prior = obj.get("statusLine").cloned();
    obj.insert("statusLine".to_string(), new_block.clone());
    Ok(match prior {
        None => "新增",
        Some(p) if p == new_block => "已是最新（無變動）",
        Some(_) => "更新",
    })
}

// ─── hooks block ──────────────────────────────────────────────────────────

const MARKER_KEY: &str = "_installed_by";
const MARKER_PREFIX: &str = "codeforge@";

/// The 2 global-safe scripts. Project-specific scripts
/// (check-improvements, check-dev-flow) belong in
/// `<codeforge>/.claude/settings.json` and are NOT installed by `--hooks`.
const HOOK_SCRIPTS: &[(&str, &str)] = &[
    ("emit-session.js", include_str!("../../.claude/scripts/emit-session.js")),
    ("session-digest.js", include_str!("../../.claude/scripts/session-digest.js")),
];

fn extract_hook_scripts() -> Result<PathBuf> {
    let version = env!("CARGO_PKG_VERSION");
    let base = data_local_dir()?
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

fn data_local_dir() -> Result<PathBuf> {
    if let Some(p) = dirs::data_local_dir() {
        return Ok(p);
    }
    // Fallback for unusual envs
    let home = std::env::var("HOME").context("HOME 未設定且無 data_local_dir")?;
    Ok(PathBuf::from(home).join(".local").join("share"))
}

fn patch_hooks(settings: &mut Value, hooks_dir: &Path) -> Result<&'static str> {
    require_object_root(settings)?;
    let version = env!("CARGO_PKG_VERSION");
    let marker = format!("{}{}", MARKER_PREFIX, version);

    let emit_path = hooks_dir.join("emit-session.js");
    let digest_path = hooks_dir.join("session-digest.js");

    let entries = [
        ("SessionStart", vec![hook_entry(&format!(
            "node {} session_start",
            emit_path.display()
        ), 3000, &marker)]),
        ("SessionEnd", vec![
            hook_entry(&format!("node {} session_end", emit_path.display()), 3000, &marker),
            hook_entry(&format!("node {}", digest_path.display()), 30000, &marker),
        ]),
        ("PreCompact", vec![hook_entry(&format!(
            "node {}",
            digest_path.display()
        ), 30000, &marker)]),
    ];

    let obj = settings.as_object_mut().expect("checked");
    let hooks_root = obj
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}));
    let hooks_root = hooks_root
        .as_object_mut()
        .ok_or_else(|| anyhow!("settings.hooks 不是 object"))?;

    let mut any_change = false;
    let mut had_prior = false;
    for (hook_type, our_entries) in entries.iter() {
        let arr = hooks_root
            .entry(hook_type.to_string())
            .or_insert_with(|| json!([]));
        let arr = arr
            .as_array_mut()
            .ok_or_else(|| anyhow!("settings.hooks.{} 不是 array", hook_type))?;
        // Drop any prior codeforge-tagged group
        let before = arr.len();
        arr.retain(|group| !group_is_codeforge(group));
        if arr.len() != before {
            had_prior = true;
        }
        // Append our group
        arr.push(json!({
            "matcher": "",
            "hooks": our_entries,
        }));
        any_change = true;
    }

    let action = match (had_prior, any_change) {
        (false, true) => "新增",
        (true, _) => "更新",
        (_, false) => "已是最新（無變動）",
    };
    Ok(action)
}

fn hook_entry(command: &str, timeout_ms: u64, marker: &str) -> Value {
    json!({
        "type": "command",
        "command": command,
        "timeout": timeout_ms,
        MARKER_KEY: marker,
    })
}

/// A "hook group" (Claude Code's `hooks` array entry) is codeforge-owned
/// when every entry in its inner `hooks` array carries our marker.
fn group_is_codeforge(group: &Value) -> bool {
    let Some(inner) = group.get("hooks").and_then(|h| h.as_array()) else {
        return false;
    };
    if inner.is_empty() {
        return false;
    }
    inner.iter().all(|h| {
        h.get(MARKER_KEY)
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.starts_with(MARKER_PREFIX))
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statusline_merge_into_empty() {
        let mut s = json!({});
        let action = patch_statusline(&mut s, "/abs/codeforge statusline").unwrap();
        assert_eq!(action, "新增");
        assert_eq!(s["statusLine"]["command"], "/abs/codeforge statusline");
    }

    #[test]
    fn statusline_merge_preserves_unrelated() {
        let mut s = json!({"theme": "auto"});
        patch_statusline(&mut s, "/x statusline").unwrap();
        assert_eq!(s["theme"], "auto");
        assert_eq!(s["statusLine"]["command"], "/x statusline");
    }

    #[test]
    fn statusline_idempotent() {
        let mut s = json!({});
        patch_statusline(&mut s, "/x statusline").unwrap();
        let action = patch_statusline(&mut s, "/x statusline").unwrap();
        assert_eq!(action, "已是最新（無變動）");
    }

    #[test]
    fn statusline_overwrites_different_cmd() {
        let mut s = json!({"statusLine": {"type": "command", "command": "/old"}});
        let action = patch_statusline(&mut s, "/new statusline").unwrap();
        assert_eq!(action, "更新");
    }

    #[test]
    fn statusline_rejects_array_root() {
        let mut s = json!(["arr"]);
        let err = patch_statusline(&mut s, "/x").unwrap_err();
        assert!(err.to_string().contains("array"));
    }

    #[test]
    fn group_is_codeforge_detects_marker() {
        let ours = json!({
            "matcher": "",
            "hooks": [{"type": "command", "command": "x", MARKER_KEY: "codeforge@0.0.1"}]
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
                {"type": "command", "command": "x", MARKER_KEY: "codeforge@0.0.1"},
                {"type": "command", "command": "y"},  // no marker → not ours
            ]
        });
        assert!(!group_is_codeforge(&mixed));

        let no_hooks = json!({"matcher": ""});
        assert!(!group_is_codeforge(&no_hooks));
    }

    #[test]
    fn patch_hooks_creates_block() {
        let mut s = json!({});
        let dir = PathBuf::from("/tmp/cf-test");
        let action = patch_hooks(&mut s, &dir).unwrap();
        assert_eq!(action, "新增");
        // Three hook types written
        assert!(s["hooks"]["SessionStart"].is_array());
        assert!(s["hooks"]["SessionEnd"].is_array());
        assert!(s["hooks"]["PreCompact"].is_array());
        // SessionEnd has 2 inner hooks (emit + digest)
        let session_end = &s["hooks"]["SessionEnd"][0]["hooks"];
        assert_eq!(session_end.as_array().unwrap().len(), 2);
        // Marker present
        assert!(s["hooks"]["SessionStart"][0]["hooks"][0][MARKER_KEY]
            .as_str()
            .unwrap()
            .starts_with("codeforge@"));
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
        patch_hooks(&mut s, Path::new("/tmp")).unwrap();
        // User's group still present (no marker → preserved)
        let session_start = s["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 2);
        assert_eq!(session_start[0]["hooks"][0]["command"], "user-script.sh");
        // Our group appended
        assert!(group_is_codeforge(&session_start[1]));
    }

    #[test]
    fn patch_hooks_replaces_own_prior_entries() {
        let mut s = json!({});
        patch_hooks(&mut s, Path::new("/tmp/v1")).unwrap();
        let len_after_first = s["hooks"]["SessionStart"].as_array().unwrap().len();
        // Re-run: prior codeforge entries should be replaced, not appended
        let action = patch_hooks(&mut s, Path::new("/tmp/v2")).unwrap();
        assert_eq!(action, "更新");
        let len_after_second = s["hooks"]["SessionStart"].as_array().unwrap().len();
        assert_eq!(
            len_after_first, len_after_second,
            "re-running --hooks should not accumulate duplicate entries"
        );
    }
}
