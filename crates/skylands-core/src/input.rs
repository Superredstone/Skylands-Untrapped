use gilrs::{Axis, Button, Event, EventType, Gilrs};

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WiiButton {
    DpadUp = 1 << 0,
    DpadDown = 1 << 1,
    DpadLeft = 1 << 2,
    DpadRight = 1 << 3,
    A = 1 << 4,
    B = 1 << 5,
    C = 1 << 6,
    Z = 1 << 7,
    One = 1 << 8,
    Two = 1 << 9,
    Plus = 1 << 10,
    Minus = 1 << 11,
    Home = 1 << 12,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WiiInputState {
    pub stick: [f32; 2],
    pub buttons: u32,
}

impl WiiInputState {
    pub fn new() -> Self {
        Self {
            stick: [0.0, 0.0],
            buttons: 0,
        }
    }

    pub fn is_down(&self, btn: WiiButton) -> bool {
        (self.buttons & (btn as u32)) != 0
    }

    pub fn press(&mut self, btn: WiiButton) {
        self.buttons |= btn as u32;
    }

    pub fn release(&mut self, btn: WiiButton) {
        self.buttons &= !(btn as u32);
    }

    pub fn set_button(&mut self, btn: WiiButton, pressed: bool) {
        if pressed {
            self.press(btn);
        } else {
            self.release(btn);
        }
    }

    pub fn set_stick(&mut self, x: f32, y: f32) {
        self.stick[0] = x.clamp(-1.0, 1.0);
        self.stick[1] = y.clamp(-1.0, 1.0);
    }

    pub fn reset(&mut self) {
        self.stick = [0.0, 0.0];
        self.buttons = 0;
    }
}

impl Default for WiiInputState {
    fn default() -> Self {
        Self::new()
    }
}

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

    pub fn handle_key_code(&mut self, key_code: u32, pressed: bool) {
        match key_code {
            17 | 25 | 103 => {
                self.key_w = pressed;
                self.state.set_button(WiiButton::DpadUp, pressed);
            }
            31 | 39 | 108 => {
                self.key_s = pressed;
                self.state.set_button(WiiButton::DpadDown, pressed);
            }
            30 | 38 | 105 => {
                self.key_a = pressed;
                self.state.set_button(WiiButton::DpadLeft, pressed);
            }
            32 | 40 | 106 => {
                self.key_d = pressed;
                self.state.set_button(WiiButton::DpadRight, pressed);
            }
            57 => self.state.set_button(WiiButton::A, pressed),
            36 | 44 => self.state.set_button(WiiButton::B, pressed),
            37 | 45 => self.state.set_button(WiiButton::Z, pressed),
            46 => self.state.set_button(WiiButton::C, pressed),
            22 => self.state.set_button(WiiButton::One, pressed),
            23 => self.state.set_button(WiiButton::Two, pressed),
            28 => self.state.set_button(WiiButton::Plus, pressed),
            1 | 14 => self.state.set_button(WiiButton::Minus, pressed),
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
        mapper.handle_key_code(57, true);
        assert!(mapper.state().is_down(WiiButton::A));

        mapper.handle_key_code(57, false);
        assert!(!mapper.state().is_down(WiiButton::A));

        mapper.handle_key_code(17, true);
        assert_eq!(mapper.state().stick, [0.0, 1.0]);

        mapper.handle_key_code(32, true);
        assert_eq!(mapper.state().stick, [1.0, 1.0]);

        mapper.handle_key_code(22, true);
        assert!(mapper.state().is_down(WiiButton::One));
        mapper.handle_key_code(23, true);
        assert!(mapper.state().is_down(WiiButton::Two));
    }
}
