use glide_sdk::iced::advanced::renderer::{self, Headless};
use glide_sdk::iced::advanced::widget::Tree;
use glide_sdk::iced::advanced::{Layout, Shell, clipboard, layout};
use glide_sdk::iced::{Color, Event, Font, Pixels, Rectangle, Renderer, Size, mouse, window};
use glide_sdk::{App, Harness, Look};
use glide_welcome::ui::{Message, WelcomeApp};
use glide_welcome::{Island, Network, Snap, Step};

#[test]
#[ignore = "writes preview images to /tmp for visual inspection"]
fn preview_all_steps() {
    glide_sdk::theme::motion::set_scale(0.0);
    glide_sdk::theme::use_ui_family(None);
    let cases = [(1120_u32, 760_u32), (850, 580), (720, 480)]
        .into_iter()
        .flat_map(|(width, height)| Step::ALL.into_iter().map(move |step| (width, height, step)));
    for (width, height, step) in cases {
        let size = Size::new(width as f32, height as f32);
        let look = Look { motion_scale: 0.0, ..Look::default() };
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", look, size);
        app.message(Message::ToggleSound);
        app.message(Message::GoTo(step));
        if step == Step::QuickLook {
            app.message(Message::QuickLook);
        }
        if step == Step::Snapping {
            app.message(Message::Snap(Snap::Right));
        }
        if step == Step::NightLight && width == 850 {
            app.message(Message::Temperature(3200.0));
        }
        if step == Step::Network && width == 850 {
            app.message(Message::SelectNetwork(Network::Guest));
        }
        if step == Step::IslandAndDock && width == 850 {
            app.message(Message::Island(Island::Notification));
        }
        let mut element = app.app().view(app.shell());
        let mut tree = Tree::new(&element);
        let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
        let node = element.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let bounds = Rectangle::with_size(size);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            &Event::Window(window::Event::RedrawRequested(std::time::Instant::now())),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut clipboard::Null,
            &mut shell,
            &bounds,
        );
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &app.theme(),
            &renderer::Style { text_color: Color::WHITE },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &bounds,
        );
        let Renderer::Secondary(renderer) = &mut renderer else { unreachable!() };
        let rgba = renderer.screenshot(Size::new(width, height), 1.0, Color::BLACK);
        image::save_buffer(
            format!("/tmp/glide-welcome-preview-{width}x{height}-{}.png", step.index()),
            &rgba,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .expect("preview is saved");
    }
}
