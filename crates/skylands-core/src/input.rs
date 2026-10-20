pub use crate::resources::{WiiButton, WiiInputState};
pub use bevy::prelude::KeyCode;
use gilrs::{Axis, Button, Event, EventType, Gilrs};

pub struct InputMapper {
    pub state: WiiInputState,
    gilrs: Option<Gilrs>,
    key_w: bool,
    key_s: bool,
    key_a: bool,
    key_d: bool,
}

impl InputMapper {
    pub fn new() -> Self {
        let gilrs = Gilrs::new().ok();
        Self {
            state: WiiInputState::new(),
            gilrs,
            key_w: false,
            key_s: false,
            key_a: false,
            key_d: false,
        }
    }

    pub fn handle_key_code(&mut self, key_code: KeyCode, pressed: bool) {
        match key_code {
            KeyCode::KeyW | KeyCode::ArrowUp => {
                self.key_w = pressed;
                self.state.set_button(WiiButton::DpadUp, pressed);
            }
            KeyCode::KeyS | KeyCode::ArrowDown => {
                self.key_s = pressed;
                self.state.set_button(WiiButton::DpadDown, pressed);
            }
            KeyCode::KeyA | KeyCode::ArrowLeft => {
                self.key_a = pressed;
                self.state.set_button(WiiButton::DpadLeft, pressed);
            }
            KeyCode::KeyD | KeyCode::ArrowRight => {
                self.key_d = pressed;
                self.state.set_button(WiiButton::DpadRight, pressed);
            }
            KeyCode::Space => self.state.set_button(WiiButton::A, pressed),
            KeyCode::KeyJ => self.state.set_button(WiiButton::B, pressed),
            KeyCode::KeyK => self.state.set_button(WiiButton::Z, pressed),
            KeyCode::KeyL => self.state.set_button(WiiButton::C, pressed),
            KeyCode::Digit1 | KeyCode::Numpad1 => self.state.set_button(WiiButton::One, pressed),
            KeyCode::Digit2 | KeyCode::Numpad2 => self.state.set_button(WiiButton::Two, pressed),
            KeyCode::Equal | KeyCode::NumpadAdd | KeyCode::Enter | KeyCode::NumpadEnter => {
                self.state.set_button(WiiButton::Plus, pressed)
            }
            KeyCode::Minus | KeyCode::NumpadSubtract | KeyCode::Escape | KeyCode::Backspace => {
                self.state.set_button(WiiButton::Minus, pressed)
            }
            _ => {}
        }

        let mut sx = 0.0f32;
        let mut sy = 0.0f32;
        if self.key_d {
            sx += 1.0;
        }
        if self.key_a {
            sx -= 1.0;
        }
        if self.key_w {
            sy += 1.0;
        }
        if self.key_s {
            sy -= 1.0;
        }
        self.state.set_stick(sx, sy);
    }

    pub fn poll_gamepads(&mut self) {
        if let Some(gilrs) = &mut self.gilrs {
            while let Some(Event { event, .. }) = gilrs.next_event() {
                match event {
                    EventType::ButtonPressed(btn, _) => {
                        Self::map_gamepad_button(&mut self.state, btn, true)
                    }
                    EventType::ButtonReleased(btn, _) => {
                        Self::map_gamepad_button(&mut self.state, btn, false)
                    }
                    EventType::AxisChanged(axis, val, _) => match axis {
                        Axis::LeftStickX => self.state.stick[0] = val,
                        Axis::LeftStickY => self.state.stick[1] = val,
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
    }

    fn map_gamepad_button(state: &mut WiiInputState, btn: Button, pressed: bool) {
        match btn {
            Button::South => state.set_button(WiiButton::A, pressed),
            Button::West => state.set_button(WiiButton::B, pressed),
            Button::East => state.set_button(WiiButton::C, pressed),
            Button::North => state.set_button(WiiButton::Z, pressed),
            Button::LeftTrigger | Button::LeftTrigger2 => state.set_button(WiiButton::One, pressed),
            Button::RightTrigger | Button::RightTrigger2 => {
                state.set_button(WiiButton::Two, pressed)
            }
            Button::Start => state.set_button(WiiButton::Plus, pressed),
            Button::Select => state.set_button(WiiButton::Minus, pressed),
            Button::DPadUp => state.set_button(WiiButton::DpadUp, pressed),
            Button::DPadDown => state.set_button(WiiButton::DpadDown, pressed),
            Button::DPadLeft => state.set_button(WiiButton::DpadLeft, pressed),
            Button::DPadRight => state.set_button(WiiButton::DpadRight, pressed),
            _ => {}
        }
    }

    pub fn state(&self) -> &WiiInputState {
        &self.state
    }
}

impl Default for InputMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyboard_to_wii_mapping() {
        let mut mapper = InputMapper::new();
        mapper.handle_key_code(KeyCode::Space, true);
        assert!(mapper.state().is_down(WiiButton::A));

        mapper.handle_key_code(KeyCode::Space, false);
        assert!(!mapper.state().is_down(WiiButton::A));

        mapper.handle_key_code(KeyCode::KeyW, true);
        assert_eq!(mapper.state().stick, [0.0, 1.0]);

        mapper.handle_key_code(KeyCode::KeyD, true);
        assert_eq!(mapper.state().stick, [1.0, 1.0]);

        mapper.handle_key_code(KeyCode::KeyW, false);
        mapper.handle_key_code(KeyCode::KeyD, false);

        mapper.handle_key_code(KeyCode::ArrowUp, true);
        mapper.handle_key_code(KeyCode::ArrowLeft, true);
        assert_eq!(mapper.state().stick, [-1.0, 1.0]);

        mapper.handle_key_code(KeyCode::KeyJ, true);
        assert!(mapper.state().is_down(WiiButton::B));

        mapper.handle_key_code(KeyCode::KeyK, true);
        assert!(mapper.state().is_down(WiiButton::Z));

        mapper.handle_key_code(KeyCode::KeyL, true);
        assert!(mapper.state().is_down(WiiButton::C));

        mapper.handle_key_code(KeyCode::Digit1, true);
        assert!(mapper.state().is_down(WiiButton::One));
        mapper.handle_key_code(KeyCode::Digit2, true);
        assert!(mapper.state().is_down(WiiButton::Two));
    }
}
