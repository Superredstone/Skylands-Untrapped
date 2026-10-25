//! Magic Moment: the portal-space sequence of a Skylander put on the portal, rebuilt from
//! the game's own data. `character/sky4_mm_streaming.bld` holds the `mmsky4_controller.ai`
//! script and its art: `MMRig` (`MMsky4_uberModel`, an animated rig whose attach nodes carry
//! the camera, the Skylander, its name, the element icon and the tunnel lights), the cloud
//! tunnel and star tubes (scrolling textures), the element icons and backgrounds. The name is
//! spelled with the 3D letters of `item/3d_font_streamed.bld`. Timing: `skylands_core::MagicMoment`.
//!
//! Notes: `agent/MAGIC_MOMENT.md`.

use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use bevy::utils::HashMap;
use skylands_core::{
    get_character_asset_paths, get_character_info, ActorIdentity, Element, MagicMoment, MM_ALIVE,
    MM_BACKGROUND, MM_PORTAL_SPACE, MM_REVEAL, MM_SHOW_MINION, MM_TUNNEL_TINT,
};
use skylands_formats::igb::{AnimationClip, IgbModel, Skeleton};

use crate::camera::{MainCamera, SkyCamera};
use crate::visual_sync::{CharacterAssetServer, CharacterVisualRegistry};

pub const MM_LAYER: usize = 2;

/// "world to portal": `mm pos` is displaced 7000 units down and 7000 back from the world.
const MM_ORIGIN: Vec3 = Vec3::new(0.0, -7000.0, -7000.0);
const MM_STREAM: &str = "sky4_mm_streaming.bld";
const FONT_STREAM: &str = "3d_font_streamed.bld";

/// `MMRig` animation states (`tfbAnimationState._id`).
const RIG_NEW_TOY: i32 = 3;
const RIG_NEW_TOY_LOOP: i32 = 4;
const RIG_MAGIC_MOMENT: i32 = 5;
const RIG_MAGIC_MOMENT_LOOP: i32 = 6;
/// "minion to world" plays `7 + 2`: "Hyper Jump (alt C)".
const RIG_HYPER_JUMP: i32 = 9;
/// Hero animation states.
const HERO_TOY_ALIVE: i32 = 80;
const HERO_TOY: i32 = 81;

/// Portal-space camera field of view (degrees): `default.field of view` 89 on the Wii
/// (`Hardware Platform` 3) sliding to 68 over 0.5 s, back to 89 over the hyper jump.
const FOV_WIDE: f32 = 89.0;
const FOV_MM: f32 = 68.0;
/// "mmsky4 - set camera state" pull-out: `target zoomed out fov add` 21 on the Wii.
const PULL_OUT_FOV_ADD: f32 = 21.0;
/// `name scale` 25 -> letters (1.8 units tall) at a quarter size.
const NAME_SCALE: f32 = 0.25;

/// `ColorMeasurement` literals (`0xAABBGGRR`) from the controller script.
const CLOUD_TUBE_TINT: i32 = -2260436;
const LIGHT_TINTS: [i32; 3] = [-1763461, -10710234, -5885026];

pub fn abgr(value: i32) -> Color {
    let v = value as u32;
    Color::srgba_u8(
        (v & 0xff) as u8,
        (v >> 8 & 0xff) as u8,
        (v >> 16 & 0xff) as u8,
        (v >> 24) as u8,
    )
}

/// What a Magic Moment piece is, for its per-frame fade / tint.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmPiece {
    CloudTube,
    FastStars,
    SlowStars,
    NeutralIcon,
    ElementIcon,
    Background,
    Name,
}

/// Root of one Magic Moment scene (`serial` = `MagicMoment::serial`).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct MagicMomentScene {
    pub serial: u32,
}

/// `MMRig` joint (bone index), animated in model space under the scene root.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct MmJoint(pub usize);

#[derive(Component)]
pub struct MagicMomentCamera;

/// The Skylander shown in portal space: drawn by the visual sync like the hero it copies.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct MagicMomentModel {
    pub serial: u32,
}

/// Clip (by name) and time the Magic Moment holds the copied Skylander at.
#[derive(Component, Debug, Clone, PartialEq, Default)]
pub struct MagicMomentPose {
    pub clip: String,
    pub time: f32,
}

/// `replace color` / `replace color percentage` of the copied Skylander's materials.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct MmOwnMaterial {
    pub base: Color,
}

/// Texture scroll (`tfbScrollUVAnimData` velocity, UV units per second).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct MmScroll(pub Vec2);

#[derive(Clone)]
struct MmPart {
    mesh: Handle<Mesh>,
    material: StandardMaterial,
    uv_scroll: Option<Vec2>,
    /// Articulated drawable: (model stem, joint) it follows.
    joint: Option<(String, usize)>,
    /// Index into the source `IgbModel.meshes`.
    source: usize,
}

/// Articulated background part (tech gears, earth boulders...): model-space mesh moved by
/// its joint, `joint(t) * joint(bind)^-1`.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct MmJointPart {
    pub rig: String,
    pub joint: usize,
}

/// Rig and looping clip of an articulated model (`MMsky4_elemental_bg_tech_vfxAnim`...).
struct PartRig {
    skeleton: Skeleton,
    clip: Option<AnimationClip>,
}

struct MmRig {
    skeleton: Skeleton,
    clips: HashMap<i32, AnimationClip>,
    /// Attach node name -> bone index.
    nodes: HashMap<String, usize>,
}

/// Magic Moment art, loaded on first use.
#[derive(Resource, Default)]
pub struct MagicMomentAssets {
    loaded: bool,
    rig: Option<MmRig>,
    /// `tfbActorInfo` name or `.mdl` file stem -> meshes.
    actors: HashMap<String, Vec<MmPart>>,
    /// Letter -> meshes and advance (model units).
    letters: HashMap<char, (Vec<MmPart>, f32)>,
    /// `MM_element_NN_<element>_color` by element key.
    element_colors: HashMap<String, Color>,
    /// Model stem -> rig of articulated models.
    part_rigs: HashMap<String, PartRig>,
}

impl MagicMomentAssets {
    fn rig_clip(&self, state: i32) -> Option<&AnimationClip> {
        self.rig.as_ref()?.clips.get(&state)
    }

    fn rig_duration(&self, state: i32) -> f32 {
        self.rig_clip(state).map(|c| c.duration).unwrap_or(1.0)
    }

    /// Tunnel colour of an element: the stream's `MM_element_NN_<element>_color`.
    fn element_color(&self, element: Element) -> Color {
        self.element_colors
            .get(element_key(element))
            .copied()
            .unwrap_or_else(|| {
                let (r, g, b) = element.rgb();
                Color::srgb_u8(r, g, b)
            })
    }

    fn actor(&self, key: &str) -> &[MmPart] {
        self.actors
            .get(&key.to_ascii_lowercase())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn load(
        &mut self,
        asset_server: Option<&CharacterAssetServer>,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
    ) {
        self.loaded = true;
        let Some(loader) = asset_server.and_then(|s| s.loader.lock().ok()) else {
            return;
        };
        if let Ok(model) = loader.load_igb(MM_STREAM) {
            for (name, value) in &model.script_colors {
                let Some(key) = name
                    .strip_prefix("MM_element_")
                    .and_then(|s| s.strip_suffix("_color"))
                    .and_then(|s| s.split_once('_'))
                    .map(|(_, key)| key.to_string())
                else {
                    continue;
                };
                self.element_colors
                    .insert(key, abgr(*value as i32).with_alpha(1.0));
            }
            self.rig = model
                .actor_models
                .iter()
                .find(|a| a.actor == "MMRig")
                .and_then(|rig| {
                    let skel = rig.skeleton?;
                    let skeleton = model.skeletons.get(skel)?.clone();
                    let bone = |name: &str| skeleton.bones.iter().position(|b| b.name == name);
                    let nodes = rig
                        .attach_nodes
                        .iter()
                        .filter_map(|n| Some((n.name.clone(), bone(&n.bone)?)))
                        .collect();
                    let clips = model
                        .animation_states
                        .iter()
                        .filter_map(|s| {
                            let clip = model.animations.get(s.clip?)?;
                            (clip.skeleton == Some(skel)).then(|| (s.id, clip.clone()))
                        })
                        .collect();
                    Some(MmRig {
                        skeleton,
                        clips,
                        nodes,
                    })
                });
            for actor in &model.actor_models {
                let stem = actor
                    .model
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".mdl")
                    .to_ascii_lowercase();
                let mut parts = load_parts(
                    &model,
                    &actor.meshes,
                    &loader,
                    MM_STREAM,
                    meshes,
                    images,
                    false,
                );
                if parts.is_empty() {
                    continue;
                }
                if actor.actor != "MMRig" {
                    if let Some(skeleton) = actor.skeleton.and_then(|s| model.skeletons.get(s)) {
                        for part in &mut parts {
                            let bone = actor
                                .meshes
                                .iter()
                                .position(|&m| m == part.source)
                                .and_then(|i| actor.mesh_bones.get(i).copied().flatten());
                            part.joint = bone.map(|b| (stem.clone(), b));
                        }
                        // Its looping clip: the state named after the model.
                        let clip = model
                            .animation_states
                            .iter()
                            .find(|s| s.name.to_ascii_lowercase().starts_with(&stem))
                            .and_then(|s| model.animations.get(s.clip?).cloned());
                        self.part_rigs.insert(
                            stem.clone(),
                            PartRig {
                                skeleton: skeleton.clone(),
                                clip,
                            },
                        );
                    }
                }
                self.actors.insert(stem, parts.clone());
                self.actors.insert(actor.actor.to_ascii_lowercase(), parts);
            }
        }
        if let Ok(font) = loader.load_igb(FONT_STREAM) {
            for actor in &font.actor_models {
                let stem = actor.model.rsplit('/').next().unwrap_or("");
                let Some(c) = letter_for_model(stem) else {
                    continue;
                };
                let parts = load_parts(
                    &font,
                    &actor.meshes,
                    &loader,
                    FONT_STREAM,
                    meshes,
                    images,
                    true,
                );
                let (lo, hi) = actor
                    .meshes
                    .iter()
                    .filter_map(|&m| font.meshes.get(m))
                    .flat_map(|m| &m.vertices)
                    .fold((f32::MAX, f32::MIN), |(lo, hi), v| {
                        (lo.min(v.position[0]), hi.max(v.position[0]))
                    });
                let width = if hi > lo { hi - lo } else { 0.0 };
                self.letters.insert(c, (parts, width));
            }
        }
    }
}

/// `letter_A.mdl` -> 'A', `number_0.mdl` -> '0', `symbol_-.mdl` -> '-'.
fn letter_for_model(stem: &str) -> Option<char> {
    let stem = stem.trim_end_matches(".mdl");
    let rest = stem
        .strip_prefix("letter_")
        .or_else(|| stem.strip_prefix("number_"))
        .or_else(|| stem.strip_prefix("symbol_"))?;
    let mut chars = rest.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c.to_ascii_uppercase())
}

/// GX vertex colours are 0..0.5 for full brightness on effect meshes (TEV doubles them).
fn double_vertex_colors(mesh: &mut Mesh) {
    use bevy::render::mesh::VertexAttributeValues;
    if let Some(VertexAttributeValues::Float32x4(colors)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    {
        if colors.iter().all(|c| c[..3].iter().all(|&v| v <= 0.51)) {
            for c in colors.iter_mut() {
                for v in &mut c[..3] {
                    *v *= 2.0;
                }
            }
        }
    }
}

fn load_parts(
    model: &IgbModel,
    indices: &[usize],
    loader: &skylands_formats::AssetLoader,
    bld: &str,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    lit: bool,
) -> Vec<MmPart> {
    indices
        .iter()
        .filter_map(|&i| Some((i, model.meshes.get(i)?)))
        .filter(|(_, m)| !m.vertices.is_empty())
        .filter(|(_, m)| m.base_color.is_none_or(|c| c[3] > 0.0))
        .map(|(source, m)| {
            let mut mesh = crate::mesh_conversion::igb_mesh_to_bevy(m);
            mesh.remove_attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
            mesh.remove_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
            double_vertex_colors(&mut mesh);
            let mut alpha_mode = AlphaMode::Blend;
            let texture = m
                .image_index
                .and_then(|i| loader.load_image_rgba(bld, i).ok())
                .map(|(w, h, rgba)| {
                    // Sparks on black (star tubes) are added, not blended.
                    let n = (rgba.len() / 4).max(1) as u32;
                    let mean = rgba
                        .chunks(4)
                        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
                        .sum::<u32>()
                        / (3 * n);
                    if mean < 16 {
                        alpha_mode = AlphaMode::Add;
                    }
                    images.add(crate::texture_conversion::rgba8_to_model_texture(
                        w,
                        h,
                        rgba,
                        m.texture_wrap,
                    ))
                });
            let color = m
                .base_color
                .map(|c| Color::srgba(c[0], c[1], c[2], c[3]))
                .unwrap_or(Color::WHITE);
            MmPart {
                mesh: meshes.add(mesh),
                material: StandardMaterial {
                    base_color: color,
                    base_color_texture: texture,
                    alpha_mode: if lit { AlphaMode::Opaque } else { alpha_mode },
                    unlit: !lit,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                },
                uv_scroll: m.uv_scroll.map(Vec2::from),
                joint: None,
                source,
            }
        })
        .collect()
}

/// `MMsky4_elemental_bg_*` / `MMsky4_icon_*` suffix of an element.
fn element_key(element: Element) -> &'static str {
    match element {
        Element::Air => "air",
        Element::Life => "life",
        Element::Undead => "undead",
        Element::Earth => "earth",
        Element::Fire => "fire",
        Element::Water => "water",
        Element::Magic => "magic",
        Element::Tech => "tech",
        Element::Kaos => "kaos",
    }
}

fn smooth(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Rig animation state and time at `t` seconds into the Magic Moment.
pub fn rig_pose(mm: &MagicMoment, duration: impl Fn(i32) -> f32) -> (i32, f32) {
    let t = mm.elapsed;
    let chain = |start: f32, first: i32, looped: i32| {
        let dt = (t - start).max(0.0);
        let d = duration(first);
        if dt < d {
            (first, dt)
        } else {
            (looped, (dt - d) % duration(looped).max(1e-3))
        }
    };
    if t >= mm.to_world() {
        (
            RIG_HYPER_JUMP,
            (t - mm.to_world()).min(duration(RIG_HYPER_JUMP)),
        )
    } else if t >= MM_REVEAL {
        chain(MM_REVEAL, RIG_MAGIC_MOMENT, RIG_MAGIC_MOMENT_LOOP)
    } else {
        chain(MM_PORTAL_SPACE, RIG_NEW_TOY, RIG_NEW_TOY_LOOP)
    }
}

/// Portal-space camera field of view (degrees).
pub fn portal_fov(mm: &MagicMoment) -> f32 {
    let t = mm.elapsed;
    if t >= mm.to_world() {
        let k = smooth((t - mm.to_world()) / skylands_core::MM_HYPER_JUMP);
        FOV_MM + (FOV_WIDE - FOV_MM) * k
    } else {
        let k = smooth((t - MM_PORTAL_SPACE) / 0.5);
        FOV_WIDE + (FOV_MM - FOV_WIDE) * k
    }
}

/// Alchemy camera field of view is horizontal; Bevy's is vertical.
pub fn vertical_fov(horizontal: f32, aspect: f32) -> f32 {
    2.0 * ((horizontal * 0.5).tan() / aspect.max(0.01)).atan()
}

/// Copied Skylander: (`replace color` grey level, `replace color percentage`).
/// Stage 1: black silhouette, colour slides to white over 0.8 s; stage 5: the percentage
/// slides from 100 to 0 over 0.12 s.
pub fn replace_color(mm: &MagicMoment) -> (f32, f32) {
    let t = mm.elapsed;
    if t < MM_ALIVE {
        (smooth((t - MM_REVEAL) / 0.8), 1.0)
    } else {
        (1.0, 1.0 - smooth((t - MM_ALIVE) / 0.12))
    }
}

/// Copied Skylander: (animation state, time). `ToyAlive` holds its first frame (playback
/// rate 0) until stage 5, then plays; "minion to world" plays `Toy`.
pub fn hero_pose(mm: &MagicMoment) -> (i32, f32) {
    let t = mm.elapsed;
    if t >= mm.to_world() {
        (HERO_TOY, t - mm.to_world())
    } else {
        (HERO_TOY_ALIVE, (t - MM_ALIVE).max(0.0))
    }
}

/// (visible, alpha 0..1, tint) of a piece at the current time.
pub fn piece_state(piece: MmPiece, mm: &MagicMoment, element: Color) -> (bool, f32, Option<Color>) {
    let t = mm.elapsed;
    let to_world = mm.to_world();
    match piece {
        MmPiece::CloudTube => {
            let k = smooth((t - MM_TUNNEL_TINT) / 0.4);
            let (a, b) = (abgr(CLOUD_TUBE_TINT).to_linear(), element.to_linear());
            let tint = LinearRgba::rgb(
                a.red + (b.red - a.red) * k,
                a.green + (b.green - a.green) * k,
                a.blue + (b.blue - a.blue) * k,
            );
            (true, 1.0, Some(tint.into()))
        }
        MmPiece::FastStars => (true, 1.0, None),
        MmPiece::SlowStars => {
            let a = smooth((t - to_world) / 0.5);
            (a > 0.0, a, None)
        }
        MmPiece::NeutralIcon => (t < MM_SHOW_MINION, 1.0, None),
        MmPiece::ElementIcon => ((MM_SHOW_MINION..to_world).contains(&t), 1.0, None),
        MmPiece::Background => {
            let a = smooth((t - MM_BACKGROUND) / 0.3) * (1.0 - smooth((t - to_world) / 2.0));
            (a > 0.0, a, None)
        }
        MmPiece::Name => ((MM_REVEAL..to_world).contains(&t), 1.0, None),
    }
}

pub fn setup_magic_moment(mut commands: Commands) {
    commands.init_resource::<MagicMomentAssets>();
    commands.spawn((
        Camera3d::default(),
        Camera {
            // Portal space covers the whole screen, over the world camera.
            order: 1,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: FOV_WIDE.to_radians(),
            near: 0.1,
            far: 5000.0,
            ..default()
        }),
        Transform::from_translation(MM_ORIGIN + Vec3::new(0.0, 1.0, 6.0)),
        RenderLayers::layer(MM_LAYER),
        MagicMomentCamera,
    ));
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.97, 0.9),
            illuminance: 6000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.5, 0.3, 0.0)),
        RenderLayers::layer(MM_LAYER),
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_scene(
    commands: &mut Commands,
    assets: &MagicMomentAssets,
    materials: &mut Assets<StandardMaterial>,
    mm: &MagicMoment,
    char_id: u16,
) {
    let (_, element) = get_character_info(char_id);
    let layer = RenderLayers::layer(MM_LAYER);
    let root = commands
        .spawn((
            MagicMomentScene { serial: mm.serial },
            Transform::from_translation(MM_ORIGIN),
            Visibility::default(),
            layer.clone(),
        ))
        .id();

    let joints: Vec<Entity> = assets
        .rig
        .as_ref()
        .map(|rig| {
            (0..rig.skeleton.bones.len())
                .map(|i| {
                    commands
                        .spawn((
                            MmJoint(i),
                            Transform::default(),
                            Visibility::default(),
                            layer.clone(),
                        ))
                        .set_parent(root)
                        .id()
                })
                .collect()
        })
        .unwrap_or_default();
    let node = |name: &str| {
        assets
            .rig
            .as_ref()
            .and_then(|r| r.nodes.get(name))
            .and_then(|&b| joints.get(b))
            .copied()
            .unwrap_or(root)
    };

    let mut spawn_parts = |commands: &mut Commands,
                           parts: &[MmPart],
                           parent: Entity,
                           piece: MmPiece,
                           at: Transform| {
        for part in parts {
            // The tubes surround the camera and their origin is behind it: draw them first,
            // as the backdrop (transparent meshes sort by origin distance + `depth_bias`).
            let depth_bias = match piece {
                MmPiece::CloudTube => -10_000.0,
                MmPiece::FastStars | MmPiece::SlowStars => -9_000.0,
                _ => 0.0,
            };
            let mut e = commands.spawn((
                Mesh3d(part.mesh.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    depth_bias,
                    ..part.material.clone()
                })),
                at,
                Visibility::Hidden,
                piece,
                layer.clone(),
            ));
            if let Some(speed) = part.uv_scroll {
                e.insert(MmScroll(speed));
            }
            if let Some((rig, joint)) = &part.joint {
                e.insert(MmJointPart {
                    rig: rig.clone(),
                    joint: *joint,
                });
            }
            e.set_parent(parent);
        }
    };

    spawn_parts(
        commands,
        assets.actor("MM_dualEndedTunnelTube"),
        root,
        MmPiece::CloudTube,
        Transform::default(),
    );
    spawn_parts(
        commands,
        assets.actor("MM_fastStars"),
        root,
        MmPiece::FastStars,
        Transform::default(),
    );
    spawn_parts(
        commands,
        assets.actor("MM_slowStars"),
        root,
        MmPiece::SlowStars,
        Transform::default(),
    );
    spawn_parts(
        commands,
        assets.actor("MMsky4_icon_random"),
        node("icon"),
        MmPiece::NeutralIcon,
        Transform::default(),
    );
    let key = element_key(element);
    spawn_parts(
        commands,
        assets.actor(&format!("MMsky4_icon_{key}")),
        node("icon"),
        MmPiece::ElementIcon,
        Transform::default(),
    );
    // Attach node 12 ("waterWave", `J_elementalSludge`): behind and below the Skylander.
    spawn_parts(
        commands,
        assets.actor(&format!("MMsky4_elemental_bg_{key}")),
        node("waterWave"),
        MmPiece::Background,
        Transform::default(),
    );
    // "displace @platform len=2 pitch=-90": two units below its node.
    spawn_parts(
        commands,
        assets.actor("MM_platform_rig"),
        node("platform"),
        MmPiece::Background,
        Transform::from_xyz(0.0, -2.0, 0.0),
    );

    // 3D name, centred on the name node. "UI - Text - 3d - String Create" at `name scale`
    // 25 (the clamp every name hits): letters at 25%.
    let name_holder = commands
        .spawn((
            Transform::from_scale(Vec3::splat(NAME_SCALE)),
            Visibility::default(),
            layer.clone(),
        ))
        .set_parent(node("name"))
        .id();
    let name: Vec<char> = mm.name.to_uppercase().chars().collect();
    let space = assets
        .letters
        .get(&'A')
        .map(|(_, w)| w * 0.6)
        .unwrap_or(0.5);
    let gap = space * 0.1;
    let advance = |c: &char| assets.letters.get(c).map(|(_, w)| *w).unwrap_or(space) + gap;
    let total: f32 = name.iter().map(advance).sum::<f32>() - gap;
    let mut x = -total * 0.5;
    for c in &name {
        let w = advance(c) - gap;
        if let Some((parts, _)) = assets.letters.get(c) {
            spawn_parts(
                commands,
                parts,
                name_holder,
                MmPiece::Name,
                Transform::from_xyz(x + w * 0.5, 0.0, 0.0),
            );
        }
        x += w + gap;
    }

    // Tunnel lights on their nodes.
    for (i, n) in ["tunnel light A", "tunnel light B", "tunnel light C"]
        .iter()
        .enumerate()
    {
        commands
            .spawn((
                PointLight {
                    color: abgr(LIGHT_TINTS[i]),
                    intensity: 400_000.0,
                    range: 60.0,
                    ..default()
                },
                Transform::default(),
                layer.clone(),
            ))
            .set_parent(node(n));
    }
    // Attach node 11 ("skylander light", just under the Skylander).
    commands
        .spawn((
            PointLight {
                color: Color::WHITE,
                intensity: 300_000.0,
                range: 30.0,
                ..default()
            },
            Transform::default(),
            layer.clone(),
        ))
        .set_parent(node("skylander light"));

    // The Skylander (`Portal - Display Actor Create`) on the Skylander node.
    let (model_path, tex_path) = get_character_asset_paths(char_id);
    commands
        .spawn((
            ActorIdentity {
                id: u32::MAX,
                character_id: char_id,
                name: mm.name.clone(),
                is_villain: false,
                model_path: Some(model_path.to_string()),
                texture_path: tex_path.map(|s| s.to_string()),
            },
            MagicMomentModel { serial: mm.serial },
            MagicMomentPose::default(),
            Transform::default(),
            Visibility::Hidden,
            layer,
        ))
        .set_parent(node("Skylander"));
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn magic_moment_system(
    mut commands: Commands,
    (time, mm, asset_server, registry): (
        Res<Time>,
        Option<Res<MagicMoment>>,
        Option<Res<CharacterAssetServer>>,
        Option<Res<CharacterVisualRegistry>>,
    ),
    (mut assets, mut meshes, mut images, mut materials): (
        ResMut<MagicMomentAssets>,
        ResMut<Assets<Mesh>>,
        ResMut<Assets<Image>>,
        ResMut<Assets<StandardMaterial>>,
    ),
    scenes: Query<(Entity, &MagicMomentScene)>,
    (mut joints, mut joint_parts): (
        Query<(&MmJoint, &mut Transform), Without<MmJointPart>>,
        Query<(&MmJointPart, &mut Transform), (Without<MmJoint>, Without<MagicMomentCamera>)>,
    ),
    mut cameras: Query<
        (&mut Camera, &mut Transform, &mut Projection),
        (
            With<MagicMomentCamera>,
            Without<MmJoint>,
            Without<MmJointPart>,
        ),
    >,
    (mut main_cam, mut world_cams): (
        Query<&mut Projection, (With<MainCamera>, Without<MagicMomentCamera>)>,
        Query<
            &mut Camera,
            (
                Or<(With<MainCamera>, With<SkyCamera>)>,
                Without<MagicMomentCamera>,
            ),
        >,
    ),
    mut pieces: Query<(
        &MmPiece,
        &MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
        Option<&MmScroll>,
    )>,
    mut heroes: Query<
        (
            Entity,
            &ActorIdentity,
            &mut MagicMomentPose,
            &mut Visibility,
        ),
        (With<MagicMomentModel>, Without<MmPiece>),
    >,
    children: Query<&Children>,
    mut hero_materials: Query<
        (
            Entity,
            &mut MeshMaterial3d<StandardMaterial>,
            Option<&MmOwnMaterial>,
        ),
        (Without<MmPiece>, Without<MagicMomentModel>),
    >,
    mut pull_out_base: Local<Option<f32>>,
) {
    let active = mm.as_ref().filter(|m| m.is_active());

    for (entity, scene) in &scenes {
        if active.is_none_or(|m| m.serial != scene.serial) {
            commands.entity(entity).despawn_recursive();
        }
    }

    // World camera pull-out before portal space appears, then restored.
    if let Ok(Projection::Perspective(p)) = main_cam.get_single_mut().as_deref_mut() {
        let base = *pull_out_base.get_or_insert(p.fov);
        p.fov = match active {
            Some(m) if m.elapsed < MM_PORTAL_SPACE => {
                let k = smooth(m.elapsed / MM_PORTAL_SPACE);
                base + (PULL_OUT_FOV_ADD * k * k).to_radians()
            }
            _ => base,
        };
    }

    let portal_space = active.filter(|m| m.elapsed >= MM_PORTAL_SPACE);
    for (mut cam, ..) in &mut cameras {
        cam.is_active = portal_space.is_some();
    }
    // The world (and the HUD on its camera) is hidden in portal space ("UI - Adjust UI
    // Visibility": gameplay and player HUD containers off).
    for mut cam in &mut world_cams {
        cam.is_active = portal_space.is_none();
    }
    let Some(m) = active else {
        return;
    };
    let Some(char_id) = m.character_id else {
        return;
    };
    if !assets.loaded {
        assets.load(asset_server.as_deref(), &mut meshes, &mut images);
    }
    if !scenes.iter().any(|(_, s)| s.serial == m.serial) {
        spawn_scene(&mut commands, &assets, &mut materials, m, char_id);
        return;
    }

    // Rig joints (model space under the scene root).
    let mut camera_global = None;
    if let Some(rig) = &assets.rig {
        let (state, t) = rig_pose(m, |s| assets.rig_duration(s));
        if let Some(clip) = rig.clips.get(&state) {
            let globals = clip.sample_global_transforms(t, &rig.skeleton);
            for (joint, mut tf) in &mut joints {
                if let Some(g) = globals.get(joint.0) {
                    *tf = Transform::from_matrix(Mat4::from_cols_array(g));
                }
            }
            camera_global = rig
                .nodes
                .get("camera")
                .and_then(|&b| globals.get(b))
                .map(Mat4::from_cols_array);
        }
    }
    for (_, mut tf, mut proj) in &mut cameras {
        *tf = match camera_global {
            // `J_camera` looks down its +Z; Bevy cameras look down -Z.
            Some(g) => Transform::from_matrix(
                Mat4::from_translation(MM_ORIGIN) * g * Mat4::from_rotation_y(std::f32::consts::PI),
            ),
            None => Transform::from_translation(MM_ORIGIN + Vec3::new(0.0, 1.0, 6.0))
                .looking_at(MM_ORIGIN + Vec3::Y, Vec3::Y),
        };
        if let Projection::Perspective(p) = proj.as_mut() {
            p.fov = vertical_fov(portal_fov(m).to_radians(), p.aspect_ratio);
        }
    }

    // Articulated background parts loop their model's clip from the moment they appear.
    let mut part_globals: HashMap<&str, Vec<[f32; 16]>> = HashMap::default();
    for (part, mut tf) in &mut joint_parts {
        let Some(rig) = assets.part_rigs.get(&part.rig) else {
            continue;
        };
        let globals = part_globals.entry(part.rig.as_str()).or_insert_with(|| {
            let Some(clip) = &rig.clip else {
                return Vec::new();
            };
            let t = (m.elapsed - MM_BACKGROUND).max(0.0) % clip.duration.max(1e-3);
            clip.sample_global_transforms(t, &rig.skeleton)
        });
        let (Some(g), Some(bone)) = (globals.get(part.joint), rig.skeleton.bones.get(part.joint))
        else {
            continue;
        };
        *tf = Transform::from_matrix(
            Mat4::from_cols_array(g) * Mat4::from_cols_array(&bone.inverse_bind_pose),
        );
    }

    // Pieces: fades, tints, texture scroll.
    let (_, element) = get_character_info(char_id);
    let element_color = assets.element_color(element);
    let now = time.elapsed_secs();
    for (piece, mat, mut vis, scroll) in &mut pieces {
        let (visible, alpha, tint) = piece_state(*piece, m, element_color);
        *vis = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let Some(material) = materials.get_mut(&mat.0) else {
            continue;
        };
        let base = tint.unwrap_or(material.base_color);
        material.base_color = base.with_alpha(alpha);
        if let Some(MmScroll(speed)) = scroll {
            let offset = (*speed * now).fract_gl();
            material.uv_transform = Affine2::from_translation(offset);
        }
    }

    // The copied Skylander: pose and replace colour.
    let (state, pose_time) = hero_pose(m);
    let (grey, percentage) = replace_color(m);
    for (hero, ident, mut pose, mut vis) in &mut heroes {
        *vis = if m.elapsed >= MM_REVEAL {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let clip = registry
            .as_ref()
            .and_then(|r| r.get_model(ident.character_id, ident.model_path.as_deref()))
            .and_then(|model| model.animation_for_state(state).map(|c| c.name.clone()))
            .unwrap_or_else(|| if state == HERO_TOY { "toy" } else { "toyalive" }.to_string());
        pose.clip = clip;
        pose.time = pose_time;

        for e in children.iter_descendants(hero) {
            let Ok((e, mut mat, own)) = hero_materials.get_mut(e) else {
                continue;
            };
            let base = match own {
                Some(own) => own.base,
                None => {
                    // Own copy of the shared character material.
                    let Some(src) = materials.get(&mat.0).cloned() else {
                        continue;
                    };
                    let base = src.base_color;
                    mat.0 = materials.add(src);
                    commands.entity(e).insert(MmOwnMaterial { base });
                    base
                }
            };
            if let Some(material) = materials.get_mut(&mat.0) {
                let keep = 1.0 - percentage;
                let b = base.to_linear();
                material.base_color =
                    Color::linear_rgba(b.red * keep, b.green * keep, b.blue * keep, b.alpha);
                material.emissive = LinearRgba::rgb(grey, grey, grey) * percentage;
            }
        }
    }
}

/// `RenderLayers` is not inherited: put everything under a Magic Moment scene (the copied
/// Skylander's submeshes, joints and attachments too) on the portal-space layer.
pub fn magic_moment_layer_system(
    mut commands: Commands,
    scenes: Query<Entity, With<MagicMomentScene>>,
    children: Query<&Children>,
    unlayered: Query<(), Without<RenderLayers>>,
) {
    for root in &scenes {
        for e in children.iter_descendants(root) {
            if unlayered.contains(e) {
                commands.entity(e).insert(RenderLayers::layer(MM_LAYER));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skylands_core::MM_HYPER_JUMP;

    fn mm_at(t: f32) -> MagicMoment {
        let mut mm = MagicMoment::default();
        mm.start(16, "Spyro", Some(3.33));
        mm.elapsed = t;
        mm
    }

    #[test]
    fn test_abgr_color_measurement() {
        // 0xFFDD822C: alpha FF, blue DD, green 82, red 2C.
        let c = abgr(CLOUD_TUBE_TINT).to_srgba();
        assert!((c.red - 0x2c as f32 / 255.0).abs() < 1e-3);
        assert!((c.green - 0x82 as f32 / 255.0).abs() < 1e-3);
        assert!((c.blue - 0xdd as f32 / 255.0).abs() < 1e-3);
        assert_eq!(c.alpha, 1.0);
    }

    #[test]
    fn test_rig_pose_follows_modes() {
        let d = |s| match s {
            RIG_NEW_TOY => 1.17,
            RIG_MAGIC_MOMENT => 1.9,
            RIG_HYPER_JUMP => 1.43,
            _ => 1.0,
        };
        assert_eq!(rig_pose(&mm_at(0.6), d).0, RIG_NEW_TOY);
        assert_eq!(
            rig_pose(&mm_at(MM_PORTAL_SPACE + 1.2), d).0,
            RIG_NEW_TOY_LOOP
        );
        assert_eq!(rig_pose(&mm_at(MM_REVEAL + 0.5), d).0, RIG_MAGIC_MOMENT);
        assert_eq!(
            rig_pose(&mm_at(MM_REVEAL + 2.0), d).0,
            RIG_MAGIC_MOMENT_LOOP
        );
        let mm = mm_at(0.0);
        let (s, t) = rig_pose(&mm_at(mm.to_world() + 0.2), d);
        assert_eq!(s, RIG_HYPER_JUMP);
        assert!((t - 0.2).abs() < 1e-4);
    }

    #[test]
    fn test_reveal_silhouette_then_alive() {
        // Toy pose, black silhouette at the reveal.
        assert_eq!(hero_pose(&mm_at(MM_REVEAL)), (HERO_TOY_ALIVE, 0.0));
        assert_eq!(replace_color(&mm_at(MM_REVEAL)), (0.0, 1.0));
        // White by the end of the 0.8 s slide, still frozen.
        assert_eq!(replace_color(&mm_at(MM_REVEAL + 0.81)), (1.0, 1.0));
        assert_eq!(hero_pose(&mm_at(MM_ALIVE - 0.01)).1, 0.0);
        // Flash gone 0.12 s after stage 5; ToyAlive runs.
        assert_eq!(replace_color(&mm_at(MM_ALIVE + 0.13)), (1.0, 0.0));
        let (s, t) = hero_pose(&mm_at(MM_ALIVE + 1.0));
        assert_eq!(s, HERO_TOY_ALIVE);
        assert!((t - 1.0).abs() < 1e-4);
        let mm = mm_at(0.0);
        assert_eq!(hero_pose(&mm_at(mm.to_world() + 0.1)).0, HERO_TOY);
    }

    #[test]
    fn test_fov_and_piece_fades() {
        assert_eq!(portal_fov(&mm_at(MM_PORTAL_SPACE)), FOV_WIDE);
        assert_eq!(portal_fov(&mm_at(MM_PORTAL_SPACE + 0.5)), FOV_MM);
        let mm = mm_at(0.0);
        assert!((portal_fov(&mm_at(mm.to_world() + MM_HYPER_JUMP)) - FOV_WIDE).abs() < 1e-3);

        let el = Color::srgb(1.0, 0.0, 0.0);
        assert!(!piece_state(MmPiece::Background, &mm_at(MM_BACKGROUND), el).0);
        assert_eq!(
            piece_state(MmPiece::Background, &mm_at(MM_BACKGROUND + 0.31), el).1,
            1.0
        );
        assert!(piece_state(MmPiece::NeutralIcon, &mm_at(1.0), el).0);
        assert!(!piece_state(MmPiece::NeutralIcon, &mm_at(MM_SHOW_MINION), el).0);
        assert!(piece_state(MmPiece::ElementIcon, &mm_at(MM_SHOW_MINION), el).0);
        assert!(!piece_state(MmPiece::Name, &mm_at(MM_REVEAL - 0.1), el).0);
        assert!(piece_state(MmPiece::Name, &mm_at(MM_REVEAL), el).0);
        let tube = piece_state(MmPiece::CloudTube, &mm_at(MM_TUNNEL_TINT + 0.4), el)
            .2
            .unwrap();
        assert!((tube.to_srgba().red - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_letter_for_model() {
        assert_eq!(letter_for_model("letter_A.mdl"), Some('A'));
        assert_eq!(letter_for_model("number_0.mdl"), Some('0'));
        assert_eq!(letter_for_model("symbol_-.mdl"), Some('-'));
        assert_eq!(letter_for_model("letter_s_(04)_Colon"), None);
    }

    #[test]
    fn test_scene_spawns_and_clears_without_assets() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<MagicMoment>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Startup, setup_magic_moment)
            .add_systems(
                Update,
                (magic_moment_system, magic_moment_layer_system).chain(),
            );
        app.update();
        app.world_mut()
            .resource_mut::<MagicMoment>()
            .start(16, "Spyro", None);
        app.world_mut().resource_mut::<MagicMoment>().elapsed = MM_REVEAL;
        app.update();
        app.update();
        let heroes: Vec<u16> = app
            .world_mut()
            .query_filtered::<&ActorIdentity, With<MagicMomentModel>>()
            .iter(app.world())
            .map(|i| i.character_id)
            .collect();
        assert_eq!(heroes, vec![16]);
        let cam_active = app
            .world_mut()
            .query_filtered::<&Camera, With<MagicMomentCamera>>()
            .single(app.world())
            .is_active;
        assert!(cam_active);
        let hero = app
            .world_mut()
            .query_filtered::<Entity, With<MagicMomentModel>>()
            .single(app.world());
        assert_eq!(
            app.world().get::<RenderLayers>(hero),
            Some(&RenderLayers::layer(MM_LAYER))
        );

        let end = app.world().resource::<MagicMoment>().duration();
        app.world_mut().resource_mut::<MagicMoment>().elapsed = end;
        app.update();
        app.update();
        let scenes = app
            .world_mut()
            .query::<&MagicMomentScene>()
            .iter(app.world())
            .count();
        assert_eq!(scenes, 0);
    }
}
