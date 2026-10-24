//! Bitmap font (`igBitmapFont`) from an IGZ `level.bld` (`fonts/Font_002.png` in
//! `permanent/global.bld`, the font of the original HUD and menus).
//!
//! `igBitmapFont`: point size f32 +0x0c, `igCharMetricsList` +0x3c, `tfbTextureBindAttr` +0x48
//! (colour `igTextureAttr2` +0x10 -> `igImage2` +0x30). List: count +0x08, data +0x14.
//! `igCharMetrics`: char code u16 +0x08, advance +0x0c, x / y offset +0x10 / +0x14, width / height
//! +0x18 / +0x1c (font pixels), UV rectangle (u0, v0, u1, v1) +0x20.
//! The sheet is GX `RGB565`, stored top row first, with one mask per channel: red = fill, green =
//! outline, blue = outer edge.

use std::collections::HashMap;

use crate::igb::{IgbModel, IgzImage2};
use crate::sprite::decode_ptr;
use crate::texture::decode_gx_rgb565;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub advance: f32,
    pub offset: [f32; 2],
    pub size: [f32; 2],
    /// Top-left and bottom-right UVs.
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct BitmapFont {
    pub point_size: f32,
    pub glyphs: HashMap<char, Glyph>,
    pub image: Option<IgzImage2>,
}

impl BitmapFont {
    /// The first `igBitmapFont` in the file.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let file = IgbModel::scan_igz(data).ok()?;
        let objects = &file.objects.as_ref()?.objects;
        let types: HashMap<usize, &str> = objects
            .iter()
            .filter_map(|o| Some((o.offset, o.type_name.as_deref()?)))
            .collect();
        let u32_at = |o: usize| {
            data.get(o..o + 4)
                .map(|_| IgbModel::read_u32(data, o, file.endian))
        };
        let f32_at = |o: usize| u32_at(o).map(f32::from_bits);
        let typed =
            |o: usize, t: &str| decode_ptr(&file, u32_at(o)?).filter(|p| types.get(p) == Some(&t));

        let font = objects
            .iter()
            .find(|o| o.type_name.as_deref() == Some("igBitmapFont"))?
            .offset;
        let list = typed(font + 0x3c, "igCharMetricsList")?;
        let count = u32_at(list + 0x08)? as usize;
        let items = decode_ptr(&file, u32_at(list + 0x14)?)?;

        let mut glyphs = HashMap::new();
        for i in 0..count {
            let Some(m) = typed(items + i * 4, "igCharMetrics") else {
                continue;
            };
            let code = IgbModel::read_u16(data, m + 0x08, file.endian);
            let (Some(c), Some(v)) = (
                char::from_u32(code as u32),
                (0..9)
                    .map(|k| f32_at(m + 0x0c + k * 4))
                    .collect::<Option<Vec<_>>>(),
            ) else {
                continue;
            };
            glyphs.insert(
                c,
                Glyph {
                    advance: v[0],
                    offset: [v[1], v[2]],
                    size: [v[3], v[4]],
                    uv_min: [v[5], v[6]],
                    uv_max: [v[7], v[8]],
                },
            );
        }

        let image = typed(font + 0x48, "tfbTextureBindAttr")
            .and_then(|bind| typed(bind + 0x10, "igTextureAttr2"))
            .and_then(|attr| typed(attr + 0x30, "igImage2"))
            .and_then(|img| {
                IgzImage2::parse(
                    data,
                    img,
                    file.version,
                    file.endian,
                    &file.exid_formats,
                    &file.tmhn_entries,
                )
            });

        Some(Self {
            point_size: f32_at(font + 0x0c)?,
            glyphs,
            image,
        })
    }

    /// Decode the glyph sheet as white fill on a black outline, so a tint colours only the fill.
    pub fn decode_rgba(&self, data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
        let img = self.image.as_ref()?;
        let (w, h) = (img.width as u32, img.height as u32);
        let raw = data.get(img.texture_offset..img.texture_offset + img.texture_size)?;
        let mut px = decode_gx_rgb565(raw, w, h).ok()?;
        for p in px.as_chunks_mut::<4>().0 {
            let (fill, outline, edge) = (p[0] as u32, p[1] as u32, p[2] as u32);
            let alpha = (fill + outline + edge).min(255);
            let white = (fill * 255).checked_div(alpha).unwrap_or(0).min(255) as u8;
            p.copy_from_slice(&[white, white, white, alpha as u8]);
        }
        Some((w, h, px))
    }

    /// Width of `text` in font pixels.
    pub fn measure(&self, text: &str) -> f32 {
        text.chars()
            .filter_map(|c| self.glyphs.get(&c))
            .map(|g| g.advance)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_font_has_hud_glyphs() {
        let Some(data) = crate::sprite::tests::global_bld() else {
            return;
        };
        let font = BitmapFont::parse(&data).expect("bitmap font");
        assert_eq!(font.point_size, 56.0);
        assert_eq!(font.glyphs.len(), 158);
        let a = font.glyphs[&'A'];
        assert_eq!((a.advance, a.size), (33.0, [34.0, 36.0]));
        assert!((a.uv_min[0] - 0.203).abs() < 1e-3 && (a.uv_min[1] - 0.506).abs() < 1e-3);
        assert_eq!(font.measure("AA"), 66.0);

        let (w, h, px) = font.decode_rgba(&data).expect("glyph sheet");
        assert_eq!((w, h), (512, 328));
        // Inside 'A' (fill is white), outside a glyph transparent.
        let at = |x: u32, y: u32| &px[((y * w + x) * 4) as usize..][..4];
        let (x0, y0) = (a.uv_min[0] * w as f32, a.uv_min[1] * h as f32);
        assert!((0..a.size[0] as u32)
            .flat_map(|x| (0..a.size[1] as u32).map(move |y| (x, y)))
            .any(|(x, y)| at(x0 as u32 + x, y0 as u32 + y) == [255, 255, 255, 255]));
        assert_eq!(at(w - 1, h - 1)[3], 0);
    }
}
