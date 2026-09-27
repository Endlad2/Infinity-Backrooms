//! Парсинг аргументов командной строки.
//! Форматы вызова:
//!   client --mode single --level 0
//!   client --mode host   --level 5
//!   client --mode join   --ip 192.168.1.42
//!
//! Дополнительно:
//!   --graphics ultra-low|low|medium|high|ultra-high
//!     Пресет качества. ultra-high=8K, high=4K, medium=2K, low=1K, ultra-low=1K.
//!   --texture-quality 1k|2k|4k|8k|16k
//!     Явное переопределение разрешения текстур (перебивает пресет graphics).

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Single,
    Host,
    Join,
}

/// Пресет графики. Определяет разрешение текстур по умолчанию.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GraphicsPreset {
    UltraLow,
    Low,
    Medium,
    High,
    UltraHigh,
}

impl GraphicsPreset {
    /// Разрешение текстур по умолчанию для пресета.
    pub fn default_texture_res(&self) -> &'static str {
        match self {
            GraphicsPreset::UltraHigh => "8k",
            GraphicsPreset::High => "4k",
            GraphicsPreset::Medium => "2k",
            // low и ultra-low специально снижены до 1k.
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

    /// Номер уровня (0..998) для одиночной игры или хоста.
    #[arg(long)]
    pub level: Option<u32>,

    /// IP хоста для режима join.
    #[arg(long)]
    pub ip: Option<String>,

    /// Текст замечания от прошлой генерации (перегенерация уровня).
    #[arg(long)]
    pub notes: Option<String>,

    /// Пресет качества графики. По умолчанию — ultra-high.
    #[arg(long, value_enum, default_value_t = GraphicsPreset::UltraHigh)]
    pub graphics: GraphicsPreset,

    /// Разрешение текстур. Если не задано — берётся из пресета graphics.
    /// Если задано — перебивает пресет.
    #[arg(long, value_enum)]
    pub texture_quality: Option<TextureQuality>,
}

/// Разрешение текстур.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TextureQuality {
    #[value(name = "1k")]
    K1,
    #[value(name = "2k")]
    K2,
    #[arg(name = "4k")]
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

    /// Размер в пикселях (для справки / логов).
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
    /// Проверка логической согласованности аргументов.
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

    /// Итоговое разрешение текстур: явный --texture-quality либо из пресета.
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
    fn parse_join_ok() {
        let cli = Cli::try_parse_from(["app", "--mode", "join", "--ip", "10.0.0.1"]).unwrap();
        assert_eq!(cli.mode, Mode::Join);
        cli.validate().unwrap();
    }

    #[test]
    fn missing_level_fails_validate() {
        let cli = Cli::try_parse_from(["app", "--mode", "single"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn default_graphics_ultra_high_8k() {
        let cli = Cli::try_parse_from(["app", "--mode", "single", "--level", "0"]).unwrap();
        assert_eq!(cli.graphics, GraphicsPreset::UltraHigh);
        assert_eq!(cli.resolved_texture_res(), "8k");
    }

    #[test]
    fn graphics_medium_gives_2k() {
        let cli = Cli::try_parse_from([
            "app", "--mode", "single", "--level", "0", "--graphics", "medium",
        ])
        .unwrap();
        assert_eq!(cli.graphics, GraphicsPreset::Medium);
        assert_eq!(cli.resolved_texture_res(), "2k");
    }

    #[test]
    fn graphics_high_gives_4k() {
        let cli = Cli::try_parse_from([
            "app", "--mode", "single", "--level", "0", "--graphics", "high",
        ])
        .unwrap();
        assert_eq!(cli.resolved_texture_res(), "4k");
    }

    #[test]
    fn graphics_low_and_ultralow_give_1k() {
        let cli = Cli::try_parse_from([
            "app", "--mode", "single", "--level", "0", "--graphics", "low",
        ])
        .unwrap();
        assert_eq!(cli.resolved_texture_res(), "1k");

        let cli = Cli::try_parse_from([
            "app", "--mode", "single", "--level", "0", "--graphics", "ultra-low",
        ])
        .unwrap();
        assert_eq!(cli.resolved_texture_res(), "1k");
    }

    #[test]
    fn texture_quality_overrides_graphics() {
        let cli = Cli::try_parse_from([
            "app", "--mode", "single", "--level", "0",
            "--graphics", "ultra-high",
            "--texture-quality", "2k",
        ])
        .unwrap();
        assert_eq!(cli.graphics, GraphicsPreset::UltraHigh);
        assert_eq!(cli.resolved_texture_res(), "2k");
    }

    #[test]
    fn texture_quality_alone() {
        let cli = Cli::try_parse_from([
            "app", "--mode", "single", "--level", "0",
            "--texture-quality", "16k",
        ])
        .unwrap();
        assert_eq!(cli.resolved_texture_res(), "16k");
    }
}
