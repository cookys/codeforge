/// ChatGPT web export 匯入器
use anyhow::Result;
use std::path::Path;
use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};

pub fn ingest(ctx: &db::Context, path: &Path) -> Result<usize> {
    let content = std::fs::read_to_string(path)?;
    let conversations: Vec<serde_json::Value> = serde_json::from_str(&content)?;

    let writer = SignalWriter::new(ctx);
    let mut count = 0;

    for conv in &conversations {
        // ChatGPT 格式：conversation.mapping.{id}.message
        if let Some(mapping) = conv.get("mapping").and_then(|m| m.as_object()) {
            for node in mapping.values() {
                let msg = match node.get("message") {
                    Some(m) => m,
                    None => continue,
                };
                let role = msg.get("author")
                    .and_then(|a| a.get("role"))
                    .and_then(|r| r.as_str())
                    .unwrap_or("");

                if role != "assistant" { continue; }

                let text: String = msg
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                    .map(|parts| {
                        parts.iter()
                            .filter_map(|p| p.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                if text.len() < 50 { continue; }

                let signal = Signal::new(text, SignalSource::WebChatChatGpt);
                writer.append(&signal)?;
                count += 1;
            }
        }
    }

    Ok(count)
}
