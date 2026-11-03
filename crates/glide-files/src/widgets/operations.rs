use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{
    Task, Tone, compact_button, elide, elide_end, elide_lines, full_button, icon_button as small, progress,
    section_heading, separator, switch_row, task_card, task_row, text,
};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Element, Fill, Padding, Point, Size};

use super::{CONTROL, SIDEBAR_WIDTH, STATUS_HEIGHT, icon};
use crate::app::{App, JOB_CARDS, Job, Message};
use crate::fs::format_bytes;
use crate::fs::jobs::Op;
use crate::i18n::{self, Key, Lang};
use crate::icons::Icon;
use crate::theme::{SP1, SP2, SP3, SP4};

pub const PANEL_WIDTH: f32 = 432.0;
const PANEL_MAX: f32 = 600.0;
const ROW: f32 = 58.0;
const HEADING: f32 = 36.0;
const TOP: f32 = 168.0;
const CARD_TEXT: f32 = SIDEBAR_WIDTH - 2.0 * SP2 - SP3 - SP2;
const CARD_TITLE: f32 = CARD_TEXT - 16.0 - SP2;
const ROW_TEXT: f32 = PANEL_WIDTH - 2.0 * SP3 - SP4 - SP1 - 3.0 * SP2 - 2.0 * CONTROL;

const fn glyph(op: Op) -> Icon {
    match op {
        Op::Copy => Icon::Copy,
        Op::Move => Icon::Move,
        Op::Delete | Op::Trash => Icon::Trash,
    }
}

fn per_second(lang: Lang) -> &'static str {
    match lang {
        Lang::Uk => "/с",
        Lang::En => "/s",
    }
}

fn detail(app: &App, job: &Job) -> String {
    let lang = app.lang;
    let progress = &job.progress;
    let mut parts = Vec::new();
    if matches!(job.op, Op::Copy | Op::Move) {
        parts.push(format!("{} {}", Key::ToPlace.text(lang), app.location_name(job.target())));
    }
    if job.launch.is_some() {
        parts.push(Key::Queued.text(lang).to_lowercase());
        return capitalised(&joined(&parts));
    }
    if job.meter.is_paused() {
        parts.push(Key::Paused.text(lang).to_lowercase());
    } else if progress.total_items == 0 {
        parts.push(Key::Preparing.text(lang).to_lowercase());
    } else {
        parts.push(format!("{} %", (progress.fraction() * 100.0).round()));
        if let Some(rate) = job.meter.rate() {
            parts.push(format!("{}{}", format_bytes(rate as u64, lang), per_second(lang)));
            if let Some(eta) = job.meter.eta(progress.total_bytes.saturating_sub(progress.bytes)) {
                parts.push(format!("~{}", i18n::duration(eta, lang)));
            }
        }
    }
    capitalised(&joined(&parts))
}

fn joined(parts: &[String]) -> String {
    parts.iter().map(|part| part.replace(' ', "\u{a0}")).collect::<Vec<_>>().join(" · ")
}

fn capitalised(line: &str) -> String {
    let mut chars = line.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

fn controls<'a>(app: &App, job: &Job) -> Vec<Element<'a, Message>> {
    let paint = super::paint(&app.theme);
    let lang = app.lang;
    let first = if job.launch.is_some() {
        small(&paint, Icon::Play, Key::StartNow.text(lang), Some(Message::JobStartNow(job.id)), false)
    } else {
        let paused = job.meter.is_paused();
        let (glyph, label) = if paused { (Icon::Play, Key::ResumeJob) } else { (Icon::Pause, Key::PauseJob) };
        small(&paint, glyph, label.text(lang), Some(Message::JobPause(job.id)), false)
    };
    vec![first, small(&paint, Icon::Close, Key::StopJob.text(lang), Some(Message::JobCancel(job.id)), false)]
}

fn fraction(job: &Job) -> Option<f32> {
    (job.progress.total_items > 0).then(|| job.progress.fraction())
}

struct Totals {
    running: usize,
    queued: usize,
    fraction: f32,
    rate: f64,
    left: Option<std::time::Duration>,
}

fn totals(app: &App) -> Totals {
    let running: Vec<&Job> = app.jobs.iter().filter(|job| job.launch.is_none()).collect();
    let fractions: f32 = running.iter().map(|job| job.progress.fraction()).sum();
    Totals {
        running: running.len(),
        queued: app.jobs.len() - running.len(),
        fraction: if running.is_empty() { 0.0 } else { fractions / running.len() as f32 },
        rate: running.iter().filter_map(|job| job.meter.rate()).sum(),
        left: running
            .iter()
            .filter_map(|job| job.meter.eta(job.progress.total_bytes.saturating_sub(job.progress.bytes)))
            .max(),
    }
}

fn summary(app: &App, totals: &Totals) -> String {
    let lang = app.lang;
    let mut parts = vec![i18n::running(totals.running, lang)];
    if totals.queued > 0 {
        parts.push(i18n::queued(totals.queued, lang));
    }
    if totals.rate >= 1.0 {
        parts.push(format!("{}{}", format_bytes(totals.rate as u64, lang), per_second(lang)));
    }
    if let Some(left) = totals.left {
        parts.push(format!("{} ~{}", Key::TimeLeft.text(lang), i18n::duration(left, lang)));
    }
    joined(&parts)
}

#[must_use]
pub fn sidebar(app: &App) -> Element<'_, Message> {
    let paint = super::paint(&app.theme);
    let lang = app.lang;
    if app.jobs.len() > JOB_CARDS {
        let totals = totals(app);
        let pause = (totals.running > 0).then_some(Message::PauseAll);
        let (glyph, label) = if app.all_paused() { (Icon::Play, Key::ResumeAll) } else { (Icon::Pause, Key::PauseAll) };
        let task = Task {
            title: i18n::operations(app.jobs.len(), lang),
            current: None,
            fraction: Some(totals.fraction),
            paused: app.all_paused(),
            detail: summary(app, &totals),
            actions: vec![small(&paint, glyph, label.text(lang), pause, false)],
            footer: Some(full_button(
                &paint,
                Key::AllOperations.text(lang),
                Tone::Tonal,
                Some(Message::ToggleOperations),
            )),
        };
        let card = glide_sdk::widget::arriving(task_card(&paint, Icon::Copy, task));
        return container(card).padding(SP2).width(Fill).into();
    }
    let cards = app.jobs.iter().map(|job| {
        let task = Task {
            title: elide_lines(&job.label, CARD_TITLE, TextStyle { weight: 630, ..TextStyle::CALL }, 2),
            current: job.progress.current.as_ref().map(|current| elide(current, CARD_TEXT, TextStyle::CAP)),
            fraction: fraction(job).or_else(|| job.launch.is_some().then_some(0.0)),
            paused: job.meter.is_paused(),
            detail: detail(app, job),
            actions: controls(app, job),
            footer: None,
        };
        glide_sdk::widget::arriving(task_card(&paint, glyph(job.op), task))
    });
    container(column(cards).spacing(SP2)).padding(SP2).width(Fill).into()
}

fn rows(app: &App) -> Vec<Element<'_, Message>> {
    let paint = super::paint(&app.theme);
    let t = &app.theme;
    let lang = app.lang;
    let totals = totals(app);
    let (toggle, label) = if app.all_paused() { (Icon::Play, Key::ResumeAll) } else { (Icon::Pause, Key::PauseAll) };
    let pause = (totals.running > 0).then_some(Message::PauseAll);
    let head = row![
        text(Key::Operations.text(lang), TextStyle::T3),
        text(app.jobs.len().to_string(), TextStyle::T3).color(t.fg_3()),
        Space::new().width(Fill),
        small(&paint, toggle, label.text(lang), pause, false),
        compact_button(&paint, Key::StopAll.text(lang), Tone::Danger, Some(Message::StopAll)),
        small(&paint, Icon::Close, Key::Close.text(lang), Some(Message::ToggleOperations), false),
    ]
    .spacing(SP2)
    .align_y(Alignment::Center);
    let overview = column![
        head,
        progress(&paint, Some(totals.fraction)),
        text(summary(app, &totals), TextStyle { size_px: 11.5, line_height_px: 16.0, ..TextStyle::NUM })
            .color(t.fg_2()),
    ]
    .spacing(SP2);
    let mut rows: Vec<Element<'_, Message>> = vec![
        container(overview).padding(Padding::from([SP3, SP3]).bottom(SP3)).into(),
        separator(&paint),
        switch_row(
            &paint,
            Key::OneAtATime.text(lang),
            Some(Key::OneAtATimeHint.text(lang)),
            app.queue,
            Some(|_| Message::ToggleQueue),
        ),
        separator(&paint),
    ];
    let running = app.jobs.iter().filter(|job| job.launch.is_none());
    let queued = app.jobs.iter().filter(|job| job.launch.is_some());
    for (key, jobs, count) in [
        (Key::Running, running.collect::<Vec<_>>(), totals.running),
        (Key::Waiting, queued.collect::<Vec<_>>(), totals.queued),
    ] {
        if count == 0 {
            continue;
        }
        let title = format!("{} · {count}", key.text(lang));
        rows.push(container(section_heading(&paint, title)).padding(Padding::from([SP2, SP3]).top(SP3)).into());
        for (index, job) in jobs.into_iter().enumerate() {
            let lead: Element<'_, Message> = if job.launch.is_some() {
                text((index + 1).to_string(), TextStyle::NUM).color(t.fg_3()).into()
            } else {
                icon(glyph(job.op), 16.0, t.fg_2())
            };
            let task = Task {
                title: elide_end(&job.label, ROW_TEXT, TextStyle { weight: 630, ..TextStyle::CALL }),
                current: None,
                fraction: if job.launch.is_some() { None } else { fraction(job).or(Some(0.0)) },
                paused: job.meter.is_paused(),
                detail: elide_end(&detail(app, job), ROW_TEXT, TextStyle::CAP),
                actions: controls(app, job),
                footer: None,
            };
            rows.push(task_row(&paint, lead, task, PANEL_WIDTH - 2.0 * SP1 - 2.0));
        }
    }
    rows
}

#[must_use]
pub fn panel(app: &App, progress: f32) -> Element<'_, Message> {
    let paint = super::paint(&app.theme.faded(progress));
    let jobs = app.jobs.len() as f32;
    let headings = 2.0 * HEADING;
    let height = (TOP + headings + jobs * ROW + 2.0 * SP2).min(PANEL_MAX);
    let window = app.window_size();
    let height = height.min(window.height - STATUS_HEIGHT - 2.0 * SP3 - super::HEADER_HEIGHT);
    let at = Point::new(SIDEBAR_WIDTH + SP2, window.height - STATUS_HEIGHT - SP2 - height);
    glide_sdk::menu::layer(
        &paint,
        rows(app),
        at,
        Size::new(PANEL_WIDTH, height),
        progress,
        app.operations_shown().then_some(Message::ToggleOperations),
    )
}
