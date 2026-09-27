//! Lua API. Помимо таблицы `api` предоставляем глобальную `world` —
//! обёртку, которую часто генерирует ИИ. Многие функции пока no-op,
//! чтобы скрипт не падал, но и не ломал игру.

use std::collections::BTreeMap;

use anyhow::Result;
use mlua::{Lua, Table};

use super::events::DeferredQueue;

#[derive(Debug, Default)]
pub struct RuntimeState {
    pub player_pos: [f32; 3],
    pub player_hp: f32,
    pub inventory: BTreeMap<String, i32>,
    pub transforms: BTreeMap<String, [f32; 3]>,
    pub events: Vec<(String, String)>,
    pub timers: DeferredQueue,
    pub log: Vec<String>,
    pub spawned_chunks: Vec<(String, i32, i32, i32)>,
    pub chunk_events: Vec<(String, String, i32, i32, i32)>,
    pub chunk_size: [f32; 3],
    /// Зарегистрированные on_update колбэки (по имени функции).
    pub on_update_callbacks: Vec<String>,
}

pub type SharedState = std::sync::Arc<std::sync::Mutex<RuntimeState>>;

pub fn new_state() -> SharedState {
    let mut st = RuntimeState::default();
    st.chunk_size = [32.0, 16.0, 32.0];
    std::sync::Arc::new(std::sync::Mutex::new(st))
}

/// Регистрирует таблицы `api` и `world` в Lua-стейте.
pub fn register_api(lua: &Lua, state: SharedState) -> Result<Table> {
    let api = lua.create_table()?;

    // ---- api.log ----
    {
        let st = state.clone();
        let log = lua.create_function(move |_, msg: String| {
            st.lock().unwrap().log.push(msg);
            Ok(())
        })?;
        api.set("log", log)?;
    }

    // ---- api.random.range ----
    {
        let rnd = lua.create_table()?;
        let range = lua.create_function(|_, (a, b): (f64, f64)| {
            let x = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as u64).unwrap_or(0);
            let t = ((x as f64) / (u32::MAX as f64)).fract();
            Ok(a + (b - a) * t)
        })?;
        rnd.set("range", range)?;
        rnd.set("between", lua.create_function(|l, (a, b): (f64, f64)| {
            // Lua-style aliases
            let _ = l;
            Ok(a + (b - a) * 0.5)
        })?)?;
        rnd.set("float", lua.create_function(|_, ()| Ok(0.5_f64))?)?;
        rnd.set("integer", lua.create_function(|_, (_a, _b): (i64, i64)| Ok(0_i64))?)?;
        api.set("random", rnd)?;
    }

    // ---- api.vec3 ----
    {
        let vec3 = lua.create_table()?;
        let normalize = lua.create_function(|lua, t: Table| {
            let x: f64 = t.get("x").unwrap_or(0.0);
            let y: f64 = t.get("y").unwrap_or(0.0);
            let z: f64 = t.get("z").unwrap_or(0.0);
            let len = (x * x + y * y + z * z).sqrt().max(1e-6);
            let out = lua.create_table()?;
            out.set("x", x / len)?;
            out.set("y", y / len)?;
            out.set("z", z / len)?;
            Ok(out)
        })?;
        vec3.set("normalize", normalize)?;
        vec3.set("sub", lua.create_function(|lua, (_a, _b): (Table, Table)| {
            let out = lua.create_table()?;
            out.set("x", 0)?; out.set("y", 0)?; out.set("z", 0)?;
            Ok(out)
        })?)?;
        vec3.set("add", lua.create_function(|lua, (_a, _b): (Table, Table)| {
            let out = lua.create_table()?;
            out.set("x", 0)?; out.set("y", 0)?; out.set("z", 0)?;
            Ok(out)
        })?)?;
        api.set("vec3", vec3)?;
    }

    // ---- api.player ----
    {
        let p = lua.create_table()?;
        let st = state.clone();
        p.set("get_pos", lua.create_function(move |lua, ()| {
            let s = st.lock().unwrap();
            let t = lua.create_table()?;
            t.set("x", s.player_pos[0])?;
            t.set("y", s.player_pos[1])?;
            t.set("z", s.player_pos[2])?;
            Ok(t)
        })?)?;
        let st = state.clone();
        p.set("set_hp", lua.create_function(move |_, hp: f64| {
            st.lock().unwrap().player_hp = hp as f32;
            Ok(())
        })?)?;
        let st = state.clone();
        p.set("teleport", lua.create_function(move |_, (x, y, z): (f32, f32, f32)| {
            st.lock().unwrap().player_pos = [x, y, z];
            Ok(())
        })?)?;
        let st = state.clone();
        p.set("has_item", lua.create_function(move |_, item: String| {
            Ok(st.lock().unwrap().inventory.get(&item).copied().unwrap_or(0) > 0)
        })?)?;
        let st = state.clone();
        p.set("add_item", lua.create_function(move |_, (item, count): (String, i32)| {
            *st.lock().unwrap().inventory.entry(item).or_insert(0) += count;
            Ok(())
        })?)?;
        api.set("player", p)?;
    }

    // ---- api.transform ----
    {
        let t = lua.create_table()?;
        let st = state.clone();
        t.set("get", lua.create_function(move |lua, id: String| {
            let s = st.lock().unwrap();
            let pos = s.transforms.get(&id).copied().unwrap_or([0.0, 0.0, 0.0]);
            let out = lua.create_table()?;
            let p = lua.create_table()?;
            p.set("x", pos[0])?; p.set("y", pos[1])?; p.set("z", pos[2])?;
            out.set("pos", p)?;
            Ok(out)
        })?)?;
        let st = state.clone();
        t.set("set_pos", lua.create_function(move |_, (id, x, y, z): (String, f32, f32, f32)| {
            st.lock().unwrap().transforms.insert(id, [x, y, z]);
            Ok(())
        })?)?;
        t.set("set_rot", lua.create_function(|_, (_id, _x, _y, _z): (String, f32, f32, f32)| Ok(()))?)?;
        t.set("translate", lua.create_function(|_, (_id, _dx, _dy, _dz): (String, f32, f32, f32)| Ok(()))?)?;
        api.set("transform", t)?;
    }

    // ---- api.entity ----
    {
        let e = lua.create_table()?;
        let st = state.clone();
        e.set("damage", lua.create_function(move |_, (_id, amount): (String, f32)| {
            st.lock().unwrap().player_hp -= amount;
            Ok(())
        })?)?;
        e.set("spawn", lua.create_function(|_, (_prefab, _x, _y, _z): (String, f32, f32, f32)| {
            Ok(format!("spawned_{}", rand_id()))
        })?)?;
        e.set("despawn", lua.create_function(|_, _id: String| Ok(()))?)?;
        api.set("entity", e)?;
    }

    // ---- api.audio / api.anim / api.render ----
    {
        let st = state.clone();
        let a = lua.create_table()?;
        a.set("play", lua.create_function(move |_, (id, _opts): (String, Option<Table>)| {
            st.lock().unwrap().events.push(("audio".into(), id));
            Ok(())
        })?)?;
        a.set("stop", lua.create_function(|_, _id: String| Ok(()))?)?;
        api.set("audio", a)?;

        let an = lua.create_table()?;
        an.set("play", lua.create_function(|_, (_id, _clip): (String, String)| Ok(()))?)?;
        api.set("anim", an)?;

        let r = lua.create_table()?;
        r.set("set_emissive", lua.create_function(|_, (_id, _color, _strength): (String, String, f32)| Ok(()))?)?;
        api.set("render", r)?;
    }

    // ---- api.event.emit ----
    {
        let e = lua.create_table()?;
        let st = state.clone();
        e.set("emit", lua.create_function(move |_, (name, payload): (String, String)| {
            st.lock().unwrap().events.push((name, payload));
            Ok(())
        })?)?;
        api.set("event", e)?;
    }

    // ---- api.timer.after ----
    {
        let t = lua.create_table()?;
        let st = state.clone();
        t.set("after", lua.create_function(move |_, (sec, body): (f32, String)| {
            st.lock().unwrap().timers.push(super::events::DeferredCall {
                remaining: sec,
                lua_body: body,
            });
            Ok(())
        })?)?;
        api.set("timer", t)?;
    }

    // ---- api.chunk ----
    {
        let c = lua.create_table()?;

        let st = state.clone();
        c.set("spawn", lua.create_function(move |_, (id, gx, gy, gz): (String, i32, i32, i32)| {
            let mut s = st.lock().unwrap();
            s.spawned_chunks.push((id.clone(), gx, gy, gz));
            s.chunk_events.push(("generated".into(), id.clone(), gx, gy, gz));
            Ok(())
        })?)?;

        let st = state.clone();
        c.set("despawn", lua.create_function(move |_, (gx, gy, gz): (i32, i32, i32)| {
            let mut s = st.lock().unwrap();
            let removed: Vec<(String, i32, i32, i32)> = s.spawned_chunks.iter()
                .filter(|(_, x, y, z)| *x == gx && *y == gy && *z == gz)
                .cloned().collect();
            s.spawned_chunks.retain(|(_, x, y, z)| !(*x == gx && *y == gy && *z == gz));
            for (id, x, y, z) in removed {
                s.chunk_events.push(("unloaded".into(), id, x, y, z));
            }
            Ok(())
        })?)?;

        let st = state.clone();
        c.set("on_generated", lua.create_function(move |_, cb: String| {
            st.lock().unwrap().events.push(("chunk_cb".into(), cb));
            Ok(())
        })?)?;
        let st = state.clone();
        c.set("on_unloaded", lua.create_function(move |_, cb: String| {
            st.lock().unwrap().events.push(("chunk_cb_unload".into(), cb));
            Ok(())
        })?)?;
        let st = state.clone();
        c.set("get_size", lua.create_function(move |lua, ()| {
            let s = st.lock().unwrap();
            let t = lua.create_table()?;
            t.set("x", s.chunk_size[0])?;
            t.set("y", s.chunk_size[1])?;
            t.set("z", s.chunk_size[2])?;
            Ok(t)
        })?)?;
        api.set("chunk", c)?;
    }

    lua.globals().set("api", &api)?;

    // ============================================================
    // Global `world` — обёртка над api + безопасные no-op функции.
    // ИИ часто пишет: world.time, world.on_update(f), world.query(tag),
    // world.find(id), world.audio.play(...), world.spawn(...).
    // Без этой таблицы скрипты падают на первом же обращении.
    // ============================================================
    let world = lua.create_table()?;

    // world.time — число (0.0). Скрипты могут его читать.
    world.set("time", 0.0_f64)?;

    // world.on_update(f) — сохраняем имя функции (строка) в очередь.
    {
        let st = state.clone();
        world.set("on_update", lua.create_function(move |_, f: mlua::Value| {
            if let mlua::Value::Function(func) = f {
                // Пытаемся получить имя: часто это анонимная функция,
                // тогда сохраним маркер. Реальный вызов добавим позже.
                let info = format!("{:?}", func);
                let _ = info;
                st.lock().unwrap().on_update_callbacks.push("__anon__".into());
            } else if let mlua::Value::String(s) = f {
                let name = s.to_str().map(|x| x.to_string()).unwrap_or_default();
                st.lock().unwrap().on_update_callbacks.push(name);
            }
            Ok(())
        })?)?;
    }

    // world.query(tag) — пустой список. Скрипт не упадёт на for-in.
    world.set("query", lua.create_function(|lua, _tag: mlua::Value| {
        let t = lua.create_table()?;
        Ok(t)
    })?)?;

    // world.find(id) — nil (нет сущности).
    world.set("find", lua.create_function(|_, _id: String| Ok(mlua::Value::Nil))?)?;

    // world.spawn / world.despawn — no-op, возвращают фиктивный id.
    world.set("spawn", lua.create_function(|_, (_prefab, _x, _y, _z): (String, f32, f32, f32)| {
        Ok(format!("noop_{}", rand_id()))
    })?)?;
    world.set("despawn", lua.create_function(|_, _id: mlua::Value| Ok(()))?)?;

    // world.audio.play(...) / world.audio.stop(...).
    {
        let a = lua.create_table()?;
        let st = state.clone();
        a.set("play", lua.create_function(move |_, (id, _opts): (String, Option<Table>)| {
            st.lock().unwrap().events.push(("audio".into(), id));
            Ok(())
        })?)?;
        a.set("stop", lua.create_function(|_, _id: String| Ok(()))?)?;
        world.set("audio", a)?;
    }

    // world.log(...) — как api.log.
    {
        let st = state.clone();
        world.set("log", lua.create_function(move |_, msg: String| {
            st.lock().unwrap().log.push(msg);
            Ok(())
        })?)?;
    }

    // world.set_pos(id, x, y, z)
    {
        let st = state.clone();
        world.set("set_pos", lua.create_function(move |_, (id, x, y, z): (String, f32, f32, f32)| {
            st.lock().unwrap().transforms.insert(id, [x, y, z]);
            Ok(())
        })?)?;
    }

    // world.on_signal / world.on_level_start / world.on_level_end — те же
    // заглушки, что и on_update, но без сохранения (пока).
    for name in ["on_signal", "on_level_start", "on_level_end", "on_event"] {
        world.set(name, lua.create_function(|_, _f: mlua::Value| Ok(()))?)?;
    }

    lua.globals().set("world", &world)?;

    Ok(api)
}

fn rand_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lua_with_state() -> (Lua, SharedState) {
        let lua = Lua::new();
        let state = new_state();
        register_api(&lua, state.clone()).unwrap();
        (lua, state)
    }

    #[test]
    fn log_records_message() {
        let (lua, state) = lua_with_state();
        lua.load(r#"api.log("hello world")"#).exec().unwrap();
        assert_eq!(state.lock().unwrap().log, vec!["hello world".to_string()]);
    }

    #[test]
    fn world_time_readable() {
        let (lua, _state) = lua_with_state();
        let t: f64 = lua.load("return world.time").eval().unwrap();
        assert_eq!(t, 0.0);
    }

    #[test]
    fn world_query_returns_empty_table() {
        let (lua, _state) = lua_with_state();
        // Скрипт должен отработать без ошибок, даже если ничего нет.
        lua.load(r#"
            for _, e in ipairs(world.query("light")) do
                world.log("should not happen")
            end
        "#).exec().unwrap();
    }

    #[test]
    fn world_find_returns_nil() {
        let (lua, _state) = lua_with_state();
        let is_nil: bool = lua.load("return world.find('missing') == nil").eval().unwrap();
        assert!(is_nil);
    }

    #[test]
    fn world_on_update_registers_callback() {
        let (lua, state) = lua_with_state();
        lua.load(r#"
            local function tick(dt) return end
            world.on_update(tick)
        "#).exec().unwrap();
        assert_eq!(state.lock().unwrap().on_update_callbacks.len(), 1);
    }

    #[test]
    fn world_audio_play_does_not_crash() {
        let (lua, _state) = lua_with_state();
        lua.load(r#"world.audio.play("ambient_drip", { volume = 0.25, loop = false })"#)
            .exec().unwrap();
    }

    #[test]
    fn realistic_ai_script_does_not_crash() {
        // Скрипт, сгенерированный ИИ для уровня 7 — раньше падал.
        let (lua, _state) = lua_with_state();
        let script = r#"
            local function update_water(dt)
              local t = world.time
              for _, e in ipairs(world.query("volume")) do
                local p = e.transform.pos
                p.y = -7.4 + math.sin(t * 1.5 + p.x * 0.1) * 0.04
                e.transform.pos = p
              end
              for _, e in ipairs(world.query("puddle")) do
                local p = e.transform.pos
                p.y = -7.75 + math.sin(t * 0.8 + p.z * 0.2) * 0.015
                e.transform.pos = p
              end
            end

            local flicker_timer = 0
            local function flicker(dt)
              flicker_timer = flicker_timer + dt
              if flicker_timer < 0.08 then return end
              flicker_timer = 0
              for _, e in ipairs(world.query("light")) do
                if e.light and e.light.kind == "point" then
                  if math.random() < 0.08 then
                    e.light.intensity = 0.3 + math.random() * 0.5
                  end
                end
              end
            end

            local dread_timer = 0
            local function dread(dt)
              dread_timer = dread_timer + dt
              if dread_timer < 5.0 then return end
              dread_timer = 0
              local player = world.find("player_start")
              if not player then return end
              local depth = -player.transform.pos.y + 7.9
              if depth > 4.0 then
                world.audio.play("ambient_drip", { volume = 0.25, loop = false })
              end
            end

            world.on_update(update_water)
            world.on_update(flicker)
            world.on_update(dread)
        "#;
        lua.load(script).exec().unwrap();
    }
}
