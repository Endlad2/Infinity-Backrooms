//! Парсер XML в структуры Level (BDS Level Format v1 + v2).
//!
//! Обновлено:
//!   * Поддержана секция `<materials>` верхнего уровня.
//!   * Алиасы атрибутов в `<material>`: albedo/texture, uv_scale/tiling,
//!     emissive_intensity/emissive_strength.
//!   * Атрибут `flicker` в `<light>`.
//!   * Исправлен тест парсинга flicker.

use anyhow::{anyhow, Result};

use super::model::*;

#[derive(Debug, Clone)]
enum NodeKind {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<NodeKind>,
}

impl Element {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    fn attr_any(&self, keys: &[&str]) -> Option<&str> {
        for k in keys {
            if let Some(v) = self.attr(k) {
                return Some(v);
            }
        }
        None
    }
    fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find_map(|c| match c {
            NodeKind::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }
    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter_map(move |c| match c {
            NodeKind::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }
    fn inner_text(&self) -> String {
        let mut out = String::new();
        for c in &self.children {
            if let NodeKind::Text(t) = c {
                out.push_str(t);
            }
        }
        out
    }
}

const RAW_TAGS: &[&str] = &["lua", "obj", "svg", "json", "text"];

fn parse_document(src: &str) -> Result<Element> {
    let (root, _rest) = parse_element(src, 0)?;
    Ok(root)
}

fn parse_element(src: &str, mut pos: usize) -> Result<(Element, usize)> {
    let bytes = src.as_bytes();
    skip_ws(bytes, &mut pos);
    if pos >= bytes.len() || bytes[pos] != b'<' {
        return Err(anyhow!("ожидался `<` на позиции {}", pos));
    }

    if pos + 1 < bytes.len() && bytes[pos + 1] == b'?' {
        let end = find(bytes, pos, b"?>")
            .ok_or_else(|| anyhow!("не закрыт `<?...?>`"))?;
        pos = end + 2;
        return parse_element(src, pos);
    }

    let (name, attrs, pos_after_open, self_closing) = parse_open_tag(src, pos)?;
    pos = pos_after_open;

    let mut children: Vec<NodeKind> = Vec::new();

    if self_closing {
        return Ok((Element { name, attrs, children }, pos));
    }

    if RAW_TAGS.contains(&name.as_str()) {
        let close = format!("</{name}>");
        let close_pos = src[pos..]
            .find(&close)
            .ok_or_else(|| anyhow!("не найден `{close}`"))?
            + pos;
        let raw = src[pos..close_pos].to_string();
        children.push(NodeKind::Text(raw));
        let new_pos = close_pos + close.len();
        return Ok((Element { name, attrs, children }, new_pos));
    }

    loop {
        let text_start = pos;
        while pos < bytes.len() && bytes[pos] != b'<' {
            pos += 1;
        }
        if pos > text_start {
            let txt = src[text_start..pos].to_string();
            if !txt.trim().is_empty() {
                children.push(NodeKind::Text(txt));
            }
        }

        if pos >= bytes.len() {
            return Err(anyhow!("неожиданный конец файла, ожидался `</{name}>`"));
        }

        if pos + 1 < bytes.len() && bytes[pos + 1] == b'/' {
            let end = src[pos..].find('>').map(|p| p + pos)
                .ok_or_else(|| anyhow!("не закрыт `</{name}>`"))?;
            let close_name = src[pos + 2..end].trim();
            if close_name != name {
                return Err(anyhow!(
                    "закрывающий `</{}>` не совпадает с `<{}>`",
                    close_name, name
                ));
            }
            return Ok((Element { name, attrs, children }, end + 1));
        }

        let (child, next_pos) = parse_element(src, pos)?;
        children.push(NodeKind::Element(child));
        pos = next_pos;
    }
}

fn parse_open_tag(src: &str, mut pos: usize) -> Result<(String, Vec<(String, String)>, usize, bool)> {
    let bytes = src.as_bytes();
    debug_assert_eq!(bytes[pos], b'<');
    pos += 1;
    skip_ws(bytes, &mut pos);

    let name_start = pos;
    while pos < bytes.len() && is_name_byte(bytes[pos]) {
        pos += 1;
    }
    if pos == name_start {
        return Err(anyhow!("пустое имя тега на позиции {}", name_start));
    }
    let name = src[name_start..pos].to_string();

    let mut attrs: Vec<(String, String)> = Vec::new();
    loop {
        skip_ws(bytes, &mut pos);
        if pos >= bytes.len() {
            return Err(anyhow!("неожиданный конец в теге <{name}>"));
        }
        if bytes[pos] == b'>' {
            return Ok((name, attrs, pos + 1, false));
        }
        if bytes[pos] == b'/' {
            if pos + 1 < bytes.len() && bytes[pos + 1] == b'>' {
                return Ok((name, attrs, pos + 2, true));
            }
            return Err(anyhow!("`/` не перед `>` в <{name}>"));
        }
        let an_start = pos;
        while pos < bytes.len() && is_attr_name_byte(bytes[pos]) {
            pos += 1;
        }
        if pos == an_start {
            return Err(anyhow!(
                "невалидный символ в атрибутах <{name}>: `{}`",
                bytes[pos] as char
            ));
        }
        let an = src[an_start..pos].to_string();

        skip_ws(bytes, &mut pos);
        if pos >= bytes.len() || bytes[pos] != b'=' {
            return Err(anyhow!("атрибут `{an}` без `=` в <{name}>"));
        }
        pos += 1;
        skip_ws(bytes, &mut pos);
        if pos >= bytes.len() {
            return Err(anyhow!("ожидалась кавычка для `{an}`"));
        }
        let q = bytes[pos];
        if q != b'"' && q != b'\'' {
            return Err(anyhow!(
                "значение `{an}` должно быть в кавычках, а не `{}`",
                q as char
            ));
        }
        pos += 1;
        let v_start = pos;
        while pos < bytes.len() && bytes[pos] != q {
            pos += 1;
        }
        if pos >= bytes.len() {
            return Err(anyhow!("не закрыта кавычка для `{an}` в <{name}>"));
        }
        let val = src[v_start..pos].to_string();
        pos += 1;
        attrs.push((an, val));
    }
}

fn skip_ws(b: &[u8], pos: &mut usize) {
    while *pos < b.len() && b[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn find(b: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from >= b.len() || pat.is_empty() {
        return None;
    }
    b[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|p| p + from)
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b':' || b == b'-' || b == b'.'
}

fn is_attr_name_byte(b: u8) -> bool {
    is_name_byte(b)
}

// ===========================================================================
// Мост: Element → модель Level
// ===========================================================================

pub fn parse_level_xml(xml: &str) -> Result<Level> {
    let root = parse_document(xml)?;
    if root.name != "level" {
        return Err(anyhow!("ожидался <level>, а не <{}>", root.name));
    }

    let mut level = Level {
        id: root.attr("id").unwrap_or_default().to_string(),
        name: root.attr("name").unwrap_or_default().to_string(),
        version: root.attr("version").unwrap_or_default().to_string(),
        engine: root.attr("engine").unwrap_or_default().to_string(),
        format: root.attr("format").unwrap_or("bds-level/1").to_string(),
        gravity: parse_vec3(root.attr("gravity")).unwrap_or([0.0, -9.81, 0.0]),
        ambient: parse_vec3(root.attr("ambient")).unwrap_or([0.3, 0.3, 0.3]),
        skybox: root.attr("skybox").map(|s| s.to_string()),
        spawn_point: root.attr("spawn_point").map(|s| s.to_string()),
        time_scale: parse_f32(root.attr("time_scale")).unwrap_or(1.0),
        seed: root.attr("seed").and_then(|s| s.trim().parse::<u64>().ok()),
        ..Default::default()
    };

    if let Some(cs) = root.child("chunk_size") {
        level.chunk_size = Some(ChunkSize {
            x: parse_f32(cs.attr("x")).unwrap_or(0.0),
            y: parse_f32(cs.attr("y")).unwrap_or(0.0),
            z: parse_f32(cs.attr("z")).unwrap_or(0.0),
        });
    }

    if let Some(cn) = root.child("chunks") {
        for ch in cn.children_named("chunk") {
            level.chunks.push(parse_chunk(ch));
        }
    }

    if let Some(m) = root.child("meta") {
        level.meta = parse_meta(m);
    }

    if let Some(r) = root.child("resources") {
        level.resources = parse_resources(r);
    }

    // Секция <materials> ВЕРХНЕГО уровня (не внутри <resources>).
    if let Some(mats) = root.child("materials") {
        for mat in mats.children_named("material") {
            level.resources.materials.push(parse_material(mat));
        }
    }

    if let Some(p) = root.child("prefabs") {
        for prefab in p.children_named("prefab") {
            let id = prefab.attr("id").unwrap_or_default().to_string();
            if let Some(ent) = prefab.child("entity") {
                if let Ok(e) = parse_entity(ent) {
                    level.prefabs.insert(id, e);
                }
            }
        }
    }

    if let Some(e) = root.child("entities") {
        for ent in e.children_named("entity") {
            if let Ok(x) = parse_entity(ent) {
                level.entities.push(x);
            }
        }
    }

    if let Some(t) = root.child("triggers") {
        for trg in t.children_named("trigger") {
            level.triggers.push(parse_trigger(trg));
        }
    }

    if let Some(s) = root.child("scripts") {
        for sc in s.children_named("script") {
            let id = sc.attr("id").unwrap_or_default().to_string();
            let lua = sc
                .child("lua")
                .map(|n| n.inner_text().trim().to_string())
                .unwrap_or_default();
            level.scripts.push(Script { id, lua });
        }
    }

    if let Some(b) = root.child("bounds") {
        level.bounds = Bounds {
            min: parse_vec3(b.attr("min")).unwrap_or([-50.0, -5.0, -50.0]),
            max: parse_vec3(b.attr("max")).unwrap_or([50.0, 50.0, 50.0]),
        };
    } else {
        level.bounds = Bounds {
            min: [-50.0, -5.0, -50.0],
            max: [50.0, 50.0, 50.0],
        };
    }

    Ok(level)
}

fn parse_meta(n: &Element) -> Meta {
    let author = n.attr("author").map(|s| s.to_string())
        .or_else(|| n.child("author").map(|c| c.inner_text().trim().to_string()));
    let created = n.attr("created").map(|s| s.to_string())
        .or_else(|| n.child("created").map(|c| c.inner_text().trim().to_string()));
    let description = n.attr("description").map(|s| s.to_string())
        .or_else(|| n.child("description").map(|c| c.inner_text().trim().to_string()));
    let tags = if let Some(t) = n.attr("tags") {
        t.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
    } else if let Some(tc) = n.child("tags") {
        tc.inner_text().split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
    } else {
        Vec::new()
    };
    Meta { author, created, description, tags }
}

fn parse_resources(n: &Element) -> Resources {
    let mut r = Resources::default();

    for t in n.children_named("texture") {
        let id = t.attr("id").unwrap_or_default().to_string();
        let tags = parse_tags_str(t.attr("tags"));
        let width = parse_u32(t.attr("width")).unwrap_or(256);
        let height = parse_u32(t.attr("height")).unwrap_or(256);
        let filter = t.attr("filter").map(|s| s.to_string());
        let wrap = t.attr("wrap").map(|s| s.to_string());

        let source = if let Some(src) = t.attr("src") {
            TextureSource::File(src.to_string())
        } else if let Some(p) = t.attr("path") {
            TextureSource::File(p.to_string())
        } else if let Some(u) = t.attr("url") {
            TextureSource::Url(u.to_string())
        } else if let Some(c) = t.attr("color") {
            TextureSource::Color(c.to_string())
        } else if let Some(svg) = t.child("svg") {
            TextureSource::InlineSvg(svg.inner_text())
        } else if !tags.is_empty() {
            TextureSource::Tags(tags.clone())
        } else {
            TextureSource::None
        };

        r.textures.push(TextureDecl { id, source, tags, width, height, filter, wrap });
    }

    for m in n.children_named("model") {
        let id = m.attr("id").unwrap_or_default().to_string();
        let tags = parse_tags_str(m.attr("tags"));
        let source = if let Some(src) = m.attr("src") {
            ModelSource::File(src.to_string())
        } else if let Some(p) = m.attr("path") {
            ModelSource::File(p.to_string())
        } else if let Some(u) = m.attr("url") {
            ModelSource::Url(u.to_string())
        } else if let Some(o) = m.child("obj") {
            ModelSource::InlineObj(o.inner_text())
        } else if !tags.is_empty() {
            ModelSource::Tags(tags.clone())
        } else {
            ModelSource::None
        };
        r.models.push(ModelDecl { id, source, tags });
    }

    for mat in n.children_named("material") {
        r.materials.push(parse_material(mat));
    }

    for s in n.children_named("sound") {
        r.sounds.push(SoundDecl {
            id: s.attr("id").unwrap_or_default().to_string(),
            path: s.attr("path").map(|s| s.to_string()),
            url: s.attr("url").map(|s| s.to_string()),
            volume: parse_f32(s.attr("volume")).unwrap_or(1.0),
            looped: parse_bool(s.attr("loop")),
        });
    }

    for a in n.children_named("animation") {
        r.animations.push(AnimationDecl {
            id: a.attr("id").unwrap_or_default().to_string(),
            model: a.attr("model").map(|s| s.to_string()),
            clip: a.attr("clip").map(|s| s.to_string()),
            looped: parse_bool(a.attr("loop")),
            speed: parse_f32(a.attr("speed")).unwrap_or(1.0),
        });
    }

    r
}

fn parse_material(mat: &Element) -> MaterialDecl {
    MaterialDecl {
        id: mat.attr("id").unwrap_or_default().to_string(),
        texture: mat.attr_any(&["texture", "albedo"]).map(|s| s.to_string()),
        normal: mat.attr_any(&["normal", "normal_map", "nor"]).map(|s| s.to_string()),
        roughness: mat.attr_any(&["roughness_tex", "rough"]).map(|s| s.to_string()),
        ao: mat.attr_any(&["ao", "occlusion"]).map(|s| s.to_string()),
        metallic: mat.attr_any(&["metallic_tex", "metal"]).map(|s| s.to_string()),
        arm: mat.attr("arm").map(|s| s.to_string()),
        emissive: mat.attr("emissive").map(|s| s.to_string()),
        emissive_strength: parse_f32(
            mat.attr_any(&["emissive_strength", "emissive_intensity"]),
        )
        .unwrap_or(0.0),
        tiling: parse_vec2_attr(mat.attr_any(&["tiling", "uv_scale"])),
        roughness_f32: parse_f32(mat.attr("roughness")).unwrap_or(0.8),
        metallic_f32: parse_f32(mat.attr("metallic")).unwrap_or(0.0),
    }
}

fn parse_chunk(n: &Element) -> Chunk {
    let gen_str = n.attr("generator").unwrap_or("default").to_string();
    let generator = match gen_str.trim().to_lowercase().as_str() {
        "axis" => ChunkGenerator::Axis(n.attr("axis").unwrap_or("y").to_lowercase()),
        "none" => ChunkGenerator::None,
        _ => ChunkGenerator::Default,
    };
    let chance = parse_f32(n.attr("chance")).unwrap_or(0.0);

    let mut entities = Vec::new();
    for ent in n.children_named("entity") {
        if let Ok(x) = parse_entity(ent) {
            entities.push(x);
        }
    }

    Chunk {
        id: n.attr("id").unwrap_or_default().to_string(),
        generator,
        chance,
        entities,
    }
}

fn parse_entity(n: &Element) -> Result<Entity> {
    let mut e = Entity {
        id: n.attr("id").unwrap_or_default().to_string(),
        kind: n.attr("type").map(|s| s.to_string()),
        inherit: n.attr("inherit").map(|s| s.to_string()),
        model: n.attr("model").map(|s| s.to_string()),
        material: n.attr("material").map(|s| s.to_string()),
        ..Default::default()
    };

    if let Some(t) = n.child("transform") {
        e.transform = Transform {
            pos: parse_vec3(t.attr("pos")).unwrap_or([0.0; 3]),
            rot: parse_vec3(t.attr("rot")).unwrap_or([0.0; 3]),
            scale: parse_vec3(t.attr("scale")).unwrap_or([1.0; 3]),
        };
    }

    if let Some(r) = n.child("render") {
        e.render = RenderDecl {
            model: r.attr("model").map(|s| s.to_string()).or(e.model.clone()),
            material: r.attr("material").map(|s| s.to_string()).or(e.material.clone()),
            mesh: r.attr("mesh").map(|s| s.to_string()),
            cast_shadow: parse_bool(r.attr("cast_shadow")),
            receive_shadow: parse_bool(r.attr("receive_shadow")),
            ..Default::default()
        };
    }

    if let Some(m) = n.child("mesh") {
        e.render.mesh_shape = m.attr("shape").map(|s| s.to_string());
        e.render.mesh_scale = parse_vec3(m.attr("scale"));
        if e.render.mesh.is_none() {
            if let Some(s) = &e.render.mesh_shape {
                e.render.mesh = Some(s.clone());
            }
        }
    }

    if let Some(mat) = n.child("material") {
        e.render.inline_material = Some(parse_material(mat));
    }

    if let Some(p) = n.child("physics") {
        e.physics = PhysicsDecl {
            body: p.attr("body").map(|s| s.to_string()),
            collider: p.attr("collider").map(|s| s.to_string()),
            size: parse_vec3(p.attr("size")),
            radius: parse_f32(p.attr("radius")),
            height: parse_f32(p.attr("height")),
            sensor: parse_bool(p.attr("sensor")),
            mass: parse_f32(p.attr("mass")).unwrap_or(1.0),
            gravity_scale: parse_f32(p.attr("gravity_scale")).unwrap_or(1.0),
        };
    }

    if let Some(s) = n.child("stats") {
        e.stats = StatsDecl {
            hp: parse_f32(s.attr("hp")).unwrap_or(100.0),
            hp_max: parse_f32(s.attr("hp_max")).unwrap_or(100.0),
            speed: parse_f32(s.attr("speed")).unwrap_or(5.0),
            damage: parse_f32(s.attr("damage")).unwrap_or(0.0),
            jump: parse_f32(s.attr("jump")).unwrap_or(5.0),
            faction: s.attr("faction").map(|s| s.to_string()),
        };
    }

    if let Some(c) = n.child("camera") {
        e.camera = Some(CameraDecl {
            mode: c.attr("mode").map(|s| s.to_string()),
            distance: parse_f32(c.attr("distance")).unwrap_or(6.0),
            pitch: parse_f32(c.attr("pitch")).unwrap_or(0.0),
            fov: parse_f32(c.attr("fov")).unwrap_or(60.0),
        });
    }

    if let Some(l) = n.child("light") {
        e.light = Some(LightDecl {
            kind: l.attr("kind").map(|s| s.to_string()),
            color: l.attr("color").map(|s| s.to_string()),
            intensity: parse_f32(l.attr("intensity")).unwrap_or(1.0),
            range: parse_f32(l.attr("range")).unwrap_or(10.0),
            shadow: parse_bool(l.attr("shadow")),
            flicker: parse_f32(l.attr("flicker")).unwrap_or(0.0),
        });
    }

    if let Some(evs) = n.child("events") {
        for c in evs.children.iter() {
            if let NodeKind::Element(el) = c {
                e.events.push(EntityEvent {
                    kind: el.name.clone(),
                    script: el.attr("script").unwrap_or_default().to_string(),
                    func: el.attr("func").unwrap_or_default().to_string(),
                });
            }
        }
    }

    Ok(e)
}

fn parse_trigger(n: &Element) -> Trigger {
    let mut t = Trigger {
        id: n.attr("id").unwrap_or_default().to_string(),
        kind: n.attr("type").map(|s| s.to_string()),
        shape: n.attr("shape").map(|s| s.to_string()),
        size: parse_vec3(n.attr("size")),
        radius: parse_f32(n.attr("radius")),
        pos: parse_vec3(n.attr("pos")).unwrap_or([0.0; 3]),
        interval: parse_f32(n.attr("interval")),
        signal: n.attr("signal").map(|s| s.to_string()),
        ..Default::default()
    };
    for c in n.children.iter() {
        if let NodeKind::Element(el) = c {
            let ev = EntityEvent {
                kind: el.name.clone(),
                script: el.attr("script").unwrap_or_default().to_string(),
                func: el.attr("func").unwrap_or_default().to_string(),
            };
            match ev.kind.as_str() {
                "on_enter" => t.on_enter = Some(ev),
                "on_exit" => t.on_exit = Some(ev),
                "on_tick" => t.on_tick = Some(ev),
                "on_signal" => t.on_signal = Some(ev),
                _ => {}
            }
        }
    }
    t
}

fn parse_f32(s: Option<&str>) -> Option<f32> {
    s.and_then(|x| x.trim().parse::<f32>().ok())
}
fn parse_u32(s: Option<&str>) -> Option<u32> {
    s.and_then(|x| x.trim().parse::<u32>().ok())
}
fn parse_bool(s: Option<&str>) -> bool {
    matches!(
        s.map(|x| x.trim().to_lowercase()).as_deref(),
        Some("true") | Some("1") | Some("yes")
    )
}
fn parse_vec3(s: Option<&str>) -> Option<[f32; 3]> {
    let s = s?;
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 3 { return None; }
    Some([parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?])
}
fn parse_vec2_attr(s: Option<&str>) -> Option<[f32; 2]> {
    let s = s?;
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 { return None; }
    Some([parts[0].parse().ok()?, parts[1].parse().ok()?])
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

    #[test]
    fn parses_top_level_materials_section() {
        let xml = r#"<level id="l" format="bds-level/2">
<chunk_size x="32" y="16" z="32"/>
<resources>
  <texture id="tex_a" src="assets/textures/A.png"/>
  <texture id="tex_a_nor" src="assets/textures/A_nor_gl.png"/>
</resources>
<materials>
  <material id="mat_wall" albedo="tex_a" normal="tex_a_nor" roughness="0.9" uv_scale="2 4"/>
</materials>
<chunks><chunk id="c" generator="none" chance="0">
<entity id="player_start" type="player"><transform pos="0 1 0"/></entity>
</chunk></chunks>
</level>"#;
        let l = parse_level_xml(xml).unwrap();
        assert_eq!(l.resources.materials.len(), 1);
        let m = &l.resources.materials[0];
        assert_eq!(m.id, "mat_wall");
        assert_eq!(m.texture.as_deref(), Some("tex_a"));
        assert_eq!(m.normal.as_deref(), Some("tex_a_nor"));
        assert_eq!(m.tiling, Some([2.0, 4.0]));
    }

    #[test]
    fn parses_ai_style_meta_attributes() {
        let xml = r#"<level id="l" format="bds-level/2" seed="1">
<meta name="Level 0" author="Backrooms Infinity" description="Жёлтые комнаты"/>
<chunk_size x="32" y="16" z="32"/>
<chunks><chunk id="c" generator="none" chance="0">
<entity id="player_start" type="player"><transform pos="0 1 0"/></entity>
</chunk></chunks>
</level>"#;
        let l = parse_level_xml(xml).unwrap();
        assert_eq!(l.meta.author.as_deref(), Some("Backrooms Infinity"));
    }

    #[test]
    fn parses_light_flicker() {
        // Тест написан без inline-литерала внутри raw string,
        // чтобы не ловить конфликты кавычек.
        let xml = concat!(
            r#"<level id="l" format="bds-level/2">"#,
            r#"<chunk_size x="32" y="16" z="32"/>"#,
            r#"<chunks><chunk id="c" generator="none" chance="0">"#,
            r#"<entity id="player_start" type="player"><transform pos="0 1 0"/></entity>"#,
            r#"<entity id="lamp" type="light">"#,
            r#"<light kind="point" color="#fff4c2" intensity="2.4" range="20" flicker="0.06"/>"#,
            r#"</entity>"#,
            r#"</chunk></chunks></level>"#,
        );
        let l = parse_level_xml(xml).unwrap();
        let lamp = &l.chunks[0].entities[1];
        assert_eq!(lamp.light.as_ref().unwrap().flicker, 0.06);
    }
}
