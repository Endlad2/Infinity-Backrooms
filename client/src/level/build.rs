//! Построение данных уровня для Bevy-сцены (BDS Level Format v1 + v2).
//! v2: уровень состоит из чанков; локальные координаты внутри чанка.

use std::collections::BTreeMap;

use anyhow::Result;

use super::assets_bridge::ResolvedAssets;
use super::model::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Player,
    Prop,
    Enemy,
    Door,
    Light,
    Pickup,
    Spawner,
    Other,
}

#[derive(Debug, Clone)]
pub enum MeshKind {
    Plane,
    Cube,
    Sphere,
    Cylinder,
    Custom { model_id: String },
}

/// Что именно рисовать для узла: либо примитив (с размерами), либо gltf-модель.
#[derive(Debug, Clone)]
pub struct MeshSpec {
    pub kind: MeshKind,
    /// Размеры меша напрямую (для Cuboid::new(...), Cylinder::new(...) и т.п.).
    /// Для Custom — не используется.
    pub size: [f32; 3],
    /// Модель из cache-files/assets — путь к .gltf, если kind = Custom.
    pub model_path: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LightKindPlan {
    Directional,
    Point,
    Spot,
}

/// Готовый к применению PBR-набор (ссылки на текстурные ID из ResolvedAssets).
#[derive(Debug, Clone, Default)]
pub struct MaterialPlan {
    /// ID для base_color_texture (diff).
    pub base: Option<String>,
    pub normal: Option<String>,
    pub roughness: Option<String>,
    pub ao: Option<String>,
    /// Inline цвет (hex) — если текстуры нет.
    pub color: Option<String>,
    /// tiling (u, v) для UV-скейла.
    pub tiling: Option<[f32; 2]>,
    pub roughness_f32: f32,
    pub metallic_f32: f32,
}

#[derive(Debug, Clone)]
pub struct NodePlan {
    pub id: String,
    pub kind: NodeKind,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
    pub mesh: Option<MeshSpec>,
    pub material: MaterialPlan,
    pub light: Option<(LightKindPlan, String, f32, f32)>,
    pub collider: Option<ColliderPlan>,
    pub stats: StatsDecl,
    pub events: Vec<EntityEvent>,
}

#[derive(Debug, Clone)]
pub enum ColliderPlan {
    Box { size: [f32; 3], sensor: bool, body: String },
    Sphere { radius: f32, sensor: bool, body: String },
    Capsule { radius: f32, height: f32, sensor: bool, body: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkGenPlan {
    Default,
    Axis(String),
    None,
}

#[derive(Debug, Clone)]
pub struct ChunkTemplatePlan {
    pub id: String,
    pub generator: ChunkGenPlan,
    pub chance: f32,
    pub nodes: Vec<NodePlan>,
}

#[derive(Debug, Clone)]
pub struct TriggerPlan {
    pub id: String,
    pub kind: String,
    pub pos: [f32; 3],
    pub size: Option<[f32; 3]>,
    pub radius: Option<f32>,
    pub interval: Option<f32>,
    pub signal: Option<String>,
    pub on_enter: Option<EntityEvent>,
    pub on_exit: Option<EntityEvent>,
    pub on_tick: Option<EntityEvent>,
    pub on_signal: Option<EntityEvent>,
}

#[derive(Debug, Clone)]
pub struct ScenePlan {
    pub spawn_point: Option<String>,
    pub gravity: [f32; 3],
    pub ambient: [f32; 3],
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub nodes: Vec<NodePlan>,
    pub triggers: Vec<TriggerPlan>,
    pub scripts: BTreeMap<String, String>,
    pub chunk_size: Option<[f32; 3]>,
    pub chunk_templates: Vec<ChunkTemplatePlan>,
    pub seed: Option<u64>,
}

impl ScenePlan {
    pub fn find_template(&self, id: &str) -> Option<&ChunkTemplatePlan> {
        self.chunk_templates.iter().find(|t| t.id == id)
    }
}

pub fn build_scene_plan(level: &Level, assets: &ResolvedAssets) -> Result<ScenePlan> {
    let mut nodes = Vec::new();
    let mut chunk_templates = Vec::new();

    for ent in &level.entities {
        nodes.push(build_node_plan(ent, level, assets));
    }

    let chunk_size = level.chunk_size.as_ref().map(|cs| [cs.x, cs.y, cs.z]);
    for ch in &level.chunks {
        let template_nodes: Vec<NodePlan> = ch
            .entities
            .iter()
            .map(|ent| build_node_plan(ent, level, assets))
            .collect();

        let gen_plan = match &ch.generator {
            ChunkGenerator::Default => ChunkGenPlan::Default,
            ChunkGenerator::Axis(ax) => ChunkGenPlan::Axis(ax.clone()),
            ChunkGenerator::None => ChunkGenPlan::None,
        };

        chunk_templates.push(ChunkTemplatePlan {
            id: ch.id.clone(),
            generator: gen_plan.clone(),
            chance: ch.chance,
            nodes: template_nodes.clone(),
        });

        // generator="none" НЕ раскрываем автоматически — он должен спавниться из Lua.
        // Но чтобы игрок хоть что-то увидел, если Lua не сработал, всё же раскроем
        // стартовый чанк (в нём лежит player_start).
        let is_spawn_chunk = template_nodes.iter().any(|n| n.kind == NodeKind::Player);
        let auto_spawn = matches!(gen_plan, ChunkGenPlan::Default | ChunkGenPlan::Axis(_))
            || is_spawn_chunk;
        if auto_spawn {
            let origin = [0.0f32, 0.0, 0.0];
            let world_nodes = expand_chunk_nodes(&template_nodes, &ch.id, origin, chunk_size);
            nodes.extend(world_nodes);
        }
    }

    let triggers = level
        .triggers
        .iter()
        .map(|t| TriggerPlan {
            id: t.id.clone(),
            kind: t.kind.clone().unwrap_or_else(|| "volume".into()),
            pos: t.pos,
            size: t.size,
            radius: t.radius,
            interval: t.interval,
            signal: t.signal.clone(),
            on_enter: t.on_enter.clone(),
            on_exit: t.on_exit.clone(),
            on_tick: t.on_tick.clone(),
            on_signal: t.on_signal.clone(),
        })
        .collect();

    let scripts = level
        .scripts
        .iter()
        .map(|s| (s.id.clone(), s.lua.clone()))
        .collect();

    Ok(ScenePlan {
        spawn_point: level.spawn_point.clone(),
        gravity: level.gravity,
        ambient: level.ambient,
        bounds_min: level.bounds.min,
        bounds_max: level.bounds.max,
        nodes,
        triggers,
        scripts,
        chunk_size,
        chunk_templates,
        seed: level.seed,
    })
}

fn build_node_plan(ent: &Entity, level: &Level, assets: &ResolvedAssets) -> NodePlan {
    let merged = resolve_inheritance(ent, &level.prefabs);
    let kind = match merged.kind.as_deref() {
        Some("player") => NodeKind::Player,
        Some("prop") => NodeKind::Prop,
        Some("enemy") => NodeKind::Enemy,
        Some("door") => NodeKind::Door,
        Some("light") => NodeKind::Light,
        Some("pickup") => NodeKind::Pickup,
        Some("spawner") => NodeKind::Spawner,
        _ => NodeKind::Other,
    };

    let mesh = plan_mesh(&merged);
    let material = plan_material(&merged, level, assets);

    NodePlan {
        id: merged.id.clone(),
        kind,
        pos: merged.transform.pos,
        rot: merged.transform.rot,
        scale: merged.transform.scale,
        mesh,
        material,
        light: plan_light(&merged),
        collider: plan_collider(&merged),
        stats: merged.stats.clone(),
        events: merged.events.clone(),
    }
}

fn hash_grid(seed: u64, grid: (i32, i32, i32)) -> u64 {
    let mut h = seed.wrapping_mul(0x9E3779B97F4A7C15);
    h ^= (grid.0 as i64 as u64).wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= (grid.1 as i64 as u64).wrapping_mul(0x94D049BB133111EB);
    h ^= (grid.2 as i64 as u64).wrapping_mul(0x2545F4914F6CDD1D);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51AFD7ED558CCD);
    h ^= h >> 33;
    h
}

pub(crate) fn pick_template_by_chance<'a>(
    templates: &'a [ChunkTemplatePlan],
    seed: u64,
    grid: (i32, i32, i32),
) -> Option<&'a ChunkTemplatePlan> {
    let total: f32 = templates.iter().map(|t| t.chance.max(0.0)).sum();
    if total <= 0.0 {
        return templates.first();
    }
    let h = hash_grid(seed, grid);
    let r = (h as f64 / u64::MAX as f64) as f32 * total;
    let mut acc = 0.0f32;
    for t in templates {
        let c = t.chance.max(0.0);
        if c <= 0.0 { continue; }
        acc += c;
        if r < acc { return Some(t); }
    }
    templates.last()
}

pub(crate) fn expand_chunk_nodes(
    nodes: &[NodePlan],
    chunk_id: &str,
    chunk_origin: [f32; 3],
    _chunk_size: Option<[f32; 3]>,
) -> Vec<NodePlan> {
    nodes
        .iter()
        .map(|n| {
            let mut p = n.clone();
            p.id = format!("{chunk_id}/{}", n.id);
            p.pos = [
                chunk_origin[0] + n.pos[0],
                chunk_origin[1] + n.pos[1],
                chunk_origin[2] + n.pos[2],
            ];
            p
        })
        .collect()
}

fn resolve_inheritance(ent: &Entity, prefabs: &BTreeMap<String, Entity>) -> Entity {
    match &ent.inherit {
        Some(pid) if prefabs.contains_key(pid) => {
            let proto = &prefabs[pid];
            let mut merged = proto.clone();
            merged.id = ent.id.clone();
            merged.kind = ent.kind.clone().or(proto.kind.clone());
            merged.inherit = None;
            merged.transform = ent.transform.clone();

            // Тонкость: transform.pos по умолчанию [0,0,0] — если у entity явно
            // указан, используем его; иначе берём из prefab.
            if ent.transform.pos == [0.0, 0.0, 0.0]
                && proto.transform.pos != [0.0, 0.0, 0.0]
            {
                merged.transform.pos = proto.transform.pos;
            }
            if ent.transform.rot == [0.0, 0.0, 0.0]
                && proto.transform.rot != [0.0, 0.0, 0.0]
            {
                merged.transform.rot = proto.transform.rot;
            }
            if ent.transform.scale == [0.0, 0.0, 0.0] {
                merged.transform.scale = proto.transform.scale;
            } else if ent.transform.scale != [1.0, 1.0, 1.0] {
                merged.transform.scale = ent.transform.scale;
            } else {
                merged.transform.scale = proto.transform.scale;
            }

            if ent.model.is_some() { merged.model = ent.model.clone(); }
            if ent.material.is_some() { merged.material = ent.material.clone(); }
            if !ent.events.is_empty() { merged.events = ent.events.clone(); }

            // Сливаем render: если у ребёнка задан mesh_shape/inline_material — берём его.
            if ent.render.mesh_shape.is_some() {
                merged.render.mesh_shape = ent.render.mesh_shape.clone();
            }
            if ent.render.mesh_scale.is_some() {
                merged.render.mesh_scale = ent.render.mesh_scale.clone();
            }
            if ent.render.inline_material.is_some() {
                merged.render.inline_material = ent.render.inline_material.clone();
            }
            if ent.render.mesh.is_some() {
                merged.render.mesh = ent.render.mesh.clone();
            }
            merged
        }
        _ => ent.clone(),
    }
}

/// Собирает MeshSpec из merged entity.
/// Приоритеты:
///   1. `<mesh shape=... scale=.../>` (вложенный тег) → примитив с размерами scale.
///   2. `render.mesh="plane|cube|sphere|cylinder"` → примитив размера 1.
///   3. `model=...` или `render.model=...` → gltf-модель.
fn plan_mesh(e: &Entity) -> Option<MeshSpec> {
    // 1. Вложенный <mesh shape= scale=/>
    if let Some(shape) = &e.render.mesh_shape {
        let s = e.render.mesh_scale.unwrap_or([1.0, 1.0, 1.0]);
        let kind = match shape.to_ascii_lowercase().as_str() {
            "plane" => MeshKind::Plane,
            "sphere" => MeshKind::Sphere,
            "cylinder" => MeshKind::Cylinder,
            _ => MeshKind::Cube,
        };
        return Some(MeshSpec { kind, size: s, model_path: None });
    }

    // 2. render.mesh="..."
    if let Some(m) = e.render.mesh.as_deref() {
        let kind = match m {
            "plane" => MeshKind::Plane,
            "sphere" => MeshKind::Sphere,
            "cylinder" => MeshKind::Cylinder,
            "cube" | "box" => MeshKind::Cube,
            other => MeshKind::Custom { model_id: other.to_string() },
        };
        // Для Cube/Plane/Sphere/Cylinder по умолчанию — единичный размер.
        return Some(MeshSpec { kind, size: [1.0, 1.0, 1.0], model_path: None });
    }

    // 3. Модель — gltf/obj.
    if let Some(model_id) = e.render.model.as_ref().or(e.model.as_ref()) {
        return Some(MeshSpec {
            kind: MeshKind::Custom { model_id: model_id.clone() },
            size: [1.0, 1.0, 1.0],
            model_path: None,
        });
    }

    None
}

fn plan_material(e: &Entity, level: &Level, assets: &ResolvedAssets) -> MaterialPlan {
    // 1. Inline-материал прямо на entity (приоритет).
    if let Some(im) = &e.render.inline_material {
        let mut p = MaterialPlan {
            base: im.texture.clone(),
            normal: im.normal.clone(),
            roughness: im.roughness.clone(),
            ao: im.ao.clone(),
            color: None,
            tiling: im.tiling,
            roughness_f32: im.roughness_f32,
            metallic_f32: im.metallic_f32,
        };
        // Fallback — ищем material= через resources (если указан).
        if p.base.is_none() {
            if let Some(mat_id) = e.material.as_ref().or(e.render.material.as_ref()) {
                if let Some(mat) = level.resources.materials.iter().find(|m| &m.id == mat_id) {
                    p = material_to_plan(mat);
                }
            }
        }
        return p;
    }

    // 2. material= → ищем в resources.materials
    if let Some(mat_id) = e.material.as_ref().or(e.render.material.as_ref()) {
        if let Some(mat) = level.resources.materials.iter().find(|m| &m.id == mat_id) {
            return material_to_plan(mat);
        }
    }

    // 3. Ничего.
    let _ = assets;
    MaterialPlan::default()
}

fn material_to_plan(m: &MaterialDecl) -> MaterialPlan {
    MaterialPlan {
        base: m.texture.clone(),
        normal: m.normal.clone(),
        roughness: m.roughness.clone(),
        ao: m.ao.clone(),
        color: None,
        tiling: m.tiling,
        roughness_f32: m.roughness_f32,
        metallic_f32: m.metallic_f32,
    }
}

fn plan_light(e: &Entity) -> Option<(LightKindPlan, String, f32, f32)> {
    let l = e.light.as_ref()?;
    let kind = match l.kind.as_deref() {
        Some("point") => LightKindPlan::Point,
        Some("spot") => LightKindPlan::Spot,
        _ => LightKindPlan::Directional,
    };
    Some((
        kind,
        l.color.clone().unwrap_or_else(|| "#ffffff".into()),
        l.intensity,
        l.range,
    ))
}

fn plan_collider(e: &Entity) -> Option<ColliderPlan> {
    let c = e.physics.collider.as_deref()?;
    let body = e.physics.body.clone().unwrap_or_else(|| "static".to_string());
    let sensor = e.physics.sensor;
    match c {
        "box" => Some(ColliderPlan::Box {
            size: e.physics.size.unwrap_or([1.0, 1.0, 1.0]),
            sensor,
            body,
        }),
        "sphere" => Some(ColliderPlan::Sphere {
            radius: e.physics.radius.unwrap_or(0.5),
            sensor,
            body,
        }),
        "capsule" => Some(ColliderPlan::Capsule {
            radius: e.physics.radius.unwrap_or(0.4),
            height: e.physics.height.unwrap_or(1.8),
            sensor,
            body,
        }),
        "cylinder" => Some(ColliderPlan::Capsule {
            radius: e.physics.radius.unwrap_or(0.5),
            height: e.physics.height.unwrap_or(1.0),
            sensor,
            body,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_simple_level() -> Level {
        let mut l = Level::default();
        l.spawn_point = Some("player_start".into());
        l.bounds = Bounds { min: [-10.0, -2.0, -10.0], max: [10.0, 10.0, 10.0] };

        l.resources.textures.push(TextureDecl {
            id: "tex_ground".into(),
            source: TextureSource::Color("#333333".into()),
            tags: vec![],
            width: 64, height: 64, filter: None, wrap: None,
        });
        l.resources.materials.push(MaterialDecl {
            id: "mat_ground".into(),
            texture: Some("tex_ground".into()),
            roughness_f32: 1.0,
            ..Default::default()
        });

        let mut player = Entity {
            id: "player_start".into(),
            kind: Some("player".into()),
            ..Default::default()
        };
        player.transform.pos = [0.0, 1.0, 0.0];
        l.entities.push(player);

        l
    }

    #[test]
    fn plans_player() {
        let l = mk_simple_level();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();
        let p = plan.nodes.iter().find(|n| n.id == "player_start").unwrap();
        assert_eq!(p.kind, NodeKind::Player);
    }

    fn mk_v2_level() -> Level {
        let mut l = Level::default();
        l.chunk_size = Some(ChunkSize { x: 32.0, y: 16.0, z: 32.0 });
        l.seed = Some(1337);
        let mut ch_default = Chunk {
            id: "chunk_field".into(),
            generator: ChunkGenerator::Default,
            chance: 90.0,
            entities: vec![],
        };
        let mut floor = Entity {
            id: "floor".into(),
            kind: Some("prop".into()),
            ..Default::default()
        };
        floor.transform.pos = [0.0, -7.9, 0.0];
        floor.render.mesh_shape = Some("box".into());
        floor.render.mesh_scale = Some([32.0, 0.2, 32.0]);
        ch_default.entities.push(floor);

        l.chunks.push(ch_default);
        l
    }

    #[test]
    fn v2_builds_chunk_templates_and_expands_default() {
        let l = mk_v2_level();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();

        assert_eq!(plan.chunk_templates.len(), 1);
        assert_eq!(plan.chunk_size, Some([32.0, 16.0, 32.0]));

        let f = plan.nodes.iter().find(|n| n.id == "chunk_field/floor").unwrap();
        assert_eq!(f.pos, [0.0, -7.9, 0.0]);
        match &f.mesh.as_ref().unwrap().kind {
            MeshKind::Cube => {}
            other => panic!("ожидался Cube, получен {:?}", other),
        }
        assert_eq!(f.mesh.as_ref().unwrap().size, [32.0, 0.2, 32.0]);
    }

    #[test]
    fn inline_material_is_used() {
        let xml = r#"<level id="l" format="bds-level/2" seed="1"><chunk_size x="32" y="16" z="32"/><chunks><chunk id="c" generator="none" chance="0">
<entity id="player_start" type="player"><transform pos="0 1 0"/></entity>
<entity id="floor" type="static">
  <mesh shape="box" scale="32 0.2 32"/>
  <material texture="tex_concrete_floor" normal="tex_concrete_floor_nor" tiling="8 8"/>
</entity>
</chunk></chunks></level>"#;
        let l = crate::level::parse::parse_level_xml(xml).unwrap();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();
        let f = plan.nodes.iter().find(|n| n.id == "c/floor").unwrap();
        assert_eq!(f.material.base.as_deref(), Some("tex_concrete_floor"));
        assert_eq!(f.material.normal.as_deref(), Some("tex_concrete_floor_nor"));
        assert_eq!(f.material.tiling, Some([8.0, 8.0]));
    }
}
