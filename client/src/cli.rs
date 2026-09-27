//! Парсинг аргументов командной строки.
//! Форматы вызова:
//!   client --mode single --level 0
//!   client --mode host   --level 5
//!   client --mode join   --ip 192.168.1.42
//!
//! Дополнительно:
//!   --graphics ultra-low|low|medium|high|ultra-high
//!   --texture-quality 1k|2k|4k|8k|16k  (перебивает пресет)

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Single,
    Host,
    Join,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GraphicsPreset {
    UltraLow,
    Low,
    Medium,
    High,
    UltraHigh,
}

impl GraphicsPreset {
    pub fn default_texture_res(&self) -> &'static str {
        match self {
            GraphicsPreset::UltraHigh => "8k",
            GraphicsPreset::High => "4k",
            GraphicsPreset::Medium => "2k",
            GraphicsPreset::Low => "1k",
            GraphicsPreset::UltraLow => "1k",
        }
    }
}

#[derive(Debug, Parser, Clone)]
#[command(name = "backrooms-client", version, about = "Backrooms Infinity game client")]
pub struct Cli {
    #[arg(long, value_enum)]
    pub mode: Mode,

    #[arg(long)]
    pub level: Option<u32>,

    #[arg(long)]
    pub ip: Option<String>,

    #[arg(long)]
    pub notes: Option<String>,

    #[arg(long, value_enum, default_value_t = GraphicsPreset::UltraHigh)]
    pub graphics: GraphicsPreset,

    #[arg(long, value_enum)]
    pub texture_quality: Option<TextureQuality>,

    /// Порт HTTP-сервера для раздачи ассетов в мультиплеере (по умолчанию 27016).
    #[arg(long, default_value_t = 27016)]
    pub http_port: u16,

    /// UDP-порт лобби (по умолчанию 27015).
    #[arg(long, default_value_t = 27015)]
    pub net_port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TextureQuality {
    #[value(name = "1k")]
    K1,
    #[value(name = "2k")]
    K2,
    #[value(name = "4k")]
    K4,
    #[value(name = "8k")]
    K8,
    #[value(name = "16k")]
    K16,
}

impl TextureQuality {
    pub fn as_str(&self) -> &'static str {
        match self {
            TextureQuality::K1 => "1k",
            TextureQuality::K2 => "2k",
            TextureQuality::K4 => "4k",
            TextureQuality::K8 => "8k",
            TextureQuality::K16 => "16k",
        }
    }

    pub fn pixels(&self) -> u32 {
        match self {
            TextureQuality::K1 => 1024,
            TextureQuality::K2 => 2048,
            TextureQuality::K4 => 4096,
            TextureQuality::K8 => 8192,
            TextureQuality::K16 => 16384,
        }
    }
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        match self.mode {
            Mode::Single | Mode::Host => {
                if self.level.is_none() {
                    return Err("--level обязателен для режимов single/host".into());
                }
                if let Some(l) = self.level {
                    if l > 998 {
                        return Err("--level должен быть в диапазоне 0..998".into());
                    }
                }
            }
            Mode::Join => {
                if self.ip.as_deref().unwrap_or("").trim().is_empty() {
                    return Err("--ip обязателен для режима join".into());
                }
            }
        }
        Ok(())
    }

    pub fn resolved_texture_res(&self) -> &'static str {
        match self.texture_quality {
            Some(q) => q.as_str(),
            None => self.graphics.default_texture_res(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_ok() {
        let cli = Cli::try_parse_from(["app", "--mode", "single", "--level", "0"]).unwrap();
        assert_eq!(cli.mode, Mode::Single);
        cli.validate().unwrap();
    }

    #[test]
    fn default_graphics_ultra_high_8k() {
        let cli = Cli::try_parse_from(["app", "--mode", "single", "--level", "0"]).unwrap();
        assert_eq!(cli.resolved_texture_res(), "8k");
    }

    #[test]
    fn graphics_medium_gives_2k() {
        let cli = Cli::try_parse_from([
            "app", "--mode", "single", "--level", "0", "--graphics", "medium",
        ])
        .unwrap();
        assert_eq!(cli.resolved_texture_res(), "2k");
    }

    #[test]
    fn texture_quality_overrides_graphics() {
        let cli = Cli::try_parse_from([
            "app", "--mode", "single", "--level", "0",
            "--graphics", "ultra-high",
            "--texture-quality", "2k",
        ])
        .unwrap();
        assert_eq!(cli.resolved_texture_res(), "2k");
    }

    #[test]
    fn parse_16k_quality() {
        let cli = Cli::try_parse_from([
            "app", "--mode", "single", "--level", "0",
            "--texture-quality", "16k",
        ])
        .unwrap();
        assert_eq!(cli.resolved_texture_res(), "16k");
    }

    #[test]
    fn http_port_default() {
        let cli = Cli::try_parse_from(["app", "--mode", "single", "--level", "0"]).unwrap();
        assert_eq!(cli.http_port, 27016);
        assert_eq!(cli.net_port, 27015);
    }
}
