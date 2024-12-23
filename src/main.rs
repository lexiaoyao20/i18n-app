use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, layer::Layer, prelude::*, registry::Registry, EnvFilter};

mod api;
mod cli;
mod config;
mod service;
mod translation;
mod update;

use cli::{Cli, Commands};
use config::Config;
use service::TranslationService;

fn setup_logging() -> Result<()> {
    // 创建日志文件
    let log_file = TranslationService::init_log_file()?;

    // 设置环境过滤器
    #[cfg(debug_assertions)]
    let stdout_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));
    #[cfg(not(debug_assertions))]
    let stdout_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    #[cfg(debug_assertions)]
    let file_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));
    #[cfg(not(debug_assertions))]
    let file_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // 设置日志订阅器
    let file_layer = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_ansi(false);

    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false);

    Registry::default()
        .with(stdout_layer.with_filter(stdout_filter))
        .with(file_layer.with_filter(file_filter))
        .init();

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging()?;
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
        Commands::Pull => handle_pull().await,
    }
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

async fn handle_pull() -> Result<()> {
    let config = Config::load()?;
    let service = TranslationService::new(config);
    service.sync_translations().await
}
