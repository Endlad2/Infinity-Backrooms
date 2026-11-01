use super::xml::XmlNode;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LevelDefinition {
    pub version: u32,
    pub meta: LevelMeta,
    pub world: WorldDef,
    pub player: PlayerDef,
    pub assets: AssetCatalog,
    pub chunks: Vec<ChunkDef>,
    pub scripts: Vec<ScriptDef>,
}

#[derive(Debug, Clone)]
pub struct LevelMeta {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub preview: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorldDef {
    pub clear_color: [f32; 4],
    pub ambient_color: [f32; 3],
    pub ambient_brightness: f32,
}

#[derive(Debug, Clone)]
pub struct PlayerDef {
    pub spawn: [f32; 3],
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub move_speed: f32,
    pub interaction_distance: f32,
}

#[derive(Debug, Clone, Default)]
pub struct AssetCatalog {
    pub textures: HashMap<String, TextureDef>,
    pub materials: HashMap<String, MaterialDef>,
    pub models: HashMap<String, ModelDef>,
    pub sounds: HashMap<String, SoundDef>,
}

#[derive(Debug, Clone)]
pub struct TextureDef {
    pub id: String,
    pub src: String,
}

#[derive(Debug, Clone)]
pub struct MaterialDef {
    pub id: String,
    pub base_color: [f32; 4],
    pub texture: Option<String>,
    pub normal: Option<String>,
    pub roughness: f32,
    pub metallic: f32,
    pub unlit: bool,
    pub alpha_blend: bool,
}

#[derive(Debug, Clone)]
pub struct ModelDef {
    pub id: String,
    pub src: String,
    pub scene: usize,
}

#[derive(Debug, Clone)]
pub struct SoundDef {
    pub id: String,
    pub src: String,
    pub looped: bool,
    pub autoplay: bool,
}

#[derive(Debug, Clone)]
pub struct TransformDef {
    pub pos: [f32; 3],
    pub rot_deg: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for TransformDef {
    fn default() -> Self {
        Self { pos: [0.0, 0.0, 0.0], rot_deg: [0.0, 0.0, 0.0], scale: [1.0, 1.0, 1.0] }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkDef {
    pub id: String,
    pub origin: [f32; 3],
    pub size: [f32; 3],
    pub load_distance: f32,
    pub unload_distance: f32,
    pub always_loaded: bool,
    pub objects: Vec<ObjectDef>,
    pub lights: Vec<LightDef>,
    pub pickups: Vec<PickupDef>,
    pub triggers: Vec<TriggerDef>,
    pub interactables: Vec<InteractableDef>,
    pub scripts: Vec<ScriptDef>,
}

#[derive(Debug, Clone)]
pub enum ObjectDef {
    Box(BoxDef),
    Sphere(SphereDef),
    Cylinder(CylinderDef),
    Model(ModelInstanceDef),
}

#[derive(Debug, Clone)]
pub struct CommonObjectDef {
    pub id: Option<String>,
    pub transform: TransformDef,
    pub material: Option<String>,
    pub collider: bool,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct BoxDef {
    pub common: CommonObjectDef,
    pub size: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct SphereDef {
    pub common: CommonObjectDef,
    pub radius: f32,
}

#[derive(Debug, Clone)]
pub struct CylinderDef {
    pub common: CommonObjectDef,
    pub radius: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct ModelInstanceDef {
    pub common: CommonObjectDef,
    pub model: String,
    pub collider_size: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind {
    Point,
    Spot,
    Directional,
}

#[derive(Debug, Clone)]
pub struct LightDef {
    pub id: Option<String>,
    pub kind: LightKind,
    pub transform: TransformDef,
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub shadows: bool,
    pub inner_angle_deg: f32,
    pub outer_angle_deg: f32,
}

#[derive(Debug, Clone)]
pub struct PickupDef {
    pub id: Option<String>,
    pub item: String,
    pub count: u16,
    pub transform: TransformDef,
}

#[derive(Debug, Clone)]
pub struct TriggerDef {
    pub id: String,
    pub pos: [f32; 3],
    pub size: [f32; 3],
    pub on_enter: Option<String>,
    pub on_exit: Option<String>,
    pub once: bool,
}

#[derive(Debug, Clone)]
pub struct InteractableDef {
    pub id: String,
    pub pos: [f32; 3],
    pub radius: f32,
    pub prompt: String,
    pub event: String,
}

#[derive(Debug, Clone)]
pub struct ScriptDef {
    pub id: String,
    pub src: Option<String>,
    pub code: Option<String>,
    pub enabled: bool,
}

pub fn parse_level(root: &XmlNode) -> Result<LevelDefinition> {
    if root.name != "level" {
        bail!("root element must be <level>, got <{}>", root.name);
    }

    let version = attr_u32(root, "version", 1)?;
    let meta_node = root.child("meta").ok_or_else(|| anyhow!("<level> requires <meta>"))?;
    let meta = LevelMeta {
        id: attr_required(meta_node, "id")?.to_string(),
        name: attr_required(meta_node, "name")?.to_string(),
        author: attr(meta_node, "author", "Unknown").to_string(),
        description: meta_node.child("description").map(|n| n.text.clone()).unwrap_or_default(),
        preview: meta_node.child("preview").and_then(|n| n.attr("src")).map(str::to_string),
    };

    let world = if let Some(node) = root.child("world") {
        WorldDef {
            clear_color: parse_color(attr(node, "clear_color", "#111009"))?,
            ambient_color: rgb(parse_color(attr(node, "ambient_color", "#a89b59"))?),
            ambient_brightness: attr_f32(node, "ambient_brightness", 90.0)?,
        }
    } else {
        WorldDef { clear_color: [0.06, 0.06, 0.04, 1.0], ambient_color: [0.65, 0.60, 0.35], ambient_brightness: 90.0 }
    };

    let player = if let Some(node) = root.child("player") {
        PlayerDef {
            spawn: attr_vec3(node, "spawn", [0.0, 1.7, 0.0])?,
            yaw_deg: attr_f32(node, "yaw", 0.0)?,
            pitch_deg: attr_f32(node, "pitch", 0.0)?,
            move_speed: attr_f32(node, "move_speed", 4.0)?,
            interaction_distance: attr_f32(node, "interaction_distance", 2.5)?,
        }
    } else {
        PlayerDef { spawn: [0.0, 1.7, 0.0], yaw_deg: 0.0, pitch_deg: 0.0, move_speed: 4.0, interaction_distance: 2.5 }
    };

    let assets = parse_assets(root.child("assets"))?;
    let scripts = root.child("scripts").map(parse_scripts).transpose()?.unwrap_or_default();
    let chunks_node = root.child("chunks").ok_or_else(|| anyhow!("<level> requires <chunks>"))?;
    let mut chunks = Vec::new();
    for node in chunks_node.children_named("chunk") {
        chunks.push(parse_chunk(node).with_context(|| format!("in chunk {:?}", node.attr("id")))?);
    }
    if chunks.is_empty() {
        bail!("<chunks> must contain at least one <chunk>");
    }

    validate_references(&assets, &chunks)?;

    Ok(LevelDefinition { version, meta, world, player, assets, chunks, scripts })
}

fn parse_assets(node: Option<&XmlNode>) -> Result<AssetCatalog> {
    let mut catalog = AssetCatalog::default();
    let Some(node) = node else { return Ok(catalog); };

    if let Some(textures) = node.child("textures") {
        for item in textures.children_named("texture") {
            let id = attr_required(item, "id")?.to_string();
            let def = TextureDef { id: id.clone(), src: attr_required(item, "src")?.to_string() };
            if catalog.textures.insert(id.clone(), def).is_some() { bail!("duplicate texture id {id}"); }
        }
    }
    if let Some(materials) = node.child("materials") {
        for item in materials.children_named("material") {
            let id = attr_required(item, "id")?.to_string();
            let def = MaterialDef {
                id: id.clone(),
                base_color: parse_color(attr(item, "color", "#ffffff"))?,
                texture: item.attr("texture").map(str::to_string),
                normal: item.attr("normal").map(str::to_string),
                roughness: attr_f32(item, "roughness", 0.9)?.clamp(0.0, 1.0),
                metallic: attr_f32(item, "metallic", 0.0)?.clamp(0.0, 1.0),
                unlit: attr_bool(item, "unlit", false)?,
                alpha_blend: attr_bool(item, "alpha_blend", false)?,
            };
            if catalog.materials.insert(id.clone(), def).is_some() { bail!("duplicate material id {id}"); }
        }
    }
    if let Some(models) = node.child("models") {
        for item in models.children_named("model") {
            let id = attr_required(item, "id")?.to_string();
            let def = ModelDef { id: id.clone(), src: attr_required(item, "src")?.to_string(), scene: attr_u32(item, "scene", 0)? as usize };
            if catalog.models.insert(id.clone(), def).is_some() { bail!("duplicate model id {id}"); }
        }
    }
    if let Some(sounds) = node.child("sounds") {
        for item in sounds.children_named("sound") {
            let id = attr_required(item, "id")?.to_string();
            let def = SoundDef {
                id: id.clone(),
                src: attr_required(item, "src")?.to_string(),
                looped: attr_bool(item, "loop", false)?,
                autoplay: attr_bool(item, "autoplay", false)?,
            };
            if catalog.sounds.insert(id.clone(), def).is_some() { bail!("duplicate sound id {id}"); }
        }
    }
    Ok(catalog)
}

fn parse_chunk(node: &XmlNode) -> Result<ChunkDef> {
    let id = attr_required(node, "id")?.to_string();
    let origin = attr_vec3(node, "origin", [0.0, 0.0, 0.0])?;
    let size = attr_vec3(node, "size", [24.0, 5.0, 24.0])?;
    let load_distance = attr_f32(node, "load_distance", 42.0)?;
    let unload_distance = attr_f32(node, "unload_distance", load_distance + 12.0)?.max(load_distance);
    let always_loaded = attr_bool(node, "always_loaded", false)?;

    let mut objects = Vec::new();
    if let Some(geometry) = node.child("geometry") {
        for item in &geometry.children {
            let object = match item.name.as_str() {
                "box" => Some(ObjectDef::Box(parse_box(item)?)),
                "sphere" => Some(ObjectDef::Sphere(parse_sphere(item)?)),
                "cylinder" => Some(ObjectDef::Cylinder(parse_cylinder(item)?)),
                "model" => Some(ObjectDef::Model(parse_model_instance(item)?)),
                other => bail!("unknown geometry tag <{other}>"),
            };
            if let Some(object) = object { objects.push(object); }
        }
    }

    let mut lights = Vec::new();
    if let Some(parent) = node.child("lights") {
        for item in parent.children_named("light") { lights.push(parse_light(item)?); }
    }
    let mut pickups = Vec::new();
    if let Some(parent) = node.child("pickups") {
        for item in parent.children_named("pickup") { pickups.push(parse_pickup(item)?); }
    }
    let mut triggers = Vec::new();
    if let Some(parent) = node.child("triggers") {
        for item in parent.children_named("trigger") { triggers.push(parse_trigger(item)?); }
    }
    let mut interactables = Vec::new();
    if let Some(parent) = node.child("interactables") {
        for item in parent.children_named("interactable") { interactables.push(parse_interactable(item)?); }
    }
    let scripts = node.child("scripts").map(parse_scripts).transpose()?.unwrap_or_default();

    Ok(ChunkDef { id, origin, size, load_distance, unload_distance, always_loaded, objects, lights, pickups, triggers, interactables, scripts })
}

fn common_object(node: &XmlNode) -> Result<CommonObjectDef> {
    Ok(CommonObjectDef {
        id: node.attr("id").map(str::to_string),
        transform: parse_transform(node)?,
        material: node.attr("material").map(str::to_string),
        collider: attr_bool(node, "collider", true)?,
        visible: attr_bool(node, "visible", true)?,
    })
}

fn parse_box(node: &XmlNode) -> Result<BoxDef> {
    Ok(BoxDef { common: common_object(node)?, size: attr_vec3(node, "size", [1.0, 1.0, 1.0])? })
}

fn parse_sphere(node: &XmlNode) -> Result<SphereDef> {
    Ok(SphereDef { common: common_object(node)?, radius: attr_f32(node, "radius", 0.5)? })
}

fn parse_cylinder(node: &XmlNode) -> Result<CylinderDef> {
    Ok(CylinderDef { common: common_object(node)?, radius: attr_f32(node, "radius", 0.5)?, height: attr_f32(node, "height", 1.0)? })
}

fn parse_model_instance(node: &XmlNode) -> Result<ModelInstanceDef> {
    Ok(ModelInstanceDef {
        common: common_object(node)?,
        model: attr_required(node, "ref")?.to_string(),
        collider_size: node.attr("collider_size").map(parse_vec3).transpose()?,
    })
}

fn parse_light(node: &XmlNode) -> Result<LightDef> {
    let kind = match attr(node, "kind", "point") {
        "point" => LightKind::Point,
        "spot" => LightKind::Spot,
        "directional" => LightKind::Directional,
        other => bail!("unknown light kind {other}"),
    };
    Ok(LightDef {
        id: node.attr("id").map(str::to_string),
        kind,
        transform: parse_transform(node)?,
        color: rgb(parse_color(attr(node, "color", "#ffffff"))?),
        intensity: attr_f32(node, "intensity", if kind == LightKind::Directional { 8000.0 } else { 1800.0 })?,
        range: attr_f32(node, "range", 14.0)?,
        shadows: attr_bool(node, "shadows", false)?,
        inner_angle_deg: attr_f32(node, "inner_angle", 20.0)?,
        outer_angle_deg: attr_f32(node, "outer_angle", 35.0)?,
    })
}

fn parse_pickup(node: &XmlNode) -> Result<PickupDef> {
    Ok(PickupDef {
        id: node.attr("id").map(str::to_string),
        item: attr_required(node, "item")?.to_string(),
        count: attr_u32(node, "count", 1)?.min(u16::MAX as u32) as u16,
        transform: parse_transform(node)?,
    })
}

fn parse_trigger(node: &XmlNode) -> Result<TriggerDef> {
    Ok(TriggerDef {
        id: attr_required(node, "id")?.to_string(),
        pos: attr_vec3(node, "pos", [0.0, 0.0, 0.0])?,
        size: attr_vec3(node, "size", [1.0, 1.0, 1.0])?,
        on_enter: node.attr("on_enter").map(str::to_string),
        on_exit: node.attr("on_exit").map(str::to_string),
        once: attr_bool(node, "once", false)?,
    })
}

fn parse_interactable(node: &XmlNode) -> Result<InteractableDef> {
    Ok(InteractableDef {
        id: attr_required(node, "id")?.to_string(),
        pos: attr_vec3(node, "pos", [0.0, 1.0, 0.0])?,
        radius: attr_f32(node, "radius", 2.0)?,
        prompt: attr(node, "prompt", "F — interact").to_string(),
        event: attr_required(node, "event")?.to_string(),
    })
}

fn parse_scripts(node: &XmlNode) -> Result<Vec<ScriptDef>> {
    let mut scripts = Vec::new();
    for (index, item) in node.children_named("script").enumerate() {
        let id = item.attr("id").map(str::to_string).unwrap_or_else(|| format!("script_{index}"));
        let src = item.attr("src").map(str::to_string);
        let code = if item.text.trim().is_empty() { None } else { Some(item.text.clone()) };
        if src.is_none() && code.is_none() { bail!("script {id} needs src= or inline CDATA/text"); }
        scripts.push(ScriptDef { id, src, code, enabled: attr_bool(item, "enabled", true)? });
    }
    Ok(scripts)
}

fn parse_transform(node: &XmlNode) -> Result<TransformDef> {
    Ok(TransformDef {
        pos: attr_vec3(node, "pos", [0.0, 0.0, 0.0])?,
        rot_deg: attr_vec3(node, "rot", [0.0, 0.0, 0.0])?,
        scale: attr_vec3(node, "scale", [1.0, 1.0, 1.0])?,
    })
}

fn validate_references(assets: &AssetCatalog, chunks: &[ChunkDef]) -> Result<()> {
    for material in assets.materials.values() {
        for (field, texture) in [("texture", material.texture.as_ref()), ("normal", material.normal.as_ref())] {
            if let Some(texture) = texture {
                if !assets.textures.contains_key(texture) { bail!("material {} references missing {field} texture {texture}", material.id); }
            }
        }
    }
    for chunk in chunks {
        for object in &chunk.objects {
            let common = match object {
                ObjectDef::Box(v) => &v.common,
                ObjectDef::Sphere(v) => &v.common,
                ObjectDef::Cylinder(v) => &v.common,
                ObjectDef::Model(v) => {
                    if !assets.models.contains_key(&v.model) { bail!("chunk {} references missing model {}", chunk.id, v.model); }
                    &v.common
                }
            };
            if let Some(material) = &common.material {
                if !assets.materials.contains_key(material) { bail!("chunk {} references missing material {material}", chunk.id); }
            }
        }
    }
    Ok(())
}

fn attr<'a>(node: &'a XmlNode, key: &str, default: &'a str) -> &'a str {
    node.attr(key).unwrap_or(default)
}

fn attr_required<'a>(node: &'a XmlNode, key: &str) -> Result<&'a str> {
    node.attr(key).ok_or_else(|| anyhow!("<{}> requires attribute {key}", node.name))
}

fn attr_bool(node: &XmlNode, key: &str, default: bool) -> Result<bool> {
    let Some(value) = node.attr(key) else { return Ok(default); };
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => bail!("<{}> attribute {key} is not a boolean: {value}", node.name),
    }
}

fn attr_f32(node: &XmlNode, key: &str, default: f32) -> Result<f32> {
    node.attr(key).map(|v| v.parse::<f32>().with_context(|| format!("invalid float {key}={v:?} on <{}>", node.name))).transpose().map(|v| v.unwrap_or(default))
}

fn attr_u32(node: &XmlNode, key: &str, default: u32) -> Result<u32> {
    node.attr(key).map(|v| v.parse::<u32>().with_context(|| format!("invalid integer {key}={v:?} on <{}>", node.name))).transpose().map(|v| v.unwrap_or(default))
}

fn attr_vec3(node: &XmlNode, key: &str, default: [f32; 3]) -> Result<[f32; 3]> {
    node.attr(key).map(parse_vec3).transpose().map(|v| v.unwrap_or(default))
}

fn parse_vec3(value: &str) -> Result<[f32; 3]> {
    let values = value.split(|c| c == ',' || c == ' ').filter(|s| !s.is_empty()).map(str::parse::<f32>).collect::<std::result::Result<Vec<_>, _>>().with_context(|| format!("invalid vec3 {value:?}"))?;
    if values.len() != 3 { bail!("vec3 must contain 3 numbers, got {value:?}"); }
    Ok([values[0], values[1], values[2]])
}

pub fn parse_color(value: &str) -> Result<[f32; 4]> {
    let s = value.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let (r, g, b, a) = match hex.len() {
            6 => (&hex[0..2], &hex[2..4], &hex[4..6], "ff"),
            8 => (&hex[0..2], &hex[2..4], &hex[4..6], &hex[6..8]),
            _ => bail!("color must be #RRGGBB or #RRGGBBAA, got {value:?}"),
        };
        let parse = |v: &str| -> Result<f32> { Ok(u8::from_str_radix(v, 16)? as f32 / 255.0) };
        return Ok([parse(r)?, parse(g)?, parse(b)?, parse(a)?]);
    }
    let values = s.split(|c| c == ',' || c == ' ').filter(|v| !v.is_empty()).map(str::parse::<f32>).collect::<std::result::Result<Vec<_>, _>>()?;
    match values.as_slice() {
        [r, g, b] => Ok([*r, *g, *b, 1.0]),
        [r, g, b, a] => Ok([*r, *g, *b, *a]),
        _ => bail!("color must contain 3 or 4 floats, got {value:?}"),
    }
}

fn rgb(value: [f32; 4]) -> [f32; 3] { [value[0], value[1], value[2]] }
