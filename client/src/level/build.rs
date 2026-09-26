//! Построение данных уровня для Bevy-сцены (BDS Level Format v1 + v2).
//! v2: уровень состоит из чанков; локальные координаты внутри чанка,
//! (0,0,0) = центр чанка. Здесь строится ЧАНК-ПЛАН и начальная сцена.
//! Здесь формируется «план» сцены (плоский набор SpawnableNode) без прямой
//! зависимости от Bevy — это позволяет юнит-тестировать логику без запуска окна.
//! Реальные Bevy-сущности создаются в game/mod.rs на основе этого плана.

use std::collections::BTreeMap;

use anyhow::Result;

use super::assets_bridge::ResolvedAssets;
use super::model::*;

/// Тип узла сцены.
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
    Custom { model_id: String },
}

#[derive(Debug, Clone)]
pub enum LightKindPlan {
    Directional,
    Point,
    Spot,
}

#[derive(Debug, Clone)]
pub struct NodePlan {
    pub id: String,
    pub kind: NodeKind,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
    pub mesh: Option<MeshKind>,
    pub texture_id: Option<String>,
    pub material_id: Option<String>,
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

/// Тип чанк-генератора (v2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkGenPlan {
    /// Сетка по X и Z (бесконечный мир-плоскость).
    Default,
    /// Сетка по одной оси (например, башня по Y).
    Axis(String),
    /// Чанк объявлен, но автоматически НЕ создаётся; спавнится вручную через Lua.
    None,
}

/// План одного ЧАНКА-ШАБЛОНА (v2). Сущности уже подготовлены как NodePlan,
/// но их координаты — ЛОКАЛЬНЫЕ (0,0,0 = центр чанка). В мировые их
/// пересчитывает `expand_chunk_node` при спавне.
#[derive(Debug, Clone)]
pub struct ChunkTemplatePlan {
    pub id: String,
    pub generator: ChunkGenPlan,
    /// Обязательный атрибут chance="0..100".
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
    /// id → inline-Lua, для загрузки в scripting loader.
    pub scripts: BTreeMap<String, String>,
    /// v2: размер чанка в мировых единицах (x,y,z). None у v1-уровней.
    pub chunk_size: Option<[f32; 3]>,
    /// v2: шаблоны чанков (для менеджера загрузки/выгрузки).
    pub chunk_templates: Vec<ChunkTemplatePlan>,
    /// v2: seed процедурной генерации.
    pub seed: Option<u64>,
}

impl ScenePlan {
    /// Найти шаблон чанка по id.
    pub fn find_template(&self, id: &str) -> Option<&ChunkTemplatePlan> {
        self.chunk_templates.iter().find(|t| t.id == id)
    }
}

/// Построить план сцены из Level и уже подготовленных ассетов.
pub fn build_scene_plan(level: &Level, assets: &ResolvedAssets) -> Result<ScenePlan> {
    let mut nodes = Vec::new();
    let mut chunk_templates = Vec::new();

    // v1: сущности верхнего уровня (<entities>)
    for ent in &level.entities {
        nodes.push(build_node_plan(ent, level, assets));
    }

    // v2: чанки (<chunks><chunk .../>)
    // Собираем шаблоны; для generator="default"/"axis" сразу раскрываем
    // начальные чанки в grid (0,0,0) с пересчётом локальных координат в мировые.
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

        // Автоматически раскрываем только Default/Axis в grid (0,0,0).
        // Дальнейшую загрузку/выгрузку выполняет менеджер чанков.
        let auto_spawn = matches!(gen_plan, ChunkGenPlan::Default | ChunkGenPlan::Axis(_));
        if auto_spawn {
            let origin = [0.0f32, 0.0, 0.0];
            let world_nodes = expand_chunk_nodes(
                &template_nodes,
                &ch.id,
                origin,
                chunk_size,
            );
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

/// Собрать NodePlan из Entity (с учётом inherit).
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
    NodePlan {
        id: merged.id.clone(),
        kind,
        pos: merged.transform.pos,
        rot: merged.transform.rot,
        scale: merged.transform.scale,
        mesh: plan_mesh(&merged),
        texture_id: pick_texture_id(&merged, level, assets),
        material_id: merged.material.clone().or_else(|| merged.render.material.clone()),
        light: plan_light(&merged),
        collider: plan_collider(&merged),
        stats: merged.stats.clone(),
        events: merged.events.clone(),
    }
}

/// Развернуть узлы чанка в мир: id получает префикс `chunkid/`,
/// позиция пересчитывается из ЛОКАЛЬНОЙ (0,0,0 = центр чанка) в мировую:
///   world = chunk_origin + local
/// (chunk_origin — центр чанка в мировых единицах).
/// Детерминированный хэш для выбора шаблона по seed и grid-ячейке.
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

/// Взвешенный выбор шаблона чанка по chance с учётом seed и grid-ячейки.
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
        if r < acc {
            return Some(t);
        }
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
            // поверх прототипа накладываем дочерние значения
            merged.id = ent.id.clone();
            merged.kind = ent.kind.clone().or(proto.kind.clone());
            merged.inherit = None;
            merged.transform = ent.transform.clone();
            if ent.model.is_some() {
                merged.model = ent.model.clone();
            }
            if ent.material.is_some() {
                merged.material = ent.material.clone();
            }
            if !ent.events.is_empty() {
                merged.events = ent.events.clone();
            }
            merged
        }
        _ => ent.clone(),
    }
}

fn plan_mesh(e: &Entity) -> Option<MeshKind> {
    if let Some(m) = e.render.mesh.as_deref() {
        return Some(match m {
            "plane" => MeshKind::Plane,
            "cube" => MeshKind::Cube,
            "sphere" => MeshKind::Sphere,
            _ => MeshKind::Cube,
        });
    }
    if let Some(model) = e.render.model.as_ref().or(e.model.as_ref()) {
        return Some(MeshKind::Custom { model_id: model.clone() });
    }
    None
}

fn pick_texture_id(
    e: &Entity,
    level: &Level,
    assets: &ResolvedAssets,
) -> Option<String> {
    let mat_id = e.material.as_ref().or(e.render.material.as_ref())?;
    let mat = level.resources.materials.iter().find(|m| &m.id == mat_id)?;
    let tex_id = mat.texture.clone()?;
    if assets.textures.contains_key(&tex_id) {
        Some(tex_id)
    } else {
        None
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
    let body = e
        .physics
        .body
        .clone()
        .unwrap_or_else(|| "static".to_string());
    let sensor = e.physics.sensor;
    match c {
        "box" => {
            let size = e.physics.size.unwrap_or([1.0, 1.0, 1.0]);
            Some(ColliderPlan::Box { size, sensor, body })
        }
        "sphere" => {
            let radius = e.physics.radius.unwrap_or(0.5);
            Some(ColliderPlan::Sphere { radius, sensor, body })
        }
        "capsule" => {
            let radius = e.physics.radius.unwrap_or(0.4);
            let height = e.physics.height.unwrap_or(1.8);
            Some(ColliderPlan::Capsule { radius, height, sensor, body })
        }
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

        // Текстура и материал для ground
        l.resources.textures.push(TextureDecl {
            id: "tex_ground".into(),
            source: TextureSource::Color("#333333".into()),
            tags: vec![],
            width: 64,
            height: 64,
            filter: None,
            wrap: None,
        });
        l.resources.materials.push(MaterialDecl {
            id: "mat_ground".into(),
            texture: Some("tex_ground".into()),
            roughness: 1.0,
            metallic: 0.0,
            emissive: None,
            emissive_strength: 0.0,
        });

        // Префаб ящика
        let mut prefab = Entity {
            id: "crate_root".into(),
            kind: Some("prop".into()),
            model: Some("mdl_cube".into()),
            material: Some("mat_ground".into()),
            ..Default::default()
        };
        prefab.render.mesh = Some("cube".into());
        prefab.physics.collider = Some("box".into());
        prefab.physics.size = Some([1.0, 1.0, 1.0]);
        prefab.physics.body = Some("dynamic".into());
        l.prefabs.insert("pf_crate".into(), prefab);

        // Player
        let mut player = Entity {
            id: "player_start".into(),
            kind: Some("player".into()),
            ..Default::default()
        };
        player.transform.pos = [0.0, 1.0, 0.0];
        player.physics.collider = Some("capsule".into());
        player.physics.radius = Some(0.4);
        player.physics.height = Some(1.8);
        player.physics.body = Some("kinematic".into());
        l.entities.push(player);

        // Inherit-ящик
        let mut crate_ent = Entity {
            id: "crate_01".into(),
            inherit: Some("pf_crate".into()),
            ..Default::default()
        };
        crate_ent.transform.pos = [3.0, 0.5, 0.0];
        l.entities.push(crate_ent);

        // Свет
        let mut sun = Entity {
            id: "sun".into(),
            kind: Some("light".into()),
            ..Default::default()
        };
        sun.light = Some(LightDecl {
            kind: Some("directional".into()),
            color: Some("#fff".into()),
            intensity: 1.5,
            range: 0.0,
            shadow: true,
        });
        l.entities.push(sun);

        // Триггер + скрипт
        l.triggers.push(Trigger {
            id: "trg1".into(),
            kind: Some("volume".into()),
            shape: Some("box".into()),
            size: Some([1.0, 2.0, 1.0]),
            ..Default::default()
        });
        l.scripts.push(Script {
            id: "intro".into(),
            lua: "function on_level_start() end".into(),
        });

        l
    }

    #[test]
    fn plans_entities_and_inherit() {
        let l = mk_simple_level();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();

        assert_eq!(plan.nodes.len(), 3);
        let crate_node = plan.nodes.iter().find(|n| n.id == "crate_01").unwrap();
        assert_eq!(crate_node.kind, NodeKind::Prop);
        assert_eq!(crate_node.pos, [3.0, 0.5, 0.0]);
        // Наследование из префаба подтянуло коллайдер и меш
        assert!(matches!(crate_node.collider, Some(ColliderPlan::Box { .. })));
        assert!(matches!(crate_node.mesh, Some(MeshKind::Cube)));
    }

    #[test]
    fn plans_player_and_light() {
        let l = mk_simple_level();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();

        let p = plan.nodes.iter().find(|n| n.id == "player_start").unwrap();
        assert_eq!(p.kind, NodeKind::Player);
        assert!(matches!(p.collider, Some(ColliderPlan::Capsule { .. })));

        let sun = plan.nodes.iter().find(|n| n.id == "sun").unwrap();
        assert_eq!(sun.kind, NodeKind::Light);
        assert!(sun.light.is_some());
    }

    #[test]
    fn plans_triggers_and_scripts() {
        let l = mk_simple_level();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();

        assert_eq!(plan.triggers.len(), 1);
        assert_eq!(plan.triggers[0].id, "trg1");
        assert!(plan.scripts.contains_key("intro"));
        assert_eq!(plan.spawn_point.as_deref(), Some("player_start"));
    }

    fn mk_v2_level() -> Level {
        let mut l = Level::default();
        l.chunk_size = Some(ChunkSize { x: 32.0, y: 16.0, z: 32.0 });
        l.seed = Some(1337);
        // default-чанк с полом в локальных координатах
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
        floor.transform.scale = [32.0, 0.2, 32.0];
        floor.render.mesh = Some("cube".into());
        ch_default.entities.push(floor);

        // axis-чанк (башня по Y)
        let mut ch_axis = Chunk {
            id: "chunk_tower".into(),
            generator: ChunkGenerator::Axis("y".into()),
            chance: 100.0,
            entities: vec![],
        };
        let mut tfloor = Entity {
            id: "tower_floor".into(),
            kind: Some("prop".into()),
            ..Default::default()
        };
        tfloor.transform.pos = [0.0, -7.9, 0.0];
        tfloor.render.mesh = Some("cube".into());
        ch_axis.entities.push(tfloor);

        // none-чанк (ручной спавн)
        let mut ch_none = Chunk {
            id: "chunk_spawn_room".into(),
            generator: ChunkGenerator::None,
            chance: 0.0,
            entities: vec![],
        };
        let mut lamp = Entity {
            id: "lamp".into(),
            kind: Some("light".into()),
            ..Default::default()
        };
        lamp.light = Some(LightDecl {
            kind: Some("point".into()),
            color: Some("#ffe9b0".into()),
            intensity: 3.0,
            range: 12.0,
            shadow: true,
        });
        ch_none.entities.push(lamp);

        l.chunks.push(ch_default);
        l.chunks.push(ch_axis);
        l.chunks.push(ch_none);
        l
    }

    #[test]
    fn v2_builds_chunk_templates_and_expands_default_and_axis() {
        let l = mk_v2_level();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();

        // 3 шаблона чанков
        assert_eq!(plan.chunk_templates.len(), 3);
        assert_eq!(plan.chunk_size, Some([32.0, 16.0, 32.0]));
        assert_eq!(plan.seed, Some(1337));

        let t_field = plan.find_template("chunk_field").unwrap();
        assert_eq!(t_field.generator, ChunkGenPlan::Default);
        assert_eq!(t_field.chance, 90.0);

        let t_tower = plan.find_template("chunk_tower").unwrap();
        assert!(matches!(t_tower.generator, ChunkGenPlan::Axis(ref a) if a == "y"));

        let t_none = plan.find_template("chunk_spawn_room").unwrap();
        assert_eq!(t_none.generator, ChunkGenPlan::None);

        // default и axis раскрылись, none — нет (итого 2 начальных узла)
        assert_eq!(plan.nodes.len(), 2);

        // id получил префикс chunkid/
        let f = plan.nodes.iter().find(|n| n.id == "chunk_field/floor").unwrap();
        // локальные координаты (0,0,0)=центр чанка → мировые равны локальным при origin (0,0,0)
        assert_eq!(f.pos, [0.0, -7.9, 0.0]);

        // none-чанк НЕ раскрылся
        assert!(plan.nodes.iter().all(|n| !n.id.starts_with("chunk_spawn_room/")));
    }

    #[test]
    fn v2_none_chunk_not_spawned() {
        let l = mk_v2_level();
        let assets = ResolvedAssets::default();
        let plan = build_scene_plan(&l, &assets).unwrap();
        assert!(plan.nodes.iter().all(|n| n.id != "chunk_spawn_room/lamp"));
    }

    #[test]
    fn texture_binding_when_asset_present() {
        let l = mk_simple_level();
        let mut assets = ResolvedAssets::default();
        assets
            .textures
            .insert("tex_ground".into(), vec![0x89, b'P', b'N', b'G']);

        let plan = build_scene_plan(&l, &assets).unwrap();
        // У ящика наследуется материал mat_ground → текстура tex_ground
        let crate_plan = plan.nodes.iter().find(|n| n.id == "crate_01").unwrap();
        assert_eq!(crate_plan.texture_id.as_deref(), Some("tex_ground"));
    }
}