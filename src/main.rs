use anyhow::Result;
use clap::Parser;
use tracing::Level;

mod api;
mod cli;
mod config;
mod service;
mod translation;
mod update;

use cli::{Cli, Commands};
use config::Config;
use service::TranslationService;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();
    let cli = Cli::parse();

    // 除了 update 命令外，其他命令都先检查更新
    if !matches!(cli.command, Commands::Update) {
        if let Some(release) = update::check_update().await? {
            tracing::info!(
                "发现新版本 {}，请运行 'i18n-app update' 进行更新",
                release.tag_name
            );
            tracing::info!("更新地址：{}", release.html_url);
            // 发现新版本时直接退出程序
            std::process::exit(1);
        }
    }

    match cli.command {
        Commands::Init => handle_init(),
        Commands::Push { path } => handle_push(path).await,
        Commands::Download { path } => handle_download(path).await,
        Commands::Update => {
            if update::update().await? {
                std::process::exit(0);
            }
            Ok(())
        }
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
