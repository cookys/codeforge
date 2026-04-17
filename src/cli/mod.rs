use anyhow::Result;
use clap::{Parser, Subcommand};

mod adopt;
mod dream;
mod emit;
mod ingest;
mod init;
mod learn;
mod pet;
mod search;
mod statusline;

#[derive(Parser)]
#[command(name = "codeforge", version, about = "Claude Code power-user toolkit")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 初始化 .codeforge/ 目錄結構
    Init,
    /// 手動新增知識條目
    Learn {
        /// 知識內容（省略時從 stdin 讀取）
        text: Option<String>,
        /// 從剪貼簿讀取
        #[arg(long)]
        paste: bool,
        /// 從檔案讀取
        #[arg(long, value_name = "FILE")]
        file: Option<std::path::PathBuf>,
    },
    /// 搜尋記憶知識庫
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// 執行 Dream cycle（compile/lint/dedup/absorb/decay/track）
    Dream {
        /// 只跑特定操作（compile/lint/dedup/absorb/decay/track）
        #[arg(long)]
        only: Option<String>,
        /// 靜默模式（session-end hook 使用，不輸出任何文字）
        #[arg(long)]
        quiet: bool,
    },
    /// 批量匯入知識來源（web chat export、markdown 檔）
    Ingest {
        /// 匯入檔案路徑
        path: std::path::PathBuf,
        /// 來源類型（claude/chatgpt/markdown/auto）
        #[arg(long, default_value = "auto")]
        source: String,
    },
    /// 選擇村落與本命寵物
    Adopt,
    /// 查看寵物狀態
    Pet,
    /// 輸出 statusline（Claude Code 呼叫）
    Statusline,
    /// 送一筆事件進 event_inbox（Claude Code hook 用）
    Emit {
        /// 事件名稱（如 session_start / file_saved / git_commit）
        event: Option<String>,
        /// 額外欄位，重複使用（--field sha=abc --field files=7）
        #[arg(short = 'f', long = "field", value_name = "KEY=VALUE")]
        fields: Vec<String>,
        /// 直接傳完整 JSON payload（與 event/fields 互斥）
        #[arg(long, conflicts_with_all = ["event", "fields"])]
        json: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum MemoryAction {
    /// 全文搜尋知識庫
    Search {
        query: String,
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// 顯示記憶系統狀態
    Status,
}

pub fn run(cli: Cli) -> Result<()> {
    let ctx = crate::db::Context::load()?;

    match cli.command {
        Commands::Init => init::run(&ctx),
        Commands::Learn { text, paste, file } => {
            learn::run(&ctx, text, paste, file)
        }
        Commands::Memory { action } => match action {
            MemoryAction::Search { query, limit } => search::run(&ctx, &query, limit),
            MemoryAction::Status => search::status(&ctx),
        },
        Commands::Dream { only, quiet } => dream::run(&ctx, only.as_deref(), quiet),
        Commands::Ingest { path, source } => ingest::run(&ctx, &path, &source),
        Commands::Adopt => adopt::run(&ctx),
        Commands::Pet => pet::run(&ctx),
        Commands::Statusline => statusline::run(&ctx),
        Commands::Emit { event, fields, json } => emit::run(&ctx, event, fields, json),
    }
}
