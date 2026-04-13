use anyhow::Result;
use crate::db;
use crate::dream::runner::DreamRunner;

pub fn run(ctx: &db::Context, only: Option<&str>) -> Result<()> {
    ctx.ensure_initialized()?;

    let ops: Vec<&str> = match only {
        Some(o) => vec![o],
        None => vec!["compile", "lint", "dedup", "absorb", "decay", "track"],
    };

    let runner = DreamRunner::new(ctx);
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(runner.run(&ops))?;

    Ok(())
}
