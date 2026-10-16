use std::time::{Duration, Instant};

use glide_theme::motion::{self, E_IO, E_OUT, E_OVER};
use iced::animation::{Animation, Easing};

const MIN: Duration = Duration::from_millis(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    Out,
    InOut,
    Over,
}

impl Curve {
    fn easing(self) -> Easing {
        match self {
            Self::Out => Easing::Custom(|x| E_OUT.ease(x)),
            Self::InOut => Easing::Custom(|x| E_IO.ease(x)),
            Self::Over => Easing::Custom(|x| E_OVER.ease(x)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    T1,
    T2,
    T3,
    T4,
}

impl Speed {
    fn duration(self) -> Duration {
        match self {
            Self::T1 => motion::t1(),
            Self::T2 => motion::t2(),
            Self::T3 => motion::t3(),
            Self::T4 => motion::t4(),
        }
        .max(MIN)
    }
}

#[derive(Debug, Clone)]
pub struct Fade {
    animation: Animation<bool>,
    enter: (Speed, Curve),
    leave: (Speed, Curve),
}

impl Fade {
    #[must_use]
    pub fn new(on: bool, speed: Speed, curve: Curve) -> Self {
        Self::asymmetric(on, (speed, curve), (speed, curve))
    }

    #[must_use]
    pub fn asymmetric(on: bool, enter: (Speed, Curve), leave: (Speed, Curve)) -> Self {
        Self { animation: Animation::new(on), enter, leave }
    }

    pub fn set(&mut self, on: bool, now: Instant) {
        if self.animation.value() == on {
            return;
        }
        let (speed, curve) = if on { self.enter } else { self.leave };
        let animation = std::mem::replace(&mut self.animation, Animation::new(on));
        self.animation = animation.duration(speed.duration()).easing(curve.easing()).go(on, now);
    }

    #[must_use]
    pub fn target(&self) -> bool {
        self.animation.value()
    }

    #[must_use]
    pub fn progress(&self, now: Instant) -> f32 {
        if motion::is_disabled() {
            return if self.target() { 1.0 } else { 0.0 };
        }
        self.animation.interpolate(0.0, 1.0, now)
    }

    #[must_use]
    pub fn is_animating(&self, now: Instant) -> bool {
        !motion::is_disabled() && self.animation.is_animating(now)
    }

    #[must_use]
    pub fn is_settled_off(&self, now: Instant) -> bool {
        !self.target() && !self.is_animating(now)
    }
}
