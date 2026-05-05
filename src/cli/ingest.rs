use crate::db;
use crate::import;
use anyhow::Result;
use std::path::Path;

pub fn run(ctx: &db::Context, path: &Path, source: &str) -> Result<()> {
    ctx.ensure_initialized()?;

    let format = if source == "auto" {
        import::detect_format(path)?
    } else {
        source.to_string()
    };

    println!("匯入 {} 格式：{}", format, path.display());

    let count = match format.as_str() {
        "claude" => import::claude::ingest(ctx, path)?,
        "chatgpt" => import::chatgpt::ingest(ctx, path)?,
        "markdown" => import::markdown::ingest(ctx, path)?,
        other => anyhow::bail!(
            "不支援的格式：{}（支援：claude/chatgpt/markdown/auto）",
            other
        ),
    };

    println!("✓ 匯入完成：{} 筆 signal 新增至 L0", count);
    println!("  執行 `codeforge dream` 將這些 signal 編譯為 L1 知識條目");

    // 匯入也給 XP（按筆數）
    crate::pet::xp::award(
        ctx,
        "ingest",
        count as i64 * 2,
        Some(&format!("ingest {}", path.display())),
    )?;

    Ok(())
}
