#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDelta {
    Wheel(f64),
    Continuous(f64),
    Stop,
}

pub const WHEEL_CLICK_V120: f64 = 120.0;
pub const CONTINUOUS_STEP_DISTANCE: f64 = 24.0;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ScrollAccumulator {
    wheel: f64,
    continuous: f64,
}

impl ScrollAccumulator {
    pub fn feed(&mut self, delta: ScrollDelta) -> i32 {
        match delta {
            ScrollDelta::Stop => {
                *self = Self::default();
                0
            }
            ScrollDelta::Wheel(v120) => Self::take_steps(&mut self.wheel, v120, WHEEL_CLICK_V120),
            ScrollDelta::Continuous(distance) => {
                Self::take_steps(&mut self.continuous, distance, CONTINUOUS_STEP_DISTANCE)
            }
        }
    }

    fn take_steps(pending: &mut f64, delta: f64, threshold: f64) -> i32 {
        if delta == 0.0 {
            return 0;
        }
        if pending.signum() != delta.signum() {
            *pending = 0.0;
        }
        *pending += delta;
        let steps = (*pending / threshold).trunc();
        *pending -= steps * threshold;
        steps as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_clicks_and_finger_travel_become_whole_steps() {
        let mut scroll = ScrollAccumulator::default();
        for (delta, steps) in [(120.0, 1), (-120.0, -1), (240.0, 2), (60.0, 0), (60.0, 1)] {
            assert_eq!(scroll.feed(ScrollDelta::Wheel(delta)), steps, "{delta}");
        }
        let per_event = CONTINUOUS_STEP_DISTANCE / 4.0;
        let steps: i32 = (0..16).map(|_| scroll.feed(ScrollDelta::Continuous(per_event))).sum();
        assert_eq!(steps, 4);

        let most = CONTINUOUS_STEP_DISTANCE * 0.9;
        for (delta, steps) in [(most, 0), (-most, 0), (-most, -1), (most, 0)] {
            assert_eq!(scroll.feed(ScrollDelta::Continuous(delta)), steps, "reversal drops travel");
        }
        assert_eq!(scroll.feed(ScrollDelta::Stop), 0);
        assert_eq!(scroll.feed(ScrollDelta::Continuous(most)), 0, "lifting drops travel");
    }
}
