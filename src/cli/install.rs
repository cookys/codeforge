use crate::db;
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

/// `codeforge install` — patch `~/.claude/settings.json` to wire the
/// `statusLine` hook to this binary's absolute path.
///
/// Why: Claude Code hook commands must be on `$PATH` or use absolute
/// paths. `~/.cargo/bin` is often missing from non-interactive shells
/// (rustup `--no-modify-path`, or dotfiles without a cargo block).
/// Writing the absolute path here makes the statusline work regardless.
///
/// Preserves all other keys in `settings.json`. Overwrites only the
/// `statusLine` key. Creates parent dir and file if missing.
pub fn run(_ctx: &db::Context) -> Result<()> {
    let exe = std::env::current_exe().context("讀取 codeforge binary 路徑失敗")?;
    let exe_str = exe
        .to_str()
        .ok_or_else(|| anyhow!("binary 路徑含非 UTF-8 字元：{}", exe.display()))?;
    let cmd = format!("{} statusline", exe_str);

    let settings_path = settings_json_path()?;
    println!("  目標 settings.json: {}", settings_path.display());

    let existing: Value = if settings_path.exists() {
        let raw = fs::read_to_string(&settings_path)
            .with_context(|| format!("讀取 {} 失敗", settings_path.display()))?;
        if raw.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&raw).with_context(|| {
                format!("解析 {} 失敗（不是合法 JSON）", settings_path.display())
            })?
        }
    } else {
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("建立目錄 {} 失敗", parent.display())
            })?;
        }
        json!({})
    };

    let prior = existing
        .as_object()
        .and_then(|o| o.get("statusLine"))
        .cloned();
    let merged = merge_statusline(existing, &cmd)?;

    let action = match prior {
        None => "新增",
        Some(p) if p == merged["statusLine"] => "已是最新（無變動）",
        Some(_) => "更新",
    };

    // Atomic write: tmp → rename
    let tmp = settings_path.with_extension("json.tmp");
    let pretty = serde_json::to_string_pretty(&merged)? + "\n";
    fs::write(&tmp, pretty).with_context(|| format!("寫入 {} 失敗", tmp.display()))?;
    fs::rename(&tmp, &settings_path)
        .with_context(|| format!("rename 到 {} 失敗", settings_path.display()))?;

    println!("✓ statusLine {}", action);
    println!("  command: {}", cmd);
    println!();
    println!("生效方式：下個 user message 或 /clear 之後 Claude Code 會 pick up 新設定。");
    Ok(())
}

/// Pure-function JSON merge — keeps tests env-free.
fn merge_statusline(existing: Value, cmd: &str) -> Result<Value> {
    let mut root = match existing {
        Value::Object(_) => existing,
        Value::Null => json!({}),
        other => {
            return Err(anyhow!(
                "settings.json 根節點不是 object（type={})",
                value_type_name(&other)
            ));
        }
    };
    root.as_object_mut()
        .expect("checked above")
        .insert(
            "statusLine".to_string(),
            json!({
                "type": "command",
                "command": cmd,
            }),
        );
    Ok(root)
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

fn settings_json_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CODEFORGE_CLAUDE_SETTINGS") {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var("HOME").context("環境變數 HOME 未設定")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_into_empty_object() {
        let after = merge_statusline(json!({}), "/abs/codeforge statusline").unwrap();
        assert_eq!(after["statusLine"]["type"], "command");
        assert_eq!(after["statusLine"]["command"], "/abs/codeforge statusline");
    }

    #[test]
    fn merge_preserves_unrelated_keys() {
        let before = json!({"theme": "auto", "skipDangerousModePermissionPrompt": true});
        let after = merge_statusline(before, "/x/codeforge statusline").unwrap();
        assert_eq!(after["theme"], "auto");
        assert_eq!(after["skipDangerousModePermissionPrompt"], true);
        assert_eq!(after["statusLine"]["command"], "/x/codeforge statusline");
    }

    #[test]
    fn merge_overwrites_existing_statusline() {
        let before = json!({"statusLine": {"type": "command", "command": "/old"}});
        let after = merge_statusline(before, "/new statusline").unwrap();
        assert_eq!(after["statusLine"]["command"], "/new statusline");
    }

    #[test]
    fn merge_into_null_is_object() {
        let after = merge_statusline(Value::Null, "/x statusline").unwrap();
        assert!(after.is_object());
        assert_eq!(after["statusLine"]["type"], "command");
    }

    #[test]
    fn merge_rejects_non_object_root() {
        let err = merge_statusline(json!(["arr"]), "/x").unwrap_err();
        assert!(err.to_string().contains("array"));
    }
}
