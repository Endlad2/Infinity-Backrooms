//! Основной игровой модуль Bevy.

pub mod chunk_registry;
pub mod net_context;

use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::texture::Image;

use crate::level::assets_bridge::ResolvedAssets;
use crate::level::build::{ColliderPlan, LightKindPlan, MeshKind, NodeKind, ScenePlan};
use crate::scripting::ScriptEngine;

#[allow(unused_imports)]
pub use net_context::{NetContext, NetRole};

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

#[derive(Resource, Default)]
pub struct ResolvedAssetsResource {
    pub textures: std::collections::BTreeMap<String, Vec<u8>>,
    pub models: std::collections::BTreeMap<String, Vec<u8>>,
    pub origins: std::collections::BTreeMap<String, &'static str>,
}

impl From<&ResolvedAssets> for ResolvedAssetsResource {
    fn from(a: &ResolvedAssets) -> Self {
        Self {
            textures: a.textures.clone(),
            models: a.models.clone(),
            origins: a.origins.clone(),
        }
    }
}

#[derive(Resource, Default)]
pub struct TextureCache {
    pub map: std::collections::HashMap<String, Handle<Image>>,
}

fn load_png_into_images(bytes: &[u8], images: &mut ResMut<Assets<Image>>) -> Option<Handle<Image>> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let bevy_img = Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba.into_raw(),
        TextureFormat::Rgba8UnormSrgb,
        bevy::render::render_asset::RenderAssetUsages::default(),
    );
    Some(images.add(bevy_img))
}

pub fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    plan: Res<ScenePlanResource>,
    resolved: Res<ResolvedAssetsResource>,
    mut tex_cache: ResMut<TextureCache>,
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
            scale: Vec3::new(
                node.scale[0].max(1e-6),
                node.scale[1].max(1e-6),
                node.scale[2].max(1e-6),
            ),
        };

        if node.kind == NodeKind::Light {
            if let Some((kind, color, intensity, range, _flicker)) = &node.light {
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
                    LightKindPlan::Point | LightKindPlan::Spot => {
                        commands.spawn((
                            PointLight {
                                color: c,
                                intensity: intensity * 100_000.0,
                                range: range.max(0.1),
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
                Camera3dBundle { transform: tf, ..default() },
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

        let mesh_handle = build_mesh_handle(&node.mesh, &mut meshes);
        let material = build_material(node, &resolved, &mut images, &mut tex_cache);
        let mat_handle = materials.add(material);

        let mut ecmd = commands.spawn((
            PbrBundle {
                mesh: mesh_handle,
                material: mat_handle,
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

    if !player_spawned {
        eprintln!("[game] нет узла Player. Спавним fallback-камеру в (0, 2, 0).");
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

        commands.spawn((
            DirectionalLight {
                illuminance: 5_000.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.5, 0.5, 0.0)),
        ));

        commands.insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 200.0,
        });

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

    if plan.nodes.iter().all(|n| n.kind != NodeKind::Light) {
        commands.insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 100.0,
        });
    }
}

fn build_mesh_handle(
    spec: &Option<crate::level::build::MeshSpec>,
    meshes: &mut ResMut<Assets<Mesh>>,
) -> Handle<Mesh> {
    let Some(spec) = spec else {
        return meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    };
    match &spec.kind {
        MeshKind::Plane => meshes.add(Plane3d::default().mesh().size(spec.size[0], spec.size[2])),
        MeshKind::Cube => meshes.add(Cuboid::new(spec.size[0], spec.size[1], spec.size[2])),
        MeshKind::Sphere => meshes.add(Sphere::new(spec.size[0].max(0.01))),
        MeshKind::Cylinder => {
            meshes.add(Cylinder::new(spec.size[0].max(0.01), spec.size[1].max(0.01)))
        }
        MeshKind::Custom { .. } => meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
    }
}

fn build_material(
    node: &crate::level::build::NodePlan,
    resolved: &ResolvedAssetsResource,
    images: &mut ResMut<Assets<Image>>,
    cache: &mut ResMut<TextureCache>,
) -> StandardMaterial {
    let mut mat = StandardMaterial {
        base_color: Color::srgb(0.7, 0.7, 0.72),
        perceptual_roughness: node.material.roughness_f32.max(0.05),
        metallic: node.material.metallic_f32,
        ..default()
    };

    if let Some(id) = &node.material.base {
        if let Some(h) = get_or_load_image(id, resolved, images, cache) {
            mat.base_color_texture = Some(h);
        }
    }
    if let Some(id) = &node.material.normal {
        if let Some(h) = get_or_load_image(id, resolved, images, cache) {
            mat.normal_map_texture = Some(h);
        }
    }
    if let Some(id) = &node.material.roughness {
        if let Some(h) = get_or_load_image(id, resolved, images, cache) {
            mat.metallic_roughness_texture = Some(h);
        }
    }
    if let Some(id) = &node.material.ao {
        if let Some(h) = get_or_load_image(id, resolved, images, cache) {
            mat.occlusion_texture = Some(h);
        }
    }

    if mat.base_color_texture.is_none() {
        if let Some(hex) = &node.material.color {
            mat.base_color = parse_hex_color(hex);
        }
    }

    mat
}

fn get_or_load_image(
    id: &str,
    resolved: &ResolvedAssetsResource,
    images: &mut ResMut<Assets<Image>>,
    cache: &mut ResMut<TextureCache>,
) -> Option<Handle<Image>> {
    if let Some(h) = cache.map.get(id) {
        return Some(h.clone());
    }
    let bytes = resolved.textures.get(id)?;
    let h = load_png_into_images(bytes, images)?;
    cache.map.insert(id.to_string(), h.clone());
    Some(h)
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
    if *done { return; }
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
    resolved: Res<ResolvedAssetsResource>,
    mut loaded: ResMut<LoadedChunks>,
    player: Query<&Transform, With<MainCamera>>,
    existing: Query<(Entity, &ChunkCell)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut tex_cache: ResMut<TextureCache>,
) {
    *acc += time.delta_seconds();
    if *acc < 0.25 { return; }
    *acc = 0.0;

    let Some(chunk_size) = plan.0.chunk_size else { return; };
    if plan.0.chunk_templates.is_empty() { return; }
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
        images: &mut ResMut<Assets<Image>>,
        tex_cache: &mut ResMut<TextureCache>,
        resolved: &ResolvedAssetsResource,
        loaded: &mut ResMut<LoadedChunks>,
        tmpl: &crate::level::build::ChunkTemplatePlan,
        grid: (i32, i32, i32),
        cs: [f32; 3],
    ) {
        if loaded.0.contains(&grid) { return; }
        let origin = [grid.0 as f32 * cs[0], grid.1 as f32 * cs[1], grid.2 as f32 * cs[2]];
        let cid = format!("{}/{}", tmpl.id, format!("{}_{}_{}", grid.0, grid.1, grid.2));
        let nodes = crate::level::build::expand_chunk_nodes(&tmpl.nodes, &cid, origin, Some(cs));
        let cell = ChunkCell(grid.0, grid.1, grid.2);

        for n in &nodes {
            spawn_one_node(commands, meshes, materials, images, tex_cache, resolved, n, cell);
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
                    seed, grid,
                ).cloned();
                if let Some(t) = picked {
                    spawn_at(&mut commands, &mut meshes, &mut materials, &mut images,
                             &mut tex_cache, &resolved, &mut loaded, &t, grid, cs);
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
                seed, grid,
            ).cloned();
            if let Some(t) = picked {
                spawn_at(&mut commands, &mut meshes, &mut materials, &mut images,
                         &mut tex_cache, &resolved, &mut loaded, &t, grid, cs);
            }
        }
    }

    if !axis_x.is_empty() {
        for dx in -radius..=radius {
            let grid = (pgx + dx, 0, 0);
            let picked = crate::level::build::pick_template_by_chance(
                &axis_x.iter().map(|x| (*x).clone()).collect::<Vec<_>>(),
                seed, grid,
            ).cloned();
            if let Some(t) = picked {
                spawn_at(&mut commands, &mut meshes, &mut materials, &mut images,
                         &mut tex_cache, &resolved, &mut loaded, &t, grid, cs);
            }
        }
    }

    if !axis_z.is_empty() {
        for dz in -radius..=radius {
            let grid = (0, 0, pgz + dz);
            let picked = crate::level::build::pick_template_by_chance(
                &axis_z.iter().map(|x| (*x).clone()).collect::<Vec<_>>(),
                seed, grid,
            ).cloned();
            if let Some(t) = picked {
                spawn_at(&mut commands, &mut meshes, &mut materials, &mut images,
                         &mut tex_cache, &resolved, &mut loaded, &t, grid, cs);
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
            if dx > radius + 1 || dz > radius + 1 || dy > radius + 1 { Some((g, e)) } else { None }
        })
        .collect();
    for (g, e) in far {
        commands.entity(e).despawn_recursive();
        loaded.0.remove(&g);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_one_node(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    tex_cache: &mut ResMut<TextureCache>,
    resolved: &ResolvedAssetsResource,
    node: &crate::level::build::NodePlan,
    cell: ChunkCell,
) {
    use crate::level::build::{LightKindPlan, NodeKind};

    let tf = Transform {
        translation: Vec3::new(node.pos[0], node.pos[1], node.pos[2]),
        rotation: Quat::from_euler(
            EulerRot::XYZ,
            node.rot[0].to_radians(),
            node.rot[1].to_radians(),
            node.rot[2].to_radians(),
        ),
        scale: Vec3::new(node.scale[0].max(1e-6), node.scale[1].max(1e-6), node.scale[2].max(1e-6)),
    };

    if node.kind == NodeKind::Light {
        if let Some((kind, color, intensity, range, _flicker)) = &node.light {
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

    let mesh_handle = build_mesh_handle(&node.mesh, meshes);
    let mat = build_material(node, resolved, images, tex_cache);
    let mat_handle = materials.add(mat);

    commands.spawn((
        PbrBundle { mesh: mesh_handle, material: mat_handle, transform: tf, ..default() },
        cell, SceneNodeId(node.id.clone()),
    ));
}

pub fn handle_export_request(
    mut events: EventReader<crate::ui::ExportLevelRequest>,
    paths: Option<Res<crate::paths::AppPathsResource>>,
    active: Res<ActiveScene>,
) {
    for _ev in events.read() {
        let Some(paths) = paths.as_ref() else {
            eprintln!("[export] нет AppPathsResource");
            continue;
        };
        match crate::level::export::export_level_zip(paths.paths(), active.level_number) {
            Ok(zip) => {
                println!("[export] zip готов: {}", zip.display());
                let default_name = format!("level_{}.zip", active.level_number);
                match crate::level::export_dialog::show_save_dialog(&default_name) {
                    Some(target) => {
                        match std::fs::copy(&zip, &target) {
                            Ok(_) => println!("[export] сохранено в {}", target.display()),
                            Err(e) => eprintln!("[export] ошибка копирования: {e}"),
                        }
                    }
                    None => println!("[export] отменено; файл в {}", zip.display()),
                }
            }
            Err(e) => eprintln!("[export] ошибка: {e}"),
        }
    }
}

pub struct GamePlugin {
    pub plan: std::sync::Arc<ScenePlan>,
    pub assets: std::sync::Arc<ResolvedAssets>,
    pub engine: std::sync::Mutex<Option<ScriptEngine>>,
    pub level_number: u32,
    pub net_ctx: Option<NetContext>,
}

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ScenePlanResource(self.plan.clone()))
            .insert_resource(ResolvedAssetsResource::from(self.assets.as_ref()))
            .init_resource::<TextureCache>()
            .init_resource::<ActiveScene>()
            .init_resource::<LoadedChunks>()
            .add_systems(Startup, spawn_scene)
            .add_systems(Update, (
                run_on_level_start,
                tick_lua_timers,
                chunk_streaming_system,
                handle_export_request,
            ));

        let mut active = ActiveScene::default();
        active.plan = ScenePlanPlaceholder::from(self.plan.as_ref());
        active.level_number = self.level_number;
        app.insert_resource(active);

        if let Some(engine) = self.engine.lock().unwrap().take() {
            app.insert_non_send_resource(LuaRuntime(std::sync::Mutex::new(engine)));
        }

        if let Some(ctx) = &self.net_ctx {
            let cloned = NetContext {
                role: ctx.role,
                host_base_url: ctx.host_base_url.clone(),
                level_number: ctx.level_number,
                net_port: ctx.net_port,
                http_port: ctx.http_port,
                chunks: crate::game::chunk_registry::ChunkRegistry::new(ctx.chunks.my_player_id),
            };
            app.insert_resource(cloned);
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
}
