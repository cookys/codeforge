pub mod claude;
pub mod chatgpt;
pub mod markdown;

use anyhow::Result;
use std::path::Path;

/// 自動偵測匯入格式
pub fn detect_format(path: &Path) -> Result<String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "md" || ext == "markdown" || ext == "txt" {
        return Ok("markdown".to_string());
    }

    // 嘗試讀取 JSON 頭部判斷格式
    let content = std::fs::read_to_string(path)?;
    let content = content.trim();

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        // Claude.ai export 格式
        if v.get("conversations").is_some() || v.get("chat_messages").is_some() {
            return Ok("claude".to_string());
        }
        // ChatGPT export 格式
        if v.as_array().map(|a| a.first().and_then(|x| x.get("title")).is_some()).unwrap_or(false) {
            return Ok("chatgpt".to_string());
        }
    }

    anyhow::bail!("無法自動偵測格式，請指定 --source claude/chatgpt/markdown")
}
