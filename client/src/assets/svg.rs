//! Конвертация SVG → PNG через resvg + tiny-skia.
//! Используется для растрирования текстур, сгенерированных ИИ.

use anyhow::{anyhow, Result};

/// Растеризовать SVG-строку в PNG-байты.
/// `width` и `height` задают размер итогового PNG. Если 0 — берётся
/// размер из viewBox самого SVG.
pub fn svg_to_png(svg: &str, width: u32, height: u32) -> Result<Vec<u8>> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt)
        .map_err(|e| anyhow!("SVG parse error: {e}"))?;

    let size = tree.size();
    let (w, h) = if width == 0 || height == 0 {
        (size.width().max(1.0) as u32, size.height().max(1.0) as u32)
    } else {
        (width, height)
    };

    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| anyhow!("не удалось создать pixmap {w}x{h}"))?;

    let ts = tiny_skia::Transform::from_scale(
        w as f32 / size.width(),
        h as f32 / size.height(),
    );
    resvg::render(&tree, ts, &mut pixmap.as_mut());

    pixmap
        .encode_png()
        .map_err(|e| anyhow!("PNG encode error: {e}"))
}

/// Сгенерировать простую однотонную SVG-заглушку (используется как fallback,
/// если ИИ-бэкенд недоступен).
pub fn solid_color_svg(color: &str, w: u32, h: u32) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"><rect width="{w}" height="{h}" fill="{color}"/></svg>"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_svg_renders_png() {
        let svg = solid_color_svg("#ff0000", 32, 32);
        let png = svg_to_png(&svg, 0, 0).unwrap();
        // PNG signature
        assert!(png.len() > 8);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn resize_works() {
        let svg = solid_color_svg("#00ff00", 16, 16);
        let png = svg_to_png(&svg, 64, 64).unwrap();
        assert!(png.len() > 8);
    }

    #[test]
    fn invalid_svg_returns_err() {
        let r = svg_to_png("not svg at all", 0, 0);
        assert!(r.is_err());
    }
}