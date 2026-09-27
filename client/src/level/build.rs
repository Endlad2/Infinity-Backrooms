//! Построение данных уровня для Bevy-сцены (v1 + v2).

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

#[derive(Debug, Clone)]
pub struct MeshSpec {
    pub kind: MeshKind,
    pub size: [f32; 3],
    pub model_path: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LightKindPlan {
    Directional,
    Point,
    Spot,
}

#[derive(Debug, Clone, Default)]
pub struct MaterialPlan {
    pub base: Option<String>,
    pub normal: Option<String>,
    pub roughness: Option<String>,
    pub ao: Option<String>,
    pub arm: Option<String>,
    pub color: Option<String>,
    pub emissive: Option<String>,
    pub emissive_strength: f32,
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
    pub light: Option<(LightKindPlan, String, f32, f32, f32)>, // + flicker
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

        let is_spawn_chunk = template_nodes.iter().any(|n| n.kind == NodeKind::Player);
        let auto_spawn = matches!(gen_plan, ChunkGenPlan::Default | ChunkGenPlan::Axis(_))
            || is_spawn_chunk;
        if auto_spawn {
            let origin = [0.0f32, 0.0, 0.0];
            let world_nodes = expand_chunk_nodes(&template_nodes, &ch.id, origin, chunk_size);
            nodes.extend(world_nodes);
        }
    }

    let triggers = level.triggers.iter().map(|t| TriggerPlan {
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
    }).collect();

    let scripts = level.scripts.iter().map(|s| (s.id.clone(), s.lua.clone())).collect();

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
        // ИИ пишет type="static_body" / "static" / "volume" / "decor" —
        // всё это статические объекты, которые рисуются как обычные меши.
        Some("static_body") | Some("static") | Some("volume") | Some("decor") => NodeKind::Prop,
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
    if total <= 0.0 { return templates.first(); }
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
    nodes.iter().map(|n| {
        let mut p = n.clone();
        p.id = format!("{chunk_id}/{}", n.id);
        p.pos = [
            chunk_origin[0] + n.pos[0],
            chunk_origin[1] + n.pos[1],
            chunk_origin[2] + n.pos[2],
        ];
        p
    }).collect()
}

fn resolve_inheritance(ent: &Entity, prefabs: &BTreeMap<String, Entity>) -> Entity {
    match &ent.inherit {
        Some(pid) if prefabs.contains_key(pid) => {
            let proto = &prefabs[pid];
            let mut merged = proto.clone();
            merged.id = ent.id.clone();
            merged.kind = ent.kind.clone().or(proto.kind.clone());
            merged.inherit = None;

            // Слияние transform: если у ребёнка pos не [0,0,0] — берём его,
            // иначе — из prefab.
            merged.transform = ent.transform.clone();
            if ent.transform.pos == [0.0, 0.0, 0.0] && proto.transform.pos != [0.0, 0.0, 0.0] {
                merged.transform.pos = proto.transform.pos;
            }
            if ent.transform.rot == [0.0, 0.0, 0.0] && proto.transform.rot != [0.0, 0.0, 0.0] {
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

            if ent.render.mesh_shape.is_some() {
                merged.render.mesh_shape = ent.render.mesh_shape.clone();
            }
            if ent.render.mesh_scale.is_some() {
                merged.render.mesh_scale = ent.render.mesh_scale.clone();
            }
            if ent.render.inline_material.is_some() {
                merged.render.inline_material = ent.render.inline_material.clone();
            }
            if ent.render.mesh.is_some() { merged.render.mesh = ent.render.mesh.clone(); }
            if ent.render.material.is_some() {
                merged.render.material = ent.render.material.clone();
            }

            merged
        }
        _ => ent.clone(),
    }
}

fn plan_mesh(e: &Entity) -> Option<MeshSpec> {
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
    if let Some(m) = e.render.mesh.as_deref() {
        let kind = match m {
            "plane" => MeshKind::Plane,
            "sphere" => MeshKind::Sphere,
            "cylinder" => MeshKind::Cylinder,
            "cube" | "box" => MeshKind::Cube,
            other => MeshKind::Custom { model_id: other.to_string() },
        };
        return Some(MeshSpec { kind, size: [1.0, 1.0, 1.0], model_path: None });
    }
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
    let _ = assets;
    if let Some(im) = &e.render.inline_material {
        return material_to_plan(im);
    }
    if let Some(mat_id) = e.material.as_ref().or(e.render.material.as_ref()) {
        if let Some(mat) = level.resources.materials.iter().find(|m| &m.id == mat_id) {
            return material_to_plan(mat);
        }
    }
    MaterialPlan::default()
}

fn material_to_plan(m: &MaterialDecl) -> MaterialPlan {
    MaterialPlan {
        base: m.texture.clone(),
        normal: m.normal.clone(),
        roughness: m.roughness.clone(),
        ao: m.ao.clone(),
        arm: m.arm.clone(),
        color: None,
        emissive: m.emissive.clone(),
        emissive_strength: m.emissive_strength,
        tiling: m.tiling,
        roughness_f32: m.roughness_f32,
        metallic_f32: m.metallic_f32,
    }
}

fn plan_light(e: &Entity) -> Option<(LightKindPlan, String, f32, f32, f32)> {
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
        l.flicker,
    ))
}

fn plan_collider(e: &Entity) -> Option<ColliderPlan> {
    let c = e.physics.collider.as_deref()?;
    let body = e.physics.body.clone().unwrap_or_else(|| "static".to_string());
    let sensor = e.physics.sensor;
    match c {
        "box" => Some(ColliderPlan::Box {
            size: e.physics.size.unwrap_or([1.0, 1.0, 1.0]),
            sensor, body,
        }),
        "sphere" => Some(ColliderPlan::Sphere {
            radius: e.physics.radius.unwrap_or(0.5),
            sensor, body,
        }),
        "capsule" => Some(ColliderPlan::Capsule {
            radius: e.physics.radius.unwrap_or(0.4),
            height: e.physics.height.unwrap_or(1.8),
            sensor, body,
        }),
        "cylinder" => Some(ColliderPlan::Capsule {
            radius: e.physics.radius.unwrap_or(0.5),
            height: e.physics.height.unwrap_or(1.0),
            sensor, body,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materials_from_top_level_section_apply() {
        let xml = r#"<level id="l" format="bds-level/2"><chunk_size x="32" y="16" z="32"/>
<resources>
<texture id="tex_a" src="assets/textures/A.png"/>
</resources>
<materials>
<material id="mat_wall" albedo="tex_a" roughness="0.9" uv_scale="2 4"/>
</materials>
<chunks><chunk id="c" generator="none" chance="0">
<entity id="player_start" type="player"><transform pos="0 1 0"/></entity>
<entity id="floor" type="static_body">
  <mesh shape="box" scale="32 0.2 32"/>
  <render material="mat_wall"/>
</entity>
</chunk></chunks></level>"#;
        let l = crate::level::parse::parse_level_xml(xml).unwrap();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();
        let f = plan.nodes.iter().find(|n| n.id == "c/floor").unwrap();
        assert_eq!(f.material.base.as_deref(), Some("tex_a"));
        assert_eq!(f.material.tiling, Some([2.0, 4.0]));
    }

    #[test]
    fn static_body_maps_to_prop() {
        let xml = r#"<level id="l" format="bds-level/2"><chunk_size x="32" y="16" z="32"/><chunks><chunk id="c" generator="none" chance="0">
<entity id="player_start" type="player"><transform pos="0 1 0"/></entity>
<entity id="w" type="static_body"><render mesh="cube"/></entity>
</chunk></chunks></level>"#;
        let l = crate::level::parse::parse_level_xml(xml).unwrap();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();
        let w = plan.nodes.iter().find(|n| n.id == "c/w").unwrap();
        assert_eq!(w.kind, NodeKind::Prop);
    }
}
