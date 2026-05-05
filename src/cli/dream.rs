use crate::db;
use crate::dream::runner::DreamRunner;
use anyhow::Result;

pub fn run(ctx: &db::Context, only: Option<&str>, quiet: bool) -> Result<()> {
    ctx.ensure_initialized()?;

    let ops: Vec<&str> = match only {
        Some(o) => vec![o],
        None => vec!["compile", "lint", "dedup", "absorb", "decay", "track"],
    };

    let runner = DreamRunner::new(ctx, quiet);
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(runner.run(&ops))?;

    Ok(())
}
