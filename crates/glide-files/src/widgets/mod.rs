pub mod content;
pub mod dialogs;
pub mod header;
pub mod menu;
pub mod pathbar;
pub mod pointer;
pub mod sidebar;

use std::time::Instant;

use iced::widget::{Stack, button, column, container, float, image, mouse_area, opaque, row, rule, space, svg, text};
use iced::{Alignment, Color, Element, Fill, Length, Padding, Vector, mouse};

use crate::app::{App, Edge, Message, Notice, NoticeAction};
use crate::fs::FileEntry;
use crate::fs::jobs::Op;
use crate::i18n::Key;
use crate::icons::Icon;
use crate::native_icons::Art;
use crate::theme::{self, Side, ThemeState, Tone};

pub const SIDEBAR_WIDTH: f32 = 208.0;
pub const HEADER_HEIGHT: f32 = 48.0;
pub const PATHBAR_HEIGHT: f32 = 28.0;
pub const CONTROL: f32 = theme::SMALL_CONTROL;

#[must_use]
pub fn window(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let mut body = column![header::view(app), divider(t)];
    if app.in_trash() {
        body = body.push(trash_banner(app)).push(divider(t));
    }
    let body = body.push(content::view(app)).push(divider(t));
    let body = body.push(pathbar::view(app));
    let corner = app.corner();

    let base = container(row![
        sidebar::view(app),
        rule::vertical(1).style(move |_| theme::line(t)),
        container(body)
            .width(Fill)
            .height(Fill)
            .style(move |_| theme::rounded(t.view_bg(), theme::window_corners(Side::Right, corner))),
    ])
    .width(Fill)
    .height(Fill)
    .style(move |_| theme::rounded(t.win_bg(), corner));
    let base = pointer::context_area(base, Message::ContextMenu).on_zoom(Message::Zoom);

    let now = Instant::now();
    let motion = &app.motion;
    let mut layers: Vec<Element<'_, Message>> = vec![base.into()];
    if !app.is_maximized() {
        let frame = container(space().width(Fill).height(Fill)).style(move |_| theme::window_frame(t, corner));
        layers.extend([frame.into(), resize_handles()]);
    }
    if let Some(rect) = app.band_rect() {
        let accent = t.acc_bg();
        let fill = accent.scale_alpha(glide_theme::motion::ST_PRESS);
        let marquee = container(space().width(rect.width).height(rect.height)).style(move |_| container::Style {
            background: Some(iced::Background::Color(fill)),
            border: iced::Border { color: accent, width: 1.0, radius: theme::R1.into() },
            ..container::Style::default()
        });
        layers.push(
            container(marquee)
                .padding(Padding { top: rect.y, left: rect.x, ..Padding::ZERO })
                .width(Fill)
                .height(Fill)
                .into(),
        );
    }
    if let Some(drag) = &app.drag {
        layers.push(drag_ghost(app, drag));
    }
    if let Some(notice) = app.notice.as_ref().or(motion.toast_ghost.as_ref()) {
        layers.push(toast(app, notice, motion.toast.progress(now), app.notice.is_some()));
    }
    if let Some(menu) = app.menu.as_ref().or(motion.menu_ghost.as_ref()) {
        layers.push(menu::overlay(app, menu, motion.menu.progress(now), app.menu.is_some()));
    }
    if let Some(modal) = app.modal.as_ref().or(motion.modal_ghost.as_ref()) {
        layers.push(dialogs::view(app, modal, motion.modal.progress(now), app.modal.is_some()));
    }
    Stack::with_children(layers).into()
}

fn resize_handles<'a>() -> Element<'a, Message> {
    const EDGE: f32 = 5.0;
    const CORNER: f32 = 10.0;
    let handle = |width: Length, height: Length, edge: Edge, cursor: mouse::Interaction| {
        mouse_area(space().width(width).height(height)).on_press(Message::WindowResize(edge)).interaction(cursor)
    };
    let fixed = Length::Fixed;
    let diagonal_down = mouse::Interaction::ResizingDiagonallyDown;
    let diagonal_up = mouse::Interaction::ResizingDiagonallyUp;
    let horizontal = mouse::Interaction::ResizingHorizontally;
    let vertical = mouse::Interaction::ResizingVertically;
    let band = |left: Edge, middle: Edge, right: Edge, left_cursor, right_cursor| {
        row![
            handle(fixed(CORNER), fixed(EDGE), left, left_cursor),
            handle(Fill, fixed(EDGE), middle, vertical),
            handle(fixed(CORNER), fixed(EDGE), right, right_cursor),
        ]
    };
    column![
        band(Edge::NorthWest, Edge::North, Edge::NorthEast, diagonal_down, diagonal_up),
        row![
            handle(fixed(EDGE), Fill, Edge::West, horizontal),
            space().width(Fill).height(Fill),
            handle(fixed(EDGE), Fill, Edge::East, horizontal),
        ]
        .height(Fill),
        band(Edge::SouthWest, Edge::South, Edge::SouthEast, diagonal_up, diagonal_down),
    ]
    .into()
}

fn trash_banner(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let lang = app.lang;
    let restorable = app.selected_entries().any(|e| app.trash.is_top_level(&e.path));
    let mut content = row![
        text(Key::Trash.text(lang)).size(theme::CALL).font(theme::SEMIBOLD),
        text(Key::TrashBanner.text(lang)).size(theme::CAP).color(t.fg_3()),
        space::horizontal(),
    ]
    .spacing(theme::SP2)
    .align_y(Alignment::Center);
    if restorable {
        content = content.push(labelled(t, Key::Restore.text(lang), Tone::Neutral, Some(Message::Restore)));
    }
    content = content.push(labelled(
        t,
        Key::EmptyTrash.text(lang),
        Tone::Neutral,
        (app.trash_count > 0).then_some(Message::EmptyTrash),
    ));
    container(content)
        .padding(Padding::from([theme::SP1, theme::SP4]))
        .width(Fill)
        .style(move |_| theme::surface(t.view_bg()))
        .into()
}

fn portion(value: f32) -> Length {
    Length::FillPortion((value.clamp(0.0, 1.0) * 1000.0).round().max(1.0) as u16)
}

fn progress_bar<'a>(t: &'a ThemeState, fraction: Option<f32>, paused: bool) -> Element<'a, Message> {
    const BAR: f32 = 6.0;
    let fill = if paused { t.fg_3() } else { t.acc_bg() };
    let segment = |width: Length, color: Color| {
        container(space().height(BAR)).width(width).style(move |_| theme::rounded(color, theme::R1)).into()
    };
    let parts: Vec<Element<'a, Message>> = if let Some(fraction) = fraction {
        vec![segment(portion(fraction), fill), segment(portion(1.0 - fraction), t.fill_2())]
    } else {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() % 1400);
        let start = millis as f32 / 1400.0 * 0.75;
        vec![
            segment(portion(start), t.fill_2()),
            segment(portion(0.25), fill),
            segment(portion(0.75 - start), t.fill_2()),
        ]
    };
    row(parts).width(Fill).into()
}

fn job_detail(app: &App, job: &crate::app::Job) -> String {
    let lang = app.lang;
    let progress = &job.progress;
    if job.launch.is_some() {
        return Key::Queued.text(lang).to_string();
    }
    if job.meter.is_paused() {
        return Key::Paused.text(lang).to_string();
    }
    if progress.total_items == 0 {
        return Key::Preparing.text(lang).to_string();
    }
    let mut parts = Vec::new();
    if progress.total_bytes > 0 {
        let of = Key::OfTotal.text(lang);
        parts.push(format!(
            "{} {of} {}",
            crate::fs::format_bytes(progress.bytes.min(progress.total_bytes), lang),
            crate::fs::format_bytes(progress.total_bytes, lang)
        ));
    }
    parts.push(format!("{}%", (progress.fraction() * 100.0).round()));
    if let Some(rate) = job.meter.rate() {
        let per_second = match lang {
            crate::i18n::Lang::Uk => "/с",
            crate::i18n::Lang::En => "/s",
        };
        parts.push(format!("{}{per_second}", crate::fs::format_bytes(rate as u64, lang)));
        if let Some(eta) = job.meter.eta(progress.total_bytes.saturating_sub(progress.bytes)) {
            parts.push(format!("{} {}", Key::TimeLeft.text(lang), crate::i18n::duration(eta, lang)));
        }
    }
    parts.join(" · ")
}

pub fn jobs(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let lang = app.lang;
    let small = |glyph: Icon, message: Option<Message>| {
        button(container(icon(glyph, 14.0, t.fg_2())).center(Fill))
            .on_press_maybe(message)
            .width(Length::Fixed(CONTROL))
            .height(Length::Fixed(CONTROL))
            .padding(0)
            .style(move |_, status| theme::action(t, Tone::Ghost, status))
    };
    let cards = app.jobs.iter().map(|job| {
        let (label, glyph) = match job.op {
            Op::Copy => (Key::Copying, Icon::Copy),
            Op::Move => (Key::Moving, Icon::Move),
            Op::Delete => (Key::Deleting, Icon::Close),
            Op::Trash => (Key::Trashing, Icon::Trash),
        };
        let progress = &job.progress;
        let mut title = label.text(lang).to_string();
        if progress.total_items > 1 {
            let done = (progress.items + 1).min(progress.total_items);
            title = format!("{title} {done}/{}", progress.total_items);
        }
        let paused = job.meter.is_paused();
        let fraction = (progress.total_items > 0 || job.launch.is_some()).then(|| progress.fraction());
        let toggle = if paused { Icon::Play } else { Icon::Pause };
        let head = row![
            icon(glyph, 14.0, t.acc_text()),
            text(title)
                .size(theme::CAP)
                .font(theme::SEMIBOLD)
                .color(t.fg_1())
                .wrapping(text::Wrapping::None)
                .width(Fill),
            small(toggle, job.launch.is_none().then_some(Message::JobPause(job.id))),
            small(Icon::Close, Some(Message::JobCancel(job.id))),
        ]
        .align_y(Alignment::Center);
        let mut card = column![head].spacing(theme::SP1);
        if let Some(current) = &progress.current {
            card = card.push(
                text(fit(current, JOB_TEXT, theme::MICRO))
                    .size(theme::MICRO)
                    .color(t.fg_2())
                    .wrapping(text::Wrapping::None),
            );
        }
        card = card
            .push(progress_bar(t, fraction, paused))
            .push(text(job_detail(app, job)).size(theme::MICRO).color(t.fg_3()));
        container(card)
            .padding(Padding::from([theme::SP2, theme::SP3]).right(theme::SP1))
            .width(Fill)
            .style(move |_| theme::rounded(t.fill_1(), theme::R3))
            .into()
    });
    container(column(cards).spacing(theme::SP2)).padding(theme::SP2).width(Fill).into()
}

fn drag_ghost<'a>(app: &'a App, drag: &'a crate::app::Drag) -> Element<'a, Message> {
    let t = &app.theme;
    let lang = app.lang;
    let first = drag.paths.first().and_then(|path| app.entries.iter().find(|entry| &entry.path == path));
    let art = match first {
        Some(entry) => file_icon(app, entry, 40.0),
        None => icon(Icon::File, 32.0, t.fg_2()),
    };
    let count = drag.paths.len();
    let art: Element<'_, Message> = if count > 1 {
        Stack::with_children([
            container(art).padding(6).into(),
            container(
                container(text(count.to_string()).size(theme::MICRO).font(theme::SEMIBOLD).color(t.acc_fg()))
                    .padding(Padding::from([1.0, theme::SP2]))
                    .style(move |_| theme::rounded(t.acc_bg(), theme::R_ROUND)),
            )
            .align_right(Length::Fixed(52.0))
            .into(),
        ])
        .into()
    } else {
        art
    };
    let bookmark = drag.action == crate::app::DropAction::Bookmark;
    let hint = (app.drop_target().is_some() || bookmark).then(|| {
        let (key, glyph) = match drag.action {
            crate::app::DropAction::Trash => (Key::MoveToTrash, Icon::Trash),
            crate::app::DropAction::Copy => (Key::DropCopy, Icon::Copy),
            crate::app::DropAction::Move => (Key::DropMove, Icon::Move),
            crate::app::DropAction::Bookmark => (Key::AddBookmark, Icon::Folder),
        };
        container(
            row![icon(glyph, 12.0, t.fg_1()), text(key.text(lang)).size(theme::CAP).color(t.fg_1())]
                .spacing(theme::SP1)
                .align_y(Alignment::Center),
        )
        .padding(Padding::from([theme::SP1, theme::SP2]))
        .style(move |_| theme::tooltip(t))
    });
    let mut ghost = column![art].spacing(theme::SP1).align_x(Alignment::Start);
    if let Some(hint) = hint {
        ghost = ghost.push(hint);
    }
    let at = drag.at;
    container(float(ghost).scale(1.0).translate(|_, _| Vector::ZERO))
        .padding(Padding { top: at.y + 8.0, left: at.x + 10.0, ..Padding::ZERO })
        .width(Fill)
        .height(Fill)
        .into()
}

fn toast<'a>(app: &'a App, notice: &'a Notice, progress: f32, live: bool) -> Element<'a, Message> {
    const RISE: f32 = 16.0;
    const SHRINK: f32 = 0.04;
    let t = app.theme.faded(progress);
    let mut content =
        row![text(&notice.text).size(theme::CALL).color(t.fg_1())].spacing(theme::SP1).align_y(Alignment::Center);
    let action = notice.action.as_ref().map(|action| match action {
        NoticeAction::Undo => (Key::Undo, Message::Undo),
        NoticeAction::Repair(object) => (Key::RepairAction, Message::Repair(object.clone())),
    });
    if let Some((label, message)) = action {
        content = content.push(
            button(container(text(label.text(app.lang)).size(theme::CALL).font(theme::SEMIBOLD)).center_y(Fill))
                .on_press_maybe(live.then_some(message))
                .height(Length::Fixed(CONTROL))
                .padding(Padding::from([0.0, theme::SP2]))
                .style(move |_, status| {
                    let mut style = theme::action(&t, Tone::Ghost, status);
                    style.text_color = t.acc_text();
                    style
                }),
        );
    }
    content = content.push(
        button(container(icon(Icon::Close, 14.0, t.fg_2())).center(Fill))
            .on_press_maybe(live.then_some(Message::DismissNotice(notice.id)))
            .width(Length::Fixed(CONTROL))
            .height(Length::Fixed(CONTROL))
            .padding(0)
            .style(move |_, status| theme::action(&t, Tone::Ghost, status)),
    );
    let (drop, scale) = if live { ((1.0 - progress) * RISE, 1.0) } else { (0.0, 1.0 - (1.0 - progress) * SHRINK) };
    let card = float(
        container(content)
            .padding(Padding::from(theme::SP1).left(theme::SP3))
            .max_width(520)
            .style(move |_| theme::popover(&t)),
    )
    .scale(scale)
    .translate(move |_, _| Vector::new(0.0, drop));
    let card: Element<'a, Message> = if live { opaque(card) } else { card.into() };
    container(card)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding { bottom: PATHBAR_HEIGHT + 16.0, left: SIDEBAR_WIDTH, ..Padding::ZERO })
        .into()
}

#[must_use]
pub fn icon<'a>(icon: Icon, size: f32, color: Color) -> Element<'a, Message> {
    let opaque = Color { a: 1.0, ..color };
    svg(icon.handle()).width(size).height(size).opacity(color.a).style(move |_, _| theme::glyph(opaque)).into()
}

#[must_use]
pub fn file_icon<'a>(app: &App, entry: &FileEntry, size: f32) -> Element<'a, Message> {
    let opacity = if app.is_cut(&entry.path) { 0.45 } else { 1.0 };
    if size >= 32.0
        && let Some(thumb) = app.thumbnail(entry)
    {
        return image(image::Handle::from_path(thumb))
            .width(size)
            .height(size)
            .content_fit(iced::ContentFit::Contain)
            .opacity(opacity)
            .into();
    }
    match app.art(entry) {
        Some(Art::Vector(handle)) => svg(handle).width(size).height(size).opacity(opacity).into(),
        Some(Art::Raster(handle)) => image(handle).width(size).height(size).opacity(opacity).into(),
        None => {
            let t = &app.theme;
            let mut color = if entry.is_dir { t.acc_bg() } else { t.fg_2() };
            color.a *= opacity;
            icon(entry.category.icon(), size, color)
        }
    }
}

#[must_use]
pub fn divider(t: &ThemeState) -> Element<'_, Message> {
    rule::horizontal(1).style(move |_| theme::line(t)).into()
}

#[must_use]
pub fn icon_button(t: &ThemeState, glyph: Icon, message: Option<Message>, active: bool) -> Element<'_, Message> {
    let color = match (&message, active) {
        (None, _) => t.fg_4(),
        (Some(_), true) => t.fg_1(),
        (Some(_), false) => t.fg_2(),
    };
    button(container(icon(glyph, 16.0, color)).center_x(Fill).center_y(Fill))
        .on_press_maybe(message)
        .width(Length::Fixed(CONTROL))
        .height(Length::Fixed(CONTROL))
        .padding(0)
        .style(move |_, status| theme::action(t, if active { Tone::Open } else { Tone::Ghost }, status))
        .into()
}

#[must_use]
pub fn labelled<'a>(t: &'a ThemeState, label: &'a str, tone: Tone, message: Option<Message>) -> Element<'a, Message> {
    button(container(text(label).size(SMALL_LABEL)).center_y(Fill))
        .on_press_maybe(message)
        .height(Length::Fixed(CONTROL))
        .padding(Padding::from([0.0, theme::SP3]))
        .style(move |_, status| theme::action(t, tone, status))
        .into()
}

const SMALL_LABEL: f32 = 12.5;
const JOB_TEXT: f32 = SIDEBAR_WIDTH - 2.0 * theme::SP2 - theme::SP3 - theme::SP1;

#[must_use]
pub fn text_width(text: &str, size: f32) -> f32 {
    let em = |c: char| match c {
        'i' | 'l' | 'j' | 'I' | 'і' | 'ї' | '.' | ',' | ':' | ';' | '!' | '|' | '\'' => 0.28,
        ' ' | 'f' | 't' | 'r' | 'г' | 'т' | '(' | ')' | '[' | ']' | '-' | '/' => 0.36,
        'm' | 'w' | 'M' | 'W' | 'м' | 'ш' | 'щ' | 'ж' | 'ю' | 'ф' | 'Ш' | 'Щ' | 'Ж' | 'Ю' | 'Ф' | 'Ы' => {
            0.82
        }
        c if c.is_uppercase() => 0.66,
        c if c.is_ascii_digit() => 0.56,
        _ => 0.55,
    };
    text.chars().map(|c| em(c) * size).sum()
}

#[must_use]
pub fn fit(name: &str, width: f32, size: f32) -> String {
    let full = text_width(name, size);
    if full <= width {
        return name.to_string();
    }
    let count = name.chars().count();
    let mut chars = ((width / (full / count as f32)) as usize + 2).min(count);
    while chars > 4 {
        chars -= 1;
        let shortened = ellipsize(name, chars);
        if text_width(&shortened, size) <= width {
            return shortened;
        }
    }
    ellipsize(name, 4)
}

#[must_use]
pub fn ellipsize(name: &str, max: usize) -> String {
    let count = name.chars().count();
    if count <= max {
        return name.to_string();
    }
    let tail =
        name.rfind('.').filter(|&dot| dot > 0).map_or(0, |dot| name[dot..].chars().count() + 4).clamp(3, max / 2);
    let head = max.saturating_sub(tail + 1);
    let start: String = name.chars().take(head).collect();
    let end: String = name.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}
