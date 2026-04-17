mod cli;
mod daemon;
mod db;
mod memory;
mod dream;
mod import;
mod brain;
mod projection;
mod pet;
mod power;
mod tui;

use anyhow::Result;
use clap::Parser;

rust_i18n::i18n!("locales", fallback = "en");

fn detect_locale() -> String {
    std::env::var("CODEFORGE_LOCALE")
        .unwrap_or_else(|_| {
            sys_locale::get_locale()
                .unwrap_or_else(|| "en".to_string())
        })
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    rust_i18n::set_locale(&detect_locale());
    let cli = cli::Cli::parse();
    cli::run(cli)
}
