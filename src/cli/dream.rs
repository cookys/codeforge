use crate::db;
use crate::dream::runner::DreamRunner;
use crate::dream::{compile, ingest_digests};
use anyhow::Result;

pub fn run(ctx: &db::Context, only: Option<&str>, quiet: bool, dry_run: bool) -> Result<()> {
    // dry-run 純讀預覽:不 ensure_initialized(不建目錄)、不寫 L0、不刪 digest、不產 L1。
    if dry_run {
        return run_dry_preview(ctx);
    }

    ctx.ensure_initialized()?;

    let ops: Vec<&str> = match only {
        Some(o) => vec![o],
        None => vec![
            "ingest-digests",
            "compile",
            "lint",
            "dedup",
            "absorb",
            "decay",
            "track",
        ],
    };

    let runner = DreamRunner::new(ctx, quiet);
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(runner.run(&ops))?;

    Ok(())
}

/// `dream --dry-run`:預覽 ingest(會吸哪些 signal)+ compile(會編出哪些 L1),全程不寫。
fn run_dry_preview(ctx: &db::Context) -> Result<()> {
    println!("⟳ dream --dry-run（預覽,不寫 L0、不刪 digest、不產 L1）\n");

    let signals = ingest_digests::preview_signals(ctx)?;
    println!(
        "[ingest 預覽] 會吸入 {} 個 high-confidence signal:",
        signals.len()
    );
    for s in &signals {
        println!("  • {}", s.content.chars().take(100).collect::<String>());
    }
    if signals.is_empty() {
        println!("\n→ 無可吸入 signal,無 L1 可萃。");
        return Ok(());
    }

    println!("\n[compile 預覽] 跑 LLM 看會編出什麼 L1（不寫檔）…");
    let rt = tokio::runtime::Runtime::new()?;
    let (created, skipped, failed) = rt.block_on(compile::run_dry(ctx, &signals))?;
    println!(
        "\n→ 預覽結果:{created} 條會升 L1、{skipped} 低品質跳過、{failed} 失敗。（實際未寫入任何東西）"
    );
    Ok(())
}
