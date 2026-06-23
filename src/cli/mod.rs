use anyhow::Result;
use clap::{Parser, Subcommand};

mod adopt;
mod attach;
mod bootstrap;
mod commentary;
mod craft;
mod daemon;
mod dream;
mod emit;
mod ingest;
mod init;
mod install;
mod inventory;
mod learn;
mod mnemos_cli;
mod pet;
mod recall;
mod search;
mod selfupdate;
mod ship;
mod snapshot;
mod statusline;
mod strategy;
mod tui;
mod use_item;
mod world;

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
    /// 將 codeforge 寫進 ~/.claude/settings.json（statusLine + 可選 hooks）
    Install {
        /// 安裝 global hooks：emit-session、session-digest、SessionStart recall 注入（memory context）、SessionEnd 記憶 pipeline（dream → ship）
        #[arg(long)]
        hooks: bool,
        /// 同時安裝 statusLine 與 hooks（等同 --hooks 加 statusLine 預設）
        #[arg(long)]
        all: bool,
        /// 安裝 codeforge-clone-only dev hooks（2 個：check-improvements SessionStart、check-dev-flow PreToolUse）到 $CWD/.claude/settings.json；只能在 codeforge clone 內執行
        #[arg(long)]
        project_hooks: bool,
        /// 印出將要套用的 settings.json 內容與 extraction 計畫，不做任何寫入
        #[arg(long)]
        dry_run: bool,
        /// 清掉 hooks 內所有 entries（包含非 codeforge）再寫入 codeforge 的條目；需搭配 --yes
        #[arg(long)]
        force: bool,
        /// 跳過 --force 的確認提示
        #[arg(long)]
        yes: bool,
        /// 覆蓋 settings.json 路徑（預設 ~/.claude/settings.json 或 $CWD/.claude/settings.json for --project-hooks）
        #[arg(long, value_name = "PATH")]
        settings_path: Option<std::path::PathBuf>,
        /// 安靜模式：不輸出 stdout（exit code 仍會反映結果）
        #[arg(long)]
        quiet: bool,
    },
    /// 從最新 GitHub release 原地更新 codeforge 二進位（B20）
    SelfUpdate {
        /// 只檢查有沒有新版，不下載/替換
        #[arg(long)]
        check: bool,
        /// 安靜模式：不輸出 stdout（exit code 仍會反映結果）
        #[arg(long)]
        quiet: bool,
    },
    /// 一鍵多機部署：install --all + fmt pin toolchain + Mnemos opt-in 狀態（B14）
    Bootstrap {
        /// 預覽三步計畫，不做任何寫入
        #[arg(long)]
        dry_run: bool,
        /// 安靜模式：不輸出 stdout（exit code 仍會反映結果）
        #[arg(long)]
        quiet: bool,
    },
    /// 移除 codeforge 寫入的 settings.json 條目與 hooks scripts
    Uninstall {
        /// 只移除 statusLine（若指向 current_exe）
        #[arg(long)]
        statusline: bool,
        /// 只移除 hooks 與 extracted scripts
        #[arg(long)]
        hooks: bool,
        /// 覆蓋 settings.json 路徑
        #[arg(long, value_name = "PATH")]
        settings_path: Option<std::path::PathBuf>,
        /// 安靜模式
        #[arg(long)]
        quiet: bool,
    },
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
    /// 管理背景 daemon（tick loop）
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// 啟動 alt-screen TUI 面板（pet / local map / combat log）
    Tui,
    /// 在 tmux session 裡切出一個旁邊 pane 跑 TUI（需 `$TMUX`）
    Attach {
        /// Companion pane 寬度百分比（預設 30，範圍 20–70）
        #[arg(long)]
        size: Option<u16>,
    },
    /// 切換或查看戰鬥策略（aggressive / defensive / explorer / scholar）
    Strategy {
        /// 省略時顯示當前策略
        name: Option<String>,
    },
    /// Phase 3c AI Commentary — pet 垃圾話 / 感性評論系統
    Commentary {
        #[command(subcommand)]
        action: CommentaryAction,
    },
    /// 顯示世界地圖（5 村 + Nation 預留格；--refresh 重新掃描 L1）
    World {
        /// 重新掃描 L1 memory 更新 concept 分佈
        #[arg(long)]
        refresh: bool,
    },
    /// 印出可分享的 ASCII 月報卡（spec §3.6，本月戰績 + 世界地圖 + pet 身份）
    Snapshot {
        /// 自訂回顧天數（預設 30 天）
        #[arg(long)]
        days: Option<i64>,
    },
    /// 顯示 Inventory + 生效中 effects + 可用配方
    Inventory,
    /// 合成 item（`codeforge craft` 無參數會列出配方）
    Craft {
        /// 配方 / 產品名（大小寫不敏感；包含空白請用引號）
        name: Option<String>,
    },
    /// 使用 item 觸發主動效果（spec §3.5/§3.7）
    Use {
        /// Item 名稱（大小寫不敏感）
        name: String,
    },
    /// 把今日 L2 ledger digest 後送進 Mnemos（active-memory source）
    Ship {
        /// 指定日期 YYYY-MM-DD（預設今日 UTC）
        #[arg(long)]
        date: Option<String>,
        /// 印出完整 ledger envelope JSON，不送、不寫任何 state
        #[arg(long)]
        dry_run: bool,
        /// SessionEnd hook 模式：單次嘗試，失敗靜默落 ship-failed/，永不阻塞
        #[arg(long)]
        no_hook: bool,
        /// 只補送 ship-failed/ queue，不重新 digest 今日
        #[arg(long)]
        resend: bool,
    },
    /// Mnemos client：取 context 注入 / 回報 atom 引用（cite write-back）
    MnemosCli {
        #[command(subcommand)]
        action: MnemosCliAction,
    },
}

#[derive(Subcommand)]
pub enum MnemosCliAction {
    /// 取相關 atom，格式化成 markdown（供 SessionStart 注入）
    Context {
        /// 查詢主題（省略時從 git branch + 近期 commit 推導）
        #[arg(long)]
        topic: Option<String>,
        /// 最多取幾筆（預設由 Mnemos 端決定，通常 5）
        #[arg(long)]
        max: Option<usize>,
        /// 最高敏感度層級（public/work/private，預設 public）
        #[arg(long)]
        max_sensitivity: Option<String>,
        /// 附 top-3 相關 theme summary（粗粒度上下文，排在 atoms 前；舊 server 無此欄則靜默略過）
        #[arg(long)]
        with_themes: bool,
    },
    /// 回報某 atom 被引用（POST cite，bump citation_count）
    Cite {
        /// 被引用的 atom id
        atom_id: String,
        /// 引用到的字串片段（省略時用 atom_id）
        #[arg(long)]
        matched_text: Option<String>,
    },
    /// SessionEnd 啟發式：掃 transcript 比對 atom 標題 → 對命中者觸發 cite
    CiteDetect {
        /// transcript 檔路徑（Claude session jsonl 或純文字）
        transcript: std::path::PathBuf,
        /// 候選 atom 的查詢主題（省略時推導）
        #[arg(long)]
        topic: Option<String>,
        /// 取多少候選 atom 做比對（預設 20）
        #[arg(long)]
        max: Option<usize>,
    },
    /// 前景探測 Mnemos /health（spec §7.4）
    Probe {
        /// 印 stderr 詳情、不寫 liveness 快取（供手動 debug）
        #[arg(long)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
pub enum CommentaryAction {
    /// 啟用 commentary（settings.commentary_opt_in = 1）
    On,
    /// 關閉 commentary
    Off,
    /// 列出最近 N 條 commentary（預設 10）
    List {
        #[arg(short = 'n', long, default_value = "10")]
        n: u32,
    },
    /// 手動觸發一條 commentary（繞過 rate limit；用於驗證 pipeline）
    Test {
        /// 指定 kind（boss_kill/level_up/session_long/long_idle/zone_unlock/manual_test）
        /// 省略時預設 manual_test
        kind: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum DaemonAction {
    /// 前景啟動 daemon（systemd 建議用此；手動使用請搭配 nohup/tmux）
    Start,
    /// 發 SIGTERM 終止目前執行中的 daemon
    Stop,
    /// 顯示 daemon 狀態（pid / last_tick / pending events）
    Status,
    /// 安裝 systemd user unit 到 ~/.config/systemd/user/
    Install,
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
    /// 本地知識 recall：ranked lean index（供 SessionStart 注入；對稱於 mnemos-cli context，免 mnemos）
    Context {
        /// lean index token 預算（約值，超出截斷）
        #[arg(long, default_value_t = crate::memory::recall::DEFAULT_MAX_TOKENS)]
        max_tokens: usize,
        /// SessionStart hook 模式：輸出 additionalContext JSON（無 active L1 則不輸出）
        #[arg(long)]
        hook: bool,
    },
}

pub fn run(cli: Cli) -> Result<()> {
    let ctx = crate::db::Context::load()?;

    match cli.command {
        Commands::Init => init::run(&ctx),
        Commands::Install {
            hooks,
            all,
            project_hooks,
            dry_run,
            force,
            yes,
            settings_path,
            quiet,
        } => install::run(install::InstallOpts {
            hooks,
            all,
            project_hooks,
            dry_run,
            force,
            yes,
            settings_path,
            quiet,
        }),
        Commands::Uninstall {
            statusline,
            hooks,
            settings_path,
            quiet,
        } => install::run_uninstall(install::UninstallOpts {
            statusline,
            hooks,
            settings_path,
            quiet,
        }),
        Commands::SelfUpdate { check, quiet } => {
            selfupdate::run(selfupdate::SelfUpdateOpts { check, quiet })
        }
        Commands::Bootstrap { dry_run, quiet } => {
            bootstrap::run(bootstrap::BootstrapOpts { dry_run, quiet })
        }
        Commands::Learn { text, paste, file } => learn::run(&ctx, text, paste, file),
        Commands::Memory { action } => match action {
            MemoryAction::Search { query, limit } => search::run(&ctx, &query, limit),
            MemoryAction::Status => search::status(&ctx),
            MemoryAction::Context { max_tokens, hook } => recall::run(&ctx, max_tokens, hook),
        },
        Commands::Dream { only, quiet } => dream::run(&ctx, only.as_deref(), quiet),
        Commands::Ingest { path, source } => ingest::run(&ctx, &path, &source),
        Commands::Adopt => adopt::run(&ctx),
        Commands::Pet => pet::run(&ctx),
        Commands::Statusline => statusline::run(&ctx),
        Commands::Emit {
            event,
            fields,
            json,
        } => emit::run(&ctx, event, fields, json),
        Commands::Daemon { action } => daemon::run(
            &ctx,
            match action {
                DaemonAction::Start => daemon::DaemonCmd::Start,
                DaemonAction::Stop => daemon::DaemonCmd::Stop,
                DaemonAction::Status => daemon::DaemonCmd::Status,
                DaemonAction::Install => daemon::DaemonCmd::Install,
            },
        ),
        Commands::Tui => tui::run(&ctx),
        Commands::Attach { size } => attach::run(size),
        Commands::Strategy { name } => strategy::run(&ctx, name.as_deref()),
        Commands::Commentary { action } => commentary::run(
            &ctx,
            match action {
                CommentaryAction::On => commentary::CommentaryCmd::On,
                CommentaryAction::Off => commentary::CommentaryCmd::Off,
                CommentaryAction::List { n } => commentary::CommentaryCmd::List { n },
                CommentaryAction::Test { kind } => commentary::CommentaryCmd::Test { kind },
            },
        ),
        Commands::World { refresh } => world::run(&ctx, refresh),
        Commands::Snapshot { days } => snapshot::run(&ctx, days),
        Commands::Inventory => inventory::run(&ctx),
        Commands::Craft { name } => craft::run(&ctx, name),
        Commands::Use { name } => use_item::run(&ctx, name),
        Commands::Ship {
            date,
            dry_run,
            no_hook,
            resend,
        } => ship::run(
            &ctx,
            ship::ShipOpts {
                date,
                dry_run,
                no_hook,
                resend,
            },
        ),
        Commands::MnemosCli { action } => mnemos_cli::run(
            &ctx,
            match action {
                MnemosCliAction::Context {
                    topic,
                    max,
                    max_sensitivity,
                    with_themes,
                } => mnemos_cli::MnemosCliCmd::Context {
                    topic,
                    max,
                    max_sensitivity,
                    with_themes,
                },
                MnemosCliAction::Cite {
                    atom_id,
                    matched_text,
                } => mnemos_cli::MnemosCliCmd::Cite {
                    atom_id,
                    matched_text,
                },
                MnemosCliAction::CiteDetect {
                    transcript,
                    topic,
                    max,
                } => mnemos_cli::MnemosCliCmd::CiteDetect {
                    transcript,
                    topic,
                    max,
                },
                MnemosCliAction::Probe { verbose } => mnemos_cli::MnemosCliCmd::Probe { verbose },
            },
        ),
    }
}
