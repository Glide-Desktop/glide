use std::time::{Duration, Instant};

use glide_catalog::{APP_ID, Catalog, Message, Page};
use glide_sdk::iced::advanced::widget::Tree;
use glide_sdk::iced::advanced::{Layout, Shell, clipboard, layout};
use glide_sdk::iced::{Element, Event, Font, Pixels, Rectangle, Renderer, Size, mouse, window};
use glide_sdk::{Harness, Look};

const SIZE: Size = Size::new(1120.0, 760.0);

fn renderer() -> Renderer {
    Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)))
}

struct Run {
    frames: usize,
    settled: bool,
    build: Duration,
}

fn run<M>(mut element: Element<'_, M>, renderer: &Renderer, limit: Duration) -> Run {
    let started = Instant::now();
    let mut tree = Tree::new(&element);
    let mut node = element.as_widget_mut().layout(&mut tree, renderer, &layout::Limits::new(Size::ZERO, SIZE));
    let build = started.elapsed();
    let mut frames = 0;
    let until = Instant::now() + limit;
    loop {
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            &Event::Window(window::Event::RedrawRequested(Instant::now())),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard::Null,
            &mut shell,
            &Rectangle::with_size(SIZE),
        );
        let asked = shell.redraw_request() == window::RedrawRequest::NextFrame;
        if shell.is_layout_invalid() {
            node = element.as_widget_mut().layout(&mut tree, renderer, &layout::Limits::new(Size::ZERO, SIZE));
        }
        if !asked {
            return Run { frames, settled: true, build };
        }
        frames += 1;
        if Instant::now() >= until {
            return Run { frames, settled: false, build };
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

#[test]
fn every_page_settles_and_stops_asking_for_frames_except_live_indicators() {
    let look = Look { motion_scale: 1.0, ..Look::default() };
    let mut harness = Harness::<Catalog>::new(APP_ID, look, SIZE);
    let renderer = renderer();
    let limit = glide_sdk::theme::motion::t4() * 3;
    let mut runs = Vec::new();
    for (index, page) in Page::ALL.iter().enumerate() {
        harness.message(Message::Page(index));
        runs.push((*page, run(harness.view(), &renderer, limit)));
    }
    for (page, run) in &runs {
        let state = if run.settled { "settled" } else { "live" };
        let build = run.build.as_secs_f64() * 1000.0;
        eprintln!("{page:?}: {} frames, {state}, view + layout {build:.1} ms", run.frames);
    }
    for (page, run) in runs {
        let spinning = matches!(page, Page::Feedback);
        assert_eq!(
            run.settled, !spinning,
            "only on-screen spinners keep animating; Cards has its below the fold: {page:?}"
        );
        if run.settled {
            assert!(run.frames <= 30, "{page:?} settles within t4 at 60 Hz: {} frames", run.frames);
        }
    }
}
