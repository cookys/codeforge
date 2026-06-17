use crate::db;
use anyhow::Result;
use std::time::Instant;

pub struct DreamRunner<'a> {
    ctx: &'a db::Context,
    quiet: bool,
}

impl<'a> DreamRunner<'a> {
    pub fn new(ctx: &'a db::Context, quiet: bool) -> Self {
        Self { ctx, quiet }
    }

    pub async fn run(&self, ops: &[&str]) -> Result<()> {
        let start = Instant::now();
        let conn = self.ctx.open_db()?;
        db::migrations::run(&conn)?;

        if !self.quiet {
            println!("⟳ Dream cycle 開始...");
        }

        let mut compiled = 0;
        let mut l1_created = 0;
        let mut l1_updated = 0;

        for op in ops {
            match *op {
                "ingest-digests" => {
                    let r = super::ingest_digests::run(self.ctx)?;
                    if !self.quiet {
                        println!(
                            "  ingest-digests ✓ {} signals 從 {} 個 session digest 吸入",
                            r.ingested, r.digests_processed
                        );
                    }
                }
                "compile" => {
                    let r = super::compile::run(self.ctx, &conn).await?;
                    compiled = r.signals_processed;
                    l1_created = r.l1_created;
                    l1_updated = r.l1_updated;
                    if !self.quiet {
                        println!(
                            "  compile  ✓ {} signals → {} 新增 {} 更新 {} 對賬略過",
                            compiled, l1_created, l1_updated, r.l1_noop
                        );
                    }
                }
                "lint" => {
                    let r = super::lint::run(self.ctx)?;
                    if !self.quiet {
                        println!("  lint     ✓ {} 問題偵測", r.issues);
                    }
                }
                "dedup" => {
                    let r = super::dedup::run(self.ctx)?;
                    if !self.quiet {
                        println!("  dedup    ✓ {} 重複標記", r.marked);
                    }
                }
                "absorb" => {
                    let r = super::absorb::run(self.ctx)?;
                    if !self.quiet {
                        println!("  absorb   ✓ {} 條 Claude Code 記憶吸收", r.absorbed);
                    }
                }
                "decay" => {
                    let r = super::decay::run(self.ctx)?;
                    if !self.quiet {
                        println!("  decay    ✓ {} 條 strength 更新", r.updated);
                    }
                }
                "track" => {
                    let r = super::track::run(self.ctx, &conn)?;
                    if !self.quiet {
                        println!("  track    ✓ {} 個 skill 更新", r.skills_updated);
                    }
                }
                unknown => {
                    if !self.quiet {
                        eprintln!("  警告：未知的 dream 操作 '{}'，跳過", unknown);
                    }
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

        if !self.quiet {
            println!("✓ Dream cycle 完成（{}ms）", elapsed_ms);
        }

        Ok(())
    }
}
