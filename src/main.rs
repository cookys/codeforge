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
mod llm;
mod memory;
mod mnemos;
mod pet;
mod power;
mod snapshot;
mod tui;
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
    let cli = cli::Cli::parse();
    cli::run(cli)
}
