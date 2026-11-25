//! `codeforge lessons candidates`——知識管線段 1 的 CLI 接線。
//! 邏輯在 `src/lessons/`（唯讀 L0 掃描＋受限詞彙候選）。

use anyhow::Result;

use crate::db;

pub fn run(ctx: &db::Context, json: bool) -> Result<()> {
    crate::lessons::run(ctx, json)
}
