use std::time::Instant;

use glide_sdk::theme::{SP2, TextStyle};
use glide_sdk::widget::{self as ui, GLASS_INK, STAGE};
use iced::widget::{Space, column, container, image, mouse_area, row, stack};
use iced::{Alignment, Color, ContentFit, Element, Fill, Padding};

use super::paper;
use crate::app::{App, Message, Show};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::reader::Doc;

fn timer(doc: &Doc, now: Instant) -> String {
    let Some(present) = &doc.present else {
        return String::new();
    };
    let until = present.paused.unwrap_or(now);
    let secs = until.saturating_duration_since(present.started).as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn view<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let page = doc.page();
    let size = doc.size(page);
    let zoom = doc.zoom();
    let (w, h) = (size.w * zoom, size.h * zoom);
    let slide: Element<'a, Message> = match doc.tile(page) {
        Some(handle) => image(handle).width(w).height(h).content_fit(ContentFit::Fill).into(),
        None => container(Space::new())
            .width(w)
            .height(h)
            .style(|_| glide_sdk::glide_iced::style::rounded(Color::WHITE, 4.0))
            .into(),
    };
    let step = f32::from(doc.step);
    let slide = glide_sdk::anim::animate(1.0, glide_sdk::anim::Speed::ARRIVE, move |_| slide)
        .from(0.0)
        .keyed(page as u64)
        .offset(move |v| iced::Vector::new(step * 0.25 * w * (1.0 - v), 0.0));
    let paused = doc.present.as_ref().is_some_and(|p| p.paused.is_some());
    let label = i18n::fill(app.t(Key::MatchOf), &[("n", &(page + 1).to_string()), ("total", &doc.count().to_string())]);
    let controls = ui::glass(
        row![
            ui::glass_icon(
                &paint,
                Icon::Back,
                app.t(Key::PreviousSlide),
                (page > 0).then_some(Message::Show(Show::Slide(-1)))
            ),
            ui::glass_label(label, 64.0),
            ui::glass_icon(
                &paint,
                Icon::Forward,
                app.t(Key::NextSlide),
                (page + 1 < doc.count()).then_some(Message::Show(Show::Slide(1)))
            ),
            ui::glass_divider(),
            ui::glass_label(timer(doc, app.now), 48.0),
            ui::glass_icon(
                &paint,
                Icon::Pause,
                app.t(if paused { Key::ResumeTimer } else { Key::PauseTimer }),
                Some(Message::Show(Show::Pause))
            ),
            ui::glass_divider(),
            ui::glass_button(
                &paint,
                ui::text(format!("{} Esc", app.t(Key::Exit)), TextStyle::CAP).color(Color::from_rgb8(0xf4, 0x94, 0x8c)),
                "",
                Some(Message::Show(Show::Exit))
            ),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding(4);
    let next = (page + 1 < doc.count()).then(|| {
        let size = doc.size(page + 1);
        let face: Element<'a, Message> = match doc.thumbs.get(&(page + 1)) {
            Some(handle) => image(handle.clone()).width(176).height(176.0 * size.h / size.w.max(1.0)).into(),
            None => Space::new().width(176).height(99).into(),
        };
        ui::glass(column![ui::text(app.t(Key::Next), TextStyle::CAP).color(GLASS_INK), face].spacing(6)).padding(8)
    });
    let recent = doc.present.as_ref().is_some_and(|p| p.moved.elapsed().as_secs() < 2);
    let mut layers: Vec<Element<'a, Message>> = vec![container(slide).center(Fill).into()];
    if recent {
        layers.push(
            container(controls)
                .center_x(Fill)
                .align_bottom(Fill)
                .padding(Padding { bottom: 20.0, ..Padding::ZERO })
                .into(),
        );
        if let Some(next) = next {
            layers.push(container(next).align_right(Fill).align_bottom(Fill).padding(20).into());
        }
        layers.push(
            container(ui::text(app.t(Key::ChromeHides), TextStyle::CAP).color(Color { a: 0.6, ..GLASS_INK }))
                .align_bottom(Fill)
                .padding(Padding { bottom: 30.0, left: 20.0, ..Padding::ZERO })
                .into(),
        );
    }
    let stage = container(stack(layers))
        .width(Fill)
        .height(Fill)
        .style(|_| glide_sdk::glide_iced::style::rounded(Color::from_rgb8(15, 14, 17), 0.0));
    mouse_area(stage).on_press(Message::Show(Show::Slide(1))).into()
}

pub fn bare<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let page = doc.page();
    let size = doc.size(page);
    let zoom = doc.zoom();
    let face: Element<'a, Message> = match doc.tile(page) {
        Some(handle) => image(handle).width(size.w * zoom).height(size.h * zoom).content_fit(ContentFit::Fill).into(),
        None => Space::new().width(size.w * zoom).height(size.h * zoom).into(),
    };
    let sheet =
        container(face).style(move |_| glide_sdk::glide_iced::style::rounded(paper(doc.memory.paper).face, 0.0));
    let fraction = (page + 1) as f32 / doc.count().max(1) as f32;
    let chip = ui::glass(
        row![
            ui::glass_label(
                i18n::fill(app.t(Key::MatchOf), &[("n", &(page + 1).to_string()), ("total", &doc.count().to_string())]),
                72.0
            ),
            ui::text(app.t(Key::EscPanels), TextStyle::CAP).color(GLASS_INK),
        ]
        .spacing(SP2)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([6.0, 12.0]));
    let layers: Vec<Element<'a, Message>> = vec![
        container(sheet).center(Fill).into(),
        container(ui::progress_line(&paint, fraction)).width(Fill).into(),
        container(chip).center_x(Fill).align_bottom(Fill).padding(Padding { bottom: 20.0, ..Padding::ZERO }).into(),
    ];
    let stage = container(stack(layers))
        .width(Fill)
        .height(Fill)
        .style(|_| glide_sdk::glide_iced::style::rounded(Color::from_rgb8(0x1a, 0x19, 0x1d), 0.0));
    let _ = STAGE;
    ui::context_area(stage, |_| Message::Show(Show::Bare(false)))
        .on_zoom(|by| Message::Read(crate::app::Read::ZoomWheel(by)))
        .into()
}
