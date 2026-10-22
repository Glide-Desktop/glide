pub mod chooser;
pub mod content;
pub mod dialogs;
pub mod header;
pub mod menu;
pub mod pathbar;
pub mod pointer;
pub mod sidebar;

use std::time::Instant;

use iced::widget::{Stack, column, container, float, image, mouse_area, opaque, row, rule, space, svg};
use iced::{Alignment, Color, Element, Fill, Length, Padding, Vector};

use crate::app::{App, Hover, Message, Notice, NoticeAction};
use crate::fs::FileEntry;
use crate::fs::jobs::Op;
use crate::i18n::Key;
use crate::icons::Icon;
use crate::native_icons::Art;
use crate::theme::{self, Side, ThemeState, Tone};

pub const SIDEBAR_WIDTH: f32 = 208.0;
pub const HEADER_HEIGHT: f32 = 35.0;
pub const PATHBAR_HEIGHT: f32 = 28.0;
pub const TRASH_BANNER: f32 = glide_sdk::widget::INFO_BAR;
pub const CONTROL: f32 = theme::SMALL_CONTROL;

#[must_use]
pub fn window(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let base = chooser::base(app).unwrap_or_else(|| browser(app));
    let base = pointer::context_area(base, Message::ContextMenu).on_zoom(Message::Zoom);

    let now = Instant::now();
    let motion = &app.motion;
    let mut layers: Vec<Element<'_, Message>> = vec![base.into()];
    if !app.is_maximized() {
        layers.push(glide_sdk::window::resize_edges(|direction| {
            Message::WindowResize(glide_sdk::window::Edge(direction))
        }));
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
        layers.push(dialogs::view(app, modal, motion.modal.progress(now)));
    }
    Stack::with_children(layers).into()
}

fn browser(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let mut body = column![header::view(app), divider(t)];
    if app.in_trash() {
        body = body.push(trash_banner(app)).push(divider(t));
    }
    let body = body.push(content::view(app)).push(divider(t)).push(pathbar::view(app));
    let corner = app.corner();
    container(row![
        sidebar::view(app),
        rule::vertical(1).style(move |_| theme::line(t)),
        container(body)
            .width(Fill)
            .height(Fill)
            .style(move |_| theme::rounded(t.view_bg(), theme::window_corners(Side::Right, corner))),
    ])
    .width(Fill)
    .height(Fill)
    .style(move |_| theme::rounded(t.win_bg(), corner))
    .into()
}

fn trash_banner(app: &App) -> Element<'_, Message> {
    use glide_sdk::widget::{Size, elide_end, measure};
    use glide_theme::TextStyle;
    let t = &app.theme;
    let lang = app.lang;
    let restorable = app.selected_entries().any(|e| app.trash.is_top_level(&e.path));
    let title = Key::Trash.text(lang);
    let mut buttons = vec![Key::EmptyTrash];
    if restorable {
        buttons.insert(0, Key::Restore);
    }
    let taken: f32 = buttons.iter().map(|key| Size::Small.width(key.text(lang)) + theme::SP2).sum();
    let heading = measure(title, TextStyle { weight: 600, ..TextStyle::CALL });
    let room = app.pane.width - 2.0 * theme::SP3 - heading - theme::SP2 - taken;
    let blurb = elide_end(Key::TrashBanner.text(lang), room, TextStyle::CAP);
    let actions = buttons
        .into_iter()
        .map(|key| {
            let message = match key {
                Key::Restore => Some(Message::Restore),
                _ => (app.trash_count > 0).then_some(Message::EmptyTrash),
            };
            labelled(t, key.text(lang), Tone::Neutral, message)
        })
        .collect();
    glide_sdk::widget::info_bar(&paint(t), None, title, &blurb, actions)
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
    if progress.total_items > 1 {
        parts.push(format!("{}/{}", (progress.items + 1).min(progress.total_items), progress.total_items));
    }
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
    use glide_sdk::widget::{Task, elide, elide_end, icon_button as small, task_card};
    use glide_theme::TextStyle;
    let paint = paint(&app.theme);
    let lang = app.lang;
    let cards = app.jobs.iter().map(|job| {
        let (label, glyph) = match job.op {
            Op::Copy => (Key::Copying, Icon::Copy),
            Op::Move => (Key::Moving, Icon::Move),
            Op::Delete => (Key::Deleting, Icon::Trash),
            Op::Trash => (Key::Trashing, Icon::Trash),
        };
        let progress = &job.progress;
        let paused = job.meter.is_paused();
        let toggle = if paused { (Icon::Play, Key::ResumeJob) } else { (Icon::Pause, Key::PauseJob) };
        let pause = job.launch.is_none().then_some(Message::JobPause(job.id));
        let task = Task {
            title: elide_end(label.text(lang), JOB_TITLE, TextStyle { weight: 600, ..TextStyle::CAP }),
            current: progress.current.as_ref().map(|current| elide(current, JOB_TEXT, TextStyle::CAP)),
            fraction: (progress.total_items > 0 || job.launch.is_some()).then(|| progress.fraction()),
            paused,
            detail: job_detail(app, job),
            actions: vec![
                small(&paint, toggle.0, toggle.1.text(lang), pause, false),
                small(&paint, Icon::Close, Key::StopJob.text(lang), Some(Message::JobCancel(job.id)), false),
            ],
        };
        task_card(&paint, glyph, task)
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
            container(art).padding(theme::SP2).into(),
            container(glide_sdk::widget::count_badge(&paint(t), u32::try_from(count).unwrap_or(u32::MAX)))
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
        glide_sdk::widget::bubble(
            &paint(t),
            row![icon(glyph, 12.0, t.fg_1()), glide_sdk::widget::text(key.text(lang), glide_theme::TextStyle::CAP)]
                .spacing(theme::SP1)
                .align_y(Alignment::Center),
        )
    });
    let mut ghost = column![art].spacing(theme::SP1).align_x(Alignment::Start);
    if let Some(hint) = hint {
        ghost = ghost.push(hint);
    }
    let at = drag.at;
    let (x, y) = crate::drag_icon::OFFSET;
    container(float(ghost).scale(1.0).translate(|_, _| Vector::ZERO))
        .padding(Padding { top: at.y + y as f32, left: at.x + x as f32, ..Padding::ZERO })
        .width(Fill)
        .height(Fill)
        .into()
}

fn toast<'a>(app: &'a App, notice: &'a Notice, progress: f32, live: bool) -> Element<'a, Message> {
    const RISE: f32 = 16.0;
    const SHRINK: f32 = 0.04;
    const TOAST_WIDTH: f32 = 470.0;
    let paint = paint(&app.theme.faded(progress));
    let dismiss = Message::DismissNotice(notice.id);
    let action = notice.action.as_ref().map(|action| {
        let label = match action {
            NoticeAction::Undo => Key::Undo,
            NoticeAction::Repair(_) => Key::RepairAction,
        };
        (label.text(app.lang), if live { Message::NoticeChosen(action.clone()) } else { dismiss.clone() })
    });
    let close = Some((dismiss, Key::Close.text(app.lang)));
    let card = glide_sdk::overlay::toast_card(&paint, &notice.text, None, action, close, TOAST_WIDTH);
    let (drop, scale) = if live { ((1.0 - progress) * RISE, 1.0) } else { (0.0, 1.0 - (1.0 - progress) * SHRINK) };
    let card = float(card).scale(scale).translate(move |_, _| Vector::new(0.0, drop));
    let card: Element<'a, Message> = if live {
        mouse_area(opaque(card))
            .on_enter(Message::Hover(Hover::Toast, true))
            .on_exit(Message::Hover(Hover::Toast, false))
            .into()
    } else {
        card.into()
    };
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
    glide_sdk::widget::Glyph::draw(icon, size, color)
}

impl glide_sdk::widget::Glyph for Icon {
    fn draw<'a, M: 'a>(self, side: f32, tint: Color) -> Element<'a, M> {
        let opaque = Color { a: 1.0, ..tint };
        svg(self.handle()).width(side).height(side).opacity(tint.a).style(move |_, _| theme::glyph(opaque)).into()
    }
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
    glide_sdk::widget::icon_button(&paint(t), glyph, "", message, active)
}

#[must_use]
pub fn hint<'a>(t: &ThemeState, content: impl Into<Element<'a, Message>>, label: &'a str) -> Element<'a, Message> {
    glide_sdk::widget::tooltip(&paint(t), content, label)
}

#[must_use]
pub fn paint(t: &ThemeState) -> glide_sdk::Paint {
    glide_sdk::Paint { tokens: t.tokens, alpha: t.alpha }
}

#[must_use]
pub fn check<'a>(t: &ThemeState, label: &'a str, on: bool, message: Message) -> Element<'a, Message> {
    let state = if on { glide_sdk::widget::Check::On } else { glide_sdk::widget::Check::Off };
    glide_sdk::widget::checkbox(&paint(t), label, state, Some(message))
}

#[must_use]
pub fn labelled<'a>(t: &'a ThemeState, label: &'a str, tone: Tone, message: Option<Message>) -> Element<'a, Message> {
    let tone = match tone {
        Tone::Primary => glide_sdk::Tone::Accent,
        Tone::Danger => glide_sdk::Tone::Danger,
        Tone::Ghost => glide_sdk::Tone::Ghost,
        Tone::Open => glide_sdk::Tone::Tonal,
        Tone::Neutral => glide_sdk::Tone::Neutral,
    };
    glide_sdk::widget::compact_button(&paint(t), label, tone, message)
}

const JOB_TEXT: f32 = SIDEBAR_WIDTH - 2.0 * theme::SP2 - theme::SP3 - theme::SP1;
const JOB_TITLE: f32 = JOB_TEXT - JOB_GLYPH - theme::SP2 - 2.0 * CONTROL;
const JOB_GLYPH: f32 = 16.0;

#[must_use]
pub fn text_width(text: &str, size: f32) -> f32 {
    let em = |c: char| match c {
        'i' | 'l' | 'j' | 'I' | 'і' | 'ї' | '.' | ',' | ':' | ';' | '!' | '|' | '\'' => 0.28,
        ' ' | 'f' | 't' | 'r' | 'г' | 'т' | '(' | ')' | '[' | ']' | '-' | '/' => 0.36,
        'm' | 'w' | 'M' | 'W' | 'м' | 'ш' | 'щ' | 'ж' | 'ю' | 'ф' | 'Ш' | 'Щ' | 'Ж' | 'Ю' | 'Ф' | 'Ы' => {
            0.82
        }
        '\u{200B}' => 0.0,
        c if c.is_uppercase() => 0.66,
        c if c.is_ascii_digit() => 0.56,
        _ => 0.55,
    };
    text.chars().map(|c| em(c) * size).sum()
}

fn break_runs(text: &str) -> Vec<String> {
    let mut runs = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c == '\u{200B}' && !current.is_empty() {
            runs.push(std::mem::take(&mut current));
            continue;
        }
        current.push(c);
        if c == ' ' || c == '-' {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

#[must_use]
pub fn wrapped_lines(text: &str, width: f32, size: f32) -> usize {
    let width = width.max(1.0);
    let mut lines = 1;
    let mut used = 0.0_f32;
    for run in break_runs(text) {
        let ink = text_width(run.trim_end_matches(' '), size);
        if used > 0.0 && used + ink > width {
            lines += 1;
            used = 0.0;
        }
        let mut rest = ink;
        while rest > width {
            lines += 1;
            rest -= width;
        }
        used += rest + text_width(&run, size) - ink;
    }
    lines
}

#[must_use]
pub fn fit_lines(name: &str, width: f32, size: f32, lines: usize) -> String {
    let fits = |text: &str| wrapped_lines(&content::breakable(text), width, size) <= lines;
    if fits(name) {
        return name.to_string();
    }
    let (mut low, mut high) = (FIT_FLOOR, name.chars().count());
    while high - low > 1 {
        let middle = low.midpoint(high);
        if fits(&ellipsize(name, middle)) {
            low = middle;
        } else {
            high = middle;
        }
    }
    ellipsize(name, low)
}

#[must_use]
pub fn fit(name: &str, width: f32, size: f32) -> String {
    let full = text_width(name, size);
    if full <= width {
        return name.to_string();
    }
    let count = name.chars().count();
    let mut chars = ((width.max(0.0) / (full / count as f32)) as usize + 2).min(count);
    while chars > FIT_FLOOR {
        chars -= 1;
        let shortened = ellipsize(name, chars);
        if text_width(&shortened, size) <= width {
            return shortened;
        }
    }
    ellipsize(name, FIT_FLOOR)
}

const FIT_FLOOR: usize = 6;

#[must_use]
pub fn fit_sentence(sentence: &str, width: f32, size: f32) -> String {
    if text_width(sentence, size) <= width {
        return sentence.to_string();
    }
    let mut kept: String = sentence.to_string();
    while kept.pop().is_some() {
        let trimmed = kept.trim_end();
        let shortened = format!("{trimmed}…");
        if text_width(&shortened, size) <= width || trimmed.is_empty() {
            return if trimmed.is_empty() { String::new() } else { shortened };
        }
    }
    String::new()
}

pub use glide_sdk::widget::ellipsize;
