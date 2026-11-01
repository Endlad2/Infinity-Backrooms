use super::{lua::{EventValue, LuaCommand, LuaRuntime, LuaSnapshot}, schema::* , LoadedLevel};
use crate::{args::GameMode, inventory::{Inventory, ItemStack}, Pickup, Player, PlayerStats, RuntimeConfig};
use bevy::prelude::*;
use std::{collections::{HashMap, HashSet}, sync::Mutex, time::Duration};

#[derive(Component, Debug, Clone)]
pub struct LevelCollider {
    pub half_extents: Vec3,
}

#[derive(Component)]
pub struct LevelNamed(pub String);

#[derive(Component)]
struct LevelOwned;

#[derive(Component)]
struct LevelTrigger {
    id: String,
    half_extents: Vec3,
    on_enter: Option<String>,
    on_exit: Option<String>,
    inside: bool,
    once: bool,
    fired: bool,
}

#[derive(Component)]
struct LevelInteractable {
    id: String,
    radius: f32,
    prompt: String,
    event: String,
}

#[derive(Component)]
struct LevelInteractionPrompt;

#[derive(Component)]
struct LevelMessageText;

#[derive(Resource, Default)]
struct LevelRenderAssets {
    materials: HashMap<String, Handle<StandardMaterial>>,
}

#[derive(Resource, Default)]
struct ChunkState {
    active: HashMap<String, Vec<Entity>>,
    forced_loaded: HashSet<String>,
    forced_unloaded: HashSet<String>,
}

#[derive(Resource)]
pub struct LuaEngine(pub Mutex<LuaRuntime>);

#[derive(Resource)]
struct LuaTickTimer(Timer);

#[derive(Resource)]
struct MessageTimer(Option<Timer>);

pub struct LevelRuntimePlugin;

impl Plugin for LevelRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelRenderAssets>()
            .init_resource::<ChunkState>()
            .insert_resource(LuaTickTimer(Timer::new(Duration::from_millis(50), TimerMode::Repeating)))
            .insert_resource(MessageTimer(None))
            .add_systems(Startup, (setup_level_resources, setup_level_ui, setup_lua))
            .add_systems(Update, (
                stream_chunks,
                sync_lua_snapshot,
                lua_tick,
                trigger_events,
                interactable_system,
                apply_lua_commands,
                clear_level_message,
            ));
    }
}

fn setup_level_resources(
    mut commands: Commands,
    level: Res<LoadedLevel>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut render_assets: ResMut<LevelRenderAssets>,
) {
    let world = &level.definition.world;
    commands.insert_resource(ClearColor(Color::srgba(world.clear_color[0], world.clear_color[1], world.clear_color[2], world.clear_color[3])));
    commands.insert_resource(AmbientLight {
        color: Color::srgb(world.ambient_color[0], world.ambient_color[1], world.ambient_color[2]),
        brightness: world.ambient_brightness,
        ..default()
    });

    for def in level.definition.assets.materials.values() {
        let base_texture: Option<Handle<Image>> = def.texture.as_ref().and_then(|id| level.definition.assets.textures.get(id)).map(|t| asset_server.load(t.src.clone()));
        let normal_map_texture: Option<Handle<Image>> = def.normal.as_ref().and_then(|id| level.definition.assets.textures.get(id)).map(|t| asset_server.load(t.src.clone()));
        let material = StandardMaterial {
            base_color: Color::srgba(def.base_color[0], def.base_color[1], def.base_color[2], def.base_color[3]),
            base_color_texture: base_texture,
            normal_map_texture,
            perceptual_roughness: def.roughness,
            metallic: def.metallic,
            unlit: def.unlit,
            alpha_mode: if def.alpha_blend { AlphaMode::Blend } else { AlphaMode::Opaque },
            ..default()
        };
        render_assets.materials.insert(def.id.clone(), materials.add(material));
    }

    for sound in level.definition.assets.sounds.values().filter(|s| s.autoplay) {
        let handle: Handle<AudioSource> = asset_server.load(sound.src.clone());
        commands.spawn((AudioPlayer(handle), if sound.looped { PlaybackSettings::LOOP } else { PlaybackSettings::DESPAWN }));
    }
}

fn setup_level_ui(mut commands: Commands) {
    commands.spawn((
        LevelInteractionPrompt,
        Text::new(""),
        TextFont { font_size: 21.0, ..default() },
        Node { position_type: PositionType::Absolute, left: Val::Percent(38.0), bottom: Val::Px(84.0), ..default() },
    ));
    commands.spawn((
        LevelMessageText,
        Text::new(""),
        TextFont { font_size: 26.0, ..default() },
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(20.0),
            right: Val::Percent(20.0),
            top: Val::Px(48.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
    ));
}

fn setup_lua(mut commands: Commands, level: Res<LoadedLevel>) {
    let runtime = match LuaRuntime::new(&level.definition.meta.id, &level.definition.meta.name) {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("could not initialize LuaJIT runtime: {err:#}");
            return;
        }
    };
    let mut scripts = level.definition.scripts.clone();
    for chunk in &level.definition.chunks { scripts.extend(chunk.scripts.clone()); }
    if let Err(err) = runtime.load_scripts(&level.root, scripts) {
        eprintln!("level Lua load error: {err:#}");
    }
    runtime.dispatch("level_start", &[
        ("id", EventValue::String(level.definition.meta.id.clone())),
        ("name", EventValue::String(level.definition.meta.name.clone())),
        ("sandboxed", EventValue::Bool(true)),
    ]);
    commands.insert_resource(LuaEngine(Mutex::new(runtime)));
}

fn stream_chunks(
    mut commands: Commands,
    level: Res<LoadedLevel>,
    render_assets: Res<LevelRenderAssets>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player: Query<&Transform, With<Player>>,
    mut state: ResMut<ChunkState>,
    lua: Option<Res<LuaEngine>>,
) {
    let Ok(player) = player.get_single() else { return; };
    let player_xz = Vec2::new(player.translation.x, player.translation.z);
    for chunk in &level.definition.chunks {
        let center = Vec2::new(chunk.origin[0], chunk.origin[2]);
        let half = Vec2::new(chunk.size[0], chunk.size[2]) * 0.5;
        let from_bounds = (player_xz - center).abs() - half;
        let distance = from_bounds.max(Vec2::ZERO).length();
        let active = state.active.contains_key(&chunk.id);
        let should_load = chunk.always_loaded || state.forced_loaded.contains(&chunk.id) || (!state.forced_unloaded.contains(&chunk.id) && distance <= chunk.load_distance);
        let should_unload = !chunk.always_loaded && !state.forced_loaded.contains(&chunk.id) && (state.forced_unloaded.contains(&chunk.id) || distance > chunk.unload_distance);

        if !active && should_load {
            let entities = spawn_chunk(&mut commands, chunk, &level.definition, &render_assets, &asset_server, &mut meshes, &mut materials);
            state.active.insert(chunk.id.clone(), entities);
            if let Some(lua) = &lua {
                if let Ok(lua) = lua.0.lock() { lua.dispatch("chunk_load", &[("chunk", EventValue::String(chunk.id.clone()))]); }
            }
        } else if active && should_unload {
            if let Some(entities) = state.active.remove(&chunk.id) {
                for entity in entities { commands.entity(entity).despawn_recursive(); }
            }
            if let Some(lua) = &lua {
                if let Ok(lua) = lua.0.lock() { lua.dispatch("chunk_unload", &[("chunk", EventValue::String(chunk.id.clone()))]); }
            }
        }
    }
}

fn spawn_chunk(
    commands: &mut Commands,
    chunk: &ChunkDef,
    definition: &LevelDefinition,
    render_assets: &LevelRenderAssets,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Vec<Entity> {
    let origin = Vec3::from_array(chunk.origin);
    let mut entities = Vec::new();
    for object in &chunk.objects {
        let entity = match object {
            ObjectDef::Box(def) => spawn_box(commands, chunk, origin, def, render_assets, meshes, materials),
            ObjectDef::Sphere(def) => spawn_sphere(commands, chunk, origin, def, render_assets, meshes, materials),
            ObjectDef::Cylinder(def) => spawn_cylinder(commands, chunk, origin, def, render_assets, meshes, materials),
            ObjectDef::Model(def) => spawn_model(commands, chunk, origin, def, definition, asset_server),
        };
        entities.push(entity);
    }
    for light in &chunk.lights { entities.push(spawn_light(commands, chunk, origin, light)); }
    for pickup in &chunk.pickups { entities.push(spawn_level_pickup(commands, chunk, origin, pickup, meshes, materials)); }
    for trigger in &chunk.triggers {
        let pos = origin + Vec3::from_array(trigger.pos);
        let entity = commands.spawn((
            LevelOwned,
            LevelTrigger {
                id: trigger.id.clone(),
                half_extents: Vec3::from_array(trigger.size) * 0.5,
                on_enter: trigger.on_enter.clone(),
                on_exit: trigger.on_exit.clone(),
                inside: false,
                once: trigger.once,
                fired: false,
            },
            Transform::from_translation(pos),
        )).id();
        entities.push(entity);
    }
    for interactable in &chunk.interactables {
        let pos = origin + Vec3::from_array(interactable.pos);
        let entity = commands.spawn((
            LevelOwned,
            LevelInteractable { id: interactable.id.clone(), radius: interactable.radius, prompt: interactable.prompt.clone(), event: interactable.event.clone() },
            Transform::from_translation(pos),
        )).id();
        entities.push(entity);
    }
    entities
}

fn spawn_box(
    commands: &mut Commands,
    chunk: &ChunkDef,
    origin: Vec3,
    def: &BoxDef,
    render_assets: &LevelRenderAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let size = Vec3::from_array(def.size) * Vec3::from_array(def.common.transform.scale);
    let transform = world_transform(origin, &def.common.transform).with_scale(Vec3::ONE);
    let material = material_handle(def.common.material.as_deref(), render_assets, materials);
    let mut entity = commands.spawn((
        LevelOwned,
        Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
        MeshMaterial3d(material),
        transform,
        if def.common.visible { Visibility::Visible } else { Visibility::Hidden },
    ));
    if let Some(id) = &def.common.id { entity.insert(LevelNamed(id.clone())); }
    if def.common.collider { entity.insert(LevelCollider { half_extents: size * 0.5 }); }
    entity.id()
}

fn spawn_sphere(
    commands: &mut Commands,
    chunk: &ChunkDef,
    origin: Vec3,
    def: &SphereDef,
    render_assets: &LevelRenderAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let scale = Vec3::from_array(def.common.transform.scale);
    let radius = def.radius * scale.max_element();
    let material = material_handle(def.common.material.as_deref(), render_assets, materials);
    let mut entity = commands.spawn((
        LevelOwned,
        Mesh3d(meshes.add(Sphere::new(radius))),
        MeshMaterial3d(material),
        world_transform(origin, &def.common.transform).with_scale(Vec3::ONE),
        if def.common.visible { Visibility::Visible } else { Visibility::Hidden },
    ));
    if let Some(id) = &def.common.id { entity.insert(LevelNamed(id.clone())); }
    if def.common.collider { entity.insert(LevelCollider { half_extents: Vec3::splat(radius) }); }
    entity.id()
}

fn spawn_cylinder(
    commands: &mut Commands,
    chunk: &ChunkDef,
    origin: Vec3,
    def: &CylinderDef,
    render_assets: &LevelRenderAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let scale = Vec3::from_array(def.common.transform.scale);
    let radius = def.radius * scale.x.max(scale.z);
    let height = def.height * scale.y;
    let material = material_handle(def.common.material.as_deref(), render_assets, materials);
    let mut entity = commands.spawn((
        LevelOwned,
        Mesh3d(meshes.add(Cylinder::new(radius, height))),
        MeshMaterial3d(material),
        world_transform(origin, &def.common.transform).with_scale(Vec3::ONE),
        if def.common.visible { Visibility::Visible } else { Visibility::Hidden },
    ));
    if let Some(id) = &def.common.id { entity.insert(LevelNamed(id.clone())); }
    if def.common.collider { entity.insert(LevelCollider { half_extents: Vec3::new(radius, height * 0.5, radius) }); }
    entity.id()
}

fn spawn_model(
    commands: &mut Commands,
    chunk: &ChunkDef,
    origin: Vec3,
    def: &ModelInstanceDef,
    definition: &LevelDefinition,
    asset_server: &AssetServer,
) -> Entity {
    let model = &definition.assets.models[&def.model];
    let path = format!("{}#Scene{}", model.src, model.scene);
    let mut entity = commands.spawn((
        LevelOwned,
        SceneRoot(asset_server.load(path)),
        world_transform(origin, &def.common.transform),
        if def.common.visible { Visibility::Visible } else { Visibility::Hidden },
    ));
    if let Some(id) = &def.common.id { entity.insert(LevelNamed(id.clone())); }
    if def.common.collider {
        if let Some(size) = def.collider_size { entity.insert(LevelCollider { half_extents: Vec3::from_array(size) * 0.5 }); }
    }
    entity.id()
}

fn spawn_light(commands: &mut Commands, chunk: &ChunkDef, origin: Vec3, def: &LightDef) -> Entity {
    let transform = world_transform(origin, &def.transform);
    let color = Color::srgb(def.color[0], def.color[1], def.color[2]);
    let mut entity = match def.kind {
        LightKind::Point => commands.spawn((
            LevelOwned,
            PointLight { color, intensity: def.intensity, range: def.range, shadows_enabled: def.shadows, ..default() },
            transform,
        )),
        LightKind::Spot => commands.spawn((
            LevelOwned,
            SpotLight {
                color,
                intensity: def.intensity,
                range: def.range,
                shadows_enabled: def.shadows,
                inner_angle: def.inner_angle_deg.to_radians(),
                outer_angle: def.outer_angle_deg.to_radians(),
                ..default()
            },
            transform,
        )),
        LightKind::Directional => commands.spawn((
            LevelOwned,
            DirectionalLight { color, illuminance: def.intensity, shadows_enabled: def.shadows, ..default() },
            transform,
        )),
    };
    if let Some(id) = &def.id { entity.insert(LevelNamed(id.clone())); }
    entity.id()
}

fn spawn_level_pickup(
    commands: &mut Commands,
    chunk: &ChunkDef,
    origin: Vec3,
    def: &PickupDef,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let pos = origin + Vec3::from_array(def.transform.pos);
    let mut entity = commands.spawn((
        LevelOwned,
        Pickup(ItemStack { id: def.item.clone(), count: def.count }),
        Mesh3d(meshes.add(Cuboid::new(0.28, 0.28, 0.28))),
        MeshMaterial3d(materials.add(Color::srgb(0.78, 0.72, 0.32))),
        Transform::from_translation(pos).with_rotation(rotation(def.transform.rot_deg)),
    ));
    if let Some(id) = &def.id { entity.insert(LevelNamed(id.clone())); }
    entity.id()
}

fn world_transform(origin: Vec3, def: &TransformDef) -> Transform {
    Transform::from_translation(origin + Vec3::from_array(def.pos))
        .with_rotation(rotation(def.rot_deg))
        .with_scale(Vec3::from_array(def.scale))
}

fn rotation(rot_deg: [f32; 3]) -> Quat {
    Quat::from_euler(EulerRot::XYZ, rot_deg[0].to_radians(), rot_deg[1].to_radians(), rot_deg[2].to_radians())
}

fn material_handle(name: Option<&str>, render_assets: &LevelRenderAssets, materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    name.and_then(|id| render_assets.materials.get(id).cloned()).unwrap_or_else(|| materials.add(Color::srgb(0.68, 0.62, 0.32)))
}

fn sync_lua_snapshot(
    time: Res<Time>,
    lua: Option<Res<LuaEngine>>,
    player: Query<&Transform, With<Player>>,
    stats: Res<PlayerStats>,
    inventory: Res<Inventory>,
    config: Res<RuntimeConfig>,
    level: Res<LoadedLevel>,
) {
    let Some(lua) = lua else { return; };
    let Ok(player) = player.get_single() else { return; };
    let mut counts = HashMap::<String, u16>::new();
    for stack in inventory.backpack.iter().chain(inventory.hotbar.iter()).chain(inventory.crafting.iter()).flatten() {
        let entry = counts.entry(stack.id.clone()).or_default();
        *entry = entry.saturating_add(stack.count);
    }
    let selected_item = inventory.hotbar.get(inventory.selected).and_then(|v| v.as_ref()).map(|v| v.id.clone());
    let snapshot = LuaSnapshot {
        elapsed: time.elapsed_secs_f64(),
        delta: time.delta_secs(),
        player_position: player.translation.to_array(),
        hunger: stats.hunger,
        sanity: stats.sanity,
        nickname: config.0.nickname.clone(),
        multiplayer: config.0.mode == GameMode::Multiplayer,
        level_id: level.definition.meta.id.clone(),
        level_name: level.definition.meta.name.clone(),
        selected_item,
        inventory_counts: counts,
    };
    if let Ok(lua) = lua.0.lock() { lua.update_snapshot(snapshot); }
}

fn lua_tick(time: Res<Time>, lua: Option<Res<LuaEngine>>, mut timer: ResMut<LuaTickTimer>) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() { return; }
    let Some(lua) = lua else { return; };
    if let Ok(lua) = lua.0.lock() {
        lua.dispatch("tick", &[("dt", EventValue::Number(timer.0.duration().as_secs_f64()))]);
    }
}

fn trigger_events(
    lua: Option<Res<LuaEngine>>,
    player: Query<&Transform, With<Player>>,
    mut triggers: Query<(&Transform, &mut LevelTrigger)>,
) {
    let Some(lua) = lua else { return; };
    let Ok(player) = player.get_single() else { return; };
    for (transform, mut trigger) in &mut triggers {
        if trigger.once && trigger.fired { continue; }
        let delta = player.translation - transform.translation;
        let inside = delta.x.abs() <= trigger.half_extents.x && delta.y.abs() <= trigger.half_extents.y && delta.z.abs() <= trigger.half_extents.z;
        if inside && !trigger.inside {
            trigger.inside = true;
            if trigger.once { trigger.fired = true; }
            if let Ok(lua) = lua.0.lock() {
                lua.dispatch("trigger_enter", &[("id", EventValue::String(trigger.id.clone()))]);
                if let Some(event) = &trigger.on_enter { lua.dispatch(event, &[("id", EventValue::String(trigger.id.clone()))]); }
            }
        } else if !inside && trigger.inside {
            trigger.inside = false;
            if let Ok(lua) = lua.0.lock() {
                lua.dispatch("trigger_exit", &[("id", EventValue::String(trigger.id.clone()))]);
                if let Some(event) = &trigger.on_exit { lua.dispatch(event, &[("id", EventValue::String(trigger.id.clone()))]); }
            }
        }
    }
}

fn interactable_system(
    keys: Res<ButtonInput<KeyCode>>,
    inventory: Res<Inventory>,
    player: Query<&Transform, With<Player>>,
    interactables: Query<(&Transform, &LevelInteractable)>,
    mut prompt: Query<&mut Text, With<LevelInteractionPrompt>>,
    lua: Option<Res<LuaEngine>>,
) {
    let Ok(mut prompt) = prompt.get_single_mut() else { return; };
    if inventory.open { **prompt = String::new(); return; }
    let Ok(player) = player.get_single() else { return; };
    let forward = player.forward().as_vec3();
    let nearest = interactables.iter().filter_map(|(transform, item)| {
        let delta = transform.translation - player.translation;
        let dist = delta.length();
        if dist <= item.radius && delta.normalize_or_zero().dot(forward) > 0.30 { Some((dist, item)) } else { None }
    }).min_by(|a, b| a.0.total_cmp(&b.0));

    if let Some((_, item)) = nearest {
        **prompt = item.prompt.clone();
        if keys.just_pressed(KeyCode::KeyF) {
            if let Some(lua) = lua {
                if let Ok(lua) = lua.0.lock() {
                    lua.dispatch("interact", &[("id", EventValue::String(item.id.clone())), ("event_name", EventValue::String(item.event.clone()))]);
                    lua.dispatch(&item.event, &[("id", EventValue::String(item.id.clone()))]);
                }
            }
        }
    } else {
        **prompt = String::new();
    }
}

fn apply_lua_commands(
    mut commands: Commands,
    lua: Option<Res<LuaEngine>>,
    mut player: Query<&mut Transform, With<Player>>,
    mut stats: ResMut<PlayerStats>,
    mut inventory: ResMut<Inventory>,
    mut state: ResMut<ChunkState>,
    mut named: Query<(Entity, &LevelNamed, Option<&mut Visibility>, Option<&mut Transform>)>,
    mut message: Query<&mut Text, With<LevelMessageText>>,
    mut message_timer: ResMut<MessageTimer>,
    level: Res<LoadedLevel>,
    render_assets: Res<LevelRenderAssets>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(lua) = lua else { return; };
    let queued = lua.0.lock().map(|lua| lua.drain_commands()).unwrap_or_default();
    for command in queued {
        match command {
            LuaCommand::Message { text, seconds } => {
                if let Ok(mut out) = message.get_single_mut() { **out = text; }
                message_timer.0 = Some(Timer::new(Duration::from_secs_f32(seconds), TimerMode::Once));
            }
            LuaCommand::Teleport(pos) => {
                if let Ok(mut player) = player.get_single_mut() { player.translation = Vec3::from_array(pos); }
            }
            LuaCommand::AddHunger(value) => stats.hunger = (stats.hunger + value).clamp(0.0, 100.0),
            LuaCommand::SetHunger(value) => stats.hunger = value.clamp(0.0, 100.0),
            LuaCommand::AddSanity(value) => stats.sanity = (stats.sanity + value).clamp(0.0, 100.0),
            LuaCommand::SetSanity(value) => stats.sanity = value.clamp(0.0, 100.0),
            LuaCommand::GiveItem { item, count } => {
                let _ = inventory.insert_backpack(ItemStack { id: item, count });
            }
            LuaCommand::RemoveItem { item, mut count } => {
                for slot in inventory.backpack.iter_mut().chain(inventory.hotbar.iter_mut()).chain(inventory.crafting.iter_mut()) {
                    if count == 0 { break; }
                    let Some(stack) = slot.as_mut() else { continue; };
                    if stack.id != item { continue; }
                    let take = count.min(stack.count);
                    stack.count -= take;
                    count -= take;
                    if stack.count == 0 { *slot = None; }
                }
            }
            LuaCommand::SpawnBox { id, pos, size, rot_deg, material, collider } => {
                let def = BoxDef {
                    common: CommonObjectDef {
                        id: Some(id),
                        transform: TransformDef { pos, rot_deg, scale: [1.0, 1.0, 1.0] },
                        material,
                        collider,
                        visible: true,
                    },
                    size,
                };
                let chunk = ChunkDef {
                    id: "__lua__".into(), origin: [0.0, 0.0, 0.0], size: [1.0, 1.0, 1.0], load_distance: 0.0, unload_distance: 0.0, always_loaded: true,
                    objects: Vec::new(), lights: Vec::new(), pickups: Vec::new(), triggers: Vec::new(), interactables: Vec::new(), scripts: Vec::new(),
                };
                spawn_box(&mut commands, &chunk, Vec3::ZERO, &def, &render_assets, &mut meshes, &mut materials);
            }
            LuaCommand::SpawnPickup { item, count, pos } => {
                commands.spawn((
                    Pickup(ItemStack { id: item, count }),
                    Mesh3d(meshes.add(Cuboid::new(0.28, 0.28, 0.28))),
                    MeshMaterial3d(materials.add(Color::srgb(0.78, 0.72, 0.32))),
                    Transform::from_translation(Vec3::from_array(pos)),
                ));
            }
            LuaCommand::SpawnModel { id, model, pos, rot_deg, scale, collider_size } => {
                if level.definition.assets.models.contains_key(&model) {
                    let def = ModelInstanceDef {
                        common: CommonObjectDef {
                            id: Some(id),
                            transform: TransformDef { pos, rot_deg, scale },
                            material: None,
                            collider: collider_size.is_some(),
                            visible: true,
                        },
                        model,
                        collider_size,
                    };
                    let chunk = ChunkDef {
                        id: "__lua__".into(), origin: [0.0, 0.0, 0.0], size: [1.0, 1.0, 1.0], load_distance: 0.0, unload_distance: 0.0, always_loaded: true,
                        objects: Vec::new(), lights: Vec::new(), pickups: Vec::new(), triggers: Vec::new(), interactables: Vec::new(), scripts: Vec::new(),
                    };
                    spawn_model(&mut commands, &chunk, Vec3::ZERO, &def, &level.definition, &asset_server);
                } else {
                    eprintln!("lua tried to spawn unknown model asset: {model}");
                }
            }
            LuaCommand::SpawnLight { id, kind, pos, rot_deg, color, intensity, range, shadows } => {
                let kind = match kind.as_str() {
                    "spot" => LightKind::Spot,
                    "directional" => LightKind::Directional,
                    _ => LightKind::Point,
                };
                let def = LightDef {
                    id: Some(id),
                    kind,
                    transform: TransformDef { pos, rot_deg, scale: [1.0, 1.0, 1.0] },
                    color,
                    intensity,
                    range,
                    shadows,
                    inner_angle_deg: 25.0,
                    outer_angle_deg: 45.0,
                };
                let chunk = ChunkDef {
                    id: "__lua__".into(), origin: [0.0, 0.0, 0.0], size: [1.0, 1.0, 1.0], load_distance: 0.0, unload_distance: 0.0, always_loaded: true,
                    objects: Vec::new(), lights: Vec::new(), pickups: Vec::new(), triggers: Vec::new(), interactables: Vec::new(), scripts: Vec::new(),
                };
                spawn_light(&mut commands, &chunk, Vec3::ZERO, &def);
            }
            LuaCommand::Despawn(id) => {
                for (entity, name, _, _) in &mut named { if name.0 == id { commands.entity(entity).despawn_recursive(); } }
            }
            LuaCommand::SetVisible { id, visible } => {
                for (_, name, visibility, _) in &mut named {
                    if name.0 == id {
                        if let Some(mut visibility) = visibility { *visibility = if visible { Visibility::Visible } else { Visibility::Hidden }; }
                    }
                }
            }
            LuaCommand::SetPosition { id, pos } => {
                for (_, name, _, transform) in &mut named {
                    if name.0 == id {
                        if let Some(mut transform) = transform { transform.translation = Vec3::from_array(pos); }
                    }
                }
            }
            LuaCommand::SetRotation { id, rot_deg } => {
                for (_, name, _, transform) in &mut named {
                    if name.0 == id {
                        if let Some(mut transform) = transform { transform.rotation = rotation(rot_deg); }
                    }
                }
            }
            LuaCommand::SetScale { id, scale } => {
                for (_, name, _, transform) in &mut named {
                    if name.0 == id {
                        if let Some(mut transform) = transform { transform.scale = Vec3::from_array(scale); }
                    }
                }
            }
            LuaCommand::LoadChunk(id) => { state.forced_unloaded.remove(&id); state.forced_loaded.insert(id); }
            LuaCommand::UnloadChunk(id) => { state.forced_loaded.remove(&id); state.forced_unloaded.insert(id); }
            LuaCommand::PlaySound(id) => {
                if let Some(sound) = level.definition.assets.sounds.get(&id) {
                    let handle: Handle<AudioSource> = asset_server.load(sound.src.clone());
                    commands.spawn((AudioPlayer(handle), if sound.looped { PlaybackSettings::LOOP } else { PlaybackSettings::DESPAWN }));
                }
            }
            LuaCommand::Emit { name, payload } => {
                if let Ok(lua) = lua.0.lock() { lua.dispatch(&name, &[("payload", EventValue::String(payload))]); }
            }
        }
    }
}

fn clear_level_message(time: Res<Time>, mut timer: ResMut<MessageTimer>, mut text: Query<&mut Text, With<LevelMessageText>>) {
    let Some(active) = timer.0.as_mut() else { return; };
    active.tick(time.delta());
    if active.just_finished() {
        if let Ok(mut text) = text.get_single_mut() { **text = String::new(); }
        timer.0 = None;
    }
}
