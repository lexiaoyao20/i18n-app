use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new configuration file
    Init,

    /// Push translation files to the server
    Push {
        /// Path to the directory containing translation files
        #[arg(short, long)]
        path: Option<String>,
    },

    /// Download translation files from the server
    Download {
        /// Path to save the downloaded files
        #[arg(short, long)]
        path: Option<String>,
    },

    /// 更新到最新版本
    Update,
}
