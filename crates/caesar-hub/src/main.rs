mod config;
mod server;
mod store;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::HubConfig;
use server::run_server;
use store::print_latest_snapshot;
use uriel_caesar_core::io::read_toml;

#[derive(Parser, Debug)]
#[command(author, version, about = "Caesar regional hub")]
struct Cli {
    #[arg(long, default_value = "configs/hub-dev.toml")]
    config: String,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Serve,
    Latest,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let settings: HubConfig = read_toml(&cli.config)?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => run_server(settings).await,
        Command::Latest => print_latest_snapshot(&settings).await,
    }
}
