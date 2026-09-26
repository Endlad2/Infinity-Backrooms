//! Структуры модели BDS Level Format v2 (infinite chunked levels).
//! v2: уровень НЕ хранит сущности напрямую — всё живёт в <chunk>.
//! Каждый чанк имеет generator: default | axis | none, и chance=0..100.
//! Координаты внутри чанка ЛОКАЛЬНЫЕ: (0,0,0) = центр чанка.
//!
//! Обновлено: добавлена поддержка inline-материалов прямо в <entity>
//! (texture/normal/roughness/ao/tiling) и вложенных <mesh shape= scale=/>.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct Level {
    pub id: String,
    pub name: String,
    pub version: String,
    pub engine: String,
    pub format: String,
    pub gravity: [f32; 3],
    pub ambient: [f32; 3],
    pub skybox: Option<String>,
    pub spawn_point: Option<String>,
    pub time_scale: f32,
    pub meta: Meta,
    pub resources: Resources,
    pub prefabs: BTreeMap<String, Entity>,
    pub chunk_size: Option<ChunkSize>,
    pub chunks: Vec<Chunk>,
    pub seed: Option<u64>,
    pub entities: Vec<Entity>,
    pub triggers: Vec<Trigger>,
    pub scripts: Vec<Script>,
    pub bounds: Bounds,
}

#[derive(Debug, Clone, Default)]
pub struct Meta {
    pub author: Option<String>,
    pub created: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Resources {
    pub textures: Vec<TextureDecl>,
    pub models: Vec<ModelDecl>,
    pub materials: Vec<MaterialDecl>,
    pub sounds: Vec<SoundDecl>,
    pub animations: Vec<AnimationDecl>,
}

#[derive(Debug, Clone)]
pub enum TextureSource {
    File(String),
    Url(String),
    Color(String),
    InlineSvg(String),
    Tags(Vec<String>),
    None,
}

#[derive(Debug, Clone)]
pub struct TextureDecl {
    pub id: String,
    pub source: TextureSource,
    pub tags: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub filter: Option<String>,
    pub wrap: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ModelSource {
    File(String),
    Url(String),
    InlineObj(String),
    Tags(Vec<String>),
    None,
}

#[derive(Debug, Clone)]
pub struct ModelDecl {
    pub id: String,
    pub source: ModelSource,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MaterialDecl {
    pub id: String,
    pub texture: Option<String>,
    pub normal: Option<String>,
    pub roughness: Option<String>,
    pub ao: Option<String>,
    pub metallic: Option<String>,
    pub emissive: Option<String>,
    pub emissive_strength: f32,
    pub tiling: Option<[f32; 2]>,
    pub roughness_f32: f32,
    pub metallic_f32: f32,
}

#[derive(Debug, Clone, Default)]
pub struct SoundDecl {
    pub id: String,
    pub path: Option<String>,
    pub url: Option<String>,
    pub volume: f32,
    pub looped: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AnimationDecl {
    pub id: String,
    pub model: Option<String>,
    pub clip: Option<String>,
    pub looped: bool,
    pub speed: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Transform {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct RenderDecl {
    pub model: Option<String>,
    pub material: Option<String>,
    pub mesh: Option<String>,
    pub cast_shadow: bool,
    pub receive_shadow: bool,
    /// Вложенный тег <mesh shape="..." scale="..."/>
    pub mesh_shape: Option<String>,
    pub mesh_scale: Option<[f32; 3]>,
    /// Inline-материал на самой entity (без <resources><material>).
    pub inline_material: Option<MaterialDecl>,
}

#[derive(Debug, Clone, Default)]
pub struct PhysicsDecl {
    pub body: Option<String>,
    pub collider: Option<String>,
    pub size: Option<[f32; 3]>,
    pub radius: Option<f32>,
    pub height: Option<f32>,
    pub sensor: bool,
    pub mass: f32,
    pub gravity_scale: f32,
}

#[derive(Debug, Clone, Default)]
pub struct StatsDecl {
    pub hp: f32,
    pub hp_max: f32,
    pub speed: f32,
    pub damage: f32,
    pub jump: f32,
    pub faction: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EntityEvent {
    pub kind: String,
    pub script: String,
    pub func: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChunkSize {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkGenerator {
    Default,
    Axis(String),
    None,
}

impl Default for ChunkGenerator {
    fn default() -> Self { ChunkGenerator::Default }
}

#[derive(Debug, Clone, Default)]
pub struct Chunk {
    pub id: String,
    pub generator: ChunkGenerator,
    pub chance: f32,
    pub entities: Vec<Entity>,
}

#[derive(Debug, Clone, Default)]
pub struct Entity {
    pub id: String,
    pub kind: Option<String>,
    pub inherit: Option<String>,
    pub model: Option<String>,
    pub material: Option<String>,
    pub transform: Transform,
    pub render: RenderDecl,
    pub physics: PhysicsDecl,
    pub stats: StatsDecl,
    pub camera: Option<CameraDecl>,
    pub light: Option<LightDecl>,
    pub events: Vec<EntityEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct CameraDecl {
    pub mode: Option<String>,
    pub distance: f32,
    pub pitch: f32,
    pub fov: f32,
}

#[derive(Debug, Clone, Default)]
pub struct LightDecl {
    pub kind: Option<String>,
    pub color: Option<String>,
    pub intensity: f32,
    pub range: f32,
    pub shadow: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Trigger {
    pub id: String,
    pub kind: Option<String>,
    pub shape: Option<String>,
    pub size: Option<[f32; 3]>,
    pub radius: Option<f32>,
    pub pos: [f32; 3],
    pub interval: Option<f32>,
    pub signal: Option<String>,
    pub on_enter: Option<EntityEvent>,
    pub on_exit: Option<EntityEvent>,
    pub on_tick: Option<EntityEvent>,
    pub on_signal: Option<EntityEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct Script {
    pub id: String,
    pub lua: String,
}

#[derive(Debug, Clone, Default)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_level_is_empty() {
        let l = Level::default();
        assert!(l.entities.is_empty());
        assert!(l.chunks.is_empty());
        assert!(l.chunk_size.is_none());
    }

    #[test]
    fn chunk_default_generator() {
        let c = Chunk::default();
        assert_eq!(c.generator, ChunkGenerator::Default);
    }

    #[test]
    fn entity_default_transform_zero() {
        let e = Entity::default();
        assert_eq!(e.transform.pos, [0.0, 0.0, 0.0]);
    }
}
