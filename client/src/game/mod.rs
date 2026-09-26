//! Основной игровой модуль Bevy: App, сборка сцены из ScenePlan,
//! запуск Lua-хуков, интеграция UI паузы.
//!
//! Сцена строится упрощённо: примитивы (Plane/Cube/Sphere) + DirectionalLight/PointLight,
//! а логика уровня обрабатывается Lua-скриптами (см. scripting::ScriptEngine).

use bevy::prelude::*;

use crate::level::build::{ColliderPlan, LightKindPlan, MeshKind, NodeKind, ScenePlan};
use crate::scripting::ScriptEngine;

/// Ресурс: активный план сцены (для интеграции с Lua и UI).
#[derive(Resource, Default)]
pub struct ActiveScene {
    pub plan: ScenePlanPlaceholder,
    pub level_number: u32,
}

/// Упрощённая «заглушка» — чтобы можно было хранить план как ресурс без Clone.
#[derive(Default)]
pub struct ScenePlanPlaceholder {
    pub loaded: bool,
    pub node_count: usize,
    pub trigger_count: usize,
}

impl From<&ScenePlan> for ScenePlanPlaceholder {
    fn from(p: &ScenePlan) -> Self {
        Self {
            loaded: true,
            node_count: p.nodes.len(),
            trigger_count: p.triggers.len(),
        }
    }
}

/// Ресурс: игровой движок Lua-скриптов.
pub struct LuaRuntime(pub std::sync::Mutex<ScriptEngine>);

/// Компонент: маркер главной камеры.
#[derive(Component)]
pub struct MainCamera;

/// Компонент: сущность узла сцены (соответствует id из XML).
#[derive(Component, Debug, Clone)]
pub struct SceneNodeId(pub String);

/// Компонент: триггер уровня.
#[derive(Component, Debug, Clone)]
pub struct SceneTriggerId(pub String);

/// Система сборки сцены из ScenePlan.
pub fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    plan: Res<ScenePlanResource>,
) {
    let plan = &plan.0;

    for node in &plan.nodes {
        let tf = Transform {
            translation: Vec3::new(node.pos[0], node.pos[1], node.pos[2]),
            rotation: Quat::from_euler(
                EulerRot::XYZ,
                node.rot[0].to_radians(),
                node.rot[1].to_radians(),
                node.rot[2].to_radians(),
            ),
            scale: Vec3::new(node.scale[0], node.scale[1], node.scale[2]),
        };

        // Свет — отдельно
        if node.kind == NodeKind::Light {
            if let Some((kind, color, intensity, range)) = &node.light {
                let c = parse_hex_color(color);
                match kind {
                    LightKindPlan::Directional => {
                        commands.spawn((
                            DirectionalLight {
                                color: c,
                                illuminance: intensity * 10_000.0,
                                shadows_enabled: true,
                                ..default()
                            },
                            tf,
                            SceneNodeId(node.id.clone()),
                        ));
                    }
                    LightKindPlan::Point => {
                        commands.spawn((
                            PointLight {
                                color: c,
                                intensity: intensity * 100_000.0,
                                range: range.max(0.1) * 1.0,
                                shadows_enabled: true,
                                ..default()
                            },
                            tf,
                            SceneNodeId(node.id.clone()),
                        ));
                    }
                    LightKindPlan::Spot => {
                        // У Bevy SpotLight пока не в приоритете — эмулируем Point.
                        commands.spawn((
                            PointLight {
                                color: c,
                                intensity: intensity * 100_000.0,
                                range: range.max(0.1) * 1.0,
                                shadows_enabled: false,
                                ..default()
                            },
                            tf,
                            SceneNodeId(node.id.clone()),
                        ));
                    }
                }
                continue;
            }
        }

        // Игрок — камера + маркер
        if node.kind == NodeKind::Player {
            commands.spawn((
                Camera3dBundle {
                    transform: tf,
                    ..default()
                },
                crate::player::camera::FpsCamera::default(),
                crate::player::Player::default(),
                crate::player::Velocity3::default(),
                crate::player::controller::Grounded(true),
                MainCamera,
                SceneNodeId(node.id.clone()),
            ));
            continue;
        }

        // Всё остальное — меш (или пустышка для коллайдера).
        let (mesh_handle, _is_plane) = match &node.mesh {
            Some(MeshKind::Plane) => (meshes.add(Plane3d::default().mesh().size(1.0, 1.0)), true),
            Some(MeshKind::Cube) | None => (meshes.add(Cuboid::new(1.0, 1.0, 1.0)), false),
            Some(MeshKind::Sphere) => (meshes.add(Sphere::new(0.5)), false),
            Some(MeshKind::Custom { .. }) => {
                // OBJ-меш в этой версии упрощён — используем куб.
                (meshes.add(Cuboid::new(1.0, 1.0, 1.0)), false)
            }
        };

        let color = if let Some(_t) = &node.texture_id {
            Color::srgb(0.8, 0.8, 0.8)
        } else {
            Color::srgb(0.6, 0.6, 0.65)
        };
        let mat = materials.add(StandardMaterial {
            base_color: color,
            ..default()
        });

        let mut ecmd = commands.spawn((
            PbrBundle {
                mesh: mesh_handle,
                material: mat,
                transform: tf,
                ..default()
            },
            SceneNodeId(node.id.clone()),
        ));

        // Коллайдер — пока логический маркер, физика отложена на интеграцию avian/rapier.
        if let Some(col) = &node.collider {
            ecmd.insert(ColliderMarker {
                sensor: match col {
                    ColliderPlan::Box { sensor, .. } => *sensor,
                    ColliderPlan::Sphere { sensor, .. } => *sensor,
                    ColliderPlan::Capsule { sensor, .. } => *sensor,
                },
            });
        }
    }

    // Триггеры — просто маркеры (логика в Lua).
    for trg in &plan.triggers {
        commands.spawn((
            Transform::from_xyz(trg.pos[0], trg.pos[1], trg.pos[2]),
            SceneTriggerId(trg.id.clone()),
        ));
    }
}

/// Маркер коллайдера (без реальной физики).
#[derive(Component, Debug, Clone, Default)]
pub struct ColliderMarker {
    pub sensor: bool,
}

/// Ресурс: план сцены (Arc, чтобы не копировать).
#[derive(Resource)]
pub struct ScenePlanResource(pub std::sync::Arc<ScenePlan>);

impl Default for ScenePlanResource {
    fn default() -> Self {
        // Пустой план по умолчанию.
        Self(std::sync::Arc::new(ScenePlan {
            spawn_point: None,
            gravity: [0.0, -9.81, 0.0],
            ambient: [0.3, 0.3, 0.3],
            bounds_min: [-50.0, -5.0, -50.0],
            bounds_max: [50.0, 5.0, 50.0],
            nodes: Vec::new(),
            triggers: Vec::new(),
            scripts: std::collections::BTreeMap::new(),
            chunk_size: None,
            chunk_templates: Vec::new(),
            seed: None,
        }))
    }
}

/// Система: разовый запуск Lua-хука on_level_start.
pub fn run_on_level_start(
    lua: Option<NonSend<LuaRuntime>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    if let Some(rt) = lua {
        if let Ok(engine) = rt.0.lock() {
            let _ = engine.call_hook0("on_level_start");
        }
    }
    *done = true;
}

/// Система: тик Lua-таймеров каждый кадр.
pub fn tick_lua_timers(
    time: Res<Time>,
    lua: Option<NonSend<LuaRuntime>>,
) {
    if let Some(rt) = lua {
        if let Ok(engine) = rt.0.lock() {
            let _ = engine.tick_timers(time.delta_seconds());
        }
    }
}

/// Компонент: сущность принадлежит чанку с grid-координатами (gx,gy,gz).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkCell(pub i32, pub i32, pub i32);

/// Ресурс: множество grid-ячеек чанков, уже загруженных в мир.
#[derive(Resource, Default)]
pub struct LoadedChunks(pub std::collections::HashSet<(i32, i32, i32)>);

/// Система: рантайм-менеджер чанков (v2).
/// Подгружает чанки вокруг игрока (default — по X/Z, axis — по одной оси),
/// выгружает далёкие. Спавнит через expand_chunk_nodes + pick_template_by_chance.
pub fn chunk_streaming_system(
    time: Res<Time>,
    mut acc: Local<f32>,
    plan: Res<ScenePlanResource>,
    mut loaded: ResMut<LoadedChunks>,
    player: Query<&Transform, With<MainCamera>>,
    existing: Query<(Entity, &ChunkCell)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    *acc += time.delta_seconds();
    if *acc < 0.25 {
        return;
    }
    *acc = 0.0;

    let Some(chunk_size) = plan.0.chunk_size else { return; };
    if plan.0.chunk_templates.is_empty() {
        return;
    }
    let Some(px) = player.iter().next().map(|t| t.translation) else { return; };
    let seed = plan.0.seed.unwrap_or(0);

    // Семейства шаблонов по генератору.
    let mut default_t: Vec<&crate::level::build::ChunkTemplatePlan> = Vec::new();
    let mut axis_y: Vec<&crate::level::build::ChunkTemplatePlan> = Vec::new();
    let mut axis_x: Vec<&crate::level::build::ChunkTemplatePlan> = Vec::new();
    let mut axis_z: Vec<&crate::level::build::ChunkTemplatePlan> = Vec::new();
    for t in &plan.0.chunk_templates {
        match &t.generator {
            crate::level::build::ChunkGenPlan::Default => default_t.push(t),
            crate::level::build::ChunkGenPlan::Axis(a) => match a.as_str() {
                "y" => axis_y.push(t),
                "x" => axis_x.push(t),
                "z" => axis_z.push(t),
                _ => {}
            },
            crate::level::build::ChunkGenPlan::None => {}
        }
    }

    let radius: i32 = 1;
    let cs = chunk_size;

    // Хелпер: заспавнить чанк-шаблон по grid-ячейке.
    let mut spawn_at = |commands: &mut Commands,
                        meshes: &mut ResMut<Assets<Mesh>>,
                        materials: &mut ResMut<Assets<StandardMaterial>>,
                        loaded: &mut ResMut<LoadedChunks>,
                        tmpl: &crate::level::build::ChunkTemplatePlan,
                        grid: (i32, i32, i32)| {
        if loaded.0.contains(&grid) {
            return;
        }
        let origin = [grid.0 as f32 * cs[0], grid.1 as f32 * cs[1], grid.2 as f32 * cs[2]];
        let cid = format!("{}/{}", tmpl.id, format!("{}_{}_{}", grid.0, grid.1, grid.2));
        let nodes = crate::level::build::expand_chunk_nodes(&tmpl.nodes, &cid, origin, Some(cs));
        let cell = ChunkCell(grid.0, grid.1, grid.2);
        for n in &nodes {
            spawn_one_node(commands, meshes, materials, n, cell);
        }
        loaded.0.insert(grid);
    };

    // Default: сетка X/Z вокруг игрока.
    let pgx = (px.x / cs[0]).floor() as i32;
    let pgz = (px.z / cs[2]).floor() as i32;
    if !default_t.is_empty() {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let grid = (pgx + dx, 0, pgz + dz);
                if let Some(t) = crate::level::build::pick_template_by_chance(&default_t.iter().map(|x| (*x).clone()).collect::<Vec<_>>(), seed, grid) {
                    spawn_at(&mut commands, &mut meshes, &mut materials, &mut loaded, t, grid);
                }
            }
        }
    }

    // Axis("y"): сетка по Y.
    let pgy = (px.y / cs[1]).floor() as i32;
    if !axis_y.is_empty() {
        for dy in -radius..=radius {
            let grid = (0, pgy + dy, 0);
            if let Some(t) = crate::level::build::pick_template_by_chance(&axis_y.iter().map(|x| (*x).clone()).collect::<Vec<_>>(), seed, grid) {
                spawn_at(&mut commands, &mut meshes, &mut materials, &mut loaded, t, grid);
            }
        }
    }

    // Axis("x"): сетка по X (y=0, z=0).
    if !axis_x.is_empty() {
        for dx in -radius..=radius {
            let grid = (pgx + dx, 0, 0);
            if let Some(t) = crate::level::build::pick_template_by_chance(&axis_x.iter().map(|x| (*x).clone()).collect::<Vec<_>>(), seed, grid) {
                spawn_at(&mut commands, &mut meshes, &mut materials, &mut loaded, t, grid);
            }
        }
    }

    // Axis("z"): сетка по Z (x=0, y=0).
    if !axis_z.is_empty() {
        for dz in -radius..=radius {
            let grid = (0, 0, pgz + dz);
            if let Some(t) = crate::level::build::pick_template_by_chance(&axis_z.iter().map(|x| (*x).clone()).collect::<Vec<_>>(), seed, grid) {
                spawn_at(&mut commands, &mut meshes, &mut materials, &mut loaded, t, grid);
            }
        }
    }

    // Выгрузка далёких чанков.
    let far: Vec<((i32, i32, i32), Entity)> = existing
        .iter()
        .filter_map(|(e, c)| {
            let g = (c.0, c.1, c.2);
            let dx = (g.0 - pgx).abs();
            let dz = (g.2 - pgz).abs();
            let dy = (g.1 - pgy).abs();
            let outside_xz = dx > radius + 1 || dz > radius + 1;
            let outside_y = dy > radius + 1;
            if outside_xz || outside_y { Some((g, e)) } else { None }
        })
        .collect();
    for (g, e) in far {
        commands.entity(e).despawn_recursive();
        loaded.0.remove(&g);
    }
}

/// Спавн одного узла NodePlan с привязкой к чанку.
pub fn spawn_one_node(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    node: &crate::level::build::NodePlan,
    cell: ChunkCell,
) {
    use crate::level::build::{LightKindPlan, MeshKind, NodeKind};

    let tf = Transform {
        translation: Vec3::new(node.pos[0], node.pos[1], node.pos[2]),
        rotation: Quat::from_euler(
            EulerRot::XYZ,
            node.rot[0].to_radians(),
            node.rot[1].to_radians(),
            node.rot[2].to_radians(),
        ),
        scale: Vec3::new(node.scale[0], node.scale[1], node.scale[2]),
    };

    if node.kind == NodeKind::Light {
        if let Some((kind, color, intensity, range)) = &node.light {
            let c = parse_hex_color(color);
            match kind {
                LightKindPlan::Directional => {
                    commands.spawn((
                        DirectionalLight { color: c, illuminance: intensity * 10_000.0, shadows_enabled: true, ..default() },
                        tf, cell, SceneNodeId(node.id.clone()),
                    ));
                }
                _ => {
                    commands.spawn((
                        PointLight { color: c, intensity: intensity * 100_000.0, range: range.max(0.1), shadows_enabled: false, ..default() },
                        tf, cell, SceneNodeId(node.id.clone()),
                    ));
                }
            }
            return;
        }
    }

    let mesh_handle = match &node.mesh {
        Some(MeshKind::Plane) => meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
        Some(MeshKind::Sphere) => meshes.add(Sphere::new(0.5)),
        _ => meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
    };
    let mat = materials.add(StandardMaterial { base_color: Color::srgb(0.65, 0.65, 0.7), ..default() });
    commands.spawn((
        PbrBundle { mesh: mesh_handle, material: mat, transform: tf, ..default() },
        cell, SceneNodeId(node.id.clone()),
    ));
}

/// Плагин игры: собирает все ресурсы, регистрирует системы.
pub struct GamePlugin {
    pub plan: std::sync::Arc<ScenePlan>,
    /// ScriptEngine внутри Mutex, чтобы GamePlugin был Sync (mlua::Lua не Sync).
    pub engine: std::sync::Mutex<Option<ScriptEngine>>,
    pub level_number: u32,
}

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ScenePlanResource(self.plan.clone()))
            .init_resource::<ActiveScene>()
            .init_resource::<LoadedChunks>()
            .add_systems(Startup, spawn_scene)
            .add_systems(Update, (run_on_level_start, tick_lua_timers, chunk_streaming_system));

        let mut active = ActiveScene::default();
        active.plan = ScenePlanPlaceholder::from(self.plan.as_ref());
        active.level_number = self.level_number;
        app.insert_resource(active);

        if let Some(engine) = self.engine.lock().unwrap().take() {
            app.insert_non_send_resource(LuaRuntime(std::sync::Mutex::new(engine)));
        }
    }
}

/// Парсит hex-цвет #rrggbb или "r g b" (0..1) в Color.
pub fn parse_hex_color(s: &str) -> Color {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix('#') {
        if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
            }
        }
    }
    Color::WHITE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_basic() {
        let c = parse_hex_color("#ff0000");
        assert_eq!(c, Color::srgb(1.0, 0.0, 0.0));
    }

    #[test]
    fn parse_hex_invalid_is_white() {
        assert_eq!(parse_hex_color("not-a-color"), Color::WHITE);
    }

    #[test]
    fn empty_plan_resource_ok() {
        let r = ScenePlanResource::default();
        assert!(r.0.nodes.is_empty());
    }

    #[test]
    fn game_plugin_builds() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin, bevy::asset::AssetPlugin::default()));
        app.init_asset::<bevy::render::mesh::Mesh>();
        app.init_asset::<bevy::render::texture::Image>();
        app.init_asset::<bevy::pbr::StandardMaterial>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_plugins(GamePlugin {
            plan: std::sync::Arc::new(ScenePlan {
                spawn_point: None,
                gravity: [0.0, -9.81, 0.0],
                ambient: [0.3, 0.3, 0.3],
                bounds_min: [-10.0, -2.0, -10.0],
                bounds_max: [10.0, 10.0, 10.0],
                nodes: Vec::new(),
                triggers: Vec::new(),
                scripts: std::collections::BTreeMap::new(),
                chunk_size: None,
                chunk_templates: Vec::new(),
                seed: None,
            }),
            engine: std::sync::Mutex::new(None),
            level_number: 0,
        });
        app.update();
    }
}