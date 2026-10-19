use std::time::Duration;

pub const TICK_RATE_HZ: u32 = 60;
pub const TICK_DURATION: Duration = Duration::from_nanos(16_666_667); // 1/60s

pub struct FixedTimestep {
    accumulator: Duration,
    tick_count: u64,
    dt_secs: f32,
}

impl FixedTimestep {
    pub fn new() -> Self {
        Self {
            accumulator: Duration::ZERO,
            tick_count: 0,
            dt_secs: 1.0 / TICK_RATE_HZ as f32,
        }
    }

    pub fn dt(&self) -> f32 {
        self.dt_secs
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn update<F: FnMut(u64, f32)>(&mut self, elapsed: Duration, mut tick_fn: F) -> u32 {
        // Clamp max accumulator to 100ms to prevent death spiral on hitch
        const MAX_ACCUMULATOR: Duration = Duration::from_millis(100);
        self.accumulator += elapsed;
        if self.accumulator > MAX_ACCUMULATOR {
            self.accumulator = MAX_ACCUMULATOR;
        }

        let mut ticks_executed = 0;
        while self.accumulator >= TICK_DURATION {
            self.tick_count += 1;
            ticks_executed += 1;
            tick_fn(self.tick_count, self.dt_secs);
            self.accumulator -= TICK_DURATION;
        }

        ticks_executed
    }

    pub fn reset(&mut self) {
        self.accumulator = Duration::ZERO;
        self.tick_count = 0;
    }
}

impl Default for FixedTimestep {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_60hz_ticks() {
        let mut loop_ctrl = FixedTimestep::new();
        let mut count = 0;

        // 33.33ms elapsed should run 2 ticks (16.66ms each)
        let ticks = loop_ctrl.update(Duration::from_millis(34), |_tick, _dt| {
            count += 1;
        });

        assert_eq!(ticks, 2);
        assert_eq!(count, 2);
        assert_eq!(loop_ctrl.tick_count(), 2);
    }
}
