use bevy::image::{Image, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use skylands_formats::igb::TextureWrap;

pub fn rgba8_to_bevy_image(width: u32, height: u32, data: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Model texture with the GX wrap modes of its `igTextureAttr2`. Bevy's default
/// sampler clamps, which smears meshes whose UVs tile (Spyro's body spans -1..1).
pub fn rgba8_to_model_texture(
    width: u32,
    height: u32,
    data: Vec<u8>,
    wrap: [TextureWrap; 2],
) -> Image {
    let mode = |w: TextureWrap| match w {
        TextureWrap::Clamp => ImageAddressMode::ClampToEdge,
        TextureWrap::Repeat => ImageAddressMode::Repeat,
    };
    let mut image = rgba8_to_bevy_image(width, height, data);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode(wrap[0]),
        address_mode_v: mode(wrap[1]),
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..ImageSamplerDescriptor::default()
    });
    image
}

/// Alpha-tested when any texel is not fully opaque (Spyro's wing membrane cut-out).
pub fn alpha_mode_for_rgba(data: &[u8]) -> AlphaMode {
    if data.iter().skip(3).step_by(4).any(|&a| a < 255) {
        AlphaMode::Mask(0.5)
    } else {
        AlphaMode::Opaque
    }
}

pub fn create_colored_material(
    color: Color,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.8,
        reflectance: 0.2,
        ..default()
    })
}

pub fn create_textured_material(
    image: Handle<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color_texture: Some(image),
        perceptual_roughness: 0.8,
        reflectance: 0.2,
        ..default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha_mode_for_rgba() {
        let opaque = [10, 20, 30, 255].repeat(4);
        assert_eq!(alpha_mode_for_rgba(&opaque), AlphaMode::Opaque);
        let mut cutout = opaque.clone();
        cutout[7] = 0;
        assert_eq!(alpha_mode_for_rgba(&cutout), AlphaMode::Mask(0.5));
    }

    #[test]
    fn test_model_texture_uses_wrap_modes() {
        let image = rgba8_to_model_texture(
            1,
            1,
            vec![255; 4],
            [TextureWrap::Repeat, TextureWrap::Clamp],
        );
        let ImageSampler::Descriptor(desc) = image.sampler else {
            panic!("expected explicit sampler");
        };
        assert!(matches!(desc.address_mode_u, ImageAddressMode::Repeat));
        assert!(matches!(desc.address_mode_v, ImageAddressMode::ClampToEdge));
    }
}
