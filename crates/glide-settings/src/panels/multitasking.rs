use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{Column, container, row};
use iced::{Alignment, Element, Task};

use super::common::{gated, switch_row, toggle};
use super::desktop::NewWindow;
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::services::config::ConfigStore;
use crate::services::keybinds;
use crate::theme;
use crate::widgets::{self, Group, Row};
use glide_sdk::widget::mock as sketch;

mod tr {
    crate::i18n::catalogue! {
        WorkspacesOverview => "Workspaces and overview", "Простори й огляд";
        HotCorner => "Hot corner", "Гарячий кут";
        HotCornerHint => "Push the pointer into the top-left corner to open the overview", "Відведіть вказівник у верхній лівий кут, щоб відкрити огляд";
        Snapping => "Edge snapping", "Прилипання до країв";
        SnappingHint => "Drag a window to the left or right edge to fill half the screen, or to the top to maximize it", "Перетягніть вікно до лівого чи правого краю, щоб зайняти половину екрана, або вгору, щоб розгорнути його";
        SnapWindows => "Snap windows to screen edges", "Прилипання вікон до країв екрана";
        LeftHalf => "Left half", "Ліва половина";
        RightHalf => "Right half", "Права половина";
        Maximize => "Maximize", "Розгорнути";
        Shortcuts => "Shortcuts", "Сполучення клавіш";
        ShortcutsHint => "Your current bindings. Change them in Keyboard Shortcuts.", "Поточні сполучення. Змінити їх можна в розділі «Сполучення клавіш».";
        OpenOverview => "Open the overview", "Відкрити огляд";
        OpenLauncher => "Show all apps", "Показати всі програми";
        SwitchApps => "Switch between apps", "Перемикання між програмами";
        SwitchAppWindows => "Switch windows of the app", "Перемикання вікон програми";
        NextWorkspace => "Next workspace", "Наступний простір";
        PreviousWorkspace => "Previous workspace", "Попередній простір";
        Unbound => "Not set", "Не призначено";
        SwitcherHint => "Hold the modifier and press Tab to move on. Q closes the selected app, H hides it.", "Утримуйте модифікатор і натискайте Tab, щоб рухатися далі. Q закриває вибрану програму, H приховує її.";
        Fixed => "Fixed number of workspaces", "Фіксована кількість просторів";
        FixedHint => "Keep the same workspaces, empty or not; new windows open where you are", "Тримати ті самі простори, порожні чи ні; нові вікна відкриваються там, де ви";
        FixedCount => "Workspaces", "Просторів";
    }
}

pub(crate) const DEFAULT_FIXED: i64 = 4;
pub(crate) const FIXED_CHOICES: [u8; 9] = [2, 3, 4, 5, 6, 7, 8, 9, 10];

#[derive(Debug, Clone, Copy)]
pub enum Msg {
    NewWindow(NewWindow),
    OverviewLabels(bool),
    HotCorner(bool),
    EdgeSnapping(bool),
    Fixed(bool),
    FixedCount(u8),
}

#[derive(Debug, Default)]
pub struct Page;

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "workspace",
        "overview",
        "hot corner",
        "snap",
        "tiling",
        "alt+tab",
        "switcher",
        "робоч",
        "простор",
        "огляд",
        "кут",
        "прилип",
        "перемикач",
    ];

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::NewWindow(policy) => config.set_str("workspaces", "new-window", policy.key()),
            Msg::OverviewLabels(on) => config.set_bool("overview", "label", on),
            Msg::HotCorner(on) => config.set_bool("overview", "hot-corner", on),
            Msg::EdgeSnapping(on) => config.set_bool("windows", "edge-snapping", on),
            Msg::Fixed(true) => config.set_int("workspaces", "fixed", DEFAULT_FIXED),
            Msg::Fixed(false) => config.remove("workspaces", "fixed"),
            Msg::FixedCount(count) => config.set_int("workspaces", "fixed", i64::from(count)),
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        Column::new()
            .spacing(theme::SECTION_GAP)
            .push(workspaces_group(&c, config))
            .push(snapping_group(&c, config))
            .push(shortcuts_group(&c, config))
            .into()
    }
}

fn workspaces_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let fixed = config.int_or("workspaces", "fixed", 0);
    let current = if fixed > 0 { NewWindow::ActiveWorkspace } else { NewWindow::from_config(config) };
    let cards_ctx = Ctx { writable: c.writable && fixed == 0, ..*c };
    let choices = vec![
        (NewWindow::NewWorkspace, c.t(K::OwnWorkspace).to_string()),
        (NewWindow::ActiveWorkspace, c.t(K::CurrentWorkspace).to_string()),
    ];
    let mut group = Group::new(t)
        .title(c.t(tr::Key::WorkspacesOverview))
        .push(
            Row::new(t, c.t(K::NewWindows))
                .suffix(gated(cards_ctx, widgets::segmented(t, choices, current, Msg::NewWindow)))
                .stacked(t.compact)
                .build(),
        )
        .push(switch_row(*c, c.t(tr::Key::Fixed), c.t(tr::Key::FixedHint), fixed > 0, Msg::Fixed));
    if fixed > 0 {
        let selected = u8::try_from(fixed).ok();
        group = group.push(
            Row::new(t, c.t(tr::Key::FixedCount))
                .suffix(gated(*c, widgets::dropdown(t, FIXED_CHOICES.to_vec(), selected, Msg::FixedCount, 120.0)))
                .build(),
        );
    }
    group
        .push(switch_row(
            *c,
            c.t(tr::Key::HotCorner),
            c.t(tr::Key::HotCornerHint),
            config.bool_or("overview", "hot-corner", false),
            Msg::HotCorner,
        ))
        .push(switch_row(
            *c,
            c.t(K::OverviewLabels),
            c.t(K::OverviewLabelsHint),
            config.bool_or("overview", "label", true),
            Msg::OverviewLabels,
        ))
        .build()
}

fn snapping_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let on = config.bool_or("windows", "edge-snapping", true);
    let zone = |left: f32, width: f32, label: tr::Key| -> Element<'a, Msg> {
        iced::widget::column![sketch::snap_zone(t, left, width, on), text(c.t(label), TextStyle::CAP).color(t.fg_2()),]
            .spacing(6)
            .align_x(Alignment::Center)
            .into()
    };
    let zones = row![
        zone(4.0, 50.0, tr::Key::LeftHalf),
        zone(58.0, 50.0, tr::Key::RightHalf),
        zone(4.0, 104.0, tr::Key::Maximize),
    ]
    .spacing(16);
    Group::new(t)
        .title(c.t(tr::Key::Snapping))
        .description(c.t(tr::Key::SnappingHint))
        .stacked_header(t.compact)
        .push(container(zones).center_x(iced::Fill).padding([12, 8]))
        .push(Row::new(t, c.t(tr::Key::SnapWindows)).suffix(toggle(*c, on, Msg::EdgeSnapping)).build())
        .build()
}

fn shortcuts_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let binds = keybinds::current(config);
    let entries = [
        (tr::Key::OpenOverview, "toggle-overview"),
        (tr::Key::OpenLauncher, "toggle-launcher"),
        (tr::Key::SwitchApps, "switch-windows"),
        (tr::Key::SwitchAppWindows, "switch-app-windows"),
        (tr::Key::NextWorkspace, "next-workspace"),
        (tr::Key::PreviousWorkspace, "prev-workspace"),
    ];
    let mut group =
        Group::new(t).title(c.t(tr::Key::Shortcuts)).description(c.t(tr::Key::ShortcutsHint)).stacked_header(t.compact);
    for (label, action) in entries {
        let suffix: Element<'a, Msg> = match keybinds::combo_for(&binds, action) {
            Some(combo) => widgets::key_combo(t, &combo),
            None => text(c.t(tr::Key::Unbound), TextStyle::CALL).color(t.fg_3()).into(),
        };
        let mut line = Row::new(t, c.t(label)).suffix(suffix);
        if action == "switch-windows" {
            line = line.subtitle(c.t(tr::Key::SwitcherHint));
        }
        group = group.push(line.build());
    }
    group.build()
}
