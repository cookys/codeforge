mod cli;
mod db;
mod memory;
mod dream;
mod import;
mod brain;
mod projection;
mod pet;
mod power;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = cli::Cli::parse();
    cli::run(cli)
}
