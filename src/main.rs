mod brain;
// clan — CodeForge consumer of CodePower's ClanContentProvider contract
// (Plan A Phase 2 skeleton, 2026-04-24). Not yet wired into pet::village;
// integration lands in Plan B Phase 6.
#[allow(dead_code)]
mod clan;
mod cli;
mod commentary;
mod craft;
mod daemon;
mod db;
mod dream;
mod import;
mod live;
mod llm;
mod memory;
mod mnemos;
mod pet;
mod platform;
mod power;
mod snapshot;
mod tui;
mod usage;
mod world;

use anyhow::Result;
use clap::Parser;

rust_i18n::i18n!("locales", fallback = "en");

fn detect_locale() -> String {
    std::env::var("CODEFORGE_LOCALE")
        .unwrap_or_else(|_| sys_locale::get_locale().unwrap_or_else(|| "en".to_string()))
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    rust_i18n::set_locale(&detect_locale());
    let cli = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        // `codeforge hook` runs inside Claude Code, where exit 2 means "blocking":
        // a malformed hook invocation must exit 1, not clap's usage-error 2.
        Err(e) if e.use_stderr() && std::env::args().nth(1).as_deref() == Some("hook") => {
            eprintln!(
                "codeforge hook：用法 codeforge hook <name> [args…]（可用：{}）",
                cli::hook::names().join(", ")
            );
            std::process::exit(1);
        }
        Err(e) => e.exit(),
    };
    cli::run(cli)
}
