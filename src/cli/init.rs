use anyhow::{Context, Result};
use crate::db;

pub fn run(ctx: &db::Context) -> Result<()> {
    if ctx.is_initialized() {
        println!("✓ .codeforge/ 已存在於 {}", ctx.project_dir.display());
        return Ok(());
    }

    // 建立目錄結構
    let dirs = [
        ctx.project_dir.join("store").join("concepts"),
        ctx.project_dir.join("store").join("connections"),
        ctx.project_dir.join("store").join("qa"),
        ctx.project_dir.join("signals"),
        ctx.project_dir.join("projections"),
        ctx.brain_dir.join("episodes"),
        ctx.brain_dir.join("skills"),
        ctx.brain_dir.join("identity"),
    ];

    for dir in &dirs {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("建立目錄失敗：{}", dir.display()))?;
    }

    // 建立 log.md（人類可讀 signal 日誌）
    let log_path = ctx.project_dir.join("log.md");
    if !log_path.exists() {
        std::fs::write(&log_path, "# CodeForge Signal Log\n\n")?;
    }

    // 初始化 DB 並跑 schema
    let conn = ctx.open_db()?;
    db::migrations::run(&conn)?;

    println!("✓ CodeForge 初始化完成");
    println!("  專案記憶：{}", ctx.project_dir.display());
    println!("  個人 brain：{}", ctx.brain_dir.display());
    println!("  狀態 DB：{}", ctx.db_path.display());
    println!();
    println!("下一步：`codeforge learn \"你學到的東西\"` 開始記憶");

    Ok(())
}
