# Backrooms Infinity level format (`level.xml`)

A level is distributed as a ZIP archive whose root contains exactly the runtime entry point `level.xml` plus an `assets/` directory. The launcher extracts the archive into `.backrooms_infinity/levels/<level-id>/`; the game client may also be started directly with either an extracted level directory or a `.zip` path.

The client does **not** use an XML library. `client/src/level/xml.rs` is a small project-owned parser that understands XML declarations, comments, quoted attributes, nested elements, CDATA, UTF-8 text, and the standard/numeric XML entities. DTD entity expansion is intentionally not supported.

## Coordinate system

Bevy's coordinate system is used: +Y is up. Distances are in metres. Rotations are Euler degrees in `X,Y,Z` order. A transform vector is written as `x,y,z`, for example `pos="4,1.6,-2"`.

Chunk object positions are **local to the chunk origin**. Chunks stream by player distance and use separate load/unload thresholds to avoid rapid thrashing.

## Root structure

```xml
<level version="1">
  <meta id="level-0" name="Level 0" author="...">
    <description>...</description>
    <preview src="assets/preview.webp"/>
  </meta>

  <world clear_color="#111108" ambient_color="#8f8246" ambient_brightness="105"/>
  <player spawn="0,1.7,11" yaw="180" pitch="0" move_speed="4.2" interaction_distance="2.6"/>

  <assets>...</assets>
  <scripts>...</scripts>
  <chunks>...</chunks>
</level>
```

### `<meta>`

Required attributes: `id`, `name`. Optional: `author`. Optional children: `<description>` and `<preview src="..."/>`. Asset paths must remain inside the level package; absolute paths and `..` traversal are rejected.

### `<world>`

`clear_color` and `ambient_color` accept `#RRGGBB`, `#RRGGBBAA`, or comma-separated float channels. `ambient_brightness` controls Bevy ambient lighting.

### `<player>`

`spawn`, `yaw`, `pitch`, `move_speed`, and `interaction_distance` define the initial first-person controller settings. The sample client uses simple axis-separated AABB collision against level colliders.

## Assets

```xml
<assets>
  <textures>
    <texture id="wall_tex" src="assets/textures/wall.png"/>
  </textures>
  <materials>
    <material id="wall"
              texture="wall_tex"
              normal="wall_normal_tex"
              color="#d4c36c"
              roughness="0.95"
              metallic="0.0"
              unlit="false"
              alpha_blend="false"/>
  </materials>
  <models>
    <model id="chair" src="assets/models/chair.glb" scene="0"/>
  </models>
  <sounds>
    <sound id="hum" src="assets/audio/hum.ogg" loop="true" autoplay="true"/>
  </sounds>
</assets>
```

`<texture>` registers an image path. The default build is intended for common Bevy-supported image formats such as PNG/JPEG; normal maps can be referenced independently. `<material>` creates a Bevy `StandardMaterial`. `<model>` imports a glTF/GLB scene and `<model ref="chair" .../>` instantiates it. `<sound>` registers an audio asset for autoplay or `bi.audio.play(id)`; concrete codecs depend on enabled Bevy audio features.

## Chunks

```xml
<chunks>
  <chunk id="lobby" origin="0,0,0" size="28,3.4,28"
         load_distance="48" unload_distance="62" always_loaded="true">
    <geometry>...</geometry>
    <lights>...</lights>
    <pickups>...</pickups>
    <triggers>...</triggers>
    <interactables>...</interactables>
    <scripts>...</scripts>
  </chunk>
</chunks>
```

`size` describes the chunk bounds. `load_distance` and `unload_distance` define streaming hysteresis. `always_loaded="true"` is useful for the spawn chunk. Lua can force a chunk with `bi.level.load_chunk(id)` or `bi.level.unload_chunk(id)`.

## Geometry tags

All geometry tags accept `id`, `pos`, `rot`, `scale`, `material`, `collider`, and `visible` where applicable. `id` makes an object addressable from Lua using `bi.world.*`.

```xml
<geometry>
  <box id="wall-a" pos="0,1.6,-4" size="8,3.2,0.2" material="wall" collider="true"/>
  <sphere id="orb" pos="2,1,1" radius="0.5" material="plastic" collider="false"/>
  <cylinder id="column" pos="-2,1.6,0" radius="0.4" height="3.2" material="wall"/>
  <model id="chair-01" ref="chair" pos="4,0,3" rot="0,90,0" scale="1,1,1"
         collider="true" collider_size="0.8,1.2,0.8"/>
</geometry>
```

Primitive colliders are currently axis-aligned at runtime; rotation affects rendering but not the collision box. Imported models need an explicit `collider_size` if they should block the player. This keeps the level format deterministic and avoids depending on model mesh topology for collision.

## Lights

```xml
<lights>
  <light id="lamp" kind="point" pos="0,2.8,0" color="#fff4c0"
         intensity="1400" range="9" shadows="false"/>
  <light kind="spot" pos="0,3,0" rot="-90,0,0" intensity="1800"
         range="12" inner_angle="20" outer_angle="38"/>
  <light kind="directional" rot="-55,-20,0" intensity="8000" shadows="true"/>
</lights>
```

Supported kinds: `point`, `spot`, `directional`.

## Pickups

```xml
<pickups>
  <pickup id="water-a" item="almond_water" count="1" pos="4,0.25,-2"/>
</pickups>
```

Pickups use the same inventory item IDs as the rest of the client. They receive the standard `F — take ...` interaction and become normal inventory stacks.

## Triggers

```xml
<triggers>
  <trigger id="pit" pos="0,1,0" size="4,2.5,4"
           on_enter="fell_into_pit" on_exit="left_pit" once="false"/>
</triggers>
```

Every enter/exit produces generic `trigger_enter`/`trigger_exit` Lua events with `event.id`. `on_enter` and `on_exit` optionally dispatch an additional custom event of that name. `once="true"` suppresses subsequent enter events after the first activation.

## Interactables

```xml
<interactables>
  <interactable id="switch" pos="1,1.2,3" radius="2.2"
                prompt="F — press the switch" event="switch_pressed"/>
</interactables>
```

The client chooses the nearest interactable in front of the player, displays its prompt, and dispatches both `interact` and the custom `event` when `F` is pressed.

## Scripts

A script can be external or inline. External scripts are preferred for larger levels.

```xml
<scripts>
  <script id="main" src="assets/scripts/main.lua"/>
  <script id="tiny-inline"><![CDATA[
    bi.events.on("level_start", function(e)
      bi.player.message("Welcome", 2.0)
    end)
  ]]></script>
</scripts>
```

Chunk-local `<scripts>` are loaded into the same sandboxed VM at level startup. Use `chunk_load`/`chunk_unload` events if behavior should follow streaming state.

## ZIP rules

The archive root must contain `level.xml`; a nested `some-folder/level.xml` is rejected. Extraction rejects absolute paths and ZIP entries that escape the destination. Every referenced asset/script/preview is validated before the level starts.
