use anyhow::Result;
use std::time::Instant;
use crate::db;

pub struct DreamRunner<'a> {
    ctx: &'a db::Context,
}

impl<'a> DreamRunner<'a> {
    pub fn new(ctx: &'a db::Context) -> Self {
        Self { ctx }
    }

    pub async fn run(&self, ops: &[&str]) -> Result<()> {
        let start = Instant::now();
        let conn = self.ctx.open_db()?;
        db::migrations::run(&conn)?;

        println!("⟳ Dream cycle 開始...");

        let mut compiled = 0;
        let mut l1_created = 0;
        let mut l1_updated = 0;

        for op in ops {
            match *op {
                "compile" => {
                    print!("  compile  ");
                    let r = super::compile::run(self.ctx, &conn).await?;
                    compiled = r.signals_processed;
                    l1_created = r.l1_created;
                    l1_updated = r.l1_updated;
                    println!("✓ {} signals → {} 新增 {} 更新", compiled, l1_created, l1_updated);
                }
                "lint" => {
                    print!("  lint     ");
                    let r = super::lint::run(self.ctx)?;
                    println!("✓ {} 問題偵測", r.issues);
                }
                "dedup" => {
                    print!("  dedup    ");
                    let r = super::dedup::run(self.ctx)?;
                    println!("✓ {} 重複標記", r.marked);
                }
                "absorb" => {
                    print!("  absorb   ");
                    let r = super::absorb::run(self.ctx)?;
                    println!("✓ {} 條 Claude Code 記憶吸收", r.absorbed);
                }
                "decay" => {
                    print!("  decay    ");
                    let r = super::decay::run(self.ctx)?;
                    println!("✓ {} 條 strength 更新", r.updated);
                }
                "track" => {
                    print!("  track    ");
                    let r = super::track::run(self.ctx, &conn)?;
                    println!("✓ {} 個 skill 更新", r.skills_updated);
                }
                unknown => {
                    eprintln!("  警告：未知的 dream 操作 '{}'，跳過", unknown);
                }
            }
        }

        let elapsed_ms = start.elapsed().as_millis();

        // 記錄到 DB
        conn.execute(
            "INSERT INTO dream_runs (operations, signals_compiled, l1_created, l1_updated, duration_ms, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'completed')",
            rusqlite::params![
                serde_json::to_string(ops)?,
                compiled as i64,
                l1_created as i64,
                l1_updated as i64,
                elapsed_ms as i64,
            ],
        )?;

        println!("✓ Dream cycle 完成（{}ms）", elapsed_ms);

        Ok(())
    }
}
