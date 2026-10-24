//! 2D UI sprites (`tfbSpriteInfo`) from an IGZ `level.bld`.
//!
//! Chain: `tfbSpriteInfo` (`_name` +0x08, `SpriteInfoList` +0x74) -> list entry +0x20 (texture
//! path, or the name of another sprite) -> `AnimatableTextureDataInfo` with that path
//! (natural size +0x24 / +0x28 in screen units, `igTextureAttr2` +0x44) -> `igImage2` (+0x30).
//! The HUD in `permanent/global.bld` is built from these by `Macros_PlayerHUD.ai`.

use std::collections::HashMap;

use crate::igb::{IgbModel, IgzFile, IgzImage2};
use crate::texture::{decode_cmpr, decode_gx_rgba8};

/// `igImage2` format handle hash for GX `RGBA8` (`format_8888` art).
pub const GX_RGBA8_FORMAT: u32 = 0x0342_d599;

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteDef {
    pub name: String,
    pub texture_path: Option<String>,
    /// Natural size in screen units (`AnimatableTextureDataInfo`).
    pub natural_size: [f32; 2],
    pub image: Option<IgzImage2>,
}

#[derive(Debug, Clone, Default)]
pub struct SpriteLibrary {
    pub sprites: Vec<SpriteDef>,
    by_name: HashMap<String, usize>,
}

struct Texture {
    natural_size: [f32; 2],
    image: Option<IgzImage2>,
}

impl SpriteLibrary {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let file = IgbModel::scan_igz(data).ok()?;
        let objects = &file.objects.as_ref()?.objects;
        let u32_at = |o: usize| {
            data.get(o..o + 4)
                .map(|_| IgbModel::read_u32(data, o, file.endian))
        };
        let ptr = |o: usize| decode_ptr(&file, u32_at(o)?).filter(|&p| p < data.len());
        let cstr = |o: usize| {
            let s = ptr(o)?;
            let len = data[s..].iter().position(|&b| b == 0)?;
            std::str::from_utf8(&data[s..s + len])
                .ok()
                .map(str::to_string)
        };
        let of_type = |t: &'static str| {
            objects
                .iter()
                .filter(move |o| o.type_name.as_deref() == Some(t))
                .map(|o| o.offset)
        };

        let mut textures = HashMap::new();
        for o in of_type("AnimatableTextureDataInfo") {
            let Some(path) = cstr(o + 0x08) else { continue };
            let size = |f: usize| f32::from_bits(u32_at(o + f).unwrap_or(0));
            let image = ptr(o + 0x44)
                .and_then(|attr| ptr(attr + 0x30))
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
            textures.insert(
                path.to_ascii_lowercase(),
                Texture {
                    natural_size: [size(0x24), size(0x28)],
                    image,
                },
            );
        }

        // name -> first SpriteInfoList entry (texture path or another sprite's name)
        let mut targets = Vec::new();
        for o in of_type("tfbSpriteInfo") {
            let Some(name) = cstr(o + 0x08) else { continue };
            let target = ptr(o + 0x74).and_then(|list| cstr(list + 0x20));
            targets.push((name, target));
        }
        let by_target: HashMap<String, Option<String>> = targets
            .iter()
            .map(|(n, t)| (n.to_ascii_lowercase(), t.clone()))
            .collect();

        let mut lib = Self::default();
        for (name, target) in targets {
            let key = name.to_ascii_lowercase();
            if lib.by_name.contains_key(&key) {
                continue;
            }
            // Follow sprite -> sprite references until a texture path.
            let mut cur = target;
            let mut texture = None;
            for _ in 0..8 {
                let Some(t) = cur.take() else { break };
                let lower = t.to_ascii_lowercase();
                if let Some(tex) = textures.get(&lower) {
                    texture = Some((t, tex));
                    break;
                }
                cur = by_target.get(&lower).cloned().flatten();
            }
            lib.by_name.insert(key, lib.sprites.len());
            lib.sprites.push(SpriteDef {
                name,
                natural_size: texture.as_ref().map_or([0.0; 2], |(_, t)| t.natural_size),
                image: texture.as_ref().and_then(|(_, t)| t.image.clone()),
                texture_path: texture.map(|(p, _)| p),
            });
        }
        Some(lib)
    }

    pub fn get(&self, name: &str) -> Option<&SpriteDef> {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .map(|&i| &self.sprites[i])
    }
}

impl SpriteDef {
    /// Decode this sprite's texture from the IGZ it was parsed from.
    pub fn decode_rgba(&self, data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
        let img = self.image.as_ref()?;
        let (w, h) = (img.width as u32, img.height as u32);
        let raw = data.get(img.texture_offset..img.texture_offset + img.texture_size)?;
        let rgba8_len = (w.div_ceil(4) * h.div_ceil(4) * 64) as usize;
        let rgba = if img.format == GX_RGBA8_FORMAT || raw.len() == rgba8_len {
            decode_gx_rgba8(raw, w, h)
        } else {
            decode_cmpr(raw, w, h)
        };
        rgba.ok().map(|px| (w, h, px))
    }
}

fn decode_ptr(file: &IgzFile, w: u32) -> Option<usize> {
    if w == 0 || w & 0x8000_0000 != 0 {
        return None;
    }
    let s = file.sections.get(((w >> 27) + 1) as usize)?;
    Some(s.offset + (w & 0x07FF_FFFF) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn global_bld() -> Option<Vec<u8>> {
        let p = ["assets/files/permanent", "../../assets/files/permanent"]
            .into_iter()
            .map(|d| std::path::Path::new(d).join("global.bld"))
            .find(|p| p.exists())?;
        let mut m = crate::archive::ArchiveMount::new();
        m.mount(&p).ok()?;
        m.read_file("level.bld").ok()
    }

    #[test]
    fn hud_sprites_resolve_to_textures() {
        let Some(data) = global_bld() else { return };
        let lib = SpriteLibrary::parse(&data).expect("sprite library");

        let coin = lib.get("sky4 hud coin").expect("coin sprite");
        assert_eq!(
            coin.texture_path.as_deref(),
            Some("UI_SKY4/HUD/PNG/format_8888/HUD_Coin2.png")
        );
        assert!((coin.natural_size[0] - 16.89).abs() < 0.01);
        let (w, h, px) = coin.decode_rgba(&data).expect("coin texture");
        assert_eq!((w, h), (17, 17));
        assert!(px.chunks(4).any(|p| p[3] > 0));

        // Generic_Container -> Generic_GraySquare -> gray.png
        let c = lib.get("Generic_Container").expect("container");
        assert_eq!(
            c.texture_path.as_deref(),
            Some("Levels/Includes/Global/sprites/gray.png")
        );
    }
}
