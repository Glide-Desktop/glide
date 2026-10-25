use glide_ipc::keychain::ItemRef;
use glide_sdk::theme::{SP2, SP3, SP4, SP5, SP6, TextStyle};
use glide_sdk::widget::{self as ui, ListRow, Severity, Size, Tone};
use iced::widget::{Column, Space, column, container, row};
use iced::{Alignment, Element, Fill, Padding};

use crate::app::{App, Message};
use crate::i18n::Key;
use crate::icons::Icon;

fn item_row<'a>(app: &'a App, item: &ItemRef) -> Option<Element<'a, Message>> {
    let paint = app.paint;
    let summary = app.summary(item)?;
    let leaked = app.data.report.leaked.contains(item);
    let subtitle = format!("{} · {}", summary.username, app.vault_name(&item.vault));
    let mut suffix = row![].spacing(SP3).align_y(Alignment::Center);
    if leaked {
        suffix = suffix.push(ui::badge(&paint, app.t(Key::LeakedBadge), Severity::Danger));
    }
    suffix = suffix.push(ui::strength_meter(&paint, crate::view::strength(summary.strength)));
    suffix = suffix.push(ui::sized_button(
        &paint,
        app.t(Key::ChangePassword),
        Tone::Outline,
        Size::Small,
        Some(Message::ChangePasswordOf(item.clone())),
    ));
    Some(
        ListRow::new(paint, summary.name.clone())
            .subtitle(subtitle)
            .prefix(ui::mark_tile(&paint, &summary.name, 32.0, false))
            .suffix(suffix)
            .into(),
    )
}

fn group<'a>(app: &'a App, title: String, items: &[ItemRef]) -> Option<Element<'a, Message>> {
    let rows: Vec<Element<'a, Message>> = items.iter().filter_map(|item| item_row(app, item)).collect();
    (!rows.is_empty()).then(|| {
        column![
            container(ui::text(title, TextStyle::MICRO).color(app.paint.fg_3())).padding(Padding::from([0.0, SP2])),
            ui::list(&app.paint, rows),
        ]
        .spacing(SP2)
        .into()
    })
}

fn tiles(app: &App, counts: [usize; 4], groups: usize) -> Element<'_, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let specs = [
        (paint.danger(), Key::WeakTitle, t(Key::WeakSub).to_string()),
        (
            paint.warn(),
            Key::ReusedTitle,
            crate::i18n::plural(app.lang, groups as u64, [Key::ReusedSubOne, Key::ReusedSubFew, Key::ReusedSubMany]),
        ),
        (paint.fg_1(), Key::OldTitle, t(Key::OldSub).to_string()),
        (paint.danger(), Key::LeakedTitle, t(Key::LeakedSub).to_string()),
    ];
    let tiles = specs.into_iter().zip(counts).enumerate().map(|(index, ((ink, title, caption), count))| {
        ui::stat_choice(
            &paint,
            count.to_string(),
            ink,
            t(title),
            caption,
            app.check_filter == index,
            Message::CheckFilter(index),
        )
    });
    iced::widget::Row::with_children(tiles).spacing(SP3).into()
}

fn in_scope(app: &App, item: &ItemRef) -> bool {
    app.vault.as_ref().is_none_or(|vault| &item.vault == vault)
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let report = &app.data.report;
    let scoped = |list: &[ItemRef]| list.iter().filter(|item| in_scope(app, item)).cloned().collect::<Vec<_>>();
    let weak = scoped(&report.weak);
    let old = scoped(&report.old);
    let leaked = scoped(&report.leaked);
    let reused: Vec<Vec<ItemRef>> =
        report.reused.iter().map(|group| scoped(group)).filter(|group| group.len() > 1).collect();
    let reused_items: usize = reused.iter().map(Vec::len).sum();
    let vaults = app.data.vaults.iter().filter(|vault| vault.unlocked).count();
    let head = row![
        column![
            ui::text(t(Key::CheckTitle), TextStyle::T1),
            ui::text(
                t(Key::CheckSub).replace("{n}", &report.checked.to_string()).replace("{m}", &vaults.to_string()),
                TextStyle::CALL
            )
            .color(paint.fg_2()),
        ]
        .width(Fill),
        ui::glyph_button(
            &paint,
            Icon::Sync,
            t(Key::CheckAgain),
            Tone::Neutral,
            Size::Medium,
            Some(Message::CheckAgain)
        ),
    ]
    .align_y(Alignment::End);
    let tiles = tiles(app, [weak.len(), reused_items, old.len(), leaked.len()], reused.len());
    let banner = ui::noted_banner(
        &paint,
        Severity::Warn,
        Icon::Shield,
        "",
        t(Key::LeakNote),
        vec![ui::switch(
            &paint,
            app.prefs.leak_check,
            Some(|on| Message::Settings(crate::settings::Event::LeakCheck(on))),
        )],
    );
    let mut groups: Vec<Element<'_, Message>> = Vec::new();
    match app.check_filter {
        0 => groups.extend(group(app, t(Key::WeakTitle).to_string(), &weak)),
        1 => {
            for set in &reused {
                groups.extend(group(app, t(Key::SameIn).replace("{n}", &set.len().to_string()), set));
            }
        }
        2 => groups.extend(group(app, t(Key::OldTitle).to_string(), &old)),
        _ => groups.extend(group(app, t(Key::LeakedTitle).to_string(), &leaked)),
    }
    let list: Element<'_, Message> = if groups.is_empty() {
        container(ui::empty_state(&paint, Icon::Shield, t(Key::NoProblems), t(Key::NoProblemsBody), None))
            .center_x(Fill)
            .into()
    } else {
        let mut column = Column::with_children(groups).spacing(SP4);
        column = column.push(ui::text(t(Key::ChangeHint), TextStyle::CAP).color(paint.fg_3()));
        column.into()
    };
    let page = column![head, tiles, banner, list, Space::new().height(SP2)].spacing(SP5);
    ui::pane(
        paint.win_bg(),
        0.0,
        ui::scrollable(&paint, container(page).padding(Padding::from([SP6, SP6 + SP2])).width(Fill)).height(Fill),
    )
    .into()
}
