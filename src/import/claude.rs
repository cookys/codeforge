use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};
/// Claude.ai web chat export 匯入器
/// 格式：{"conversations": [...]} 或直接 [{...}]
use anyhow::Result;
use std::path::Path;

pub fn ingest(ctx: &db::Context, path: &Path) -> Result<usize> {
    let content = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&content)?;

    let conversations = if let Some(arr) = v.get("conversations").and_then(|c| c.as_array()) {
        arr.clone()
    } else if let Some(arr) = v.as_array() {
        arr.clone()
    } else {
        anyhow::bail!("無法解析 Claude.ai export 格式")
    };

    let writer = SignalWriter::new(ctx);
    let mut count = 0;

    for conv in &conversations {
        let messages = conv
            .get("chat_messages")
            .or_else(|| conv.get("messages"))
            .and_then(|m| m.as_array());

        if let Some(msgs) = messages {
            for msg in msgs {
                let role = msg
                    .get("sender")
                    .or_else(|| msg.get("role"))
                    .and_then(|r| r.as_str())
                    .unwrap_or("");

                // 只取 assistant（Claude）的回應，品質較高
                if role != "assistant" {
                    continue;
                }

                let text = extract_message_text(msg);
                if text.len() < 50 {
                    continue;
                }

                let signal = Signal::new(text, SignalSource::WebChatClaude);
                writer.append(&signal)?;
                count += 1;
            }
        }
    }

    Ok(count)
}

fn extract_message_text(msg: &serde_json::Value) -> String {
    // 嘗試不同欄位名稱
    if let Some(text) = msg.get("text").and_then(|t| t.as_str()) {
        return text.to_string();
    }
    if let Some(content) = msg.get("content") {
        if let Some(text) = content.as_str() {
            return text.to_string();
        }
        if let Some(arr) = content.as_array() {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|p| {
                    p.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            return parts.join("\n");
        }
    }
    String::new()
}
