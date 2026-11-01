use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum GameMode {
    Singleplayer,
    Multiplayer,
}

#[derive(Debug, Clone, Parser)]
#[command(name = "client", about = "Backrooms Infinity game client")]
pub struct ClientArgs {
    #[arg(long, value_enum)]
    pub mode: GameMode,

    #[arg(long, default_value = "Wanderer")]
    pub nickname: String,

    /// Level id, extracted level directory, or local level ZIP archive.
    #[arg(long)]
    pub level: String,

    /// Local bi-lan WebSocket bridge. Required in multiplayer.
    #[arg(long, default_value = "ws://127.0.0.1:37610/client")]
    pub bridge_ws: String,

    #[arg(long)]
    pub data_dir: Option<PathBuf>,
}
