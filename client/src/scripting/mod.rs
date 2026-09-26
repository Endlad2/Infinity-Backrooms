//! Инлайн Lua-скриптинг (mlua, lua54).
//! Скрипты хранятся прямо в XML уровня, здесь — движок и API для них.

pub mod api;
pub mod loader;
pub mod events;

pub use loader::ScriptEngine;
pub use events::{HookKind, HookCall};