use super::{schema::ScriptDef, safe_join};
use anyhow::{Context, Result};
use mlua::{Function, Lua, Table, Value};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone)]
pub enum LuaCommand {
    Message { text: String, seconds: f32 },
    Teleport([f32; 3]),
    AddHunger(f32),
    SetHunger(f32),
    AddSanity(f32),
    SetSanity(f32),
    GiveItem { item: String, count: u16 },
    RemoveItem { item: String, count: u16 },
    SpawnBox {
        id: String,
        pos: [f32; 3],
        size: [f32; 3],
        rot_deg: [f32; 3],
        material: Option<String>,
        collider: bool,
    },
    SpawnPickup { item: String, count: u16, pos: [f32; 3] },
    SpawnModel {
        id: String,
        model: String,
        pos: [f32; 3],
        rot_deg: [f32; 3],
        scale: [f32; 3],
        collider_size: Option<[f32; 3]>,
    },
    SpawnLight {
        id: String,
        kind: String,
        pos: [f32; 3],
        rot_deg: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        range: f32,
        shadows: bool,
    },
    Despawn(String),
    SetVisible { id: String, visible: bool },
    SetPosition { id: String, pos: [f32; 3] },
    SetRotation { id: String, rot_deg: [f32; 3] },
    SetScale { id: String, scale: [f32; 3] },
    LoadChunk(String),
    UnloadChunk(String),
    PlaySound(String),
    Emit { name: String, payload: String },
}

#[derive(Debug, Clone)]
pub struct LuaSnapshot {
    pub elapsed: f64,
    pub delta: f32,
    pub player_position: [f32; 3],
    pub hunger: f32,
    pub sanity: f32,
    pub nickname: String,
    pub multiplayer: bool,
    pub level_id: String,
    pub level_name: String,
    pub selected_item: Option<String>,
    pub inventory_counts: HashMap<String, u16>,
}

impl Default for LuaSnapshot {
    fn default() -> Self {
        Self {
            elapsed: 0.0,
            delta: 0.0,
            player_position: [0.0, 0.0, 0.0],
            hunger: 100.0,
            sanity: 100.0,
            nickname: "Wanderer".into(),
            multiplayer: false,
            level_id: String::new(),
            level_name: String::new(),
            selected_item: None,
            inventory_counts: HashMap::new(),
        }
    }
}

pub struct LuaRuntime {
    lua: Lua,
    queue: Arc<Mutex<VecDeque<LuaCommand>>>,
    snapshot: Arc<Mutex<LuaSnapshot>>,
}

impl LuaRuntime {
    pub fn new(level_id: &str, level_name: &str) -> Result<Self> {
        // Lua::new() intentionally loads only mlua's safe standard-library subset.
        // We additionally hide file/process/module entry points so downloaded levels cannot
        // access the host OS or LuaJIT FFI. Level code talks to the game only through `bi`.
        let lua = Lua::new();
        let globals = lua.globals();
        for forbidden in ["os", "io", "package", "debug", "require", "dofile", "loadfile", "ffi", "jit"] {
            globals.set(forbidden, Value::Nil)?;
        }

        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let snapshot = Arc::new(Mutex::new(LuaSnapshot {
            level_id: level_id.to_string(),
            level_name: level_name.to_string(),
            ..Default::default()
        }));
        install_api(&lua, queue.clone(), snapshot.clone())?;
        Ok(Self { lua, queue, snapshot })
    }

    pub fn load_scripts(&self, root: &Path, scripts: impl IntoIterator<Item = ScriptDef>) -> Result<()> {
        for script in scripts {
            if !script.enabled { continue; }
            if let Some(src) = &script.src {
                let path = safe_join(root, src)?;
                let code = std::fs::read_to_string(&path).with_context(|| format!("reading Lua script {}", path.display()))?;
                self.lua.load(&code).set_name(&script.id).exec().with_context(|| format!("executing Lua script {}", script.id))?;
            }
            if let Some(code) = &script.code {
                self.lua.load(code).set_name(&script.id).exec().with_context(|| format!("executing inline Lua script {}", script.id))?;
            }
        }
        Ok(())
    }

    pub fn update_snapshot(&self, snapshot: LuaSnapshot) {
        if let Ok(mut value) = self.snapshot.lock() { *value = snapshot; }
    }

    pub fn drain_commands(&self) -> Vec<LuaCommand> {
        let Ok(mut queue) = self.queue.lock() else { return Vec::new(); };
        queue.drain(..).collect()
    }

    pub fn dispatch(&self, event: &str, fields: &[(&str, EventValue)]) {
        if let Err(err) = self.dispatch_inner(event, fields) {
            eprintln!("lua event {event}: {err}");
        }
    }

    fn dispatch_inner(&self, event: &str, fields: &[(&str, EventValue)]) -> mlua::Result<()> {
        let globals = self.lua.globals();
        let handlers: Table = globals.get("__bi_handlers")?;
        let list: Value = handlers.get(event)?;
        let Value::Table(list) = list else { return Ok(()); };
        let payload = self.lua.create_table()?;
        payload.set("event", event)?;
        for (key, value) in fields {
            match value {
                EventValue::String(value) => payload.set(*key, value.clone())?,
                EventValue::Number(value) => payload.set(*key, *value)?,
                EventValue::Bool(value) => payload.set(*key, *value)?,
            }
        }
        for function in list.sequence_values::<Function>() {
            let function = function?;
            if let Err(err) = function.call::<()>(payload.clone()) {
                eprintln!("lua handler for {event}: {err}");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum EventValue {
    String(String),
    Number(f64),
    Bool(bool),
}

fn install_api(lua: &Lua, queue: Arc<Mutex<VecDeque<LuaCommand>>>, snapshot: Arc<Mutex<LuaSnapshot>>) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("__bi_handlers", lua.create_table()?)?;

    let bi = lua.create_table()?;

    let log = lua.create_function(|_, (level, message): (Option<String>, String)| {
        let level = level.unwrap_or_else(|| "info".into());
        println!("[lua/{level}] {message}");
        Ok(())
    })?;
    bi.set("log", log)?;

    let events = lua.create_table()?;
    events.set("on", lua.create_function(|lua, (name, callback): (String, Function)| {
        let handlers: Table = lua.globals().get("__bi_handlers")?;
        let list = match handlers.get::<Value>(name.as_str())? {
            Value::Table(table) => table,
            _ => {
                let table = lua.create_table()?;
                handlers.set(name.as_str(), table.clone())?;
                table
            }
        };
        list.raw_set(list.raw_len() + 1, callback)?;
        Ok(())
    })?)?;
    {
        let queue = queue.clone();
        events.set("emit", lua.create_function(move |_, (name, payload): (String, Option<String>)| {
            push(&queue, LuaCommand::Emit { name, payload: payload.unwrap_or_default() });
            Ok(())
        })?)?;
    }
    bi.set("events", events)?;

    let time = lua.create_table()?;
    {
        let snapshot = snapshot.clone();
        time.set("now", lua.create_function(move |_, ()| Ok(read_snapshot(&snapshot).elapsed))?)?;
    }
    {
        let snapshot = snapshot.clone();
        time.set("delta", lua.create_function(move |_, ()| Ok(read_snapshot(&snapshot).delta))?)?;
    }
    bi.set("time", time)?;

    let level = lua.create_table()?;
    {
        let snapshot = snapshot.clone();
        level.set("id", lua.create_function(move |_, ()| Ok(read_snapshot(&snapshot).level_id))?)?;
    }
    {
        let snapshot = snapshot.clone();
        level.set("name", lua.create_function(move |_, ()| Ok(read_snapshot(&snapshot).level_name))?)?;
    }
    {
        let queue = queue.clone();
        level.set("load_chunk", lua.create_function(move |_, id: String| { push(&queue, LuaCommand::LoadChunk(id)); Ok(()) })?)?;
    }
    {
        let queue = queue.clone();
        level.set("unload_chunk", lua.create_function(move |_, id: String| { push(&queue, LuaCommand::UnloadChunk(id)); Ok(()) })?)?;
    }
    bi.set("level", level)?;

    let net = lua.create_table()?;
    {
        let snapshot = snapshot.clone();
        net.set("is_multiplayer", lua.create_function(move |_, ()| Ok(read_snapshot(&snapshot).multiplayer))?)?;
    }
    {
        let snapshot = snapshot.clone();
        net.set("nickname", lua.create_function(move |_, ()| Ok(read_snapshot(&snapshot).nickname))?)?;
    }
    bi.set("net", net)?;

    let player = lua.create_table()?;
    {
        let snapshot = snapshot.clone();
        player.set("position", lua.create_function(move |_, ()| {
            let p = read_snapshot(&snapshot).player_position;
            Ok((p[0], p[1], p[2]))
        })?)?;
    }
    {
        let queue = queue.clone();
        player.set("teleport", lua.create_function(move |_, (x, y, z): (f32, f32, f32)| { push(&queue, LuaCommand::Teleport([x, y, z])); Ok(()) })?)?;
    }
    {
        let queue = queue.clone();
        player.set("message", lua.create_function(move |_, (text, seconds): (String, Option<f32>)| {
            push(&queue, LuaCommand::Message { text, seconds: seconds.unwrap_or(3.0).clamp(0.2, 30.0) });
            Ok(())
        })?)?;
    }
    bi.set("player", player)?;

    let stats = lua.create_table()?;
    {
        let snapshot = snapshot.clone();
        stats.set("hunger", lua.create_function(move |_, ()| Ok(read_snapshot(&snapshot).hunger))?)?;
    }
    {
        let snapshot = snapshot.clone();
        stats.set("sanity", lua.create_function(move |_, ()| Ok(read_snapshot(&snapshot).sanity))?)?;
    }
    {
        let queue = queue.clone();
        stats.set("add_hunger", lua.create_function(move |_, amount: f32| { push(&queue, LuaCommand::AddHunger(amount)); Ok(()) })?)?;
    }
    {
        let queue = queue.clone();
        stats.set("set_hunger", lua.create_function(move |_, amount: f32| { push(&queue, LuaCommand::SetHunger(amount)); Ok(()) })?)?;
    }
    {
        let queue = queue.clone();
        stats.set("add_sanity", lua.create_function(move |_, amount: f32| { push(&queue, LuaCommand::AddSanity(amount)); Ok(()) })?)?;
    }
    {
        let queue = queue.clone();
        stats.set("set_sanity", lua.create_function(move |_, amount: f32| { push(&queue, LuaCommand::SetSanity(amount)); Ok(()) })?)?;
    }
    bi.set("stats", stats)?;

    let inventory = lua.create_table()?;
    {
        let snapshot = snapshot.clone();
        inventory.set("has", lua.create_function(move |_, (item, count): (String, Option<u16>)| {
            let snapshot = read_snapshot(&snapshot);
            Ok(snapshot.inventory_counts.get(&item).copied().unwrap_or(0) >= count.unwrap_or(1))
        })?)?;
    }
    {
        let snapshot = snapshot.clone();
        inventory.set("count", lua.create_function(move |_, item: String| Ok(read_snapshot(&snapshot).inventory_counts.get(&item).copied().unwrap_or(0)))?)?;
    }
    {
        let snapshot = snapshot.clone();
        inventory.set("selected", lua.create_function(move |_, ()| Ok(read_snapshot(&snapshot).selected_item))?)?;
    }
    {
        let queue = queue.clone();
        inventory.set("give", lua.create_function(move |_, (item, count): (String, Option<u16>)| {
            push(&queue, LuaCommand::GiveItem { item, count: count.unwrap_or(1).max(1) });
            Ok(())
        })?)?;
    }
    {
        let queue = queue.clone();
        inventory.set("remove", lua.create_function(move |_, (item, count): (String, Option<u16>)| {
            push(&queue, LuaCommand::RemoveItem { item, count: count.unwrap_or(1).max(1) });
            Ok(())
        })?)?;
    }
    bi.set("inventory", inventory)?;

    let world = lua.create_table()?;
    {
        let queue = queue.clone();
        world.set("spawn_box", lua.create_function(move |_, spec: Table| {
            let id: String = spec.get("id")?;
            let pos = lua_vec3(spec.get("pos")?)?;
            let size = lua_vec3(spec.get("size")?)?;
            let rot_deg = match spec.get::<Value>("rot")? {
                Value::Table(t) => lua_vec3(t)?,
                _ => [0.0, 0.0, 0.0],
            };
            let material: Option<String> = spec.get("material")?;
            let collider: Option<bool> = spec.get("collider")?;
            push(&queue, LuaCommand::SpawnBox { id, pos, size, rot_deg, material, collider: collider.unwrap_or(true) });
            Ok(())
        })?)?;
    }
    {
        let queue = queue.clone();
        world.set("spawn_pickup", lua.create_function(move |_, spec: Table| {
            let item: String = spec.get("item")?;
            let count: Option<u16> = spec.get("count")?;
            let pos = lua_vec3(spec.get("pos")?)?;
            push(&queue, LuaCommand::SpawnPickup { item, count: count.unwrap_or(1).max(1), pos });
            Ok(())
        })?)?;
    }
    {
        let queue = queue.clone();
        world.set("spawn_model", lua.create_function(move |_, spec: Table| {
            let id: String = spec.get("id")?;
            let model: String = spec.get("model")?;
            let pos = lua_vec3(spec.get("pos")?)?;
            let rot_deg = match spec.get::<Value>("rot")? {
                Value::Table(t) => lua_vec3(t)?,
                _ => [0.0, 0.0, 0.0],
            };
            let scale = match spec.get::<Value>("scale")? {
                Value::Table(t) => lua_vec3(t)?,
                _ => [1.0, 1.0, 1.0],
            };
            let collider_size = match spec.get::<Value>("collider_size")? {
                Value::Table(t) => Some(lua_vec3(t)?),
                _ => None,
            };
            push(&queue, LuaCommand::SpawnModel { id, model, pos, rot_deg, scale, collider_size });
            Ok(())
        })?)?;
    }
    {
        let queue = queue.clone();
        world.set("spawn_light", lua.create_function(move |_, spec: Table| {
            let id: String = spec.get("id")?;
            let kind: Option<String> = spec.get("kind")?;
            let pos = lua_vec3(spec.get("pos")?)?;
            let rot_deg = match spec.get::<Value>("rot")? {
                Value::Table(t) => lua_vec3(t)?,
                _ => [0.0, 0.0, 0.0],
            };
            let color = match spec.get::<Value>("color")? {
                Value::Table(t) => lua_vec3(t)?,
                _ => [1.0, 1.0, 1.0],
            };
            let intensity: Option<f32> = spec.get("intensity")?;
            let range: Option<f32> = spec.get("range")?;
            let shadows: Option<bool> = spec.get("shadows")?;
            push(&queue, LuaCommand::SpawnLight {
                id,
                kind: kind.unwrap_or_else(|| "point".into()),
                pos,
                rot_deg,
                color,
                intensity: intensity.unwrap_or(1000.0),
                range: range.unwrap_or(10.0),
                shadows: shadows.unwrap_or(false),
            });
            Ok(())
        })?)?;
    }
    {
        let queue = queue.clone();
        world.set("despawn", lua.create_function(move |_, id: String| { push(&queue, LuaCommand::Despawn(id)); Ok(()) })?)?;
    }
    {
        let queue = queue.clone();
        world.set("set_visible", lua.create_function(move |_, (id, visible): (String, bool)| { push(&queue, LuaCommand::SetVisible { id, visible }); Ok(()) })?)?;
    }
    {
        let queue = queue.clone();
        world.set("set_position", lua.create_function(move |_, (id, x, y, z): (String, f32, f32, f32)| { push(&queue, LuaCommand::SetPosition { id, pos: [x, y, z] }); Ok(()) })?)?;
    }
    {
        let queue = queue.clone();
        world.set("set_rotation", lua.create_function(move |_, (id, x, y, z): (String, f32, f32, f32)| { push(&queue, LuaCommand::SetRotation { id, rot_deg: [x, y, z] }); Ok(()) })?)?;
    }
    {
        let queue = queue.clone();
        world.set("set_scale", lua.create_function(move |_, (id, x, y, z): (String, f32, f32, f32)| { push(&queue, LuaCommand::SetScale { id, scale: [x, y, z] }); Ok(()) })?)?;
    }
    bi.set("world", world)?;

    let audio = lua.create_table()?;
    {
        let queue = queue.clone();
        audio.set("play", lua.create_function(move |_, sound: String| { push(&queue, LuaCommand::PlaySound(sound)); Ok(()) })?)?;
    }
    bi.set("audio", audio)?;

    globals.set("bi", bi)?;
    Ok(())
}

fn push(queue: &Arc<Mutex<VecDeque<LuaCommand>>>, command: LuaCommand) {
    if let Ok(mut queue) = queue.lock() { queue.push_back(command); }
}

fn read_snapshot(snapshot: &Arc<Mutex<LuaSnapshot>>) -> LuaSnapshot {
    snapshot.lock().map(|v| v.clone()).unwrap_or_default()
}

fn lua_vec3(table: Table) -> mlua::Result<[f32; 3]> {
    let x: f32 = table.get(1).or_else(|_| table.get("x"))?;
    let y: f32 = table.get(2).or_else(|_| table.get("y"))?;
    let z: f32 = table.get(3).or_else(|_| table.get("z"))?;
    Ok([x, y, z])
}
