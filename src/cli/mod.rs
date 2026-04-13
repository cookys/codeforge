use anyhow::Result;
use clap::{Parser, Subcommand};

mod init;
mod learn;
mod search;
mod dream;
mod ingest;
mod adopt;
mod pet;
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
        Commands::Dream { only } => dream::run(&ctx, only.as_deref()),
        Commands::Ingest { path, source } => ingest::run(&ctx, &path, &source),
        Commands::Adopt => adopt::run(&ctx),
        Commands::Pet => pet::run(&ctx),
        Commands::Statusline => statusline::run(&ctx),
    }
}
