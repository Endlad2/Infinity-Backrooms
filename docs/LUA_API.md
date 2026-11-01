# Backrooms Infinity LuaJIT API

Level scripts run through `mlua` with the vendored **LuaJIT** backend. The VM uses mlua's safe standard-library subset and the client additionally removes `os`, `io`, `package`, `debug`, `require`, `dofile`, `loadfile`, `ffi`, and `jit`. Downloaded level code therefore cannot intentionally use the supported API to read arbitrary files, start processes, load native modules, or access LuaJIT FFI.

All mutations are queued and applied by Bevy on the game thread. Read APIs use a frame snapshot, so a value written by a mutation may become visible to a read on the following update.

## Event model

```lua
bi.events.on("level_start", function(event)
    bi.log("info", "loaded " .. event.name)
end)
```

Built-in events:

- `level_start`: `id`, `name`
- `tick`: `dt` (20 Hz by default)
- `chunk_load`: `chunk`
- `chunk_unload`: `chunk`
- `trigger_enter`: `id`
- `trigger_exit`: `id`
- `interact`: `id`, `event_name`
- any custom trigger/interactable event named in `level.xml`
- custom events emitted with `bi.events.emit(name, payload)`; handler receives `payload`

## Logging and time

```lua
bi.log("info", "message")
local now = bi.time.now()     -- seconds since game start
local dt  = bi.time.delta()   -- most recent frame delta
```

## Level/session

```lua
local id = bi.level.id()
local name = bi.level.name()
bi.level.load_chunk("basement")
bi.level.unload_chunk("lobby")

local multiplayer = bi.net.is_multiplayer()
local nickname = bi.net.nickname()
```

Forced chunk state overrides ordinary distance streaming until the opposite command is issued.

## Player

```lua
local x, y, z = bi.player.position()
bi.player.teleport(10.0, 1.7, -5.0)
bi.player.message("Something moved behind you.", 3.0)
```

## Hunger / sanity

```lua
local hunger = bi.stats.hunger()
local sanity = bi.stats.sanity()
bi.stats.add_hunger(15)
bi.stats.set_hunger(100)
bi.stats.add_sanity(-8)
bi.stats.set_sanity(50)
```

Values are clamped to `0..100` by the client.

## Inventory

```lua
if bi.inventory.has("wood", 2) then
    bi.inventory.remove("wood", 2)
    bi.inventory.give("scrap", 1)
end

local wood = bi.inventory.count("wood")
local selected = bi.inventory.selected() -- item id or nil
```

`give` inserts into the backpack if there is space. `remove` consumes matching stacks across backpack, hotbar and crafting slots.

## World mutation

```lua
bi.world.spawn_box({
    id = "temporary_wall",
    pos = { 2, 1.6, 4 },
    size = { 3, 3.2, 0.2 },
    rot = { 0, 0, 0 },
    material = "wallpaper",
    collider = true,
})

bi.world.spawn_pickup({
    item = "almond_water",
    count = 1,
    pos = { 0, 0.25, 2 },
})

-- `model` refers to an id declared in <assets><models>.
bi.world.spawn_model({
    id = "moving_chair",
    model = "chair",
    pos = { 4, 0, -2 },
    rot = { 0, 90, 0 },
    scale = { 1, 1, 1 },
    collider_size = { 0.8, 1.2, 0.8 }, -- optional
})

bi.world.spawn_light({
    id = "alarm_light",
    kind = "point", -- point | spot | directional
    pos = { 0, 2.7, 0 },
    color = { 1.0, 0.15, 0.08 },
    intensity = 1800,
    range = 12,
    shadows = false,
})

bi.world.set_position("temporary_wall", 3, 1.6, 4)
bi.world.set_rotation("temporary_wall", 0, 45, 0)
bi.world.set_scale("temporary_wall", 1, 1.5, 1)
bi.world.set_visible("temporary_wall", false)
bi.world.despawn("temporary_wall")
```

Objects addressed by `id` are named level entities or entities dynamically created through the world API. `spawn_model` only accepts model ids declared by the level package, so scripts cannot request arbitrary host paths.

## Audio

```lua
bi.audio.play("door_slam")
```

The sound ID must be declared under `<assets><sounds>`.

## Custom events

```lua
bi.events.on("power_changed", function(e)
    bi.player.message("Power state: " .. e.payload, 1.5)
end)

bi.events.emit("power_changed", "off")
```

## Level-authoring pattern

Prefer XML for static scene structure and Lua for stateful behavior. Keep large mazes in chunks, use triggers/interactables as event sources, name only objects that scripts need to modify, and avoid spawning thousands of Lua objects every tick. This keeps multiplayer state and future persistence easier to reason about.
