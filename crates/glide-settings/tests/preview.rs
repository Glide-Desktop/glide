use glide_sdk::iced::advanced::renderer::{self, Headless};
use glide_sdk::iced::advanced::widget::Tree;
use glide_sdk::iced::advanced::{Layout, Shell, clipboard, layout};
use glide_sdk::iced::{Color, Event, Font, Pixels, Rectangle, Renderer, Size, mouse, window};
use glide_settings::panels::{Panel, home};
use glide_settings::services::config::ConfigStore;
use glide_settings::services::wallpaper::{Gallery, Source, Thumbnail};
use glide_settings::{App, Message};

fn fixture_toolkits(app: &mut App) {
    if let Ok(state) = std::env::var("GLIDE_SETTINGS_TOOLKITS") {
        use glide_settings::panels::appearance::Msg;
        let support = match state.as_str() {
            "failed" => None,
            "ready" | "details" => Some(glide_config::toolkits::Support::default()),
            _ => panic!("unknown toolkit preview state"),
        };
        let _ = app.update(Message::Appearance(Msg::Toolkits(support)));
        if state == "details" {
            let _ = app.update(Message::Appearance(Msg::ToggleInfo));
        }
    }
}

fn fixture_bluetooth(app: &mut App) {
    use glide_settings::panels::bluetooth::Msg;
    use glide_settings::services::bluetooth::{Adapter, BluetoothState, BtDevice};

    let device = |address: &str, name: &str, icon: &str, paired, connected, battery| BtDevice {
        address: address.into(),
        name: name.into(),
        icon: icon.into(),
        paired,
        connected,
        trusted: paired,
        battery,
        ..BtDevice::default()
    };
    let state = BluetoothState {
        available: true,
        powered: true,
        alias: "laptop".into(),
        adapter: Adapter { powered: true, pairable: true, ..Adapter::default() },
        devices: vec![
            device("94:DB:00:00:00:3E", "WH-1000XM4", "audio-headphones", true, true, Some(70)),
            device("11:22:33:44:55:01", "MX Master 3S", "input-mouse", true, true, Some(45)),
            device("11:22:33:44:55:02", "Keychron K3", "input-keyboard", true, false, None),
            device("11:22:33:44:55:03", "Pixel 8", "phone", false, false, None),
            device("11:22:33:44:55:04", "JBL Flip 6", "audio-speakers", false, false, None),
        ],
        ..BluetoothState::default()
    };
    let _ = app.update(Message::Bluetooth(Msg::Loaded(0, state)));
    let _ = app.update(Message::Bluetooth(Msg::Expand("94:DB:00:00:00:3E".into())));
}

fn fixture_vpn(app: &mut App) {
    use glide_settings::panels::vpn::Msg;
    use glide_settings::services::vpn::{Vpn, VpnState};

    let connection = |name: &str, uuid: &str, kind: &str, active: bool| Vpn {
        name: name.into(),
        uuid: uuid.into(),
        kind: kind.into(),
        active,
        state: if active { "activated" } else { "disconnected" }.into(),
        ..Vpn::default()
    };
    let state = VpnState {
        available: true,
        connections: vec![
            connection("Офіс", "office", "wireguard", true),
            Vpn {
                service: "org.freedesktop.NetworkManager.openvpn".into(),
                ..connection("Домашній сервер", "home", "vpn", false)
            },
        ],
        ..VpnState::default()
    };
    let _ = app.update(Message::Vpn(Msg::Loaded(0, state)));
}

fn fixture_sound(app: &mut App) {
    use glide_settings::panels::sound::Msg;
    use glide_settings::services::audio::{AudioState, Device, Level, Stereo, Stream};

    let device = |id, name: &str, is_default| Device { id, name: name.into(), is_default };
    let state = AudioState {
        available: true,
        sinks: vec![device(1, "Bluetooth Headphones", true), device(2, "Speakers", false)],
        sources: vec![device(3, "Built-in Microphone", true)],
        output: Some(Level { volume: 0.62, muted: std::env::var_os("GLIDE_SETTINGS_SOUND_MUTED").is_some() }),
        input: Some(Level { volume: 0.8, muted: false }),
        streams: vec![Stream {
            index: 1,
            app: "Browser".into(),
            title: "Video".into(),
            icon_name: None,
            icon: std::env::var_os("GLIDE_SETTINGS_SOUND_ICON_FIXTURE")
                .map(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/glide-folio.svg")),
            volume: 0.7,
            muted: false,
        }],
        streams_supported: true,
        stereo: std::env::var_os("GLIDE_SETTINGS_SOUND_NO_STEREO")
            .is_none()
            .then_some(Stereo { left: 1.0, right: 1.0 }),
        meter: true,
        ..AudioState::default()
    };
    let _ = app.update(Message::Sound(Msg::Loaded(state)));
    if std::env::var_os("GLIDE_SETTINGS_SOUND_METER_ACTIVE").is_some() {
        let _ = app.update(Message::Sound(Msg::MeterStart));
        let _ = app.update(Message::Sound(Msg::Meter(1, glide_settings::services::audio::MeterEvent::Level(0.6))));
    }
}

fn click_sound_output(
    element: &mut glide_sdk::iced::Element<'_, Message>, tree: &mut Tree, renderer: &Renderer, node: &layout::Node,
    bounds: &Rectangle, messages: &mut Vec<Message>,
) {
    messages.clear();
    let cursor = mouse::Cursor::Available(glide_sdk::iced::Point::new(1100.0, 175.0));
    for event in [mouse::Event::ButtonPressed(mouse::Button::Left), mouse::Event::ButtonReleased(mouse::Button::Left)] {
        let mut shell = Shell::new(messages);
        element.as_widget_mut().update(
            tree,
            &Event::Mouse(event),
            Layout::new(node),
            cursor,
            renderer,
            &mut clipboard::Null,
            &mut shell,
            bounds,
        );
    }
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::Sound(glide_settings::panels::sound::Msg::Device(
            glide_settings::panels::sound::Channel::Output,
            device
        )) if device.id == 2
    )));
    messages.clear();
    let cursor = mouse::Cursor::Available(glide_sdk::iced::Point::new(950.0, 273.0));
    for event in [mouse::Event::ButtonPressed(mouse::Button::Left), mouse::Event::ButtonReleased(mouse::Button::Left)] {
        let mut shell = Shell::new(messages);
        element.as_widget_mut().update(
            tree,
            &Event::Mouse(event),
            Layout::new(node),
            cursor,
            renderer,
            &mut clipboard::Null,
            &mut shell,
            bounds,
        );
    }
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::Sound(glide_settings::panels::sound::Msg::Mute(glide_settings::panels::sound::Channel::Output, muted))
            if *muted == std::env::var_os("GLIDE_SETTINGS_SOUND_MUTED").is_none()
    )));
}

fn click_dock_autohide(
    element: &mut glide_sdk::iced::Element<'_, Message>, tree: &mut Tree, renderer: &Renderer, node: &layout::Node,
    bounds: &Rectangle, messages: &mut Vec<Message>,
) {
    messages.clear();
    let cursor = mouse::Cursor::Available(glide_sdk::iced::Point::new(1190.0, 443.0));
    for event in [mouse::Event::ButtonPressed(mouse::Button::Left), mouse::Event::ButtonReleased(mouse::Button::Left)] {
        let mut shell = Shell::new(messages);
        element.as_widget_mut().update(
            tree,
            &Event::Mouse(event),
            Layout::new(node),
            cursor,
            renderer,
            &mut clipboard::Null,
            &mut shell,
            bounds,
        );
    }
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::Desktop(glide_settings::panels::desktop::Msg::Autohide(
            glide_settings::panels::desktop::Autohide::Always
        ))
    )));
}

fn click_window_buttons(
    element: &mut glide_sdk::iced::Element<'_, Message>, tree: &mut Tree, renderer: &Renderer, node: &layout::Node,
    bounds: &Rectangle, messages: &mut Vec<Message>,
) {
    messages.clear();
    let cursor = mouse::Cursor::Available(glide_sdk::iced::Point::new(1240.0, 1585.0));
    for event in [mouse::Event::ButtonPressed(mouse::Button::Left), mouse::Event::ButtonReleased(mouse::Button::Left)] {
        let mut shell = Shell::new(messages);
        element.as_widget_mut().update(
            tree,
            &Event::Mouse(event),
            Layout::new(node),
            cursor,
            renderer,
            &mut clipboard::Null,
            &mut shell,
            bounds,
        );
    }
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::Windows(glide_settings::panels::windows::Msg::Buttons(
            glide_settings::panels::windows::ButtonLayout::CloseLeft
        ))
    )));
}

fn click_workspace_policy(
    element: &mut glide_sdk::iced::Element<'_, Message>, tree: &mut Tree, renderer: &Renderer, node: &layout::Node,
    bounds: &Rectangle, messages: &mut Vec<Message>,
) {
    messages.clear();
    let cursor = mouse::Cursor::Available(glide_sdk::iced::Point::new(1240.0, 2087.0));
    for event in [mouse::Event::ButtonPressed(mouse::Button::Left), mouse::Event::ButtonReleased(mouse::Button::Left)] {
        let mut shell = Shell::new(messages);
        element.as_widget_mut().update(
            tree,
            &Event::Mouse(event),
            Layout::new(node),
            cursor,
            renderer,
            &mut clipboard::Null,
            &mut shell,
            bounds,
        );
    }
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::Multitasking(glide_settings::panels::multitasking::Msg::NewWindow(
            glide_settings::panels::desktop::NewWindow::ActiveWorkspace
        ))
    )));
}

fn fixture_wallpaper(app: &mut App) {
    let items = [[179, 202, 237], [231, 188, 191], [122, 184, 159], [44, 47, 68], [235, 215, 167]]
        .into_iter()
        .enumerate()
        .map(|(index, [r, g, b])| {
            let (width, height) = if index % 2 == 0 { (160, 90) } else { (90, 160) };
            Thumbnail {
                path: format!("/preview/{index}.png").into(),
                handle: glide_sdk::iced::widget::image::Handle::from_rgba(
                    width,
                    height,
                    [r, g, b, 255].repeat((width * height) as usize),
                ),
                source: if index == 4 { Source::System } else { Source::Yours },
                width,
                height,
            }
        })
        .collect();
    let _ = app.update(Message::Wallpaper(glide_settings::panels::wallpaper::Msg::Wallpapers(Gallery {
        builtin: None,
        items,
    })));
    if std::env::var_os("GLIDE_SETTINGS_SELECTED_WALLPAPER").is_some() {
        let _ = app.update(Message::Wallpaper(glide_settings::panels::wallpaper::Msg::Pick("/preview/0.png".into())));
    }
}

fn check_appearance_pixels(rgba: &[u8], width: u32, height: u32) {
    let pixel = |x: usize, y: usize| &rgba[(y * width as usize + x) * 4..][..3];
    assert_ne!(pixel(470, 660), pixel(340, 660), "wallpaper must not cover the gap after it");
    assert_ne!(pixel(470, 660), pixel(490, 660), "wallpaper must not cover the gap before it");
    if std::env::var_os("GLIDE_SETTINGS_SELECTED_WALLPAPER").is_some() {
        assert_ne!(pixel(328, 660), pixel(470, 660), "selected wallpaper must have a visible frame");
    } else {
        assert_eq!(pixel(328, 660), pixel(470, 660), "unselected wallpaper must not have a frame");
    }
    assert_eq!(pixel(333, 618), pixel(470, 660), "thumbnail corner must match the surface");
    assert_ne!(pixel(340, 625), pixel(333, 618), "rounded corner must leave the thumbnail visible");
    if height >= 1600 {
        assert_eq!(pixel(925, 618), pixel(914, 660), "system thumbnail corner must match the surface");
        assert_ne!(pixel(932, 625), pixel(925, 618), "system thumbnail must remain visible inside its corner");
        if std::env::var_os("GLIDE_SETTINGS_SAMPLES").is_some() {
            assert_ne!(pixel(350, 1084), pixel(1250, 1084), "the sample well appears when requested");
        } else {
            assert_eq!(pixel(350, 1084), pixel(1250, 1084), "the sample well starts hidden");
        }
    }
}

fn check_home_pixels(rgba: &[u8], width: u32) {
    let pixel = |x: usize, y: usize| &rgba[(y * width as usize + x) * 4..][..3];
    if std::env::var_os("GLIDE_SETTINGS_HOME_LOADING").is_some() {
        assert_eq!(pixel(500, 160), pixel(1250, 160), "loading must not claim everything is fine");
    } else {
        assert_ne!(pixel(500, 160), pixel(1250, 160), "the checked state shows its attention card");
    }
}

#[test]
#[ignore = "writes a preview image for visual inspection"]
fn preview_page() {
    glide_sdk::theme::motion::set_scale(0.0);
    glide_sdk::theme::use_ui_family(None);
    let appearance = std::env::var("GLIDE_SETTINGS_APPEARANCE").unwrap_or_else(|_| "light".into());
    assert!(matches!(appearance.as_str(), "light" | "dark"));
    let scheduled = std::env::var_os("GLIDE_SETTINGS_SCHEDULE").is_some();
    let notification_schedule = std::env::var_os("GLIDE_SETTINGS_NOTIFICATION_SCHEDULE").is_some();
    let animations = std::env::var_os("GLIDE_SETTINGS_ANIMATIONS_OFF").is_none();
    let mut app = App::with_config(ConfigStore::parse(&format!(
        "[topbar]\nappearance = \"{appearance}\"\nappearance-schedule = {scheduled}\n[general]\nlanguage = \"uk\"\n[animations]\nenabled = {animations}\n[notifications]\nschedule = {notification_schedule}\n"
    )));
    let slug = std::env::var("GLIDE_SETTINGS_PANEL").unwrap_or_else(|_| "home".into());
    let panel = Panel::ALL.iter().copied().find(|panel| panel.slug() == slug).expect("unknown panel slug");
    let _ = app.update(Message::PanelSelected(panel));
    if panel == Panel::Bluetooth && std::env::var_os("GLIDE_SETTINGS_BLUETOOTH_FIXTURE").is_some() {
        fixture_bluetooth(&mut app);
    }
    if panel == Panel::Vpn && std::env::var_os("GLIDE_SETTINGS_VPN_FIXTURE").is_some() {
        fixture_vpn(&mut app);
    }
    if panel == Panel::Sound && std::env::var_os("GLIDE_SETTINGS_SOUND_FIXTURE").is_some() {
        fixture_sound(&mut app);
    }
    if panel == Panel::Home && std::env::var_os("GLIDE_SETTINGS_HOME_LOADING").is_none() {
        let _ = app.update(Message::Home(home::Msg::Loaded(1, Box::default())));
    }
    if matches!(panel, Panel::Appearance | Panel::Wallpaper) {
        fixture_wallpaper(&mut app);
        if std::env::var_os("GLIDE_SETTINGS_SAMPLES").is_some() {
            let _ = app.update(Message::Appearance(glide_settings::panels::appearance::Msg::ToggleSamples));
        }
        fixture_toolkits(&mut app);
    }
    let width = std::env::var("GLIDE_SETTINGS_WIDTH").ok().and_then(|s| s.parse().ok()).unwrap_or(1280_u32);
    let height = std::env::var("GLIDE_SETTINGS_HEIGHT").ok().and_then(|s| s.parse().ok()).unwrap_or(930_u32);
    let size = Size::new(width as f32, height as f32);
    app.follow_window(glide_sdk::window::Window { size, ..Default::default() });
    if scheduled {
        app.theme.set_appearance(if appearance == "dark" {
            glide_sdk::theme::Appearance::Dark
        } else {
            glide_sdk::theme::Appearance::Light
        });
    }
    let bounds = Rectangle::with_size(size);
    let mut element = app.view();
    let mut tree = Tree::new(&element);
    let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
    let node = element.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(size, size));
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
        &app.iced_theme(),
        &renderer::Style { text_color: app.theme.paint.fg_1() },
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &bounds,
    );
    if panel == Panel::Sound
        && std::env::var_os("GLIDE_SETTINGS_SOUND_FIXTURE").is_some()
        && width == 1500
        && height == 1800
    {
        click_sound_output(&mut element, &mut tree, &renderer, &node, &bounds, &mut messages);
    }
    if panel == Panel::Desktop && width == 1500 && height == 2000 {
        click_dock_autohide(&mut element, &mut tree, &renderer, &node, &bounds, &mut messages);
    }
    if panel == Panel::Windows && width == 1500 && height == 2000 {
        click_window_buttons(&mut element, &mut tree, &renderer, &node, &bounds, &mut messages);
    }
    if panel == Panel::Multitasking && width == 1500 && height == 2600 {
        click_workspace_policy(&mut element, &mut tree, &renderer, &node, &bounds, &mut messages);
    }
    let Renderer::Secondary(renderer) = &mut renderer else { unreachable!() };
    let rgba = renderer.screenshot(Size::new(width, height), 1.0, Color::BLACK);
    let path = std::env::var_os("GLIDE_SETTINGS_PREVIEW")
        .map_or_else(|| std::path::PathBuf::from("/tmp/opencode/glide-settings-preview.png"), std::path::PathBuf::from);
    image::save_buffer(path, &rgba, width, height, image::ColorType::Rgba8).expect("preview is saved");
    if panel == Panel::Appearance && width == 1280 && !scheduled {
        check_appearance_pixels(&rgba, width, height);
    }
    if panel == Panel::Home && width == 1280 && height >= 200 {
        check_home_pixels(&rgba, width);
    }
}

#[test]
#[ignore = "writes an open dropdown preview for visual inspection"]
fn preview_long_dropdown() {
    use glide_sdk::iced::widget::container;
    use glide_sdk::iced::{Point, Vector};

    glide_sdk::theme::motion::set_scale(0.0);
    glide_sdk::theme::use_ui_family(None);
    let appearance = std::env::var("GLIDE_SETTINGS_APPEARANCE").unwrap_or_else(|_| "light".into());
    let app = App::with_config(ConfigStore::parse(&format!("[topbar]\nappearance = \"{appearance}\"\n")));
    let paint = app.theme.paint;
    let long = "A very long custom icon theme or font family name";
    let choices = vec!["System default", long, "Another option"];
    let dropdown = glide_sdk::widget::dropdown_compact(&paint, choices, Some(long), |value| value, 180.0);
    let mut element: glide_sdk::iced::Element<'_, &str> = container(container(dropdown).width(180)).padding(40).into();
    let mut tree = Tree::new(&element);
    let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
    let width = std::env::var("GLIDE_SETTINGS_WIDTH").ok().and_then(|s| s.parse().ok()).unwrap_or(800_u32);
    let size = Size::new(width as f32, 400.0);
    let bounds = Rectangle::with_size(size);
    let node = element.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(size, size));
    let cursor = mouse::Cursor::Available(Point::new(100.0, 55.0));
    let mut messages = Vec::new();
    let mut shell = Shell::new(&mut messages);
    for event in [
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Window(window::Event::RedrawRequested(std::time::Instant::now())),
    ] {
        element.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            cursor,
            &renderer,
            &mut clipboard::Null,
            &mut shell,
            &bounds,
        );
    }
    let style = renderer::Style { text_color: paint.fg_1() };
    element.as_widget().draw(&tree, &mut renderer, &app.iced_theme(), &style, Layout::new(&node), cursor, &bounds);
    let mut overlay = element
        .as_widget_mut()
        .overlay(&mut tree, Layout::new(&node), &renderer, &bounds, Vector::ZERO)
        .expect("the dropdown opens");
    let popup = overlay.as_overlay_mut().layout(&renderer, size);
    let [card] = popup.children() else { panic!("the menu contains one surface") };
    assert!(card.size().width > 180.0, "the menu is wider than its opener");
    overlay.as_overlay().draw(&mut renderer, &app.iced_theme(), &style, Layout::new(&popup), cursor);
    let Renderer::Secondary(renderer) = &mut renderer else { unreachable!() };
    let rgba = renderer.screenshot(Size::new(width, 400), 1.0, paint.win_bg());
    let path = std::env::var_os("GLIDE_SETTINGS_PREVIEW")
        .map_or_else(|| std::path::PathBuf::from("/tmp/opencode/glide-dropdown.png"), std::path::PathBuf::from);
    image::save_buffer(path, &rgba, width, 400, image::ColorType::Rgba8).expect("preview is saved");
}
