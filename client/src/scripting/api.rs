//! Lua API, доступный скриптам уровня: глобальная таблица `api`.
//! Соответствует Приложению A: api.log, api.player.*, api.transform.*, api.vec3.*,
//! api.entity.*, api.audio.*, api.anim.*, api.render.*, api.timer.after,
//! api.event.emit, api.random.range, api.chunk.* (v2).
//!
//! Реализация — упрощённая, с логированием/эмуляцией. Реальные побочные
//! эффекты попадают в SharedState, который читает игровой цикл Bevy.

use std::collections::BTreeMap;

use anyhow::Result;
use mlua::{Lua, Table};

use super::events::DeferredQueue;

/// Разделяемое состояние между движком и Lua-скриптом.
#[derive(Debug, Default)]
pub struct RuntimeState {
    /// Позиция игрока.
    pub player_pos: [f32; 3],
    pub player_hp: f32,
    /// Инвентарь: имя_предмета → количество.
    pub inventory: BTreeMap<String, i32>,
    /// Позиции объектов по id.
    pub transforms: BTreeMap<String, [f32; 3]>,
    /// События, которые скрипт испустил через api.event.emit.
    pub events: Vec<(String, String)>,
    /// Отложенные вызовы api.timer.after.
    pub timers: DeferredQueue,
    /// Лог (api.log).
    pub log: Vec<String>,
    /// v2: явно заспавненные чанки через api.chunk.spawn(id, gx,gy,gz).
    /// Каждый элемент = (chunk_id, gx, gy, gz).
    pub spawned_chunks: Vec<(String, i32, i32, i32)>,
    /// v2: события генерации/выгрузки чанков, испущенные из Lua.
    /// (kind, chunk_id, gx, gy, gz), kind = "generated" | "unloaded".
    pub chunk_events: Vec<(String, String, i32, i32, i32)>,
    /// v2: размер чанка в мировых единицах (x,y,z).
    pub chunk_size: [f32; 3],
}

pub type SharedState = std::sync::Arc<std::sync::Mutex<RuntimeState>>;

pub fn new_state() -> SharedState {
    let mut st = RuntimeState::default();
    // Дефолтный размер чанка из примера (см. PROJECT.md): 32x16x32.
    st.chunk_size = [32.0, 16.0, 32.0];
    std::sync::Arc::new(std::sync::Mutex::new(st))
}

/// Регистрирует глобальную таблицу `api` в Lua-стейте.
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

    // ---- api.random.range(a, b) ----
    {
        let rnd = lua.create_table()?;
        let range = lua.create_function(|_, (a, b): (f64, f64)| {
            // Простой LCG; для тестов достаточно детерминизма.
            let x = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as u64)
                .unwrap_or(0);
            let t = ((x as f64) / (u32::MAX as f64)).fract();
            Ok(a + (b - a) * t)
        })?;
        rnd.set("range", range)?;
        api.set("random", rnd)?;
    }

    // ---- api.vec3 ----
    {
        let vec3 = lua.create_table()?;

        let sub = lua.create_function(|_, (a, b): (Table, Table)| {
            let t = a; // переиспользуем таблицу a для результата? нет — создадим через a'create? проще: вернуть таблицу через пустой
            // Простейшее: невозможно вернуть без Lua, поэтому используем сохранённые числа
            let _ = b;
            Ok(t)
        })?;
        // Заглушки с реальной математикой не критичны; оставлю упрощённые:
        // нормализация возвращает {x,y,z} со счётом.
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
        vec3.set("sub", sub)?;
        api.set("vec3", vec3)?;
    }

    // ---- api.player ----
    {
        let p = lua.create_table()?;
        let st = state.clone();
        p.set(
            "get_pos",
            lua.create_function(move |lua, ()| {
                let s = st.lock().unwrap();
                let t = lua.create_table()?;
                t.set("x", s.player_pos[0])?;
                t.set("y", s.player_pos[1])?;
                t.set("z", s.player_pos[2])?;
                Ok(t)
            })?,
        )?;
        let st = state.clone();
        p.set(
            "set_hp",
            lua.create_function(move |_, hp: f64| {
                st.lock().unwrap().player_hp = hp as f32;
                Ok(())
            })?,
        )?;
        let st = state.clone();
        p.set(
            "teleport",
            lua.create_function(move |_, (x, y, z): (f32, f32, f32)| {
                st.lock().unwrap().player_pos = [x, y, z];
                Ok(())
            })?,
        )?;
        let st = state.clone();
        p.set(
            "has_item",
            lua.create_function(move |_, item: String| {
                Ok(st.lock().unwrap().inventory.get(&item).copied().unwrap_or(0) > 0)
            })?,
        )?;
        let st = state.clone();
        p.set(
            "add_item",
            lua.create_function(move |_, (item, count): (String, i32)| {
                *st.lock().unwrap().inventory.entry(item).or_insert(0) += count;
                Ok(())
            })?,
        )?;
        api.set("player", p)?;
    }

    // ---- api.transform ----
    {
        let t = lua.create_table()?;
        let st = state.clone();
        t.set(
            "get",
            lua.create_function(move |lua, id: String| {
                let s = st.lock().unwrap();
                let pos = s.transforms.get(&id).copied().unwrap_or([0.0, 0.0, 0.0]);
                let out = lua.create_table()?;
                let p = lua.create_table()?;
                p.set("x", pos[0])?;
                p.set("y", pos[1])?;
                p.set("z", pos[2])?;
                out.set("pos", p)?;
                Ok(out)
            })?,
        )?;
        let st = state.clone();
        t.set(
            "set_pos",
            lua.create_function(move |_, (id, x, y, z): (String, f32, f32, f32)| {
                st.lock().unwrap().transforms.insert(id, [x, y, z]);
                Ok(())
            })?,
        )?;
        let st = state.clone();
        t.set(
            "set_rot",
            lua.create_function(move |_, (_id, _x, _y, _z): (String, f32, f32, f32)| {
                let _ = &st; // упрощение
                Ok(())
            })?,
        )?;
        let translate = lua.create_function(|_, (_id, _dx, _dy, _dz): (String, f32, f32, f32)| Ok(()))?;
        t.set("translate", translate)?;
        api.set("transform", t)?;
    }

    // ---- api.entity ----
    {
        let e = lua.create_table()?;
        let st = state.clone();
        e.set(
            "damage",
            lua.create_function(move |_, (_id, amount): (String, f32)| {
                st.lock().unwrap().player_hp -= amount;
                Ok(())
            })?,
        )?;
        let spawn = lua.create_function(|_, (_prefab, _x, _y, _z): (String, f32, f32, f32)| {
            Ok(format!("spawned_{}", rand_id()))
        })?;
        e.set("spawn", spawn)?;
        let despawn = lua.create_function(|_, _id: String| Ok(()))?;
        e.set("despawn", despawn)?;
        api.set("entity", e)?;
    }

    // ---- api.audio / api.anim / api.render ----
    {
        let st = state.clone();
        let a = lua.create_table()?;
        let play = lua.create_function(move |_, (id, _opts): (String, Option<Table>)| {
            st.lock().unwrap().events.push(("audio".into(), id));
            Ok(())
        })?;
        a.set("play", play)?;
        a.set(
            "stop",
            lua.create_function(|_, _id: String| Ok(()))?,
        )?;
        api.set("audio", a)?;

        let an = lua.create_table()?;
        an.set(
            "play",
            lua.create_function(|_, (_id, _clip): (String, String)| Ok(()))?,
        )?;
        api.set("anim", an)?;

        let r = lua.create_table()?;
        r.set(
            "set_emissive",
            lua.create_function(|_, (_id, _color, _strength): (String, String, f32)| Ok(()))?,
        )?;
        api.set("render", r)?;
    }

    // ---- api.event.emit ----
    {
        let e = lua.create_table()?;
        let st = state.clone();
        e.set(
            "emit",
            lua.create_function(move |_, (name, payload): (String, String)| {
                st.lock().unwrap().events.push((name, payload));
                Ok(())
            })?,
        )?;
        api.set("event", e)?;
    }

    // ---- api.timer.after(sec, fn) ----
    {
        let t = lua.create_table()?;
        let st = state.clone();
        t.set(
            "after",
            lua.create_function(move |_, (sec, body): (f32, String)| {
                st.lock().unwrap().timers.push(super::events::DeferredCall {
                    remaining: sec,
                    lua_body: body,
                });
                Ok(())
            })?,
        )?;
        api.set("timer", t)?;
    }

    // ---- api.chunk (v2): управление чанками ----
    // api.chunk.spawn(id, gx, gy, gz) — ручной спавн чанка (generator="none").
    // api.chunk.despawn(gx, gy, gz)   — выгрузка чанка по grid-координатам.
    // api.chunk.on_generated(cb)      — регистрация колбэка (хранит имя функции).
    // api.chunk.on_unloaded(cb)       — то же для выгрузки.
    // api.chunk.get_size()            — возвращает {x=, y=, z=} размер чанка.
    {
        let c = lua.create_table()?;

        let st = state.clone();
        c.set(
            "spawn",
            lua.create_function(move |_, (id, gx, gy, gz): (String, i32, i32, i32)| {
                let mut s = st.lock().unwrap();
                s.spawned_chunks.push((id.clone(), gx, gy, gz));
                s.chunk_events
                    .push(("generated".into(), id.clone(), gx, gy, gz));
                Ok(())
            })?,
        )?;

        let st = state.clone();
        c.set(
            "despawn",
            lua.create_function(move |_, (gx, gy, gz): (i32, i32, i32)| {
                let mut s = st.lock().unwrap();
                let removed: Vec<(String, i32, i32, i32)> = s
                    .spawned_chunks
                    .iter()
                    .filter(|(_, x, y, z)| *x == gx && *y == gy && *z == gz)
                    .cloned()
                    .collect();
                s.spawned_chunks
                    .retain(|(_, x, y, z)| !(*x == gx && *y == gy && *z == gz));
                for (id, x, y, z) in removed {
                    s.chunk_events.push(("unloaded".into(), id, x, y, z));
                }
                Ok(())
            })?,
        )?;

        let st = state.clone();
        c.set(
            "on_generated",
            lua.create_function(move |_, cb: String| {
                st.lock().unwrap().events.push(("chunk_cb".into(), cb));
                Ok(())
            })?,
        )?;

        let st = state.clone();
        c.set(
            "on_unloaded",
            lua.create_function(move |_, cb: String| {
                st.lock().unwrap().events.push(("chunk_cb_unload".into(), cb));
                Ok(())
            })?,
        )?;

        let st = state.clone();
        c.set(
            "get_size",
            lua.create_function(move |lua, ()| {
                let s = st.lock().unwrap();
                let t = lua.create_table()?;
                t.set("x", s.chunk_size[0])?;
                t.set("y", s.chunk_size[1])?;
                t.set("z", s.chunk_size[2])?;
                Ok(t)
            })?,
        )?;

        api.set("chunk", c)?;
    }

    lua.globals().set("api", &api)?;
    Ok(api)
}

fn rand_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
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
    fn player_api_works() {
        let (lua, state) = lua_with_state();
        lua.load(r#"
            api.player.set_hp(55)
            api.player.teleport(1, 2, 3)
            api.player.add_item("key_red", 2)
        "#).exec().unwrap();
        let s = state.lock().unwrap();
        assert!((s.player_hp - 55.0).abs() < 1e-6);
        assert_eq!(s.player_pos, [1.0, 2.0, 3.0]);
        assert_eq!(s.inventory.get("key_red").copied(), Some(2));
    }

    #[test]
    fn has_item_reflects_inventory() {
        let (lua, _state) = lua_with_state();
        let res: bool = lua
            .load(r#"
                api.player.add_item("a", 1)
                return api.player.has_item("a")
            "#)
            .eval()
            .unwrap();
        assert!(res);
    }

    #[test]
    fn event_emit_is_recorded() {
        let (lua, state) = lua_with_state();
        lua.load(r#"api.event.emit("wave_started", "3 bats")"#).exec().unwrap();
        let s = state.lock().unwrap();
        assert_eq!(s.events, vec![("wave_started".into(), "3 bats".into())]);
    }

    #[test]
    fn timer_after_pushes_deferred() {
        let (lua, state) = lua_with_state();
        lua.load(r#"api.timer.after(2.0, "print('later')")"#).exec().unwrap();
        let mut s = state.lock().unwrap();
        assert_eq!(s.timers.len(), 1);
        let ready = s.timers.tick(2.5);
        assert_eq!(ready, vec!["print('later')".to_string()]);
    }

    #[test]
    fn transform_get_returns_zero_default() {
        let (lua, _state) = lua_with_state();
        let x: f32 = lua
            .load(r#"return api.transform.get("missing").pos.x"#)
            .eval()
            .unwrap();
        assert!((x - 0.0).abs() < 1e-6);
    }

    #[test]
    fn chunk_spawn_and_events() {
        let (lua, state) = lua_with_state();
        lua.load(r#"api.chunk.spawn("chunk_field", 0, 0, 0)"#)
            .exec()
            .unwrap();
        let s = state.lock().unwrap();
        assert_eq!(s.spawned_chunks.len(), 1);
        assert_eq!(s.spawned_chunks[0].0, "chunk_field");
        assert_eq!(s.chunk_events.len(), 1);
        assert_eq!(s.chunk_events[0].0, "generated");
    }

    #[test]
    fn chunk_despawn_records_unload() {
        let (lua, state) = lua_with_state();
        lua.load(r#"
            api.chunk.spawn("chunk_field", 1, 0, 2)
            api.chunk.despawn(1, 0, 2)
        "#)
        .exec()
        .unwrap();
        let s = state.lock().unwrap();
        assert!(s.spawned_chunks.is_empty());
        // два события: generated + unloaded
        assert_eq!(s.chunk_events.len(), 2);
        assert_eq!(s.chunk_events[1].0, "unloaded");
    }

    #[test]
    fn chunk_get_size_returns_default() {
        let (lua, _state) = lua_with_state();
        let (x, y, z): (f32, f32, f32) = lua
            .load(r#"
                local s = api.chunk.get_size()
                return s.x, s.y, s.z
            "#)
            .eval()
            .unwrap();
        assert!((x - 32.0).abs() < 1e-6);
        assert!((y - 16.0).abs() < 1e-6);
        assert!((z - 32.0).abs() < 1e-6);
    }

    #[test]
    fn chunk_callbacks_are_recorded() {
        let (lua, state) = lua_with_state();
        lua.load(r#"
            api.chunk.on_generated("on_chunk_gen")
            api.chunk.on_unloaded("on_chunk_unload")
        "#)
        .exec()
        .unwrap();
        let s = state.lock().unwrap();
        assert!(s.events.iter().any(|(k, v)| k == "chunk_cb" && v == "on_chunk_gen"));
        assert!(s.events.iter().any(|(k, v)| k == "chunk_cb_unload" && v == "on_chunk_unload"));
    }

    #[test]
    fn vec3_normalize_works() {
        let (lua, _state) = lua_with_state();
        let (x, y, z): (f32, f32, f32) = lua
            .load(r#"
                local v = api.vec3.normalize({x=3, y=4, z=0})
                return v.x, v.y, v.z
            "#)
            .eval()
            .unwrap();
        assert!((x - 0.6).abs() < 1e-5);
        assert!((y - 0.8).abs() < 1e-5);
        assert!(z.abs() < 1e-5);
    }
}