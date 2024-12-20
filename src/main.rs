use anyhow::Result;
use clap::Parser;
use tracing::Level;

mod api;
mod cli;
mod config;
mod service;
mod translation;

use cli::{Cli, Commands};
use config::Config;
use service::TranslationService;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => handle_init(),
        Commands::Push { path } => handle_push(path).await,
        Commands::Download { path } => handle_download(path).await,
    }
}

fn setup_logging() {
    #[cfg(debug_assertions)]
    let level = Level::DEBUG;
    #[cfg(not(debug_assertions))]
    let level = Level::INFO;

    tracing_subscriber::fmt().with_max_level(level).init();
}

fn handle_init() -> Result<()> {
    match Config::init() {
        Ok(()) => {
            tracing::info!("Configuration file created successfully");
            tracing::info!("Please update the configuration file with your settings");
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to create configuration file: {}", e);
            Err(e)
        }
    }
}

async fn handle_push(path: Option<String>) -> Result<()> {
    let config = Config::load()?;
    let service = TranslationService::new(config);
    service.push_translations(path).await
}

async fn handle_download(path: Option<String>) -> Result<()> {
    let config = Config::load()?;
    let service = TranslationService::new(config);
    service.download_translations(path).await
}
