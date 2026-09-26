//! Парсинг аргументов командной строки.
//! Форматы вызова:
//!   backrooms-client --mode single --level 0
//!   backrooms-client --mode host   --level 5
//!   backrooms-client --mode join   --ip 192.168.1.42

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Single,
    Host,
    Join,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_ok() {
        let cli = Cli::try_parse_from(["app", "--mode", "single", "--level", "0"]).unwrap();
        assert_eq!(cli.mode, Mode::Single);
        assert_eq!(cli.level, Some(0));
        cli.validate().unwrap();
    }

    #[test]
    fn parse_join_ok() {
        let cli = Cli::try_parse_from(["app", "--mode", "join", "--ip", "10.0.0.1"]).unwrap();
        assert_eq!(cli.mode, Mode::Join);
        assert_eq!(cli.ip.as_deref(), Some("10.0.0.1"));
        cli.validate().unwrap();
    }

    #[test]
    fn missing_level_fails_validate() {
        let cli = Cli::try_parse_from(["app", "--mode", "single"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn level_out_of_range_fails() {
        let cli = Cli::try_parse_from(["app", "--mode", "single", "--level", "9999"]).unwrap();
        assert!(cli.validate().is_err());
    }
}