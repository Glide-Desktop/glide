use std::cell::Cell;
use std::marker::PhantomData;
use std::time::{Duration, Instant};

thread_local! {
    static FRAME: Cell<Option<Instant>> = const { Cell::new(None) };
}

#[must_use]
pub fn now() -> Instant {
    FRAME.with(Cell::get).unwrap_or_else(Instant::now)
}

#[must_use]
pub fn since(start: Instant) -> Duration {
    now().saturating_duration_since(start)
}

#[must_use]
pub struct FramePass {
    previous: Option<Instant>,
    _thread: PhantomData<*const ()>,
}

impl FramePass {
    pub fn enter(at: Instant) -> Self {
        let previous = FRAME.with(|frame| frame.replace(Some(at)));
        Self { previous, _thread: PhantomData }
    }
}

impl Drop for FramePass {
    fn drop(&mut self) {
        FRAME.with(|frame| frame.set(self.previous));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_inside_a_frame_pass_are_identical_across_a_sleep() {
        let at = Instant::now() + Duration::from_millis(16);
        let _pass = FramePass::enter(at);
        let first = now();
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(first, at);
        assert_eq!(now(), at, "every element of one frame reads the same presentation time");
    }

    #[test]
    fn wall_time_resumes_when_the_pass_ends() {
        let at = Instant::now() + Duration::from_mins(1);
        drop(FramePass::enter(at));
        assert!(now() < at);
    }

    #[test]
    fn a_nested_pass_restores_the_outer_time() {
        let outer = Instant::now() + Duration::from_millis(16);
        let inner = outer + Duration::from_millis(16);
        let _outer = FramePass::enter(outer);
        drop(FramePass::enter(inner));
        assert_eq!(now(), outer);
    }

    #[test]
    fn a_panicking_pass_restores_the_previous_time() {
        let at = Instant::now() + Duration::from_mins(1);
        let result = std::panic::catch_unwind(|| {
            let _pass = FramePass::enter(at);
            panic!("render failed");
        });
        assert!(result.is_err());
        assert!(now() < at);
    }

    #[test]
    fn a_start_in_the_future_reads_as_not_yet_begun() {
        let later = Instant::now() + Duration::from_secs(1);
        assert_eq!(since(later), Duration::ZERO);
    }

    #[test]
    fn another_thread_never_sees_this_threads_pass() {
        let at = Instant::now() + Duration::from_mins(1);
        let _pass = FramePass::enter(at);
        let seen = std::thread::spawn(now).join().expect("thread");
        assert!(seen < at);
    }
}
