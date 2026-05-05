use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};
/// 通用 Markdown / 文字檔案匯入器
use anyhow::Result;
use std::path::Path;

pub fn ingest(ctx: &db::Context, path: &Path) -> Result<usize> {
    let content = std::fs::read_to_string(path)?;
    let writer = SignalWriter::new(ctx);
    let mut count = 0;

    // 按 ## 標題分割成多個 signal
    let sections: Vec<&str> = content.split("\n## ").collect();

    for (i, section) in sections.iter().enumerate() {
        let text = if i == 0 {
            section.trim()
        } else {
            &format!("## {}", section.trim())
        };
        if text.len() < 20 {
            continue;
        }

        let signal = Signal::new(
            text.to_string(),
            SignalSource::Ingest {
                path: path.display().to_string(),
            },
        );
        writer.append(&signal)?;
        count += 1;
    }

    // 若沒有分段，整個檔案作為一個 signal
    if count == 0 && content.len() >= 20 {
        let signal = Signal::new(
            content.trim().to_string(),
            SignalSource::Ingest {
                path: path.display().to_string(),
            },
        );
        writer.append(&signal)?;
        count = 1;
    }

    Ok(count)
}
