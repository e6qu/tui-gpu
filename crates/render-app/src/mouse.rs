use doom_input::{DOOM_KEY_LEFT, DOOM_KEY_RIGHT};

const TURN_THRESHOLD: f64 = 0.8;
const TURN_HOLD_TICKS: u8 = 2;

pub struct MouseTurnState {
    left_timer: u8,
    right_timer: u8,
    left_active: bool,
    right_active: bool,
}

impl MouseTurnState {
    pub fn new() -> Self {
        Self {
            left_timer: 0,
            right_timer: 0,
            left_active: false,
            right_active: false,
        }
    }

    pub fn process_delta(&mut self, delta: f64) -> Vec<(bool, u8)> {
        let mut events = Vec::new();
        if delta > TURN_THRESHOLD {
            self.right_timer = TURN_HOLD_TICKS;
            if !self.right_active {
                events.push((true, DOOM_KEY_RIGHT));
                self.right_active = true;
            }
            if self.left_active {
                events.push((false, DOOM_KEY_LEFT));
                self.left_active = false;
                self.left_timer = 0;
            }
        } else if delta < -TURN_THRESHOLD {
            self.left_timer = TURN_HOLD_TICKS;
            if !self.left_active {
                events.push((true, DOOM_KEY_LEFT));
                self.left_active = true;
            }
            if self.right_active {
                events.push((false, DOOM_KEY_RIGHT));
                self.right_active = false;
                self.right_timer = 0;
            }
        }
        events
    }

    pub fn tick(&mut self) -> Vec<u8> {
        let mut releases = Vec::new();
        if self.left_timer > 0 {
            self.left_timer -= 1;
            if self.left_timer == 0 && self.left_active {
                releases.push(DOOM_KEY_LEFT);
                self.left_active = false;
            }
        }
        if self.right_timer > 0 {
            self.right_timer -= 1;
            if self.right_timer == 0 && self.right_active {
                releases.push(DOOM_KEY_RIGHT);
                self.right_active = false;
            }
        }
        releases
    }

    pub fn reset(&mut self) -> Vec<u8> {
        let mut releases = Vec::new();
        if self.left_active {
            releases.push(DOOM_KEY_LEFT);
        }
        if self.right_active {
            releases.push(DOOM_KEY_RIGHT);
        }
        self.left_active = false;
        self.right_active = false;
        self.left_timer = 0;
        self.right_timer = 0;
        releases
    }
}
