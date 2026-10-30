//! Title main menu, rebuilt from `level/Title.bld`: `MainMenuMacros.ai` "Menu - Main - Create"
//! defines a rotating-rig page (`ui_menu_3d_carousel.ai`) with three buttons and Settings on
//! action 4. This is the menu state; `skylands-render` draws it. Notes: `agent/TITLE_MENU.md`.

use bevy::prelude::*;

use crate::input::{WiiButton, WiiInputState};
use crate::states::GameState;

/// "Menu - Main - Create" `loop 3`, in button id order ("button actions").
pub const TITLE_BUTTONS: [TitleButton; 3] = [
    TitleButton::HordeMode,
    TitleButton::Adventure,
    TitleButton::ArenaMode,
];
/// `"rig def"."initial selection"` = 1: "Page Layout" selects next once from 0.
pub const TITLE_INITIAL_SELECTION: usize = 1;
/// `MainMenuCarousel` (`LevelSelect_Base.mdl`) attach nodes `J_Node_1` .. `J_Node_18`.
pub const TITLE_RIG_NODES: usize = 18;
/// "Rotating Rig - Select": the rig turns `360 / attach nodes` per step...
pub const TITLE_RIG_STEP_DEGREES: f32 = 360.0 / TITLE_RIG_NODES as f32;
/// ...sliding over 0.2 s.
pub const TITLE_RIG_TURN_TIME: f32 = 0.2;
/// Stick deflection read as a press (assumed; menus take the d-pad and stick).
const STICK_PRESS: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleButton {
    /// `MainMenu_Horde_*.mdl`, "Menu - Main - Horde Mode".
    HordeMode,
    /// `MainMenu_Story_*.mdl`, "Menu - Main - Adventure".
    Adventure,
    /// `MainMenu_Arena_*.mdl`, "Menu - Main - Arena Mode".
    ArenaMode,
}

/// The main page's `"menu info"`.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TitleMenu {
    /// `"menu info"."selected"`.
    pub selected: usize,
    /// `"menu info"."rig facing val"` (degrees) before and after the current turn.
    pub rig_facing_from: f32,
    pub rig_facing_to: f32,
    /// Seconds since the current turn started.
    pub turn_elapsed: f32,
    /// Seconds since the selected button took focus.
    pub focus_elapsed: f32,
    prev_input: u8,
}

impl Default for TitleMenu {
    fn default() -> Self {
        let mut menu = Self {
            selected: 0,
            rig_facing_from: 0.0,
            rig_facing_to: 0.0,
            turn_elapsed: TITLE_RIG_TURN_TIME,
            focus_elapsed: 0.0,
            prev_input: 0,
        };
        // "Page Layout" calls "Select" without "anim": the rig is set, not slid.
        for _ in 0..TITLE_INITIAL_SELECTION {
            menu.select(true);
        }
        menu.rig_facing_from = menu.rig_facing_to;
        menu
    }
}

impl TitleMenu {
    pub fn button(&self) -> TitleButton {
        TITLE_BUTTONS[self.selected]
    }

    /// "Menu - 3d - Rotating Rig - Select": no wrap; at either end the move is invalid.
    pub fn select(&mut self, next: bool) -> bool {
        if next && self.selected + 1 >= TITLE_BUTTONS.len() || !next && self.selected == 0 {
            return false;
        }
        self.rig_facing_from = self.rig_facing();
        if next {
            self.selected += 1;
            self.rig_facing_to -= TITLE_RIG_STEP_DEGREES;
        } else {
            self.selected -= 1;
            self.rig_facing_to += TITLE_RIG_STEP_DEGREES;
        }
        self.turn_elapsed = 0.0;
        self.focus_elapsed = 0.0;
        true
    }

    /// Rig heading (degrees) now: linear slide from the old facing to the new one.
    pub fn rig_facing(&self) -> f32 {
        let k = (self.turn_elapsed / TITLE_RIG_TURN_TIME).clamp(0.0, 1.0);
        self.rig_facing_from + (self.rig_facing_to - self.rig_facing_from) * k
    }

    /// Advances one tick; returns the button confirmed this tick, if any.
    pub fn tick(&mut self, input: &WiiInputState, dt: f32) -> Option<TitleButton> {
        self.turn_elapsed += dt;
        self.focus_elapsed += dt;
        const LEFT: u8 = 1;
        const RIGHT: u8 = 2;
        const CONFIRM: u8 = 4;
        let mut now = 0;
        if input.is_down(WiiButton::DpadLeft) || input.stick[0] <= -STICK_PRESS {
            now |= LEFT;
        }
        if input.is_down(WiiButton::DpadRight) || input.stick[0] >= STICK_PRESS {
            now |= RIGHT;
        }
        if input.is_down(WiiButton::A) {
            now |= CONFIRM;
        }
        let pressed = now & !self.prev_input;
        self.prev_input = now;
        if pressed & LEFT != 0 {
            self.select(false);
        }
        if pressed & RIGHT != 0 {
            self.select(true);
        }
        (pressed & CONFIRM != 0).then(|| self.button())
    }
}

/// Runs the main page while the game is on the title screen. Adventure starts the game in
/// the hub; the Horde and Arena pages are not rebuilt yet.
pub fn title_menu_system(
    time: Res<Time>,
    input: Res<WiiInputState>,
    mut menu: ResMut<TitleMenu>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if menu.tick(&input, time.delta_secs()) == Some(TitleButton::Adventure) {
        next_state.set(GameState::Hub);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn press(menu: &mut TitleMenu, btn: WiiButton) -> Option<TitleButton> {
        let mut input = WiiInputState::default();
        input.press(btn);
        let out = menu.tick(&input, DT);
        menu.tick(&WiiInputState::default(), DT);
        out
    }

    #[test]
    fn starts_on_adventure_with_rig_turned_one_step() {
        let menu = TitleMenu::default();
        assert_eq!(menu.button(), TitleButton::Adventure);
        assert_eq!(menu.rig_facing(), -TITLE_RIG_STEP_DEGREES);
    }

    #[test]
    fn selection_stops_at_both_ends_and_turns_the_rig() {
        let mut menu = TitleMenu::default();
        press(&mut menu, WiiButton::DpadRight);
        assert_eq!(menu.button(), TitleButton::ArenaMode);
        press(&mut menu, WiiButton::DpadRight);
        assert_eq!(menu.button(), TitleButton::ArenaMode);
        for _ in 0..30 {
            menu.tick(&WiiInputState::default(), DT);
        }
        assert!((menu.rig_facing() + 2.0 * TITLE_RIG_STEP_DEGREES).abs() < 1e-4);
        press(&mut menu, WiiButton::DpadLeft);
        press(&mut menu, WiiButton::DpadLeft);
        press(&mut menu, WiiButton::DpadLeft);
        assert_eq!(menu.button(), TitleButton::HordeMode);
    }

    #[test]
    fn held_direction_moves_once() {
        let mut menu = TitleMenu::default();
        let mut input = WiiInputState::default();
        input.set_stick(1.0, 0.0);
        for _ in 0..10 {
            menu.tick(&input, DT);
        }
        assert_eq!(menu.button(), TitleButton::ArenaMode);
        assert_eq!(menu.selected, 2);
    }

    #[test]
    fn confirm_reports_selected_button() {
        let mut menu = TitleMenu::default();
        assert_eq!(press(&mut menu, WiiButton::A), Some(TitleButton::Adventure));
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn choosing_adventure_leaves_the_title_for_the_hub() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_resource::<Time>()
            .init_resource::<TitleMenu>()
            .init_resource::<WiiInputState>()
            .insert_state(GameState::Title)
            .add_systems(Update, title_menu_system);
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::A);
        app.update();
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Hub
        );
    }
}
