use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};
use anyhow::Result;
use std::path::PathBuf;

pub fn run(
    ctx: &db::Context,
    text: Option<String>,
    paste: bool,
    file: Option<PathBuf>,
) -> Result<()> {
    ctx.ensure_initialized()?;

    let content = if let Some(t) = text {
        t
    } else if paste {
        read_clipboard()?
    } else if let Some(f) = file {
        std::fs::read_to_string(&f)?
    } else {
        // 從 stdin 讀取
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    };

    let content = content.trim().to_string();
    if content.is_empty() {
        anyhow::bail!("輸入內容不能為空");
    }

    let signal = Signal::new(content.clone(), SignalSource::Manual);
    let writer = SignalWriter::new(ctx);
    let signal_id = writer.append(&signal)?;

    // 給 pet 加 XP
    let preview: String = content.chars().take(80).collect();
    crate::pet::xp::award(ctx, "learn", 5, Some(&preview))?;

    println!("✓ 已記錄到 L0（signal id: {}）", signal_id);
    println!("  執行 `codeforge dream` 將此筆記編譯為 L1 知識條目");

    Ok(())
}

fn read_clipboard() -> Result<String> {
    // 嘗試常見剪貼簿工具
    for cmd in &[
        "xclip -selection clipboard -o",
        "xsel --clipboard --output",
        "pbpaste",
    ] {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if let Ok(out) = std::process::Command::new(parts[0])
            .args(&parts[1..])
            .output()
        {
            if out.status.success() {
                return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
            }
        }
    }
    anyhow::bail!("無法讀取剪貼簿，請安裝 xclip、xsel（Linux）或使用 pbpaste（macOS）")
}
