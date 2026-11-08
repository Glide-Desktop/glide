use glide_sdk::theme::{SP2, TextStyle};
use glide_sdk::widget::{self as ui, Size};
use glide_sdk::window::{self, Header, Side, side_corners};
use iced::widget::{Column, Row, Space, button, column, container, image, row, scrollable, stack};
use iced::{Alignment, Color, ContentFit, Element, Fill, Padding, Radians, Rotation};

use crate::app::{App, Message, Org};
use crate::doc::write::PageSource;
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::reader::Doc;

const CELL: f32 = 100.0;

fn cell<'a>(app: &'a App, doc: &'a Doc, index: usize) -> Element<'a, Message> {
    let paint = app.paint;
    let Some(org) = doc.organizer.as_ref() else {
        return Space::new().into();
    };
    let item = &org.pages[index];
    let selected = org.selected.contains(&index);
    let (picture, size, label) = match &item.source {
        PageSource::Here(page) => {
            (doc.thumbs.get(page).cloned(), doc.size(*page), doc.opened.labels.get(*page).cloned().unwrap_or_default())
        }
        PageSource::Other(path, page) => (
            org.extra.get(&(path.clone(), *page)).cloned(),
            crate::doc::Size { w: 612.0, h: 792.0 },
            format!("{} · {}", page + 1, app.t(Key::FromOtherFile)),
        ),
    };
    let label = if item.removed { format!("{label} · {}", app.t(Key::Removed)) } else { label };
    let tall = (CELL * size.h / size.w.max(1.0)).round().min(150.0);
    let quarter = item.rotate.rem_euclid(180) == 90;
    let (width, height) = if quarter { (tall, CELL) } else { (CELL, tall) };
    let face: Element<'a, Message> = match picture {
        Some(handle) => image(handle)
            .width(CELL)
            .height(tall)
            .content_fit(ContentFit::Fill)
            .rotation(Rotation::Solid(Radians((item.rotate as f32).to_radians())))
            .into(),
        None => Space::new().width(width).height(height).into(),
    };
    let sheet = container(face).width(width).height(height).style(move |_| container::Style {
        background: Some(iced::Background::Color(Color::WHITE)),
        border: iced::Border {
            color: if selected { paint.acc_bg() } else { paint.line_2() },
            width: if selected { 2.0 } else { 1.0 },
            radius: 2.0.into(),
        },
        ..container::Style::default()
    });
    let sheet = glide_sdk::glide_iced::widgets::surface(sheet, &paint, glide_sdk::theme::Level::El1, 2.0);
    let mut layers: Vec<Element<'a, Message>> = vec![sheet];
    if selected {
        layers.push(
            container(ui::inked_tile(Icon::Check, paint.acc_bg(), Color::WHITE, 20.0))
                .align_right(Fill)
                .padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 })
                .into(),
        );
    }
    let scale = if item.removed { 0.92 } else { 1.0 };
    let body = column![
        glide_sdk::glide_iced::widgets::scale(stack(layers), scale),
        ui::text(label, TextStyle::CAP)
            .color(if selected { paint.acc_soft_fg() } else { paint.fg_2() })
            .wrapping(ui::Wrapping::None)
    ]
    .spacing(SP2)
    .align_x(Alignment::Center);
    let removed = item.removed;
    button(body)
        .on_press(Message::Org(Org::Pick(index)))
        .padding(Padding::from([8.0, 4.0]))
        .style(move |_, status| {
            let base = if selected { paint.acc_soft() } else { Color::TRANSPARENT };
            let mut style = glide_sdk::glide_iced::style::flat(paint.layered(base, status), paint.fg_2(), 12.0);
            if removed {
                style.text_color = paint.fg_3();
            }
            style
        })
        .into()
}

pub fn view<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let corner = app.window.corner();
    let Some(org) = doc.organizer.as_ref() else {
        return Space::new().into();
    };
    let subtitle = i18n::fill(app.t(Key::PagesInCopy), &[("name", &doc.name), ("n", &org.kept().to_string())]);
    let back = row![
        ui::icon(Icon::Back, 16.0, paint.fg_2()),
        ui::sized_button(&paint, app.t(Key::ToReading), ui::Tone::Ghost, Size::Small, Some(Message::Org(Org::Exit)))
    ]
    .align_y(Alignment::Center);
    let bar = Header::new(app.t(Key::OrganiseTitle))
        .subtitle(Some(subtitle))
        .labels(app.t(Key::Maximize), app.t(Key::CloseWindow))
        .corners(side_corners(Side::Top, corner))
        .leading(back)
        .trailing(ui::sized_button(
            &paint,
            app.t(Key::InsertFromFile),
            ui::Tone::Neutral,
            Size::Small,
            Some(Message::Org(Org::Insert)),
        ))
        .trailing(ui::sized_icon_button(
            &paint,
            Icon::Undo,
            app.t(Key::Undo),
            Some(Message::Org(Org::Reset)),
            false,
            Size::Medium,
        ))
        .trailing(ui::sized_button(
            &paint,
            app.t(Key::SaveCopy),
            ui::Tone::Accent,
            Size::Small,
            Some(Message::Org(Org::Save)),
        ));
    let header = window::header(&paint, &app.window, bar, Message::Window, Message::Close, 120.0);
    let columns = ((app.window.size.width - 64.0 + 12.0) / (CELL + 8.0 + 12.0)).floor().max(1.0) as usize;
    let mut grid = Column::new().spacing(20).padding(Padding { top: 24.0, right: 32.0, bottom: 96.0, left: 32.0 });
    for start in (0..org.pages.len()).step_by(columns) {
        let mut line = Row::new().spacing(12);
        for index in start..(start + columns).min(org.pages.len()) {
            line = line.push(cell(app, doc, index));
        }
        grid = grid.push(line);
    }
    let mut layers: Vec<Element<'a, Message>> = vec![
        scrollable(container(grid).center_x(Fill))
            .height(Fill)
            .on_scroll(|viewport| Message::Org(Org::Scrolled(viewport.relative_offset().y)))
            .into(),
    ];
    if let Some(bar) = actions(app, org.selected.len()) {
        layers.push(bar);
    }
    let status = ui::status_bar(
        &paint,
        vec![ui::text(app.t(Key::OrganiseHint), TextStyle::CAP).color(paint.fg_2()).into()],
        vec![ui::text(app.t(Key::OriginalKept), TextStyle::CAP).color(paint.fg_2()).into()],
        side_corners(Side::Bottom, corner),
    );
    ui::pane(paint.view_bg(), corner, column![header, container(stack(layers)).height(Fill), status]).into()
}

fn actions(app: &App, selected: usize) -> Option<Element<'_, Message>> {
    let paint = app.paint;
    if selected > 0 {
        let label = i18n::fill(app.t(Key::SelectedPages), &[("n", &selected.to_string())]);
        let bar = row![
            container(ui::text(label, TextStyle::HEAD)).padding(Padding::from([0.0, 10.0])),
            ui::tool_label(
                &paint,
                Some(ui::icon(Icon::RotateLeft, 16.0, paint.fg_2())),
                app.t(Key::Left),
                false,
                Some(Message::Org(Org::Rotate(-90)))
            ),
            ui::tool_label(
                &paint,
                Some(ui::icon(Icon::RotateRight, 16.0, paint.fg_2())),
                app.t(Key::Right),
                false,
                Some(Message::Org(Org::Rotate(90)))
            ),
            ui::tool_label(
                &paint,
                Some(ui::icon(Icon::Pages, 16.0, paint.fg_2())),
                app.t(Key::Extract),
                false,
                Some(Message::Org(Org::Extract))
            ),
            ui::tool_label(
                &paint,
                Some(ui::icon(Icon::Trash, 16.0, paint.danger())),
                app.t(Key::Delete),
                false,
                Some(Message::Org(Org::Remove))
            ),
            ui::tool_divider(&paint),
            ui::tool_button(&paint, Icon::Close, app.t(Key::ClearSelection), false, Some(Message::Org(Org::Clear))),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let float =
            glide_sdk::anim::animate(1.0, glide_sdk::anim::Speed::EXPAND, move |_| ui::floating(&paint, bar, 4))
                .from(0.0)
                .offset(|v| iced::Vector::new(0.0, 24.0 * (1.0 - v)));
        return Some(
            container(float)
                .center_x(Fill)
                .align_bottom(Fill)
                .padding(Padding { top: 0.0, right: 0.0, bottom: 20.0, left: 0.0 })
                .into(),
        );
    }
    None
}
