use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

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
