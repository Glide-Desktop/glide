use glide_sdk::Tone;
use glide_sdk::theme::{SP2, SP3, SP4, TextStyle};
use glide_sdk::widget::{self as ui, Size};
use glide_sdk::window::{self, Header};
use iced::widget::{Column, Row, Space, column, container, image, mouse_area, scrollable, stack, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use crate::app::{App, Backdrop, Message, Viewer, ViewerEvent};
use crate::formats::Family;
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::view::{FILMSTRIP, HEADER, backdrop_colour, failure_text, note_text};

fn event(event: ViewerEvent) -> Message {
    Message::Viewer(event)
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let Some(viewer) = app.viewer.as_ref() else {
        return Space::new().into();
    };
    let paint = app.paint;
    let chrome = viewer.chrome || viewer.info;
    let mut body = Column::new();
    if chrome {
        body = body.push(header(app, viewer));
    }
    let mut middle = Row::new().push(container(stage(app, viewer, chrome)).width(Fill).height(Fill));
    if viewer.info {
        middle = middle.push(ui::vertical_separator(&paint)).push(info(app, viewer));
    }
    body = body.push(middle.height(Fill));
    ui::pane(if chrome { paint.view_bg() } else { ui::STAGE }, app.window.corner(), body).into()
}

fn header<'a>(app: &'a App, viewer: &'a Viewer) -> Element<'a, Message> {
    let paint = app.paint;
    let Some(index) = viewer.current() else {
        return Space::new().into();
    };
    let photo = &app.photos[index];
    let title = i18n::stamp(app.lang, photo.date, photo.minute, app.today);
    let folder = photo.folder.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let position = app
        .t(Key::OfTotal)
        .replace("{n}", &i18n::number(viewer.at + 1))
        .replace("{total}", &i18n::number(viewer.list.len()));
    let subtitle = if folder.is_empty() { position } else { format!("{folder} · {position}") };
    let favourite = app.prefs.is_favourite(&photo.path);
    let center = column![
        ui::text(title, TextStyle::WIN_TITLE).color(if app.window.focused { paint.fg_1() } else { paint.fg_3() }),
        ui::text(subtitle, TextStyle::CAP).color(paint.fg_2()),
    ]
    .align_x(Alignment::Center);
    let edit = viewer.loaded.is_some().then_some(event(ViewerEvent::Edit));
    let bar = Header::new("")
        .labels(app.t(Key::Maximize), app.t(Key::CloseWindow))
        .leading(ui::sized_button(
            &paint,
            format!("‹ {}", app.t(Key::Back)),
            Tone::Ghost,
            Size::Small,
            Some(event(ViewerEvent::Back)),
        ))
        .center(center)
        .trailing(ui::sized_icon_button(
            &paint,
            if favourite { Icon::HeartFilled } else { Icon::Heart },
            app.t(Key::Favourite),
            Some(Message::Favourite(vec![photo.path.clone()], !favourite)),
            favourite,
            Size::Medium,
        ))
        .trailing(ui::sized_icon_button(
            &paint,
            Icon::Panel,
            app.t(Key::Info),
            Some(event(ViewerEvent::ToggleInfo)),
            viewer.info,
            Size::Medium,
        ))
        .trailing(ui::sized_button(&paint, app.t(Key::Edit), Tone::Tonal, Size::Small, edit))
        .trailing(crate::view::more(app));
    window::header(&paint, &app.window, bar, Message::Window, Message::Close, 0.0)
}

fn picture<'a>(app: &'a App, viewer: &'a Viewer) -> Element<'a, Message> {
    let paint = app.paint;
    let Some(loaded) = &viewer.loaded else {
        if let Some(failure) = &viewer.failure {
            let photo = viewer.current().map(|index| &app.photos[index]);
            let name = photo
                .map_or_else(String::new, |photo| format!("{} · {}", photo.name, i18n::bytes(app.lang, photo.bytes)));
            let reveal = photo.map(|photo| Message::Reveal(photo.path.clone()));
            let card = column![
                ui::icon(Icon::Broken, 26.0, paint.fg_2()),
                ui::text(app.t(Key::CannotOpen), TextStyle::HEAD).color(paint.fg_1()),
                ui::text(name, TextStyle::CAP).color(paint.fg_2()),
                ui::text(failure_text(app, failure), TextStyle::CAP).color(paint.fg_2()),
                ui::sized_button(&paint, app.t(Key::ShowInFiles), Tone::Neutral, Size::Small, reveal),
            ]
            .spacing(SP2)
            .align_x(Alignment::Center)
            .max_width(360);
            return container(container(card).padding(SP4)).center(Fill).into();
        }
        return container(ui::busy_label(&paint, app.t(Key::Loading))).center(Fill).into();
    };
    let zoom = viewer.zoom.unwrap_or_else(|| app.fit_scale());
    let (w, h) = ((loaded.size.0 as f32 * zoom).round().max(1.0), (loaded.size.1 as f32 * zoom).round().max(1.0));
    let handle = loaded.frames.get(viewer.frame).map_or_else(|| loaded.handle.clone(), |(frame, _)| frame.clone());
    let shot = image(handle).width(w).height(h).content_fit(iced::ContentFit::Fill);
    let shot: Element<'_, Message> =
        if loaded.transparent || app.photos[viewer.current().unwrap_or(0)].format.family == Family::Vector {
            match backdrop_colour(viewer.backdrop) {
                Some(colour) => stack![
                    container(Space::new())
                        .width(w)
                        .height(h)
                        .style(move |_| glide_sdk::glide_iced::style::rounded(colour, 0.0)),
                    shot
                ]
                .into(),
                None => stack![ui::checkerboard(w, h), shot].into(),
            }
        } else {
            shot.into()
        };
    let (stage_w, stage_h) = app.stage();
    let content = container(shot).center_x(Length::Fixed(w.max(stage_w))).center_y(Length::Fixed(h.max(stage_h - 2.0)));
    scrollable(content)
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::new().width(6).scroller_width(6),
            horizontal: scrollable::Scrollbar::new().width(6).scroller_width(6),
        })
        .width(Fill)
        .height(Fill)
        .into()
}

fn toolbar<'a>(app: &'a App, viewer: &'a Viewer) -> Element<'a, Message> {
    let paint = app.paint;
    let zoom = viewer.zoom.unwrap_or_else(|| app.fit_scale());
    let mut bar = Row::new().spacing(2).align_y(Alignment::Center);
    if let Some(loaded) = &viewer.loaded {
        if loaded.frames.len() > 1 {
            bar = bar
                .push(ui::glass_icon(
                    &paint,
                    if viewer.playing { Icon::Pause } else { Icon::Play },
                    app.t(if viewer.playing { Key::Pause } else { Key::Play }),
                    Some(event(ViewerEvent::TogglePlay)),
                ))
                .push(ui::glass_icon(&paint, Icon::Step, app.t(Key::NextFrame), Some(event(ViewerEvent::NextFrame))))
                .push(ui::glass_label(format!("{} / {}", viewer.frame + 1, loaded.frames.len()), 64.0))
                .push(ui::glass_divider());
        }
        if loaded.pages > 1 {
            let previous = (viewer.page > 0).then(|| event(ViewerEvent::Page(viewer.page - 1)));
            let next = (viewer.page + 1 < loaded.pages).then(|| event(ViewerEvent::Page(viewer.page + 1)));
            let label = app
                .t(Key::PageOf)
                .replace("{n}", &(viewer.page + 1).to_string())
                .replace("{total}", &loaded.pages.to_string());
            bar = bar
                .push(ui::glass_icon(&paint, Icon::Back, app.t(Key::PreviousPage), previous))
                .push(ui::glass_label(label, 128.0))
                .push(ui::glass_icon(&paint, Icon::Next, app.t(Key::NextPage), next))
                .push(ui::glass_divider());
        }
        if loaded.hdr.is_some() {
            let exposure = viewer.exposure;
            let label = format!("{exposure:+.1} EV").replace('-', "−").replace('.', ",");
            bar = bar
                .push(container(ui::icon(Icon::Sun, 16.0, ui::GLASS_INK)).padding([0.0, 6.0]))
                .push(
                    container(ui::slider(&paint, -3.0..=3.0, exposure, |value| {
                        event(ViewerEvent::Exposure((value * 2.0).round() / 2.0))
                    }))
                    .width(140),
                )
                .push(ui::glass_label(label, 70.0))
                .push(ui::glass_divider());
        }
        let photo_family = viewer.current().map(|index| app.photos[index].format.family);
        if loaded.transparent || photo_family == Some(Family::Vector) {
            for (backdrop, key) in
                [(Backdrop::Checks, Key::Checker), (Backdrop::White, Key::White), (Backdrop::Dark, Key::Dark)]
            {
                let label =
                    if viewer.backdrop == backdrop { format!("• {}", app.t(key)) } else { app.t(key).to_string() };
                bar = bar.push(ui::glass_button(
                    &paint,
                    ui::text(label, TextStyle::CAP).color(ui::GLASS_INK),
                    "",
                    Some(event(ViewerEvent::Backdrop(backdrop))),
                ));
            }
            bar = bar.push(ui::glass_divider());
        }
    }
    bar = bar
        .push(ui::glass_icon(&paint, Icon::Minus, app.t(Key::ZoomOut), Some(event(ViewerEvent::ZoomOut))))
        .push(ui::glass_label(format!("{} %", (zoom * 100.0).round()), 54.0))
        .push(ui::glass_icon(&paint, Icon::Plus, app.t(Key::ZoomIn), Some(event(ViewerEvent::ZoomIn))))
        .push(ui::glass_button(
            &paint,
            ui::text(app.t(Key::Fit), TextStyle::CAP).color(ui::GLASS_INK),
            "",
            Some(event(ViewerEvent::Fit)),
        ))
        .push(ui::glass_divider())
        .push(ui::glass_icon(&paint, Icon::Fullscreen, app.t(Key::Fullscreen), Some(event(ViewerEvent::Fullscreen))));
    ui::glass(bar).into()
}

fn stage<'a>(app: &'a App, viewer: &'a Viewer, chrome: bool) -> Element<'a, Message> {
    let paint = app.paint;
    let mut layers: Vec<Element<'_, Message>> = vec![picture(app, viewer)];
    if chrome {
        if viewer.at > 0 {
            layers.push(
                container(ui::glass_circle(&paint, Icon::Back, app.t(Key::Previous), event(ViewerEvent::Step(-1))))
                    .padding(16)
                    .center_y(Fill)
                    .align_left(Fill)
                    .into(),
            );
        }
        if viewer.at + 1 < viewer.list.len() {
            layers.push(
                container(ui::glass_circle(&paint, Icon::Next, app.t(Key::Next), event(ViewerEvent::Step(1))))
                    .padding(16)
                    .center_y(Fill)
                    .align_right(Fill)
                    .into(),
            );
        }
        layers.push(container(toolbar(app, viewer)).padding(16).center_x(Fill).align_bottom(Fill).into());
        if let Some(index) = viewer.current() {
            let photo = &app.photos[index];
            let mut tag = photo.format.name.to_string();
            if let Some(note) = viewer.loaded.as_ref().and_then(|loaded| note_text(app, loaded.note)) {
                tag = format!("{tag} · {note}");
            }
            if photo.format.family != Family::Raster
                || viewer.loaded.as_ref().is_some_and(|loaded| loaded.pages > 1 || loaded.frames.len() > 1)
            {
                layers.push(container(ui::format_badge(&tag)).padding(12).align_left(Fill).align_top(Fill).into());
            }
        }
    }
    let area: Element<'_, Message> =
        mouse_area(stack(layers).width(Fill).height(Fill)).on_double_click(event(ViewerEvent::Fullscreen)).into();
    let area = ui::context_area(area, |_| event(ViewerEvent::More))
        .on_zoom(|delta| event(if delta > 0 { ViewerEvent::ZoomIn } else { ViewerEvent::ZoomOut }));
    let mut column = Column::new().push(container(area).width(Fill).height(Fill));
    if chrome {
        column = column.push(filmstrip(app, viewer));
    }
    ui::pane(ui::STAGE, 0.0, column).into()
}

fn filmstrip<'a>(app: &'a App, viewer: &'a Viewer) -> Element<'a, Message> {
    let start = viewer.at.saturating_sub(12);
    let thumbs = viewer.list.iter().enumerate().skip(start).take(25).map(|(at, &index)| {
        ui::film_thumb(app.thumb(&app.photos[index].path), at == viewer.at, event(ViewerEvent::Jump(at)))
    });
    container(Row::with_children(thumbs).spacing(4).align_y(Alignment::Center))
        .center_x(Fill)
        .center_y(Length::Fixed(FILMSTRIP))
        .into()
}

fn info<'a>(app: &'a App, viewer: &'a Viewer) -> Element<'a, Message> {
    let paint = app.paint;
    let Some(index) = viewer.current() else {
        return Space::new().into();
    };
    let photo = &app.photos[index];
    let caption = app.prefs.captions.get(&photo.path).map_or("", String::as_str);
    let input = text_input(app.t(Key::AddCaption), caption).on_input(Message::Caption);
    let mut panel =
        ui::Inspector::new(&paint, photo.name.clone()).section(app.t(Key::Caption), ui::field(&paint, input));
    let size = viewer.loaded.as_ref().map_or(photo.dims, |loaded| Some(loaded.size));
    if let Some((w, h)) = size {
        let mp = (f64::from(w) * f64::from(h) / 1_000_000.0 * 10.0).round() / 10.0;
        let mp = if app.lang == i18n::Lang::Uk { mp.to_string().replace('.', ",") } else { mp.to_string() };
        panel = panel.figure(app.t(Key::SizeLabel), format!("{w} × {h} · {mp} {}", app.t(Key::Megapixels)));
    }
    panel =
        panel.figure(app.t(Key::BytesLabel), format!("{} · {}", i18n::bytes(app.lang, photo.bytes), photo.format.name));
    if let Some(note) = viewer.loaded.as_ref().and_then(|loaded| note_text(app, loaded.note)) {
        panel = panel.fact(app.t(Key::ColourLabel), note);
    }
    let folder = photo.folder.to_string_lossy().into_owned();
    let folder = glide_config::home()
        .map_or(folder.clone(), |home| folder.replacen(&home.to_string_lossy().into_owned(), "~", 1));
    panel = panel.fact(app.t(Key::FolderLabel), folder);
    if let Some(camera) = viewer.loaded.as_ref().map(|loaded| &loaded.camera) {
        let cells: Vec<String> = [&camera.aperture, &camera.exposure, &camera.iso, &camera.focal]
            .into_iter()
            .filter_map(Clone::clone)
            .collect();
        if !cells.is_empty() || camera.model.is_some() {
            let mut block = Column::new().spacing(SP2);
            if !cells.is_empty() {
                block = block.push(ui::exif_strip(&paint, cells));
            }
            panel = panel.section(camera.model.clone().unwrap_or_default(), block);
        }
        if let Some((lat, lon)) = camera.gps {
            let place = format!("{lat:.4}°, {lon:.4}°");
            panel = panel.section(String::new(), ui::text(place, TextStyle::NUM).color(paint.fg_2()));
        }
    }
    let albums: Vec<Element<'_, Message>> = app
        .prefs
        .albums
        .iter()
        .enumerate()
        .filter(|(_, album)| album.photos.contains(&photo.path))
        .map(|(number, album)| {
            ui::chip(&paint, album.name.clone(), true, Some(Message::Go(crate::app::Place::Album(number))))
        })
        .chain(std::iter::once(ui::chip(
            &paint,
            app.t(Key::AddToAlbum).trim_end_matches('…').to_string(),
            false,
            Some(Message::AlbumMenu(vec![photo.path.clone()])),
        )))
        .collect();
    panel = panel.section(app.t(Key::Albums), Row::with_children(albums).spacing(6).wrap()).action(
        container(ui::outline(&paint, app.t(Key::ShowInFiles), Message::Reveal(photo.path.clone())))
            .width(Fill)
            .center_x(Fill),
    );
    container(scrollable(panel).height(Fill)).padding(Padding::default()).width(ui::INSPECTOR_WIDTH).height(Fill).into()
}

#[must_use]
pub fn gap() -> f32 {
    SP3 + HEADER
}
