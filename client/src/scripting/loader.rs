//! ScriptEngine: загрузка инлайн-Lua из XML уровня и вызов хуков.
//!
//! Все скрипты делят ОДИН Lua-стейт и глобальную таблицу `api`, что
//! соответствует оригинальному формату BDS (script id="..." — это namespace,
//! но вызовы делаются по имени функции).

use anyhow::Result;
use mlua::Lua;
use std::collections::BTreeMap;

use super::api::{new_state, register_api, SharedState};

pub struct ScriptEngine {
    pub lua: Lua,
    pub state: SharedState,
    /// script_id → source code (для диагностики/повторного запуска).
    pub scripts: BTreeMap<String, String>,
}

impl ScriptEngine {
    /// Создаёт пустой движок и регистрирует таблицу `api`.
    pub fn new() -> Result<Self> {
        let lua = Lua::new();
        let state = new_state();
        register_api(&lua, state.clone())?;
        Ok(Self {
            lua,
            state,
            scripts: BTreeMap::new(),
        })
    }

    /// Загружает все скрипты уровня (id → код Lua). Каждый выполняется сразу,
    /// определяя глобальные функции (on_spawn, on_update и т.д.).
    pub fn load_scripts<'a, I>(&mut self, scripts: I) -> Result<()>
    where
        I: IntoIterator<Item = (&'a String, &'a String)>,
    {
        for (id, code) in scripts {
            // Запоминаем код
            self.scripts.insert(id.clone(), code.clone());
            // Оборачиваем в do ... end, чтобы не загрязнять окружение лишними
            // локальными, но с сохранением глобальных функций.
            let wrapped = format!("do\n{}\nend", code);
            self.lua
                .load(&wrapped)
                .set_name(id)
                .exec()
                .map_err(|e| anyhow::anyhow!("Lua compile/exec error in script '{id}': {e}"))?;
        }
        Ok(())
    }

    /// Вызывает хуковую функцию по имени с произвольным числом аргументов.
    /// Если функции нет — тихо ничего не делаем.
    pub fn call_hook(&self, func: &str, args: &[LuaValue]) -> Result<bool> {
        let globals = self.lua.globals();
        let f: mlua::Value = globals.get(func)?;
        match f {
            mlua::Value::Function(fun) => {
                let lua_args: Vec<mlua::Value> = args.iter().map(|v| v.to_lua(&self.lua)).collect::<Result<_, _>>()?;
                fun.call::<_, ()>(mlua::MultiValue::from_vec(lua_args))?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Удобный вызов хука без аргументов.
    pub fn call_hook0(&self, func: &str) -> Result<bool> {
        self.call_hook(func, &[])
    }

    /// Тикнуть таймеры: выполняет отложенные Lua-скрипты, время которых пришло.
    pub fn tick_timers(&self, dt: f32) -> Result<usize> {
        let bodies = {
            let mut s = self.state.lock().unwrap();
            s.timers.tick(dt)
        };
        let n = bodies.len();
        for b in bodies {
            self.lua
                .load(&b)
                .exec()
                .map_err(|e| anyhow::anyhow!("deferred Lua error: {e}"))?;
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

/// Обёртка для аргументов хука.
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
            "#
            .to_string(),
        )]);
        engine.load_scripts(scripts.iter()).unwrap();

        let called = engine
            .call_hook("on_spawn", &[LuaValue::Str("player_start".into())])
            .unwrap();
        assert!(called);
        let log = engine.read_log();
        assert_eq!(log, vec!["spawn player_start".to_string()]);
    }

    #[test]
    fn missing_hook_returns_false() {
        let engine = ScriptEngine::new().unwrap();
        let called = engine.call_hook0("nonexistent").unwrap();
        assert!(!called);
    }

    #[test]
    fn multiple_scripts_share_globals() {
        let mut engine = ScriptEngine::new().unwrap();
        let scripts = BTreeMap::from([
            (
                "a".to_string(),
                r#"function set_shared() shared = 42 end"#.to_string(),
            ),
            (
                "b".to_string(),
                r#"function read_shared() api.log("v=" .. tostring(shared)) end"#.to_string(),
            ),
        ]);
        engine.load_scripts(scripts.iter()).unwrap();
        engine.call_hook0("set_shared").unwrap();
        engine.call_hook0("read_shared").unwrap();
        assert_eq!(engine.read_log(), vec!["v=42".to_string()]);
    }

    #[test]
    fn timers_are_ticked() {
        let mut engine = ScriptEngine::new().unwrap();
        let scripts = BTreeMap::from([(
            "t".to_string(),
            r#"function setup() api.timer.after(0.5, "api.log('tick')") end"#.to_string(),
        )]);
        engine.load_scripts(scripts.iter()).unwrap();
        engine.call_hook0("setup").unwrap();

        assert_eq!(engine.tick_timers(0.1).unwrap(), 0);
        assert_eq!(engine.tick_timers(0.5).unwrap(), 1);
        assert_eq!(engine.read_log(), vec!["tick".to_string()]);
    }

    #[test]
    fn error_in_script_is_reported() {
        let mut engine = ScriptEngine::new().unwrap();
        let scripts = BTreeMap::from([(
            "bad".to_string(),
            "this is not valid lua !!!".to_string(),
        )]);
        let r = engine.load_scripts(scripts.iter());
        assert!(r.is_err());
    }
}