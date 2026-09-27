//! Инлайн Lua-скриптинг (mlua, lua54).

pub mod api;
pub mod loader;
pub mod events;

pub use loader::ScriptEngine;

#[allow(unused_imports)]
pub use events::HookKind;
#[allow(unused_imports)]
pub use events::HookCall;
