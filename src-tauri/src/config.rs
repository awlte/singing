use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MIN_BUFFER_SECS: u32 = 60;
pub const MAX_BUFFER_SECS: u32 = 3600;
pub const DEFAULT_BUFFER_SECS: u32 = 600;
pub const DEFAULT_PLAY_TAIL_SECS: u32 = 30;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub save_folder: PathBuf,
    pub buffer_secs: u32,
    pub play_tail_secs: u32,
    pub input_device: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            save_folder: default_save_folder(),
            buffer_secs: DEFAULT_BUFFER_SECS,
            play_tail_secs: DEFAULT_PLAY_TAIL_SECS,
            input_device: None,
        }
    }
}

pub fn default_save_folder() -> PathBuf {
    dirs::audio_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Captures")
}

pub fn load(path: &Path) -> Config {
    match std::fs::read_to_string(path) {
        Ok(s) => match serde_json::from_str::<Config>(&s) {
            Ok(mut c) => {
                c.buffer_secs = c.buffer_secs.clamp(MIN_BUFFER_SECS, MAX_BUFFER_SECS);
                c
            }
            Err(_) => Config::default(),
        },
        Err(_) => Config::default(),
    }
}

pub fn save(path: &Path, config: &Config) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let s = serde_json::to_string_pretty(config)?;
    std::fs::write(path, s)
}
