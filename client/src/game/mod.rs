//! Основной игровой модуль Bevy: App, сборка сцены из ScenePlan,
//! запуск Lua-хуков, интеграция UI паузы, чанковый стриминг.

use bevy::prelude::*;

use crate::level::build::{ColliderPlan, LightKindPlan, MeshKind, NodeKind, ScenePlan};
use crate::scripting::ScriptEngine;

#[derive(Resource, Default)]
pub struct ActiveScene {
    pub plan: ScenePlanPlaceholder,
    pub level_number: u32,
}

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

pub struct LuaRuntime(pub std::sync::Mutex<ScriptEngine>);

#[derive(Component)]
pub struct MainCamera;

#[derive(Component, Debug, Clone)]
pub struct SceneNodeId(pub String);

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
    let mut player_spawned = false;

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
            player_spawned = true;
            continue;
        }

        let (mesh_handle, _is_plane) = match &node.mesh {
            Some(MeshKind::Plane) => (meshes.add(Plane3d::default().mesh().size(1.0, 1.0)), true),
            Some(MeshKind::Cube) | None => (meshes.add(Cuboid::new(1.0, 1.0, 1.0)), false),
            Some(MeshKind::Sphere) => (meshes.add(Sphere::new(0.5)), false),
            Some(MeshKind::Custom { .. }) => {
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

    for trg in &plan.triggers {
        commands.spawn((
            Transform::from_xyz(trg.pos[0], trg.pos[1], trg.pos[2]),
            SceneTriggerId(trg.id.clone()),
        ));
    }

    // === FALLBACK: если игрок не был найден в плане — спавним камеру сами.
    // Иначе Bevy не рендерит UI и сразу закрывает окно.
    if !player_spawned {
        eprintln!(
            "[game] ВНИМАНИЕ: в ScenePlan нет узла kind=Player. \
             Спавним fallback-камеру в (0, 2, 0). Проверь XML уровня — \
             там должна быть сущность type=\"player\" (например player_start)."
        );

        // Ищем точку спавна из <spawn_point> — если есть, используем её координаты.
        let spawn_pos = plan
            .spawn_point
            .as_ref()
            .and_then(|id| plan.nodes.iter().find(|n| &n.id == id))
            .map(|n| Vec3::new(n.pos[0], n.pos[1], n.pos[2]))
            .unwrap_or(Vec3::new(0.0, 2.0, 0.0));

        commands.spawn((
            Camera3dBundle {
                transform: Transform::from_translation(spawn_pos),
                ..default()
            },
            crate::player::camera::FpsCamera::default(),
            crate::player::Player::default(),
            crate::player::Velocity3::default(),
            crate::player::controller::Grounded(true),
            MainCamera,
            SceneNodeId("__fallback_camera__".into()),
        ));

        // И добавляем свет, чтобы не было совсем чёрно.
        commands.spawn((
            DirectionalLight {
                illuminance: 5_000.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.5, 0.5, 0.0)),
        ));

        // И ambient, чтобы PBR-материалы были видны даже без источников света.
        commands.insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 200.0,
        });

        // И пол под ногами — чтобы было куда смотреть.
        let floor_mesh = meshes.add(Plane3d::default().mesh().size(64.0, 64.0));
        let floor_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.45, 0.40, 0.28),
            ..default()
        });
        commands.spawn((
            PbrBundle {
                mesh: floor_mesh,
                material: floor_mat,
                transform: Transform::from_xyz(0.0, -0.5, 0.0),
                ..default()
            },
        ));
    }

    // На всякий случай — если в плане вообще не было света, добавим ambient.
    if plan.nodes.iter().all(|n| n.kind != NodeKind::Light) {
        commands.insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 100.0,
        });
    }
}

#[derive(Component, Debug, Clone, Default)]
pub struct ColliderMarker {
    pub sensor: bool,
}

#[derive(Resource)]
pub struct ScenePlanResource(pub std::sync::Arc<ScenePlan>);

impl Default for ScenePlanResource {
    fn default() -> Self {
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

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkCell(pub i32, pub i32, pub i32);

#[derive(Resource, Default)]
pub struct LoadedChunks(pub std::collections::HashSet<(i32, i32, i32)>);

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

    fn spawn_at(
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
        loaded: &mut ResMut<LoadedChunks>,
        tmpl: &crate::level::build::ChunkTemplatePlan,
        grid: (i32, i32, i32),
        cs: [f32; 3],
    ) {
        if loaded.0.contains(&grid) {
            return;
        }
        let origin = [
            grid.0 as f32 * cs[0],
            grid.1 as f32 * cs[1],
            grid.2 as f32 * cs[2],
        ];
        let cid = format!("{}/{}", tmpl.id, format!("{}_{}_{}", grid.0, grid.1, grid.2));
        let nodes = crate::level::build::expand_chunk_nodes(&tmpl.nodes, &cid, origin, Some(cs));
        let cell = ChunkCell(grid.0, grid.1, grid.2);
        for n in &nodes {
            spawn_one_node(commands, meshes, materials, n, cell);
        }
        loaded.0.insert(grid);
    }

    let pgx = (px.x / cs[0]).floor() as i32;
    let pgz = (px.z / cs[2]).floor() as i32;
    if !default_t.is_empty() {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let grid = (pgx + dx, 0, pgz + dz);
                let picked = crate::level::build::pick_template_by_chance(
                    &default_t.iter().map(|x| (*x).clone()).collect::<Vec<_>>(),
                    seed,
                    grid,
                )
                .cloned();
                if let Some(t) = picked {
                    spawn_at(&mut commands, &mut meshes, &mut materials, &mut loaded, &t, grid, cs);
                }
            }
        }
    }

    let pgy = (px.y / cs[1]).floor() as i32;
    if !axis_y.is_empty() {
        for dy in -radius..=radius {
            let grid = (0, pgy + dy, 0);
            let picked = crate::level::build::pick_template_by_chance(
                &axis_y.iter().map(|x| (*x).clone()).collect::<Vec<_>>(),
                seed,
                grid,
            )
            .cloned();
            if let Some(t) = picked {
                spawn_at(&mut commands, &mut meshes, &mut materials, &mut loaded, &t, grid, cs);
            }
        }
    }

    if !axis_x.is_empty() {
        for dx in -radius..=radius {
            let grid = (pgx + dx, 0, 0);
            let picked = crate::level::build::pick_template_by_chance(
                &axis_x.iter().map(|x| (*x).clone()).collect::<Vec<_>>(),
                seed,
                grid,
            )
            .cloned();
            if let Some(t) = picked {
                spawn_at(&mut commands, &mut meshes, &mut materials, &mut loaded, &t, grid, cs);
            }
        }
    }

    if !axis_z.is_empty() {
        for dz in -radius..=radius {
            let grid = (0, 0, pgz + dz);
            let picked = crate::level::build::pick_template_by_chance(
                &axis_z.iter().map(|x| (*x).clone()).collect::<Vec<_>>(),
                seed,
                grid,
            )
            .cloned();
            if let Some(t) = picked {
                spawn_at(&mut commands, &mut meshes, &mut materials, &mut loaded, &t, grid, cs);
            }
        }
    }

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

pub struct GamePlugin {
    pub plan: std::sync::Arc<ScenePlan>,
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
