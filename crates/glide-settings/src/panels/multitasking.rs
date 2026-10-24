use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{Column, container, row};
use iced::{Alignment, Element, Length, Task};

use super::common::{gated, switch_row, toggle};
use super::desktop::NewWindow;
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::services::config::ConfigStore;
use crate::services::keybinds;
use crate::theme::{self, ThemeState};
use crate::widgets::{self, Group, Row};
use glide_sdk::widget::mock as sketch;

mod tr {
    crate::i18n::catalogue! {
        NewWorkspaceHint => "Each app gets a clean workspace of its own", "Кожна програма отримує власний чистий простір";
        ActiveWorkspaceHint => "Windows pile up where you are working", "Вікна збираються там, де ви працюєте";
        Overview => "Overview", "Огляд";
        OverviewHint => "Every window of the workspace side by side", "Усі вікна простору поруч одне з одним";
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
            .push(overview_group(&c, config))
            .push(snapping_group(&c, config))
            .push(shortcuts_group(&c, config))
            .into()
    }
}

fn workspaces_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let fixed = config.int_or("workspaces", "fixed", 0);
    let current = if fixed > 0 { NewWindow::ActiveWorkspace } else { NewWindow::from_config(config) };
    let card = |policy: NewWindow, heading: K, hint: tr::Key| {
        widgets::radio_card(
            t,
            workspace_sketch(&t, policy),
            c.t(heading),
            Some(c.t(hint).to_string()),
            current == policy,
            Msg::NewWindow(policy),
        )
    };
    let cards = row![
        card(NewWindow::NewWorkspace, K::OwnWorkspace, tr::Key::NewWorkspaceHint),
        card(NewWindow::ActiveWorkspace, K::CurrentWorkspace, tr::Key::ActiveWorkspaceHint),
    ]
    .spacing(12);
    let cards_ctx = Ctx { writable: c.writable && fixed == 0, ..*c };
    let mut group = Group::new(t)
        .title(c.t(K::NewWindows))
        .description(c.t(K::WorkspacesHint))
        .push(container(gated(cards_ctx, cards)).padding(8))
        .push(switch_row(*c, c.t(tr::Key::Fixed), c.t(tr::Key::FixedHint), fixed > 0, Msg::Fixed));
    if fixed > 0 {
        let selected = u8::try_from(fixed).ok();
        group = group.push(
            Row::new(t, c.t(tr::Key::FixedCount))
                .suffix(gated(*c, widgets::dropdown(t, FIXED_CHOICES.to_vec(), selected, Msg::FixedCount, 120.0)))
                .build(),
        );
    }
    group.build()
}

fn overview_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    Group::new(t)
        .title(c.t(tr::Key::Overview))
        .description(c.t(tr::Key::OverviewHint))
        .push(switch_row(
            *c,
            c.t(K::OverviewLabels),
            c.t(K::OverviewLabelsHint),
            config.bool_or("overview", "label", true),
            Msg::OverviewLabels,
        ))
        .push(
            Row::new(t, c.t(tr::Key::HotCorner))
                .subtitle(c.t(tr::Key::HotCornerHint))
                .prefix(corner_sketch(&t))
                .suffix(toggle(*c, config.bool_or("overview", "hot-corner", false), Msg::HotCorner))
                .build(),
        )
        .build()
}

fn snapping_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let on = config.bool_or("windows", "edge-snapping", true);
    let zone = |left: f32, width: f32, label: tr::Key| -> Element<'a, Msg> {
        let fill = if on { t.acc_soft() } else { t.fill_2() };
        let stroke = if on { t.acc_bg() } else { t.line_2() };
        let highlight = sketch::outlined(fill, stroke, 1.5, width, 52.0, theme::R1);
        iced::widget::column![
            sketch::screen(t, Length::Fixed(112.0), 68.0, vec![sketch::at(left, 12.0, highlight)],),
            text(c.t(label), TextStyle::CAP).color(t.fg_2()),
        ]
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
    let mut group = Group::new(t).title(c.t(tr::Key::Shortcuts)).description(c.t(tr::Key::ShortcutsHint));
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

fn workspace_sketch<'a>(t: &ThemeState, policy: NewWindow) -> Element<'a, Msg> {
    let t = *t;
    let screen = |layers: Vec<Element<'a, Msg>>| sketch::screen(t, Length::Fixed(88.0), 58.0, layers);
    match policy {
        NewWindow::NewWorkspace => row![
            screen(vec![sketch::at(10.0, 10.0, sketch::window(t, 60.0, 38.0))]),
            screen(vec![sketch::at(14.0, 12.0, sketch::window(t, 60.0, 38.0))]),
        ]
        .spacing(8)
        .into(),
        NewWindow::ActiveWorkspace => screen(vec![
            sketch::at(8.0, 8.0, sketch::window(t, 52.0, 34.0)),
            sketch::at(28.0, 18.0, sketch::window(t, 52.0, 34.0)),
        ]),
    }
}

fn corner_sketch<'a>(t: &ThemeState) -> Element<'a, Msg> {
    let t = *t;
    let accent = t.acc_bg();
    sketch::screen(t, Length::Fixed(44.0), 30.0, vec![sketch::at(1.0, 1.0, sketch::block(accent, 9.0, 9.0, theme::R1))])
}
