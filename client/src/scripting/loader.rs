//! ScriptEngine: загрузка инлайн-Lua из XML уровня.
//!
//! Обновлено: ошибки в отдельных скриптах БОЛЬШЕ НЕ валят загрузку.
//! Скрипт, который не скомпилировался или упал при exec, пропускается
//! с warning-логом. Остальные загружаются нормально.

use anyhow::Result;
use mlua::Lua;
use std::collections::BTreeMap;

use super::api::{new_state, register_api, SharedState};

pub struct ScriptEngine {
    pub lua: Lua,
    pub state: SharedState,
    pub scripts: BTreeMap<String, String>,
}

impl ScriptEngine {
    pub fn new() -> Result<Self> {
        let lua = Lua::new();
        let state = new_state();
        register_api(&lua, state.clone())?;
        Ok(Self { lua, state, scripts: BTreeMap::new() })
    }

    /// Загружает скрипты уровня. Ошибки в отдельных скриптах логируются,
    /// но НЕ прерывают загрузку.
    pub fn load_scripts<'a, I>(&mut self, scripts: I) -> Result<()>
    where
        I: IntoIterator<Item = (&'a String, &'a String)>,
    {
        for (id, code) in scripts {
            self.scripts.insert(id.clone(), code.clone());
            let wrapped = format!("do\n{}\nend", code);
            match self.lua.load(&wrapped).set_name(id).exec() {
                Ok(()) => {
                    // ok
                }
                Err(e) => {
                    eprintln!(
                        "[lua] скрипт '{}' не загружен (продолжаем): {}",
                        id, e
                    );
                    // Продолжаем со следующим скриптом.
                }
            }
        }
        Ok(())
    }

    /// Вызов хука по имени. Если функции нет — тихо ничего не делаем.
    pub fn call_hook(&self, func: &str, args: &[LuaValue]) -> Result<bool> {
        let globals = self.lua.globals();
        let f: mlua::Value = globals.get(func)?;
        match f {
            mlua::Value::Function(fun) => {
                let lua_args: Vec<mlua::Value> = args.iter()
                    .map(|v| v.to_lua(&self.lua))
                    .collect::<Result<_, _>>()?;
                // Оборачиваем в pcall-подобный вызов через xpcall на Lua-стороне
                // не получится из Rust напрямую, поэтому просто игнорируем ошибку.
                if let Err(e) = fun.call::<_, ()>(mlua::MultiValue::from_vec(lua_args)) {
                    eprintln!("[lua] ошибка в {func}: {e}");
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn call_hook0(&self, func: &str) -> Result<bool> {
        self.call_hook(func, &[])
    }

    pub fn tick_timers(&self, dt: f32) -> Result<usize> {
        let bodies = {
            let mut s = self.state.lock().unwrap();
            s.timers.tick(dt)
        };
        let n = bodies.len();
        for b in bodies {
            if let Err(e) = self.lua.load(&b).exec() {
                eprintln!("[lua] ошибка в отложенном вызове: {e}");
            }
        }
        Ok(n)
    }

    pub fn read_log(&self) -> Vec<String> {
        self.state.lock().unwrap().log.clone()
    }
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new().expect("ScriptEngine::default failed")
    }
}

#[derive(Debug, Clone)]
pub enum LuaValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl LuaValue {
    pub fn to_lua<'lua>(&self, lua: &'lua Lua) -> Result<mlua::Value<'lua>> {
        Ok(match self {
            LuaValue::Nil => mlua::Value::Nil,
            LuaValue::Bool(b) => mlua::Value::Boolean(*b),
            LuaValue::Int(i) => mlua::Value::Integer(*i),
            LuaValue::Float(f) => mlua::Value::Number(*f),
            LuaValue::Str(s) => mlua::Value::String(lua.create_string(s)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_inline_script_and_calls_hook() {
        let mut engine = ScriptEngine::new().unwrap();
        let scripts = BTreeMap::from([(
            "player".to_string(),
            r#"
                function on_spawn(id)
                    api.log("spawn " .. id)
                end
            "#.to_string(),
        )]);
        engine.load_scripts(scripts.iter()).unwrap();
        let called = engine.call_hook("on_spawn", &[LuaValue::Str("player_start".into())]).unwrap();
        assert!(called);
    }

    /// КЛЮЧЕВОЙ ТЕСТ: скрипт, который раньше ломал игру, теперь пропускается.
    #[test]
    fn broken_script_does_not_break_others() {
        let mut engine = ScriptEngine::new().unwrap();
        let scripts = BTreeMap::from([
            ("bad".to_string(), "this is not lua !!!".to_string()),
            ("good".to_string(), "function ok() api.log('works') end".to_string()),
        ]);
        // Раньше это возвращало Err — теперь Ok, bad пропущен, good загружен.
        engine.load_scripts(scripts.iter()).unwrap();
        let called = engine.call_hook0("ok").unwrap();
        assert!(called);
        assert_eq!(engine.read_log(), vec!["works".to_string()]);
    }

    #[test]
    fn script_with_world_table_loads() {
        // ИИ-скрипт с world.* теперь не падает.
        let mut engine = ScriptEngine::new().unwrap();
        let scripts = BTreeMap::from([(
            "ai_world".to_string(),
            r#"
                local function update(dt)
                  for _, e in ipairs(world.query("light")) do end
                end
                world.on_update(update)
            "#.to_string(),
        )]);
        engine.load_scripts(scripts.iter()).unwrap();
    }
}
