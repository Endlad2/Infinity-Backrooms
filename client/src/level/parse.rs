//! Парсер XML в структуры Level (BDS Level Format v1 + v2).
//! v2: <chunk_size x y z/> + <chunks><chunk generator="default|axis|none" chance="0..100">...
//! Обратная совместимость: top-level <entities> тоже читается (для v1-уровней).

use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

use super::model::*;

pub fn parse_level_xml(xml: &str) -> Result<Level> {
    let doc = Document::parse(xml).map_err(|e| anyhow!("XML parse error: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "level" {
        return Err(anyhow!("ожидался <level>, а не <{}>", root.tag_name().name()));
    }

    let mut level = Level {
        id: attr(root, "id").unwrap_or_default(),
        name: attr(root, "name").unwrap_or_default(),
        version: attr(root, "version").unwrap_or_default(),
        engine: attr(root, "engine").unwrap_or_default(),
        format: attr(root, "format").unwrap_or_else(|| "bds-level/1".to_string()),
        gravity: parse_vec3(attr(root, "gravity").as_deref()).unwrap_or([0.0, -9.81, 0.0]),
        ambient: parse_vec3(attr(root, "ambient").as_deref()).unwrap_or([0.3, 0.3, 0.3]),
        skybox: attr(root, "skybox"),
        spawn_point: attr(root, "spawn_point"),
        time_scale: parse_f32(attr(root, "time_scale").as_deref()).unwrap_or(1.0),
        seed: attr(root, "seed").and_then(|s| s.trim().parse::<u64>().ok()),
        ..Default::default()
    };

    // v2: <chunk_size x y z/> (обязателен для формата bds-level/2)
    if let Some(cs) = child(root, "chunk_size") {
        level.chunk_size = Some(ChunkSize {
            x: parse_f32(attr(cs, "x").as_deref()).unwrap_or(0.0),
            y: parse_f32(attr(cs, "y").as_deref()).unwrap_or(0.0),
            z: parse_f32(attr(cs, "z").as_deref()).unwrap_or(0.0),
        });
    }

    // v2: <chunks><chunk .../></chunks>
    if let Some(cn) = child(root, "chunks") {
        for ch in children(cn, "chunk") {
            level.chunks.push(parse_chunk(ch));
        }
    }

    // meta
    if let Some(m) = child(root, "meta") {
        level.meta = parse_meta(m);
    }

    // resources
    if let Some(r) = child(root, "resources") {
        level.resources = parse_resources(r)?;
    }

    // prefabs
    if let Some(p) = child(root, "prefabs") {
        for prefab in children(p, "prefab") {
            let id = attr(prefab, "id").unwrap_or_default();
            if let Some(ent) = child(prefab, "entity") {
                if let Ok(e) = parse_entity(ent) {
                    level.prefabs.insert(id, e);
                }
            }
        }
    }

    // entities
    if let Some(e) = child(root, "entities") {
        for ent in children(e, "entity") {
            if let Ok(x) = parse_entity(ent) {
                level.entities.push(x);
            }
        }
    }

    // triggers
    if let Some(t) = child(root, "triggers") {
        for trg in children(t, "trigger") {
            level.triggers.push(parse_trigger(trg));
        }
    }

    // scripts (inline Lua)
    if let Some(s) = child(root, "scripts") {
        for sc in children(s, "script") {
            let id = attr(sc, "id").unwrap_or_default();
            let lua = child(sc, "lua").and_then(|n| n.text()).unwrap_or("").to_string();
            level.scripts.push(Script { id, lua });
        }
    }

    // bounds
    if let Some(b) = child(root, "bounds") {
        level.bounds = Bounds {
            min: parse_vec3(attr(b, "min").as_deref()).unwrap_or([-50.0, -5.0, -50.0]),
            max: parse_vec3(attr(b, "max").as_deref()).unwrap_or([50.0, 50.0, 50.0]),
        };
    } else {
        level.bounds = Bounds {
            min: [-50.0, -5.0, -50.0],
            max: [50.0, 50.0, 50.0],
        };
    }

    Ok(level)
}

fn parse_meta(n: Node) -> Meta {
    Meta {
        author: child(n, "author").and_then(|c| c.text()).map(|s| s.to_string()),
        created: child(n, "created").and_then(|c| c.text()).map(|s| s.to_string()),
        description: child(n, "description").and_then(|c| c.text()).map(|s| s.trim().to_string()),
        tags: child(n, "tags").and_then(|c| c.text())
            .map(|t| t.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
            .unwrap_or_default(),
    }
}

fn parse_resources(n: Node) -> Result<Resources> {
    let mut r = Resources::default();

    for t in children(n, "texture") {
        let id = attr(t, "id").unwrap_or_default();
        let tags = parse_tags_str(attr(t, "tags").as_deref());
        let width = parse_u32(attr(t, "width").as_deref()).unwrap_or(256);
        let height = parse_u32(attr(t, "height").as_deref()).unwrap_or(256);
        let filter = attr(t, "filter");
        let wrap = attr(t, "wrap");

        let source = if let Some(src) = attr(t, "src") {
            TextureSource::File(src)
        } else if let Some(p) = attr(t, "path") {
            TextureSource::File(p)
        } else if let Some(u) = attr(t, "url") {
            TextureSource::Url(u)
        } else if let Some(c) = attr(t, "color") {
            TextureSource::Color(c)
        } else if let Some(svg) = child(t, "svg") {
            TextureSource::InlineSvg(node_inner_xml(svg))
        } else if !tags.is_empty() {
            TextureSource::Tags(tags.clone())
        } else {
            TextureSource::None
        };

        r.textures.push(TextureDecl { id, source, tags, width, height, filter, wrap });
    }

    for m in children(n, "model") {
        let id = attr(m, "id").unwrap_or_default();
        let tags = parse_tags_str(attr(m, "tags").as_deref());
        let source = if let Some(src) = attr(m, "src") {
            ModelSource::File(src)
        } else if let Some(p) = attr(m, "path") {
            ModelSource::File(p)
        } else if let Some(u) = attr(m, "url") {
            ModelSource::Url(u)
        } else if let Some(o) = child(m, "obj") {
            ModelSource::InlineObj(node_inner_xml(o))
        } else if !tags.is_empty() {
            ModelSource::Tags(tags.clone())
        } else {
            ModelSource::None
        };
        r.models.push(ModelDecl { id, source, tags });
    }

    for mat in children(n, "material") {
        r.materials.push(MaterialDecl {
            id: attr(mat, "id").unwrap_or_default(),
            texture: attr(mat, "texture"),
            roughness: parse_f32(attr(mat, "roughness").as_deref()).unwrap_or(0.8),
            metallic: parse_f32(attr(mat, "metallic").as_deref()).unwrap_or(0.0),
            emissive: attr(mat, "emissive"),
            emissive_strength: parse_f32(attr(mat, "emissive_strength").as_deref()).unwrap_or(0.0),
        });
    }

    for s in children(n, "sound") {
        r.sounds.push(SoundDecl {
            id: attr(s, "id").unwrap_or_default(),
            path: attr(s, "path"),
            url: attr(s, "url"),
            volume: parse_f32(attr(s, "volume").as_deref()).unwrap_or(1.0),
            looped: parse_bool(attr(s, "loop").as_deref()),
        });
    }

    for a in children(n, "animation") {
        r.animations.push(AnimationDecl {
            id: attr(a, "id").unwrap_or_default(),
            model: attr(a, "model"),
            clip: attr(a, "clip"),
            looped: parse_bool(attr(a, "loop").as_deref()),
            speed: parse_f32(attr(a, "speed").as_deref()).unwrap_or(1.0),
        });
    }

    Ok(r)
}

fn parse_chunk(n: Node) -> Chunk {
    let gen_str = attr(n, "generator").unwrap_or_else(|| "default".to_string());
    let generator = match gen_str.trim().to_lowercase().as_str() {
        "axis" => ChunkGenerator::Axis(
            attr(n, "axis").unwrap_or_else(|| "y".to_string()).to_lowercase()
        ),
        "none" => ChunkGenerator::None,
        _ => ChunkGenerator::Default,
    };
    let chance = parse_f32(attr(n, "chance").as_deref()).unwrap_or(0.0);

    let mut entities = Vec::new();
    for ent in children(n, "entity") {
        if let Ok(x) = parse_entity(ent) {
            entities.push(x);
        }
    }

    Chunk {
        id: attr(n, "id").unwrap_or_default(),
        generator,
        chance,
        entities,
    }
}

fn parse_entity(n: Node) -> Result<Entity> {
    let mut e = Entity {
        id: attr(n, "id").unwrap_or_default(),
        kind: attr(n, "type"),
        inherit: attr(n, "inherit"),
        model: attr(n, "model"),
        material: attr(n, "material"),
        ..Default::default()
    };

    if let Some(t) = child(n, "transform") {
        e.transform = Transform {
            pos: parse_vec3(attr(t, "pos").as_deref()).unwrap_or([0.0; 3]),
            rot: parse_vec3(attr(t, "rot").as_deref()).unwrap_or([0.0; 3]),
            scale: parse_vec3(attr(t, "scale").as_deref()).unwrap_or([1.0; 3]),
        };
    }

    if let Some(r) = child(n, "render") {
        e.render = RenderDecl {
            model: attr(r, "model").or_else(|| e.model.clone()),
            material: attr(r, "material").or_else(|| e.material.clone()),
            mesh: attr(r, "mesh"),
            cast_shadow: parse_bool(attr(r, "cast_shadow").as_deref()),
            receive_shadow: parse_bool(attr(r, "receive_shadow").as_deref()),
        };
    }

    if let Some(p) = child(n, "physics") {
        e.physics = PhysicsDecl {
            body: attr(p, "body"),
            collider: attr(p, "collider"),
            size: parse_vec3(attr(p, "size").as_deref()),
            radius: parse_f32(attr(p, "radius").as_deref()),
            height: parse_f32(attr(p, "height").as_deref()),
            sensor: parse_bool(attr(p, "sensor").as_deref()),
            mass: parse_f32(attr(p, "mass").as_deref()).unwrap_or(1.0),
            gravity_scale: parse_f32(attr(p, "gravity_scale").as_deref()).unwrap_or(1.0),
        };
    }

    if let Some(s) = child(n, "stats") {
        e.stats = StatsDecl {
            hp: parse_f32(attr(s, "hp").as_deref()).unwrap_or(100.0),
            hp_max: parse_f32(attr(s, "hp_max").as_deref()).unwrap_or(100.0),
            speed: parse_f32(attr(s, "speed").as_deref()).unwrap_or(5.0),
            damage: parse_f32(attr(s, "damage").as_deref()).unwrap_or(0.0),
            jump: parse_f32(attr(s, "jump").as_deref()).unwrap_or(5.0),
            faction: attr(s, "faction"),
        };
    }

    if let Some(c) = child(n, "camera") {
        e.camera = Some(CameraDecl {
            mode: attr(c, "mode"),
            distance: parse_f32(attr(c, "distance").as_deref()).unwrap_or(6.0),
            pitch: parse_f32(attr(c, "pitch").as_deref()).unwrap_or(0.0),
            fov: parse_f32(attr(c, "fov").as_deref()).unwrap_or(60.0),
        });
    }

    if let Some(l) = child(n, "light") {
        e.light = Some(LightDecl {
            kind: attr(l, "kind"),
            color: attr(l, "color"),
            intensity: parse_f32(attr(l, "intensity").as_deref()).unwrap_or(1.0),
            range: parse_f32(attr(l, "range").as_deref()).unwrap_or(10.0),
            shadow: parse_bool(attr(l, "shadow").as_deref()),
        });
    }

    if let Some(evs) = child(n, "events") {
        for c in evs.children().filter(|x| x.is_element()) {
            e.events.push(EntityEvent {
                kind: c.tag_name().name().to_string(),
                script: attr(c, "script").unwrap_or_default(),
                func: attr(c, "func").unwrap_or_default(),
            });
        }
    }

    Ok(e)
}

fn parse_trigger(n: Node) -> Trigger {
    let mut t = Trigger {
        id: attr(n, "id").unwrap_or_default(),
        kind: attr(n, "type"),
        shape: attr(n, "shape"),
        size: parse_vec3(attr(n, "size").as_deref()),
        radius: parse_f32(attr(n, "radius").as_deref()),
        pos: parse_vec3(attr(n, "pos").as_deref()).unwrap_or([0.0; 3]),
        interval: parse_f32(attr(n, "interval").as_deref()),
        signal: attr(n, "signal"),
        ..Default::default()
    };
    for c in n.children().filter(|x| x.is_element()) {
        let ev = EntityEvent {
            kind: c.tag_name().name().to_string(),
            script: attr(c, "script").unwrap_or_default(),
            func: attr(c, "func").unwrap_or_default(),
        };
        match ev.kind.as_str() {
            "on_enter" => t.on_enter = Some(ev),
            "on_exit" => t.on_exit = Some(ev),
            "on_tick" => t.on_tick = Some(ev),
            "on_signal" => t.on_signal = Some(ev),
            _ => {}
        }
    }
    t
}

// ---------- helpers ----------

fn attr<'a>(n: Node<'a, 'a>, name: &str) -> Option<String> {
    n.attribute(name).map(|s| s.to_string())
}

fn child<'a>(n: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn children<'a>(n: Node<'a, 'a>, name: &'a str) -> impl Iterator<Item = Node<'a, 'a>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

/// Возвращает внутренний текст узла вместе с дочерними элементами (для инлайн-<svg>/<obj>).
fn node_inner_xml(n: Node) -> String {
    let s = n.range();
    let src = n.document().input_text();
    src[s.start..s.end].to_string()
}

fn parse_f32(s: Option<&str>) -> Option<f32> {
    s.and_then(|x| x.trim().parse::<f32>().ok())
}

fn parse_u32(s: Option<&str>) -> Option<u32> {
    s.and_then(|x| x.trim().parse::<u32>().ok())
}

fn parse_bool(s: Option<&str>) -> bool {
    matches!(s.map(|x| x.trim().to_lowercase()).as_deref(), Some("true") | Some("1") | Some("yes"))
}

fn parse_vec3(s: Option<&str>) -> Option<[f32; 3]> {
    let s = s?;
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

fn parse_tags_str(s: Option<&str>) -> Vec<String> {
    s.map(|x| {
        x.split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r##"<?xml version="1.0"?>
<level id="l1" name="Test" engine="bevy" format="bds-level/1" gravity="0 -9.81 0" spawn_point="player_start">
  <meta><author>me</author><tags>a, b, c</tags></meta>
  <resources>
    <texture id="tex_wall" tags="wall,yellow"/>
    <texture id="tex_red" color="#ff0000"/>
    <texture id="tex_sky" width="512" height="512" format="svg"><svg><svg xmlns="http://www.w3.org/2000/svg"/></svg></texture>
    <model id="mdl_a" tags="chair,wood"/>
    <model id="mdl_b"><obj><obj>o cube_n 0 0 0</obj></obj></model>
    <material id="mat_x" texture="tex_wall" roughness="0.9"/>
    <sound id="sfx_hit" path="audio/hit.ogg" volume="0.7"/>
    <animation id="anim1" model="mdl_a" clip="run"/>
  </resources>
  <prefabs>
    <prefab id="pf_crate"><entity id="root" type="prop" model="mdl_a" material="mat_x"><transform pos="1 2 3"/></entity></prefab>
  </prefabs>
  <entities>
    <entity id="player_start" type="player">
      <transform pos="0 1 0"/>
      <stats hp="100" hp_max="100" speed="5" jump="6" faction="players"/>
      <camera mode="first_person" fov="70"/>
    </entity>
    <entity inherit="pf_crate" id="crate_01"><transform pos="5 1 0"/></entity>
    <entity id="sun" type="light"><light kind="directional" color="#ffffff" intensity="2.5"/></entity>
  </entities>
  <triggers>
    <trigger id="trg1" type="volume" shape="box" size="1 2 3" pos="0 1 0">
      <on_enter script="s1" func="f1"/>
      <on_exit  script="s1" func="f2"/>
    </trigger>
    <trigger id="trg2" type="timer" interval="5"><on_tick script="s2" func="f3"/></trigger>
  </triggers>
  <scripts>
    <script id="intro"><lua>function on_level_start() end</lua></script>
  </scripts>
  <bounds min="-10 -2 -10" max="10 10 10"/>
</level>"##;

    #[test]
    fn parses_header_and_meta() {
        let l = parse_level_xml(XML).unwrap();
        assert_eq!(l.id, "l1");
        assert_eq!(l.name, "Test");
        assert_eq!(l.engine, "bevy");
        assert_eq!(l.gravity, [0.0, -9.81, 0.0]);
        assert_eq!(l.spawn_point.as_deref(), Some("player_start"));
        assert_eq!(l.meta.author.as_deref(), Some("me"));
        assert_eq!(l.meta.tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn parses_resources() {
        let l = parse_level_xml(XML).unwrap();
        assert_eq!(l.resources.textures.len(), 3);
        assert!(matches!(l.resources.textures[0].source, TextureSource::Tags(ref t) if t == &vec!["wall".to_string(), "yellow".to_string()]));
        assert!(matches!(l.resources.textures[1].source, TextureSource::Color(_)));
        assert!(matches!(l.resources.textures[2].source, TextureSource::InlineSvg(_)));
        assert_eq!(l.resources.models.len(), 2);
        assert_eq!(l.resources.materials.len(), 1);
        assert_eq!(l.resources.sounds.len(), 1);
        assert_eq!(l.resources.animations.len(), 1);
    }

    #[test]
    fn parses_entities_and_inherit() {
        let l = parse_level_xml(XML).unwrap();
        assert!(l.prefabs.contains_key("pf_crate"));
        assert_eq!(l.entities.len(), 3);
        assert_eq!(l.entities[0].id, "player_start");
        assert_eq!(l.entities[0].kind.as_deref(), Some("player"));
        assert_eq!(l.entities[0].camera.as_ref().unwrap().fov, 70.0);
        assert_eq!(l.entities[1].inherit.as_deref(), Some("pf_crate"));
        assert_eq!(l.entities[2].light.as_ref().unwrap().intensity, 2.5);
    }

    #[test]
    fn parses_triggers_and_scripts() {
        let l = parse_level_xml(XML).unwrap();
        assert_eq!(l.triggers.len(), 2);
        assert!(l.triggers[0].on_enter.is_some());
        assert!(l.triggers[1].on_tick.is_some());
        assert_eq!(l.scripts.len(), 1);
        assert_eq!(l.scripts[0].id, "intro");
        assert!(l.scripts[0].lua.contains("on_level_start"));
    }

    #[test]
    fn parses_bounds() {
        let l = parse_level_xml(XML).unwrap();
        assert_eq!(l.bounds.min, [-10.0, -2.0, -10.0]);
        assert_eq!(l.bounds.max, [10.0, 10.0, 10.0]);
    }

    #[test]
    fn rejects_wrong_root() {
        let bad = String::from("<notlevel>") + "/>";
        let r = parse_level_xml(&bad);
        assert!(r.is_err());
    }
}