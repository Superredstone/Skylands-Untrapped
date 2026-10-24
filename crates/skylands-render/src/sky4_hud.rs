//! Original Trap Team player HUD, rebuilt from `permanent/global.bld`.
//!
//! Sprites, sizes and colours come from the disc (`SpriteLibrary`); the layout is the one built by
//! `Macros_PlayerHUD.ai` "UI - HUD - Player HUD - Create" / "UI - HUD - Minion HUD - Create" and
//! `Macros_UI_Main.ai` "UI - Health Bar - Create" for player 1. Text uses the disc's bitmap font
//! (`igBitmapFont`), the portrait comes from the hero's own `level.bld`. Notes: `agent/HUD.md`.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use skylands_core::{ActorIdentity, Element, GameStats, Health, Hero};
use skylands_formats::{BitmapFont, SpriteDef, SpriteLibrary};

use crate::texture_conversion::rgba8_to_bevy_image;
use crate::visual_sync::CharacterAssetServer;

/// Script screen space is 480 units tall; width follows the window aspect.
pub const SCREEN_HEIGHT: f32 = 480.0;
/// Title-safe inset around the HUD corner (assumed; not found in the scripts yet).
pub const SAFE_INSET: Vec2 = Vec2::new(16.0, 16.0);
/// `"UI Constants"."hud - player health bar width"`; the bar art is authored at 177.
pub const HEALTH_BAR_WIDTH: f32 = 177.0;
/// Stands in for the cutout masks that keep a bar fill inside its trim.
pub const BAR_FILL_INSET: f32 = 2.0;
/// `"input minion hud data"."xp bar max width"`.
pub const XP_BAR_WIDTH: f32 = 70.0;
/// `"UI Constants"."font point size - small"` / `"- smaller"` (assumed).
pub const POINT_SIZE_SMALL: f32 = 16.0;
pub const POINT_SIZE_SMALLER: f32 = 13.0;

/// `SpriteAlignment` (DOL string table): rows top/mid/bottom, columns left/center/right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    TopLeft,
    TopCenter,
    TopRight,
    MidLeft,
    #[default]
    MidCenter,
    MidRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Align {
    /// Anchor point as a fraction of the size (y down).
    pub fn factor(self) -> Vec2 {
        let i = self as u8;
        Vec2::new((i % 3) as f32 * 0.5, (i / 3) as f32 * 0.5)
    }
}

/// `ColorMeasurement`: `0xAABBGGRR` (bytes A, B, G, R).
pub fn script_color(abgr: u32) -> Color {
    let ch = |s: u32| ((abgr >> s) & 0xff) as f32 / 255.0;
    Color::srgba(ch(0), ch(8), ch(16), ch(24))
}

pub const COLOR_WHITE: u32 = 0xffff_ffff;
pub const COLOR_TRIM_SILVER: u32 = 0xff80_8080;
/// `"UI Constants"."trim gold"`; its initial value was not found (assumed gold).
pub const COLOR_TRIM_GOLD: u32 = 0xff40_c8ff;
/// "UI - Health Bar - Update" thresholds: below 25% / 40% / otherwise.
pub const COLOR_HEALTH_LOW: u32 = 0x7800_00aa;
pub const COLOR_HEALTH_MID: u32 = 0x7800_aaaa;
pub const COLOR_HEALTH_FULL: u32 = 0x7800_aa00;
pub const COLOR_XP_FILL: u32 = 0xc8ff_aa55;

pub fn health_bar_color(current: u32, max: u32) -> u32 {
    let pct = current as f32 / max.max(1) as f32;
    if pct < 0.25 {
        COLOR_HEALTH_LOW
    } else if pct < 0.4 {
        COLOR_HEALTH_MID
    } else {
        COLOR_HEALTH_FULL
    }
}

/// One engine sprite: placed `location` away from the parent's `align_to_parent` anchor, by its
/// own `alignment` anchor. Priority adds up along the parent chain; lower draws in front.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct HudSprite {
    pub parent: Option<Entity>,
    pub size: Vec2,
    pub alignment: Align,
    pub align_to_parent: Align,
    pub location: Vec2,
    pub priority: i32,
}

impl HudSprite {
    pub fn new(parent: Option<Entity>, size: Vec2) -> Self {
        Self {
            parent,
            size,
            alignment: Align::MidCenter,
            align_to_parent: Align::MidCenter,
            location: Vec2::ZERO,
            priority: 0,
        }
    }
    fn align(mut self, own: Align, parent: Align) -> Self {
        self.alignment = own;
        self.align_to_parent = parent;
        self
    }
    fn at(mut self, x: f32, y: f32) -> Self {
        self.location = Vec2::new(x, y);
        self
    }
    fn prio(mut self, p: i32) -> Self {
        self.priority = p;
        self
    }

    /// Top-left corner inside a parent rectangle.
    pub fn place(&self, parent_min: Vec2, parent_size: Vec2) -> Vec2 {
        parent_min + parent_size * self.align_to_parent.factor() + self.location
            - self.size * self.alignment.factor()
    }
}

/// Text drawn by a sprite (`point size` in script units) with the bitmap font.
#[derive(Component, Debug, Clone, Copy)]
pub struct HudText {
    pub point_size: f32,
    pub justify: JustifyText,
    pub color: Color,
}

/// The string a `HudText` shows.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct HudString(pub String);

/// Glyph sprites currently spawned for a `HudText`.
#[derive(Component, Debug, Default)]
pub struct HudGlyphs(pub Vec<Entity>);

#[derive(Component)]
pub struct HudGlyph;

#[derive(Component)]
pub struct Sky4HudRoot;
#[derive(Component)]
pub struct HudHealthFill;
#[derive(Component, Clone, Copy)]
pub struct HudHpText;
#[derive(Component, Clone, Copy)]
pub struct HudNameText;
#[derive(Component, Clone, Copy)]
pub struct HudCoinText;
#[derive(Component, Clone, Copy)]
pub struct HudLevelText;
#[derive(Component)]
pub struct HudElementIcon;
/// Hero portrait (`<id>_MinionHead` sprite of the hero's `level.bld`).
#[derive(Component)]
pub struct HudPortrait;
/// Black outline copy of a text sprite ("UI - Create Stroke").
#[derive(Component)]
pub struct HudStroke;

/// Element badge textures (`sky4 hud element NN`) and the HUD font.
#[derive(Resource, Default)]
pub struct Sky4HudAssets {
    pub element_icons: Vec<Handle<Image>>,
    pub font: Option<HudFont>,
}

pub struct HudFont {
    pub font: BitmapFont,
    pub image: Handle<Image>,
    pub sheet_size: Vec2,
}

impl HudFont {
    /// Glyph rectangles (script units, relative to the text sprite's top-left) and their sheet
    /// rectangles (pixels) for `text` laid out in `box_size`.
    pub fn layout(
        &self,
        text: &str,
        point_size: f32,
        justify: JustifyText,
        box_size: Vec2,
    ) -> Vec<(Vec2, Vec2, Rect)> {
        let k = point_size / self.font.point_size.max(1.0);
        let width = self.font.measure(text) * k;
        let mut x = match justify {
            JustifyText::Center => (box_size.x - width) * 0.5,
            JustifyText::Right => box_size.x - width,
            _ => 0.0,
        };
        // The font's em box is centred in the sprite.
        let top = (box_size.y - point_size) * 0.5;
        let mut out = Vec::new();
        for c in text.chars() {
            let Some(g) = self.font.glyphs.get(&c) else {
                continue;
            };
            if g.size[0] > 0.0 && g.size[1] > 0.0 {
                out.push((
                    Vec2::new(x + g.offset[0] * k, top + g.offset[1] * k),
                    Vec2::from(g.size) * k,
                    Rect::from_corners(
                        Vec2::from(g.uv_min) * self.sheet_size,
                        Vec2::from(g.uv_max) * self.sheet_size,
                    ),
                ));
            }
            x += g.advance * k;
        }
        out
    }
}

const ELEMENT_SPRITES: [&str; 11] = [
    "sky4 hud element 00 magic",
    "sky4 hud element 01 water",
    "sky4 hud element 02 air",
    "sky4 hud element 03 death",
    "sky4 hud element 04 tech",
    "sky4 hud element 05 fire",
    "sky4 hud element 06 earth",
    "sky4 hud element 07 life",
    "sky4 hud element 08 dark",
    "sky4 hud element 09 light",
    "sky4 hud element 10 kaos",
];

pub fn element_icon_index(element: Element) -> usize {
    match element {
        Element::Magic => 0,
        Element::Water => 1,
        Element::Air => 2,
        Element::Undead => 3,
        Element::Tech => 4,
        Element::Fire => 5,
        Element::Earth => 6,
        Element::Life => 7,
        Element::Kaos => 10,
    }
}

struct Sprites<'a> {
    lib: &'a SpriteLibrary,
    data: &'a [u8],
}

impl Sprites<'_> {
    fn image(&self, name: &str, images: &mut Assets<Image>) -> Option<(Handle<Image>, Vec2)> {
        let def = self.lib.get(name)?;
        let (w, h, px) = def.decode_rgba(self.data)?;
        Some((
            images.add(rgba8_to_bevy_image(w, h, px)),
            Vec2::from(def.natural_size),
        ))
    }
}

pub fn setup_sky4_hud(
    mut commands: Commands,
    asset_server: Option<Res<CharacterAssetServer>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(server) = asset_server else { return };
    let Ok(data) = server
        .loader
        .lock()
        .unwrap()
        .vfs()
        .read_from_archive("global.bld", "level.bld")
    else {
        warn!("global.bld not mounted; original HUD disabled");
        return;
    };
    let Some(lib) = SpriteLibrary::parse(&data) else {
        warn!("No sprites in global.bld; original HUD disabled");
        return;
    };
    let sprites = Sprites {
        lib: &lib,
        data: &data,
    };

    let element_icons = ELEMENT_SPRITES
        .iter()
        .filter_map(|n| sprites.image(n, &mut images).map(|(h, _)| h))
        .collect::<Vec<_>>();
    let font = BitmapFont::parse(&data).and_then(|font| {
        let (w, h, px) = font.decode_rgba(&data)?;
        Some(HudFont {
            font,
            image: images.add(rgba8_to_bevy_image(w, h, px)),
            sheet_size: Vec2::new(w as f32, h as f32),
        })
    });
    if font.is_none() {
        warn!("No bitmap font in global.bld; HUD text disabled");
    }

    let root = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            Sky4HudRoot,
        ))
        .id();

    let mut hud = HudBuilder {
        commands: &mut commands,
        root,
    };

    // "UI - HUD - Player HUD - Create": player 1 corner container.
    let main = hud.container(
        HudSprite::new(None, Vec2::new(237.0, 96.0))
            .align(Align::BottomLeft, Align::BottomLeft)
            .prio(-25),
    );
    // "Minion HUD - Create": portrait container (A) and bar container (B).
    let a = hud.container(HudSprite::new(Some(main), Vec2::new(237.0, 96.0)).at(0.0, 30.0));
    let b = hud.container(HudSprite::new(Some(main), Vec2::new(237.0, 96.0)));

    // Notification-style backdrop plate behind the lower row.
    let plate = hud.container(
        HudSprite::new(Some(a), Vec2::splat(3.555))
            .at(-128.0, 2.0)
            .prio(200),
    );
    if let Some((img, size)) = sprites.image("box_left", &mut images) {
        let left = hud.image(
            HudSprite::new(Some(plate), size).align(Align::MidLeft, Align::MidRight),
            img,
            0xc871_3720,
        );
        if let Some((img, size)) = sprites.image("dialog_back_edge", &mut images) {
            hud.image(
                HudSprite::new(Some(left), Vec2::new(200.0, size.y))
                    .align(Align::MidLeft, Align::MidRight),
                img,
                0xff71_3720,
            );
        }
    }
    let base_line = sprites
        .image("sky4 hud portrait base", &mut images)
        .map(|(img, size)| {
            hud.image(
                HudSprite::new(Some(a), size).at(0.0, -18.0).prio(-1),
                img,
                0xff98_987a,
            )
        });
    // Portrait: bottom-left on the base line's top-left, priority 50. Image set per hero.
    let portrait_size = lib
        .get("ui_hud_portrait_default")
        .map_or(Vec2::new(142.22, 71.11), |d| Vec2::from(d.natural_size));
    let portrait = hud.image(
        HudSprite::new(Some(base_line.unwrap_or(a)), portrait_size)
            .align(Align::BottomLeft, Align::TopLeft)
            .prio(50),
        Handle::default(),
        COLOR_WHITE,
    );
    hud.commands
        .entity(portrait)
        .insert((HudPortrait, Visibility::Hidden));

    // Element badge ("sky4 hud element base" + "Elemental Icon - Create").
    if let Some((img, size)) = sprites.image("sky4 hud element base", &mut images) {
        hud.image(
            HudSprite::new(Some(b), size).at(-105.0, 2.0).prio(-1),
            img,
            COLOR_TRIM_SILVER,
        );
    }
    if let Some(img) = element_icons.first().cloned() {
        let e = hud.image(
            HudSprite::new(Some(b), Vec2::splat(36.0))
                .at(-105.0, 2.0)
                .prio(-3),
            img,
            COLOR_WHITE,
        );
        hud.commands.entity(e).insert(HudElementIcon);
    }

    // Name text.
    hud.text(
        HudSprite::new(Some(b), Vec2::new(175.0, POINT_SIZE_SMALL))
            .align(Align::MidLeft, Align::MidLeft)
            .at(32.0, -27.0)
            .prio(-100),
        POINT_SIZE_SMALL,
        JustifyText::Left,
        COLOR_WHITE,
        HudNameText,
    );

    // "UI - Health Bar - Create".
    let bar_h = lib
        .get("sky4 hud health bar center")
        .map_or(18.67, |d| d.natural_size[1]);
    let health = hud.container(
        HudSprite::new(Some(b), Vec2::new(HEALTH_BAR_WIDTH, bar_h))
            .align(Align::MidLeft, Align::MidLeft)
            .at(30.0, -6.0),
    );
    let scale = HEALTH_BAR_WIDTH / 177.0;
    if let Some((img, size)) = sprites.image("sky4 hud health bar base", &mut images) {
        hud.image(
            HudSprite::new(Some(health), Vec2::new(size.x * scale, size.y)).prio(-2),
            img,
            0x9628_2828,
        );
    }
    if let Some((img, size)) = sprites.image("sky4 hud health bar trim", &mut images) {
        hud.image(
            HudSprite::new(Some(health), Vec2::new(size.x * scale, size.y)).prio(-4),
            img,
            0xffff_ffff,
        );
    }
    if let Some(fill) = hud.bar(
        &sprites,
        &mut images,
        health,
        "sky4 hud health bar",
        HEALTH_BAR_WIDTH - 2.0 * BAR_FILL_INSET,
        COLOR_HEALTH_FULL,
        -5,
    ) {
        hud.commands.entity(fill).insert(HudHealthFill);
    }
    hud.text(
        HudSprite::new(Some(health), Vec2::new(70.0, POINT_SIZE_SMALLER))
            .align(Align::MidLeft, Align::MidLeft)
            .at(6.0, 1.0)
            .prio(-12),
        POINT_SIZE_SMALLER,
        JustifyText::Left,
        0xffff_ffff,
        HudHpText,
    );

    // "Minion HUD - XP Create".
    let xp = hud.container(
        HudSprite::new(Some(b), Vec2::splat(3.555))
            .at(-43.0, 13.0)
            .prio(-150),
    );
    if let Some((img, size)) = sprites.image("sky4 hud xp bar base", &mut images) {
        hud.image(
            HudSprite::new(Some(xp), size).at(11.0, 0.0).prio(-5),
            img,
            0x9628_2828,
        );
    }
    if let Some((img, size)) = sprites.image("sky4 hud xp bar trim", &mut images) {
        hud.image(
            HudSprite::new(Some(xp), size).prio(-7),
            img,
            COLOR_TRIM_SILVER,
        );
    }
    let xp_bar = hud.container(
        HudSprite::new(Some(xp), Vec2::new(XP_BAR_WIDTH, bar_h))
            .align(Align::MidLeft, Align::MidLeft)
            .at(-25.0, 0.0)
            .prio(-15),
    );
    // No XP data in the core yet: empty bar.
    hud.bar(
        &sprites,
        &mut images,
        xp_bar,
        "sky4 hud xp bar",
        0.0,
        COLOR_XP_FILL,
        0,
    );
    hud.text(
        HudSprite::new(Some(xp), Vec2::new(100.0, POINT_SIZE_SMALLER))
            .at(-33.0, 1.0)
            .prio(-25),
        POINT_SIZE_SMALLER,
        JustifyText::Center,
        0xfff2_e714,
        HudLevelText,
    );

    // "Minion HUD - Money - Create".
    let money = hud.container(
        HudSprite::new(Some(b), Vec2::splat(3.555))
            .align(Align::MidCenter, Align::MidRight)
            .at(-90.0, 14.0)
            .prio(-10),
    );
    hud.text(
        HudSprite::new(Some(money), Vec2::new(50.0, POINT_SIZE_SMALLER))
            .align(Align::MidLeft, Align::MidLeft)
            .prio(-10),
        POINT_SIZE_SMALLER,
        JustifyText::Left,
        0xff00_aaff,
        HudCoinText,
    );
    if let Some((img, size)) = sprites.image("sky4 hud coin base", &mut images) {
        hud.image(
            HudSprite::new(Some(money), size)
                .align(Align::MidCenter, Align::MidRight)
                .at(-size.x * 0.5 - 2.0, 0.0)
                .prio(-1),
            img,
            0x6400_0000,
        );
    }
    if let Some((img, size)) = sprites.image("sky4 hud coin", &mut images) {
        hud.image(
            HudSprite::new(Some(money), size)
                .align(Align::MidCenter, Align::MidRight)
                .at(-size.x * 0.5 - 2.0, 0.0)
                .prio(-2),
            img,
            COLOR_TRIM_GOLD,
        );
    }

    commands.insert_resource(Sky4HudAssets {
        element_icons,
        font,
    });
}

struct HudBuilder<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    root: Entity,
}

impl HudBuilder<'_, '_, '_> {
    fn spawn(&mut self, bundle: impl Bundle) -> Entity {
        let e = self.commands.spawn(bundle).id();
        self.commands.entity(self.root).add_child(e);
        e
    }

    fn container(&mut self, sprite: HudSprite) -> Entity {
        self.spawn((absolute_node(), sprite))
    }

    fn image(&mut self, sprite: HudSprite, image: Handle<Image>, color: u32) -> Entity {
        self.spawn((
            absolute_node(),
            ImageNode {
                image,
                color: script_color(color),
                ..default()
            },
            sprite,
        ))
    }

    fn text(
        &mut self,
        sprite: HudSprite,
        point_size: f32,
        justify: JustifyText,
        color: u32,
        marker: impl Component + Copy,
    ) {
        // "UI - Create Stroke": four black copies offset by one unit, behind the text.
        for (dx, dy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            let mut s = sprite.clone();
            s.location += Vec2::new(dx, dy);
            s.priority += 1;
            self.spawn((
                absolute_node(),
                HudText {
                    point_size,
                    justify,
                    color: Color::BLACK,
                },
                HudString::default(),
                HudGlyphs::default(),
                s,
                marker,
                HudStroke,
            ));
        }
        self.spawn((
            absolute_node(),
            HudText {
                point_size,
                justify,
                color: script_color(color),
            },
            HudString::default(),
            HudGlyphs::default(),
            sprite,
            marker,
        ));
    }

    /// "UI - Bar2 - Create": a `<prefix> center` fill stretched to `width`. Its caps are
    /// zero-width cutout masks and the fill uses the receive-cutout blend, so it is drawn
    /// opaque here.
    #[allow(clippy::too_many_arguments)]
    fn bar(
        &mut self,
        sprites: &Sprites,
        images: &mut Assets<Image>,
        parent: Entity,
        prefix: &str,
        width: f32,
        color: u32,
        prio: i32,
    ) -> Option<Entity> {
        let (center, size) = sprites.image(&format!("{prefix} center"), images)?;
        Some(
            self.image(
                HudSprite::new(
                    Some(parent),
                    Vec2::new(width, size.y - 2.0 * BAR_FILL_INSET),
                )
                .align(Align::MidLeft, Align::MidLeft)
                .at(BAR_FILL_INSET, 0.0)
                .prio(prio),
                center,
                color | 0xff00_0000,
            ),
        )
    }
}

fn absolute_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        ..default()
    }
}

/// Writes data into the HUD sprites.
#[allow(clippy::type_complexity)]
pub fn sky4_hud_update_system(
    stats: Res<GameStats>,
    assets: Option<Res<Sky4HudAssets>>,
    hero_query: Query<(&Health, &ActorIdentity), With<Hero>>,
    mut fill: Query<(&mut HudSprite, &mut ImageNode), With<HudHealthFill>>,
    mut icons: Query<&mut ImageNode, (With<HudElementIcon>, Without<HudHealthFill>)>,
    mut texts: Query<(
        &mut HudString,
        Option<&HudHpText>,
        Option<&HudNameText>,
        Option<&HudCoinText>,
        Option<&HudLevelText>,
    )>,
) {
    let hero = hero_query.get_single().ok();

    for (mut text, hp, name, coin, level) in &mut texts {
        let value = if hp.is_some() {
            hero.map(|(h, _)| h.current.to_string())
        } else if name.is_some() {
            hero.map(|(_, id)| id.name.clone())
        } else if coin.is_some() {
            Some(stats.coins.to_string())
        } else if level.is_some() {
            Some("1".to_string())
        } else {
            None
        };
        if let Some(v) = value {
            if text.0 != v {
                text.0 = v;
            }
        }
    }

    let Some((health, ident)) = hero else { return };
    // "UI - Health Bar - Update": width follows hp / hp max, colour by threshold.
    let color = script_color(health_bar_color(health.current, health.max) | 0xff00_0000);
    let pct = (health.current as f32 / health.max.max(1) as f32).clamp(0.0, 1.0);
    for (mut sprite, mut image) in &mut fill {
        sprite.size.x = (HEALTH_BAR_WIDTH - 2.0 * BAR_FILL_INSET) * pct;
        image.color = color;
    }

    if let Some(assets) = assets {
        let (_, element) = skylands_core::get_character_info(ident.character_id);
        if let Some(handle) = assets.element_icons.get(element_icon_index(element)) {
            for mut icon in &mut icons {
                if icon.image != *handle {
                    icon.image = handle.clone();
                }
            }
        }
    }
}

/// Resolves sprite rectangles (script units) into UI pixels for the current window.
pub fn sky4_hud_layout_system(
    windows: Query<&Window, With<PrimaryWindow>>,
    sprites: Query<(Entity, &HudSprite)>,
    mut nodes: Query<(&mut Node, &mut ZIndex)>,
) {
    let (w, h) = windows
        .get_single()
        .map_or((640.0, SCREEN_HEIGHT), |w| (w.width(), w.height()));
    let scale = h / SCREEN_HEIGHT;
    let screen = Vec2::new(w / scale, SCREEN_HEIGHT);

    for (entity, sprite) in &sprites {
        let Some((min, prio)) = resolve(entity, &sprites, screen, 0) else {
            continue;
        };
        let Ok((mut node, mut z)) = nodes.get_mut(entity) else {
            continue;
        };
        let px = |v: f32| Val::Px((v * scale).round());
        node.left = px(min.x);
        node.top = px(min.y);
        node.width = px(sprite.size.x);
        node.height = px(sprite.size.y);
        *z = ZIndex(-prio);
    }
}

/// Rebuilds the glyph sprites of texts whose string changed.
pub fn sky4_hud_text_system(
    mut commands: Commands,
    assets: Option<Res<Sky4HudAssets>>,
    root: Query<Entity, With<Sky4HudRoot>>,
    mut texts: Query<
        (Entity, &HudText, &HudSprite, &HudString, &mut HudGlyphs),
        Changed<HudString>,
    >,
) {
    let (Some(font), Ok(root)) = (
        assets.as_ref().and_then(|a| a.font.as_ref()),
        root.get_single(),
    ) else {
        return;
    };
    for (entity, text, sprite, string, mut glyphs) in &mut texts {
        for old in glyphs.0.drain(..) {
            commands.entity(old).despawn_recursive();
        }
        for (pos, size, rect) in font.layout(&string.0, text.point_size, text.justify, sprite.size)
        {
            let glyph = commands
                .spawn((
                    absolute_node(),
                    ImageNode {
                        image: font.image.clone(),
                        color: text.color,
                        rect: Some(rect),
                        ..default()
                    },
                    HudSprite::new(Some(entity), size)
                        .align(Align::TopLeft, Align::TopLeft)
                        .at(pos.x, pos.y),
                    HudGlyph,
                ))
                .id();
            commands.entity(root).add_child(glyph);
            glyphs.0.push(glyph);
        }
    }
}

/// Loads the hero's portrait (`<id>_MinionHead` in its `level.bld`) when the hero changes.
pub fn sky4_hud_portrait_system(
    asset_server: Option<Res<CharacterAssetServer>>,
    mut images: ResMut<Assets<Image>>,
    hero: Query<&ActorIdentity, With<Hero>>,
    mut portrait: Query<(&mut ImageNode, &mut Visibility), With<HudPortrait>>,
    mut loaded: Local<Option<String>>,
) {
    let model = hero.get_single().ok().and_then(|id| id.model_path.clone());
    if *loaded == model {
        return;
    }
    let handle = model
        .as_deref()
        .zip(asset_server.as_ref())
        .and_then(|(path, server)| {
            let data = server
                .loader
                .lock()
                .ok()?
                .vfs()
                .read_from_archive(path, "level.bld")
                .ok()?;
            let lib = SpriteLibrary::parse(&data)?;
            let (w, h, px) = portrait_sprite(&lib.sprites)?.decode_rgba(&data)?;
            Some(images.add(rgba8_to_bevy_image(w, h, px)))
        });
    for (mut image, mut vis) in &mut portrait {
        *vis = if handle.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        image.image = handle.clone().unwrap_or_default();
    }
    *loaded = model;
}

/// The default portrait of a character file: `NNN_MinionHead`, else any portrait sprite.
pub fn portrait_sprite(sprites: &[SpriteDef]) -> Option<&SpriteDef> {
    let has_image = |s: &&SpriteDef| s.image.is_some();
    sprites
        .iter()
        .filter(has_image)
        .find(|s| s.name.to_ascii_lowercase().ends_with("_minionhead"))
        .or_else(|| {
            sprites.iter().filter(has_image).find(|s| {
                s.texture_path
                    .as_deref()
                    .is_some_and(|p| p.to_ascii_lowercase().contains("/portraits/"))
            })
        })
}

/// Top-left and accumulated priority of a sprite, following its parent chain.
fn resolve(
    entity: Entity,
    sprites: &Query<(Entity, &HudSprite)>,
    screen: Vec2,
    depth: u32,
) -> Option<(Vec2, i32)> {
    let (_, s) = sprites.get(entity).ok()?;
    let (parent_min, parent_size, parent_prio) = match s.parent {
        Some(p) if depth < 16 => {
            let (min, prio) = resolve(p, sprites, screen, depth + 1)?;
            (min, sprites.get(p).ok()?.1.size, prio)
        }
        _ => (SAFE_INSET, screen - 2.0 * SAFE_INSET, 0),
    };
    Some((s.place(parent_min, parent_size), parent_prio + s.priority))
}

#[cfg(test)]
mod tests {
    use super::*;
    use skylands_core::ActorIdentity;

    #[test]
    fn align_matches_dol_enum_order() {
        assert_eq!(Align::TopLeft.factor(), Vec2::new(0.0, 0.0));
        assert_eq!(Align::MidCenter.factor(), Vec2::new(0.5, 0.5));
        // Player 1 HUD uses alignment 6 = ALIGN_BOTTOM_LEFT.
        assert_eq!(Align::BottomLeft as u8, 6);
        assert_eq!(Align::BottomLeft.factor(), Vec2::new(0.0, 1.0));
    }

    #[test]
    fn place_anchors_child_to_parent() {
        // Player HUD box: bottom-left of a 640x480 screen.
        let main = HudSprite::new(None, Vec2::new(237.0, 96.0))
            .align(Align::BottomLeft, Align::BottomLeft);
        let min = main.place(Vec2::ZERO, Vec2::new(640.0, 480.0));
        assert_eq!(min, Vec2::new(0.0, 384.0));

        // Health bar: mid-left to mid-left, (30, -6).
        let bar = HudSprite::new(None, Vec2::new(177.0, 18.0))
            .align(Align::MidLeft, Align::MidLeft)
            .at(30.0, -6.0);
        assert_eq!(
            bar.place(min, main.size),
            Vec2::new(30.0, 384.0 + 48.0 - 6.0 - 9.0)
        );
    }

    #[test]
    fn script_colors_are_abgr() {
        let money = script_color(0xff00_aaff).to_srgba();
        assert_eq!(
            (money.red, money.green, money.blue, money.alpha),
            (1.0, 170.0 / 255.0, 0.0, 1.0)
        );
        let low = script_color(COLOR_HEALTH_LOW).to_srgba();
        assert!(low.red > 0.6 && low.green == 0.0 && low.blue == 0.0);
        assert_eq!(health_bar_color(20, 100), COLOR_HEALTH_LOW);
        assert_eq!(health_bar_color(30, 100), COLOR_HEALTH_MID);
        assert_eq!(health_bar_color(90, 100), COLOR_HEALTH_FULL);
    }

    fn hud_app() -> App {
        let mut app = App::new();
        app.insert_resource(GameStats {
            coins: 42,
            ..default()
        })
        .add_systems(
            Update,
            (sky4_hud_update_system, sky4_hud_layout_system).chain(),
        );
        app
    }

    #[test]
    fn health_fill_and_texts_follow_hero() {
        let mut app = hud_app();
        app.world_mut().spawn((
            Hero,
            Health {
                current: 20,
                max: 100,
            },
            ActorIdentity {
                id: 1,
                character_id: 400,
                name: "Snap Shot".into(),
                is_villain: false,
                model_path: None,
                texture_path: None,
            },
        ));
        let fill = app
            .world_mut()
            .spawn((
                absolute_node(),
                ZIndex::default(),
                ImageNode::default(),
                HudSprite::new(None, Vec2::new(HEALTH_BAR_WIDTH, 18.0)),
                HudHealthFill,
            ))
            .id();
        let coin = app
            .world_mut()
            .spawn((HudString::default(), HudCoinText))
            .id();
        let name = app
            .world_mut()
            .spawn((HudString::default(), HudNameText))
            .id();
        let hp = app
            .world_mut()
            .spawn((HudString::default(), HudHpText))
            .id();

        app.update();

        let w = app.world();
        let sprite = w.get::<HudSprite>(fill).unwrap();
        assert!((sprite.size.x - (HEALTH_BAR_WIDTH - 4.0) * 0.2).abs() < 1e-4);
        assert_eq!(
            w.get::<ImageNode>(fill).unwrap().color,
            script_color(COLOR_HEALTH_LOW | 0xff00_0000)
        );
        // No window: 480-unit screen at scale 1.
        assert_eq!(
            w.get::<Node>(fill).unwrap().width,
            Val::Px(((HEALTH_BAR_WIDTH - 4.0) * 0.2).round())
        );
        assert_eq!(w.get::<HudString>(coin).unwrap().0, "42");
        assert_eq!(w.get::<HudString>(name).unwrap().0, "Snap Shot");
        assert_eq!(w.get::<HudString>(hp).unwrap().0, "20");
    }

    #[test]
    fn layout_sums_priority_and_scales_to_window() {
        let mut app = hud_app();
        app.world_mut().spawn((
            Window {
                resolution: (1280.0_f32, 960.0_f32).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        let parent = app
            .world_mut()
            .spawn((
                absolute_node(),
                ZIndex::default(),
                HudSprite::new(None, Vec2::new(100.0, 50.0))
                    .align(Align::BottomLeft, Align::BottomLeft)
                    .prio(-25),
            ))
            .id();
        let child = app
            .world_mut()
            .spawn((
                absolute_node(),
                ZIndex::default(),
                HudSprite::new(Some(parent), Vec2::new(10.0, 10.0)).prio(-2),
            ))
            .id();

        app.update();

        let w = app.world();
        let node = w.get::<Node>(child).unwrap();
        // Parent at (16, 480 - 16 - 50); child centred in it; x2 window scale.
        assert_eq!(node.left, Val::Px((16.0 + 45.0) * 2.0));
        assert_eq!(node.top, Val::Px((480.0 - 16.0 - 50.0 + 20.0) * 2.0));
        assert_eq!(*w.get::<ZIndex>(child).unwrap(), ZIndex(27));
    }

    fn test_font() -> HudFont {
        let glyph = |advance: f32, w: f32, u: f32| skylands_formats::Glyph {
            advance,
            offset: [0.0, 9.0],
            size: [w, 36.0],
            uv_min: [u, 0.5],
            uv_max: [u + 0.25, 0.75],
        };
        let mut glyphs = std::collections::HashMap::new();
        glyphs.insert('A', glyph(32.0, 34.0, 0.0));
        glyphs.insert('B', glyph(24.0, 24.0, 0.5));
        glyphs.insert(' ', glyph(16.0, 0.0, 0.0));
        HudFont {
            font: BitmapFont {
                point_size: 56.0,
                glyphs,
                image: None,
            },
            image: Handle::default(),
            sheet_size: Vec2::new(512.0, 256.0),
        }
    }

    #[test]
    fn font_layout_scales_and_justifies() {
        let font = test_font();
        // 28pt = half the 56pt font; text box 100 x 28.
        let left = font.layout("A B", 28.0, JustifyText::Left, Vec2::new(100.0, 28.0));
        // Space has no quad but advances.
        assert_eq!(left.len(), 2);
        assert_eq!(left[0].0, Vec2::new(0.0, 4.5));
        assert_eq!(left[0].1, Vec2::new(17.0, 18.0));
        assert_eq!(left[0].2, Rect::new(0.0, 128.0, 128.0, 192.0));
        assert_eq!(left[1].0.x, 24.0);

        // Total width (32 + 16 + 24) / 2 = 36.
        let right = font.layout("A B", 28.0, JustifyText::Right, Vec2::new(100.0, 28.0));
        assert_eq!(right[0].0.x, 64.0);
        let center = font.layout("A B", 28.0, JustifyText::Center, Vec2::new(100.0, 28.0));
        assert_eq!(center[0].0.x, 32.0);
        // Missing glyphs are skipped.
        assert!(font
            .layout("?", 28.0, JustifyText::Left, Vec2::ONE)
            .is_empty());
    }

    #[test]
    fn text_system_spawns_tinted_glyphs_on_change() {
        let mut app = App::new();
        app.insert_resource(Sky4HudAssets {
            element_icons: Vec::new(),
            font: Some(test_font()),
        })
        .add_systems(Update, sky4_hud_text_system);
        app.world_mut().spawn(Sky4HudRoot);
        let red = Color::srgb(1.0, 0.0, 0.0);
        let text = app
            .world_mut()
            .spawn((
                HudText {
                    point_size: 28.0,
                    justify: JustifyText::Left,
                    color: red,
                },
                HudString("AB".into()),
                HudGlyphs::default(),
                HudSprite::new(None, Vec2::new(100.0, 28.0)),
            ))
            .id();
        app.update();

        let glyphs = app.world().get::<HudGlyphs>(text).unwrap().0.clone();
        assert_eq!(glyphs.len(), 2);
        let node = app.world().get::<ImageNode>(glyphs[1]).unwrap();
        assert_eq!(node.color, red);
        assert_eq!(node.rect, Some(Rect::new(256.0, 128.0, 384.0, 192.0)));
        let sprite = app.world().get::<HudSprite>(glyphs[1]).unwrap();
        assert_eq!(sprite.parent, Some(text));
        assert_eq!(sprite.location, Vec2::new(16.0, 4.5));

        // A new string replaces the old glyphs.
        app.world_mut().get_mut::<HudString>(text).unwrap().0 = "B".into();
        app.update();
        let new = app.world().get::<HudGlyphs>(text).unwrap().0.clone();
        assert_eq!(new.len(), 1);
        assert!(app.world().get_entity(glyphs[0]).is_err());
    }

    #[test]
    fn portrait_prefers_default_minion_head() {
        let def = |name: &str, path: &str, image: bool| SpriteDef {
            name: name.into(),
            texture_path: Some(path.into()),
            natural_size: [142.2, 71.1],
            image: image.then_some(skylands_formats::IgzImage2 {
                offset: 0,
                width: 142,
                height: 71,
                depth: 1,
                mipmap_count: 1,
                array: 1,
                format: 0,
                texture_offset: 0,
                texture_size: 0,
            }),
            alpha_image: None,
        };
        let sprites = [
            def(
                "016_MinionHead_Series3",
                "UI_SKY4/HUD/Portraits/2014/256x128/a.png",
                true,
            ),
            def(
                "016_MinionHead",
                "UI_SKY4/HUD/Portraits/2014/256x128/b.png",
                true,
            ),
        ];
        assert_eq!(portrait_sprite(&sprites).unwrap().name, "016_MinionHead");
        let sprites = [
            def("016_MinionHead", "x.png", false),
            def("Other", "UI_SKY4/HUD/Portraits/2014/256x128/c.png", true),
        ];
        assert_eq!(portrait_sprite(&sprites).unwrap().name, "Other");
    }
}
