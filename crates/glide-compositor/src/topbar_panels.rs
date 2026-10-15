use super::{
    Anim, Canvas, Color, E_OUT, GlyphPainter, LocalRect, Logical, MemoryRenderBufferRenderElement, OpenPopup, Placed,
    Point, QS_PAD, QS_WIDTH, QsRenderContext, Rectangle, Scale, Slot, SwitchMotion, TextAlign, TextStyle, Topbar,
    TopbarTheme, band, draw_switch, local_rect, motion, push_placed, qs_geometry, text_w,
};

use crate::i18n::{Key, t};
use crate::launcher::field::{CaretMove, TextField};
use crate::power::PowerAction;
use crate::qs_net::{
    AudioOutput, BtDevice, CHARGE_LIMIT_KEY, ChargeLimit, ConnectError, NetEvent, Security, WifiNetwork,
};
use crate::system_info::{BatteryInfo, BatteryState, PowerProfile};
use glide_theme::shape::{R_ROUND, R1, R2};
use glide_theme::spacing::{SP1, SP2, SP3};
use smithay::input::keyboard::Keysym;

const DETAIL_BACK: i32 = 28;
const DETAIL_BACK_ICON: i32 = 12;
const DETAIL_HEAD_GAP: i32 = 2;
const DETAIL_SWITCH_W: i32 = 32;
const DETAIL_SWITCH_H: i32 = 18;
const DETAIL_SWITCH_THUMB: i32 = 14;
const DETAIL_SWITCH_INSET: i32 = 2;
const DETAIL_SECTION: i32 = 14;
const DETAIL_CARD: i32 = 48;
const DETAIL_CARD_GAP: i32 = 6;
const DETAIL_GROUP_GAP: i32 = SP1 + SP3 - DETAIL_CARD_GAP;
const DETAIL_SETTINGS: i32 = 32;
const DETAIL_CHEVRON: i32 = 14;
const DETAIL_DIAG: i32 = DIAG_PAD_Y * 2 + DIAG_LABEL + DIAG_GAP + DIAG_VALUE + 2;
const DETAIL_PROFILE: i32 = CARD_PAD_X * 2 + PROFILE_TITLE + 2 + PROFILE_HINT + 2;
const CARD_PAD_X: i32 = 10;
const CARD_GAP: i32 = 10;
const CARD_ICON: i32 = 24;
const CARD_GLYPH: i32 = 14;
const CARD_TRAILING: i32 = 12;
const CARD_ACTION_H: i32 = 22;
const CARD_ACTION_PAD: i32 = SP2;
const CARD_PRIMARY: TextStyle = TextStyle { size_px: 12.5, line_height_px: 16.0, weight: 600, ..TextStyle::CAP };
const CARD_ACTION: TextStyle = TextStyle { size_px: 11.0, line_height_px: 14.0, weight: 550, ..TextStyle::CAP };
const RADIO: i32 = 16;
const RADIO_DOT: i32 = 8;
const PROFILE_TITLE: i32 = 16;
const PROFILE_HINT: i32 = 14;
const PROMPT_PAD_X: i32 = 10;
const PROMPT_PAD_TOP: i32 = SP2;
const PROMPT_PAD_BOTTOM: i32 = 10;
const PROMPT_TITLE: i32 = 16;
const PROMPT_FIELD: i32 = 32;
const PROMPT_ERROR: i32 = 14;
const PROMPT_PILL: i32 = 28;
const PROMPT_REVEAL: i32 = 24;
const PROMPT_EYE: i32 = 14;
const DIAG_PAD_X: i32 = 10;
const DIAG_PAD_Y: i32 = SP2;
const DIAG_LABEL: i32 = 13;
const DIAG_GAP: i32 = 2;
const DIAG_VALUE: i32 = 18;
const DIAG_UNIT_GAP: i32 = 6;
const DIAG_PILL_H: i32 = 16;
const DIAG_PILL_PAD: i32 = 6;
const DIAG_PILL: TextStyle = TextStyle { size_px: 10.0, line_height_px: 12.0, weight: 600, ..TextStyle::CAP };
const HEALTHY_PCT: u8 = 80;
const DEFAULT_CHARGE_END: u32 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Detail {
    Wifi,
    Bluetooth,
    Battery,
    Sound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelAction {
    Open(Detail),
    Back,
    Toggle,
    Row(u16),
    FocusPassword,
    PromptReveal,
    PromptCancel,
    PromptSubmit,
    ChargeLimit,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldKey {
    Type(char),
    Backspace,
    DeleteWord,
    Delete,
    Caret(CaretMove),
    SelectAll,
    Submit,
    Cancel,
}

impl std::fmt::Debug for FieldKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Type(_) => f.write_str("Type(*)"),
            Self::Backspace => f.write_str("Backspace"),
            Self::DeleteWord => f.write_str("DeleteWord"),
            Self::Delete => f.write_str("Delete"),
            Self::Caret(movement) => write!(f, "Caret({movement:?})"),
            Self::SelectAll => f.write_str("SelectAll"),
            Self::Submit => f.write_str("Submit"),
            Self::Cancel => f.write_str("Cancel"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QsKeyMode {
    Normal,
    Detail,
    Typing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowTarget {
    Network(WifiNetwork),
    Device(BtDevice),
    Output(AudioOutput),
}

#[derive(PartialEq, Eq)]
pub enum PromptOutcome {
    Unchanged,
    Changed,
    Cancelled,
    Submit { ssid: String, password: String },
}

pub const POWER_ROW: [PowerAction; 5] =
    [PowerAction::Lock, PowerAction::Sleep, PowerAction::Restart, PowerAction::ShutDown, PowerAction::LogOut];

pub(super) const fn power_icon(action: PowerAction) -> &'static str {
    match action {
        PowerAction::Lock => "system-lock-screen",
        PowerAction::Sleep => "system-suspend",
        PowerAction::Restart => "system-reboot",
        PowerAction::ShutDown => "system-shutdown",
        PowerAction::LogOut => "system-log-out",
    }
}

pub(super) fn power_glyph(action: PowerAction) -> GlyphPainter {
    match action {
        PowerAction::Lock => crate::paint::draw_lock_glyph_over,
        PowerAction::Sleep => |b, w, h, side, _, s, c| crate::paint::draw_moon_glyph_over(b, w, h, side, s, c),
        PowerAction::Restart | PowerAction::ShutDown | PowerAction::LogOut => crate::paint::draw_power_glyph_over,
    }
}

pub(super) const fn more_slot(detail: Detail) -> Slot {
    match detail {
        Detail::Wifi => Slot::QsWifiMore,
        Detail::Bluetooth => Slot::QsBluetoothMore,
        Detail::Battery => Slot::QsBattery,
        Detail::Sound => Slot::QsVolumeMore,
    }
}

fn time_line(info: &BatteryInfo) -> String {
    let state = match info.state {
        BatteryState::Charging => Key::TbBatteryCharging,
        BatteryState::Discharging => Key::TbBatteryOnBattery,
        BatteryState::Full => Key::TbBatteryFull,
        BatteryState::NotCharging => Key::TbBatteryNotCharging,
    };
    let eta = info.minutes.and_then(|minutes| {
        let key = match info.state {
            BatteryState::Charging => Key::TbBatteryTimeToFull,
            BatteryState::Discharging => Key::TbBatteryTimeLeft,
            _ => return None,
        };
        Some(t(key).replace("{}", &crate::i18n::duration_label(minutes)))
    });
    match eta {
        Some(eta) => format!("{} \u{b7} {eta}", t(state)),
        None => t(state).to_string(),
    }
}

const fn profile_label(profile: PowerProfile) -> Key {
    match profile {
        PowerProfile::PowerSaver => Key::TbPowerSaver,
        PowerProfile::Balanced => Key::TbPowerBalanced,
        PowerProfile::Performance => Key::TbPowerPerformance,
    }
}

fn battery_state_line(status: &crate::system_info::SystemStatus) -> String {
    status.battery.as_ref().map_or_else(
        || t(if status.battery_charging { Key::TbBatteryCharging } else { Key::TbBatteryOnBattery }).to_string(),
        time_line,
    )
}

pub(super) fn battery_head_line(status: &crate::system_info::SystemStatus) -> String {
    let Some(info) = status.battery.as_ref() else {
        return battery_state_line(status);
    };
    let estimate = |key: Key| info.minutes.map(|minutes| t(key).replace("{}", &crate::i18n::duration_label(minutes)));
    let line = match info.state {
        BatteryState::Discharging => estimate(Key::TbBatteryTimeLeft),
        BatteryState::Charging => estimate(Key::TbBatteryTimeToFull),
        BatteryState::Full | BatteryState::NotCharging => None,
    };
    line.unwrap_or_else(|| time_line(info))
}

pub fn field_key(
    ctrl_only: bool, plain: bool, typed: Option<char>, pressed: impl Fn(Keysym) -> bool,
) -> Option<FieldKey> {
    if pressed(Keysym::Escape) {
        return Some(FieldKey::Cancel);
    }
    if pressed(Keysym::Return) || pressed(Keysym::KP_Enter) {
        return Some(FieldKey::Submit);
    }
    if pressed(Keysym::BackSpace) {
        return Some(if ctrl_only { FieldKey::DeleteWord } else { FieldKey::Backspace });
    }
    if pressed(Keysym::Delete) || pressed(Keysym::KP_Delete) {
        return Some(FieldKey::Delete);
    }
    let caret = [
        (Keysym::Left, CaretMove::Left, CaretMove::WordLeft),
        (Keysym::Right, CaretMove::Right, CaretMove::WordRight),
        (Keysym::Home, CaretMove::Home, CaretMove::Home),
        (Keysym::End, CaretMove::End, CaretMove::End),
    ];
    for (sym, plain_move, word_move) in caret {
        if pressed(sym) {
            return Some(FieldKey::Caret(if ctrl_only { word_move } else { plain_move }));
        }
    }
    if ctrl_only && (pressed(Keysym::a) || pressed(Keysym::A)) {
        return Some(FieldKey::SelectAll);
    }
    if !plain {
        return None;
    }
    typed.filter(|ch| !ch.is_control()).map(FieldKey::Type)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    Connect(String),
    Disconnect(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Failure {
    key: String,
    error: ConnectError,
}

struct Prompt {
    ssid: String,
    field: TextField,
    caps_lock: bool,
    reveal: bool,
}

impl Prompt {
    fn new(ssid: String) -> Self {
        Self { ssid, field: TextField::default(), caps_lock: false, reveal: false }
    }

    fn shown(&self) -> String {
        if self.reveal { self.field.text().to_string() } else { masked(self.field.text()) }
    }
}

impl std::fmt::Debug for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prompt")
            .field("ssid", &self.ssid)
            .field("len", &self.field.text().chars().count())
            .field("caps_lock", &self.caps_lock)
            .field("reveal", &self.reveal)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowStatus {
    Idle,
    Connecting,
    Disconnecting,
    Failed(ConnectError),
}

#[derive(Debug, Default)]
pub(super) struct DetailPanel {
    pub(super) view: Option<Detail>,
    pub(super) shown: Option<Detail>,
    pub(super) slide: Anim,
    wifi: Vec<WifiNetwork>,
    wifi_ready: bool,
    devices: Vec<BtDevice>,
    devices_ready: bool,
    outputs: Vec<AudioOutput>,
    outputs_ready: bool,
    inputs: Option<Vec<AudioOutput>>,
    charge: Option<ChargeLimit>,
    pending: Option<Pending>,
    failure: Option<Failure>,
    prompt: Option<Prompt>,
    pub(super) rev: u64,
}

impl DetailPanel {
    const fn bump(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }

    fn status_for(&self, key: &str) -> RowStatus {
        match &self.pending {
            Some(Pending::Connect(pending)) if pending == key => return RowStatus::Connecting,
            Some(Pending::Disconnect(pending)) if pending == key => {
                return RowStatus::Disconnecting;
            }
            _ => {}
        }
        match &self.failure {
            Some(failure) if failure.key == key => RowStatus::Failed(failure.error),
            _ => RowStatus::Idle,
        }
    }

    pub(super) fn slide_progress(&self) -> f64 {
        self.slide.value().clamp(0.0, 1.0)
    }

    pub(super) fn is_animating(&self) -> bool {
        self.slide.is_animating()
    }

    pub(super) fn reset(&mut self) {
        self.view = None;
        self.slide.jump(0.0);
        self.prompt = None;
        self.failure = None;
        self.bump();
    }

    pub(super) fn drop_prompt(&mut self) {
        if self.prompt.take().is_some() {
            self.bump();
        }
    }

    fn failed(&self, index: usize) -> bool {
        let Some(network) = self.wifi.get(index) else {
            return false;
        };
        self.failure.as_ref().is_some_and(|failure| failure.key == network.ssid)
    }

    pub(super) fn active_output(&self) -> Option<&AudioOutput> {
        self.outputs.iter().find(|output| output.active)
    }

    pub(super) fn active_input(&self) -> Option<&AudioOutput> {
        self.inputs.iter().flatten().find(|input| input.active)
    }

    pub(super) const fn devices_ready(&self, drawer: super::Drawer) -> bool {
        match drawer {
            super::Drawer::Output => self.outputs_ready,
            super::Drawer::Input => self.inputs.is_some(),
        }
    }

    pub(super) fn devices(&self, drawer: super::Drawer) -> &[AudioOutput] {
        match drawer {
            super::Drawer::Output => &self.outputs,
            super::Drawer::Input => self.inputs.as_deref().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ItemKind {
    Section(Key),
    Message(Key),
    Network(usize),
    Device(usize),
    Output(usize),
    Input(usize),
    Prompt(usize),
    Diag(usize),
    Profile(PowerProfile),
    ChargeLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DetailItem {
    pub(super) kind: ItemKind,
    pub(super) rect: LocalRect,
    pub(super) slot: Option<Slot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DetailGeometry {
    pub(super) back: LocalRect,
    pub(super) toggle: LocalRect,
    pub(super) title_left: i32,
    pub(super) title_right: i32,
    pub(super) body: LocalRect,
    pub(super) divider_bottom: i32,
    pub(super) settings: LocalRect,
}

pub(super) fn detail_geometry(height: i32, radio: bool) -> DetailGeometry {
    let row_w = QS_WIDTH - QS_PAD * 2;
    let back = local_rect(QS_PAD, QS_PAD, DETAIL_BACK, DETAIL_BACK);
    let toggle = local_rect(
        QS_WIDTH - QS_PAD - DETAIL_SWITCH_W,
        QS_PAD + (DETAIL_BACK - DETAIL_SWITCH_H) / 2,
        DETAIL_SWITCH_W,
        DETAIL_SWITCH_H,
    );
    let trailing = if radio { DETAIL_SWITCH_W } else { super::QS_READOUT_W };
    let body_top = QS_PAD + DETAIL_BACK + DETAIL_HEAD_GAP + SP3;
    let settings = local_rect(QS_PAD, height - QS_PAD - DETAIL_SETTINGS, row_w, DETAIL_SETTINGS);
    let divider_bottom = settings.loc.y - SP1 - 1;
    let body = local_rect(QS_PAD, body_top, row_w, (divider_bottom - SP1 - body_top).max(0));
    DetailGeometry {
        back,
        toggle,
        title_left: back.loc.x + back.size.w + SP2,
        title_right: QS_WIDTH - QS_PAD - trailing - SP2,
        body,
        divider_bottom,
        settings,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PromptParts {
    pub(super) title: LocalRect,
    pub(super) field: LocalRect,
    pub(super) reveal: LocalRect,
    pub(super) error: Option<LocalRect>,
    pub(super) cancel: LocalRect,
    pub(super) connect: LocalRect,
}

const fn prompt_height(failed: bool) -> i32 {
    let error = if failed { PROMPT_ERROR + SP2 } else { 0 };
    PROMPT_PAD_TOP + PROMPT_TITLE + SP2 + PROMPT_FIELD + SP2 + error + PROMPT_PILL + PROMPT_PAD_BOTTOM
}

pub(super) fn prompt_parts(rect: LocalRect, failed: bool, (cancel_w, connect_w): (i32, i32)) -> PromptParts {
    let (left, right) = (rect.loc.x + PROMPT_PAD_X, rect.loc.x + rect.size.w - PROMPT_PAD_X);
    let width = right - left;
    let mut y = rect.loc.y + PROMPT_PAD_TOP;
    let title = local_rect(left, y, width, PROMPT_TITLE);
    y += PROMPT_TITLE + SP2;
    let field = local_rect(left, y, width, PROMPT_FIELD);
    let reveal_x = field.loc.x + field.size.w - SP1 - PROMPT_REVEAL;
    let reveal = local_rect(reveal_x, y + (PROMPT_FIELD - PROMPT_REVEAL) / 2, PROMPT_REVEAL, PROMPT_REVEAL);
    y += PROMPT_FIELD + SP2;
    let error = failed.then(|| {
        let error = local_rect(left, y, width, PROMPT_ERROR);
        y += PROMPT_ERROR + SP2;
        error
    });
    let connect = local_rect(right - connect_w, y, connect_w, PROMPT_PILL);
    let cancel = local_rect(connect.loc.x - SP1 - cancel_w, y, cancel_w, PROMPT_PILL);
    PromptParts { title, field, reveal, error, cancel, connect }
}

struct Column<'a> {
    left: i32,
    width: i32,
    y: i32,
    bottom: i32,
    items: &'a mut Vec<DetailItem>,
}

impl Column<'_> {
    const fn fits(&self, height: i32) -> bool {
        self.y + height <= self.bottom
    }

    fn push(&mut self, kind: ItemKind, height: i32, slot: Option<Slot>) -> bool {
        if !self.fits(height) {
            return false;
        }
        self.items.push(DetailItem { kind, rect: local_rect(self.left, self.y, self.width, height), slot });
        self.y += height;
        true
    }

    fn section(&mut self, key: Key) -> bool {
        let pushed = self.push(ItemKind::Section(key), DETAIL_SECTION, None);
        if pushed {
            self.y += SP3;
        }
        pushed
    }

    fn card(&mut self, kind: ItemKind, slot: Option<Slot>) -> bool {
        let pushed = self.push(kind, DETAIL_CARD, slot);
        if pushed {
            self.y += DETAIL_CARD_GAP;
        }
        pushed
    }

    fn message(&mut self, key: Key) {
        self.card(ItemKind::Message(key), None);
    }

    const fn end_group(&mut self) {
        self.y += DETAIL_GROUP_GAP;
    }
}

fn row_slot(index: usize) -> Option<Slot> {
    u16::try_from(index).ok().map(Slot::QsDetailRow)
}

fn push_network(column: &mut Column<'_>, panel: &DetailPanel, index: usize) -> bool {
    let Some(network) = panel.wifi.get(index) else {
        return false;
    };
    let prompt = panel.prompt.as_ref().filter(|prompt| prompt.ssid == network.ssid);
    let failed = panel.failure.as_ref().is_some_and(|failure| failure.key == network.ssid);
    let extra = prompt.map_or(0, |_| prompt_height(failed));
    if !column.fits(DETAIL_CARD + extra) {
        return false;
    }
    column.card(ItemKind::Network(index), row_slot(index));
    if prompt.is_some() {
        column.y -= DETAIL_CARD_GAP;
        column.push(ItemKind::Prompt(index), extra, None);
        column.y += DETAIL_CARD_GAP;
    }
    true
}

fn wifi_items(column: &mut Column<'_>, panel: &DetailPanel, status: &crate::system_info::SystemStatus) {
    if !status.wifi_enabled {
        return column.message(Key::QsWifiIsOff);
    }
    if panel.wifi.is_empty() {
        return column.message(if panel.wifi_ready { Key::QsNoNetworks } else { Key::QsSearching });
    }
    let (connected, available): (Vec<usize>, Vec<usize>) =
        (0..panel.wifi.len()).partition(|index| panel.wifi[*index].connected);
    if !connected.is_empty() && column.section(Key::QsConnectedNetwork) {
        for index in connected {
            push_network(column, panel, index);
        }
        column.end_group();
    }
    if !available.is_empty() && column.section(Key::QsAvailableNetworks) {
        for index in available {
            if !push_network(column, panel, index) {
                break;
            }
        }
    }
}

fn bluetooth_items(column: &mut Column<'_>, panel: &DetailPanel, status: &crate::system_info::SystemStatus) {
    if !status.bluetooth_on {
        return column.message(Key::QsBluetoothIsOff);
    }
    if panel.devices.is_empty() {
        return column.message(if panel.devices_ready { Key::QsNoDevices } else { Key::QsSearching });
    }
    let (connected, paired): (Vec<usize>, Vec<usize>) =
        (0..panel.devices.len()).partition(|index| panel.devices[*index].connected);
    for (key, group) in [(Key::QsConnectedDevices, connected), (Key::QsPairedDevices, paired)] {
        if group.is_empty() || !column.section(key) {
            continue;
        }
        for index in group {
            if !column.card(ItemKind::Device(index), row_slot(index)) {
                return;
            }
        }
        column.end_group();
    }
}

fn sound_items(column: &mut Column<'_>, panel: &DetailPanel) {
    let groups = [
        (Key::QsOutputDevice, super::Drawer::Output, Key::QsNoOutputs),
        (Key::QsInputDevice, super::Drawer::Input, Key::QsNoInputs),
    ];
    for (section, drawer, empty) in groups {
        if !column.section(section) {
            return;
        }
        let devices = panel.devices(drawer);
        if devices.is_empty() {
            column.message(if panel.devices_ready(drawer) { empty } else { Key::QsSearching });
        }
        for index in 0..devices.len() {
            let (kind, slot) = match drawer {
                super::Drawer::Output => (ItemKind::Output(index), row_slot(index)),
                super::Drawer::Input => {
                    (ItemKind::Input(index), u16::try_from(index).ok().map(|i| Slot::QsDrawerItem(drawer, i)))
                }
            };
            if !column.card(kind, slot) {
                return;
            }
        }
        column.end_group();
    }
}

fn battery_items(column: &mut Column<'_>, panel: &DetailPanel, status: &crate::system_info::SystemStatus) {
    battery_health_items(column, status);
    if panel.charge.is_some() {
        column.y += SP1;
        column.card(ItemKind::ChargeLimit, Some(Slot::QsChargeLimit));
    }
}

fn battery_health_items(column: &mut Column<'_>, status: &crate::system_info::SystemStatus) {
    if column.section(Key::QsBatteryTelemetry) {
        let half = (column.width - SP2) / 2;
        for pair in 0..2 {
            if !column.fits(DETAIL_DIAG) {
                return;
            }
            for side in 0..2 {
                let x = column.left + side * (half + SP2);
                let rect = local_rect(x, column.y, half, DETAIL_DIAG);
                let index = usize::try_from(pair * 2 + side).unwrap_or(0);
                column.items.push(DetailItem { kind: ItemKind::Diag(index), rect, slot: None });
            }
            column.y += DETAIL_DIAG + SP2;
        }
        column.y -= SP2;
        column.end_group();
        column.y += DETAIL_CARD_GAP;
    }
    if status.power_profile.is_some() && column.section(Key::QsPerformanceProfile) {
        for profile in super::QS_PROFILES {
            if !column.fits(DETAIL_PROFILE) {
                return;
            }
            column.push(ItemKind::Profile(profile), DETAIL_PROFILE, Some(Slot::QsProfile(profile)));
            column.y += DETAIL_CARD_GAP;
        }
    }
}

pub(super) fn detail_items(
    panel: &DetailPanel, detail: Detail, status: &crate::system_info::SystemStatus, body: LocalRect,
) -> Vec<DetailItem> {
    let mut items = Vec::new();
    let mut column = Column {
        left: body.loc.x,
        width: body.size.w,
        y: body.loc.y,
        bottom: body.loc.y + body.size.h,
        items: &mut items,
    };
    match detail {
        Detail::Wifi => wifi_items(&mut column, panel, status),
        Detail::Bluetooth => bluetooth_items(&mut column, panel, status),
        Detail::Sound => sound_items(&mut column, panel),
        Detail::Battery => battery_items(&mut column, panel, status),
    }
    items
}

fn radio_on(detail: Detail, status: &crate::system_info::SystemStatus) -> bool {
    match detail {
        Detail::Wifi => status.wifi_enabled,
        Detail::Bluetooth => status.bluetooth_on,
        Detail::Battery => status.battery_pct.is_some(),
        Detail::Sound => true,
    }
}

const fn has_radio(detail: Detail) -> bool {
    !matches!(detail, Detail::Battery | Detail::Sound)
}

impl Topbar {
    #[must_use]
    pub fn detail(&self) -> Option<Detail> {
        if self.open_popup == Some(OpenPopup::QuickSettings) { self.qs_panel.view } else { None }
    }

    pub fn open_detail(&mut self, detail: Detail) -> bool {
        if self.open_popup != Some(OpenPopup::QuickSettings) || self.qs_panel.view == Some(detail) {
            return false;
        }
        let keyboard = self.focused.is_some();
        self.forget_power();
        self.forget_drawer();
        self.qs_panel.view = Some(detail);
        self.qs_panel.shown = Some(detail);
        self.qs_panel.slide.retarget(1.0, motion::t2(), E_OUT);
        self.qs_panel.prompt = None;
        self.qs_panel.failure = None;
        self.qs_panel.bump();
        self.set_focused(keyboard.then_some(Slot::QsBack));
        self.touch();
        true
    }

    pub fn close_detail(&mut self) -> bool {
        let Some(detail) = self.qs_panel.view.take() else {
            return false;
        };
        let keyboard = self.focused.is_some();
        self.qs_panel.slide.retarget(0.0, motion::t2(), E_OUT);
        self.qs_panel.prompt = None;
        self.qs_panel.bump();
        self.set_focused(keyboard.then_some(more_slot(detail)));
        self.touch();
        true
    }

    #[must_use]
    pub fn detail_row(&self, index: u16) -> Option<RowTarget> {
        let index = usize::from(index);
        match self.detail()? {
            Detail::Wifi => self.qs_panel.wifi.get(index).cloned().map(RowTarget::Network),
            Detail::Bluetooth => self.qs_panel.devices.get(index).cloned().map(RowTarget::Device),
            Detail::Sound => self.qs_panel.outputs.get(index).cloned().map(RowTarget::Output),
            Detail::Battery => None,
        }
    }

    fn device_slot_for(&self, drawer: super::Drawer, key: &str) -> Option<Slot> {
        let index = self.qs_panel.devices(drawer).iter().position(|device| device.name == key)?;
        Some(Slot::QsDrawerItem(drawer, u16::try_from(index).ok()?))
    }

    fn row_slot_for(&self, key: &str) -> Option<Slot> {
        let row = |index: usize| u16::try_from(index).ok().map(Slot::QsDetailRow);
        match self.qs_panel.view {
            Some(Detail::Wifi) => row(self.qs_panel.wifi.iter().position(|n| n.ssid == key)?),
            Some(Detail::Bluetooth) => row(self.qs_panel.devices.iter().position(|d| d.mac == key)?),
            Some(Detail::Sound) => self
                .qs_panel
                .outputs
                .iter()
                .position(|o| o.name == key)
                .and_then(row)
                .or_else(|| self.device_slot_for(super::Drawer::Input, key)),
            Some(Detail::Battery) => None,
            None => self.qs_drawer.and_then(|drawer| self.device_slot_for(drawer, key)),
        }
    }

    fn settle_focus_on_row(&mut self, key: &str) {
        if self.focused.is_some()
            && let Some(slot) = self.row_slot_for(key)
        {
            self.set_focused(Some(slot));
        }
    }

    pub fn begin_connect(&mut self, key: &str) {
        self.qs_panel.pending = Some(Pending::Connect(key.to_string()));
        self.qs_panel.failure = None;
        self.qs_panel.prompt = None;
        self.settle_focus_on_row(key);
        self.qs_panel.bump();
        self.touch();
    }

    pub fn begin_disconnect(&mut self, key: &str) {
        self.qs_panel.pending = Some(Pending::Disconnect(key.to_string()));
        self.qs_panel.failure = None;
        self.qs_panel.prompt = None;
        self.qs_panel.bump();
        self.touch();
    }

    pub fn toggle_prompt(&mut self, ssid: &str) {
        if self.qs_panel.prompt.as_ref().is_some_and(|prompt| prompt.ssid == ssid) {
            self.qs_panel.prompt = None;
            self.settle_focus_on_row(ssid);
        } else {
            self.qs_panel.prompt = Some(Prompt::new(ssid.to_string()));
            self.set_focused(Some(Slot::QsPasswordField));
        }
        self.qs_panel.failure = None;
        self.qs_panel.bump();
        self.touch();
    }

    pub fn set_caps_lock(&mut self, on: bool) -> bool {
        let Some(prompt) = self.qs_panel.prompt.as_mut() else {
            return false;
        };
        if prompt.caps_lock == on {
            return false;
        }
        prompt.caps_lock = on;
        self.qs_panel.bump();
        true
    }

    pub fn toggle_reveal(&mut self) -> bool {
        let Some(prompt) = self.qs_panel.prompt.as_mut() else {
            return false;
        };
        prompt.reveal = !prompt.reveal;
        self.qs_panel.bump();
        self.touch();
        true
    }

    #[must_use]
    pub fn charge_limit(&self) -> Option<ChargeLimit> {
        self.qs_panel.charge.clone().filter(|_| self.detail() == Some(Detail::Battery))
    }

    pub fn focus_password(&mut self) {
        if self.qs_panel.prompt.is_some() && self.focused != Some(Slot::QsPasswordField) {
            self.set_focused(Some(Slot::QsPasswordField));
        }
    }

    pub fn prompt_key(&mut self, key: FieldKey) -> PromptOutcome {
        let Some(prompt) = self.qs_panel.prompt.as_mut() else {
            return PromptOutcome::Unchanged;
        };
        let changed = match key {
            FieldKey::Type(ch) => prompt.field.insert(ch),
            FieldKey::Backspace => prompt.field.backspace(),
            FieldKey::DeleteWord => prompt.field.delete_word_back(),
            FieldKey::Delete => prompt.field.delete(),
            FieldKey::Caret(movement) => prompt.field.move_caret(movement),
            FieldKey::SelectAll => prompt.field.select_all(),
            FieldKey::Submit => {
                if prompt.field.is_empty() {
                    return PromptOutcome::Unchanged;
                }
                let ssid = prompt.ssid.clone();
                let password = prompt.field.text().to_string();
                self.qs_panel.prompt = None;
                self.qs_panel.bump();
                self.touch();
                return PromptOutcome::Submit { ssid, password };
            }
            FieldKey::Cancel => {
                let ssid = prompt.ssid.clone();
                self.qs_panel.prompt = None;
                self.settle_focus_on_row(&ssid);
                self.qs_panel.bump();
                self.touch();
                return PromptOutcome::Cancelled;
            }
        };
        if !changed {
            return PromptOutcome::Unchanged;
        }
        self.qs_panel.bump();
        PromptOutcome::Changed
    }

    #[must_use]
    pub fn qs_key_mode(&self) -> QsKeyMode {
        if self.detail().is_none() {
            return QsKeyMode::Normal;
        }
        if self.qs_panel.prompt.is_some() && self.focused == Some(Slot::QsPasswordField) {
            QsKeyMode::Typing
        } else {
            QsKeyMode::Detail
        }
    }

    fn settle_pending(&mut self, key: &str, result: Result<(), ConnectError>) {
        let matches = matches!(
            &self.qs_panel.pending,
            Some(Pending::Connect(pending) | Pending::Disconnect(pending)) if pending == key
        );
        if matches {
            self.qs_panel.pending = None;
        }
        self.qs_panel.failure = result.err().map(|error| Failure { key: key.to_string(), error });
    }

    pub fn apply_net_event(&mut self, event: NetEvent) -> bool {
        match event {
            NetEvent::Wifi(networks) => {
                if self.qs_panel.wifi_ready && self.qs_panel.wifi == networks {
                    return false;
                }
                self.qs_panel.wifi = networks;
                self.qs_panel.wifi_ready = true;
            }
            NetEvent::Bluetooth(devices) => {
                if self.qs_panel.devices_ready && self.qs_panel.devices == devices {
                    return false;
                }
                self.qs_panel.devices = devices;
                self.qs_panel.devices_ready = true;
            }
            NetEvent::WifiResult { ssid, result } => {
                self.settle_pending(&ssid, result);
                if result == Err(ConnectError::WrongPassword) && self.detail() == Some(Detail::Wifi) {
                    self.qs_panel.prompt = Some(Prompt::new(ssid));
                    self.set_focused(Some(Slot::QsPasswordField));
                }
            }
            NetEvent::BluetoothResult { mac, result } => self.settle_pending(&mac, result),
            NetEvent::Outputs(outputs) => {
                if self.qs_panel.outputs_ready && self.qs_panel.outputs == outputs {
                    return false;
                }
                self.qs_panel.outputs = outputs;
                self.qs_panel.outputs_ready = true;
            }
            NetEvent::OutputResult { name, result } | NetEvent::InputResult { name, result } => {
                self.settle_pending(&name, result);
            }
            NetEvent::Inputs(inputs) => {
                if self.qs_panel.inputs.as_ref() == Some(&inputs) {
                    return false;
                }
                self.qs_panel.inputs = Some(inputs);
            }
            NetEvent::ChargeLimit(limit) => {
                if self.qs_panel.charge == limit {
                    return false;
                }
                self.qs_panel.charge = limit;
            }
            NetEvent::ChargeLimitResult(result) => self.settle_pending(CHARGE_LIMIT_KEY, result),
        }
        self.qs_panel.bump();
        self.touch();
        true
    }

    pub(super) fn detail_slots(&self, painter: &crate::paint::Painter) -> Option<Vec<(Slot, LocalRect)>> {
        let detail = self.qs_panel.view?;
        let radio = has_radio(detail);
        let geometry = detail_geometry(qs_geometry(&self.qs_shape(None)).height, radio);
        let mut slots = vec![(Slot::QsBack, geometry.back)];
        if radio {
            slots.push((Slot::QsDetailToggle, geometry.toggle));
        }
        for item in detail_items(&self.qs_panel, detail, &self.system_status, geometry.body) {
            if let Some(slot) = item.slot {
                slots.push((slot, item.rect));
            }
            if let ItemKind::Prompt(index) = item.kind {
                let parts = prompt_parts(item.rect, self.qs_panel.failed(index), prompt_pills(painter));
                slots.push((Slot::QsPromptReveal, parts.reveal));
                slots.push((Slot::QsPasswordField, parts.field));
                slots.push((Slot::QsPromptCancel, parts.cancel));
                slots.push((Slot::QsPromptConnect, parts.connect));
            }
        }
        slots.push((Slot::QsDetailSettings, geometry.settings));
        Some(slots)
    }
}

fn prompt_pills(painter: &crate::paint::Painter) -> (i32, i32) {
    (
        super::qs_confirm_pill_width(painter, t(Key::PowerCancel)),
        super::qs_confirm_pill_width(painter, t(Key::QsConnect)),
    )
}

type DetailKey = (u64, u64, Option<Slot>, Detail);

#[allow(clippy::too_many_arguments)]
pub(super) fn push_detail<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, icons: &mut crate::icon::IconLoader,
    rect: Rectangle<i32, Logical>, reveal: f64, scale: Scale<f64>, output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: crate::raster::ShellRenderer,
{
    let Some(detail) = topbar.qs_panel.shown else {
        return;
    };
    let slide = topbar.qs_panel.slide_progress();
    if slide <= 0.0 {
        return;
    }
    let hovered = topbar.hovered_in(Slot::in_quick_settings).filter(|_| topbar.qs_panel.view.is_some());
    let key: DetailKey = (topbar.qs_panel.rev, topbar.detail_rev, hovered, detail);
    let theme = &topbar.theme;
    let status = &topbar.system_status;
    let panel = &topbar.qs_panel;
    let Some(buffer) = topbar.qs_detail_cache.resolve(key, output_scale, || {
        let ctx = QsRenderContext { painter, theme, hovered };
        let mut canvas = Canvas::new(rect.size.w, rect.size.h, output_scale)?.with_icons(Some(icons));
        draw_detail(&mut canvas, &ctx, panel, detail, status);
        Some(canvas.finish())
    }) else {
        return;
    };
    let shift = ((1.0 - slide) * f64::from(QS_PAD)).round() as i32;
    let alpha = slide as f32;
    push_placed(
        renderer,
        buffer,
        Placed {
            popup: rect,
            element: Rectangle::new(Point::from((rect.loc.x + shift, rect.loc.y)), rect.size),
            progress: reveal,
            alpha,
        },
        scale,
        elements,
    );
}

pub(super) fn draw_detail(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, panel: &DetailPanel, detail: Detail,
    status: &crate::system_info::SystemStatus,
) {
    let geometry = detail_geometry(c.h, has_radio(detail));
    draw_detail_header(c, ctx, &geometry, detail, status);

    for item in detail_items(panel, detail, status, geometry.body) {
        draw_item(c, ctx, (panel, status), item);
    }

    c.fill(local_rect(QS_PAD, geometry.divider_bottom, c.w - QS_PAD * 2, 1), ctx.theme.line, 0.0);
    let settings = geometry.settings;
    let settings_label = t(match detail {
        Detail::Wifi => Key::QsWifiSettings,
        Detail::Bluetooth => Key::QsBluetoothSettings,
        Detail::Battery => Key::TbBatterySettings,
        Detail::Sound => Key::QsSoundSettings,
    });
    let (left, right, top, bottom) = band(settings);
    let chevron = local_rect(right - SP2 - DETAIL_CHEVRON, top, DETAIL_CHEVRON, settings.size.h);
    c.text(
        ctx.painter,
        (left + SP2, chevron.loc.x - SP2, top, bottom),
        TextAlign::Left,
        settings_label,
        ctx.theme.label,
        &super::QS_LABEL,
    );
    super::draw_qs_chevron(c, ctx, chevron, (false, ctx.hovered == Some(Slot::QsDetailSettings)));
}

fn draw_item(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, (panel, status): (&DetailPanel, &crate::system_info::SystemStatus),
    item: DetailItem,
) {
    let rect = item.rect;
    match item.kind {
        ItemKind::Section(key) => {
            c.text_in(ctx.painter, rect, TextAlign::Left, t(key), ctx.theme.faint, &TextStyle::MICRO);
        }
        ItemKind::Message(key) => {
            c.text_in(ctx.painter, rect, TextAlign::Center, t(key), ctx.theme.faint, &TextStyle::CAP);
        }
        ItemKind::Network(index) => {
            if let Some(network) = panel.wifi.get(index) {
                let open = panel.prompt.as_ref().is_some_and(|prompt| prompt.ssid == network.ssid);
                draw_network_card(c, ctx, rect, network, (panel.status_for(&network.ssid), open));
            }
        }
        ItemKind::Prompt(index) => {
            if let Some(prompt) = panel.prompt.as_ref() {
                draw_prompt(c, ctx, rect, prompt, panel.failed(index));
            }
        }
        ItemKind::Device(index) => {
            if let Some(device) = panel.devices.get(index) {
                draw_device_card(c, ctx, rect, device, panel.status_for(&device.mac));
            }
        }
        ItemKind::Output(index) | ItemKind::Input(index) => {
            let drawer =
                if matches!(item.kind, ItemKind::Output(_)) { super::Drawer::Output } else { super::Drawer::Input };
            if let Some(device) = panel.devices(drawer).get(index) {
                draw_audio_card(c, ctx, rect, device, panel.status_for(&device.name));
            }
        }
        ItemKind::Diag(index) => draw_diag(c, ctx, rect, index, status),
        ItemKind::Profile(profile) => draw_profile_card(c, ctx, rect, profile, status.power_profile == Some(profile)),
        ItemKind::ChargeLimit => {
            if let Some(limit) = panel.charge.as_ref() {
                draw_charge_card(c, ctx, rect, limit, panel.status_for(CHARGE_LIMIT_KEY));
            }
        }
    }
}

fn battery_low(theme: &TopbarTheme, status: &crate::system_info::SystemStatus) -> bool {
    status.battery_pct.is_some_and(|pct| !status.battery_charging && pct <= theme.battery_low_pct)
}

fn draw_detail_header(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, geometry: &DetailGeometry, detail: Detail,
    status: &crate::system_info::SystemStatus,
) {
    let theme = ctx.theme;
    let back = geometry.back;
    c.fill(back, theme.track_soft, R2);
    let colour = if ctx.hovered == Some(Slot::QsBack) { theme.label } else { theme.muted };
    let side = DETAIL_BACK_ICON;
    let at = (back.loc.x + (back.size.w - side) / 2, back.loc.y + (back.size.h - side) / 2);
    if !c.icon("go-previous", side, at, colour) {
        c.text_in(ctx.painter, back, TextAlign::Center, "\u{2039}", colour, &TextStyle::HEAD);
    }
    let title = t(match detail {
        Detail::Wifi => Key::WiFi,
        Detail::Bluetooth => Key::Bluetooth,
        Detail::Battery => Key::TbBattery,
        Detail::Sound => Key::SettingsSound,
    });
    let (top, bottom) = (back.loc.y, back.loc.y + back.size.h);
    let title_band = (geometry.title_left, geometry.title_right, top, bottom);
    c.text(ctx.painter, title_band, TextAlign::Left, title, theme.label, &TextStyle::HEAD);
    if !has_radio(detail) {
        let (pct, colour) = if detail == Detail::Sound {
            let colour = if status.volume_muted { theme.danger } else { theme.muted };
            (format!("{}%", status.volume_pct), colour)
        } else {
            let pct = status.battery_pct.map_or_else(String::new, |pct| format!("{pct}%"));
            (pct, if battery_low(theme, status) { theme.danger } else { theme.muted })
        };
        let pct_band = (geometry.title_right + SP2, QS_WIDTH - QS_PAD, top, bottom);
        c.text(ctx.painter, pct_band, TextAlign::Right, &pct, colour, &super::QS_READOUT);
        return;
    }
    let position = f64::from(u8::from(radio_on(detail, status)));
    let switch = SwitchMotion { thumb: position, track: position };
    draw_switch(c, theme, geometry.toggle, (DETAIL_SWITCH_INSET, DETAIL_SWITCH_THUMB), switch);
}

fn tenths(value: u32) -> String {
    format!("{}.{}", value / 10, value % 10)
}

fn draw_diag(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, index: usize,
    status: &crate::system_info::SystemStatus,
) {
    let theme = ctx.theme;
    c.fill(rect, theme.track_soft, R2);
    c.stroke(rect, 1.0, R2, theme.line);
    let health = status.battery.map(|info| info.health).unwrap_or_default();
    let (label, value, unit) = match index {
        0 => (Key::QsHealth, health.wear_pct.map(|pct| format!("{pct}%")), None),
        1 => (Key::QsCycles, health.cycles.map(|cycles| cycles.to_string()), Some(t(Key::QsCyclesUnit).to_string())),
        2 => (Key::QsDraw, health.draw_dw.map(tenths), Some(t(Key::QsWatts).to_string())),
        _ => (
            Key::QsCapacity,
            health.full_dwh.map(tenths),
            health.design_dwh.map(|design| t(Key::QsOfWattHours).replace("{}", &tenths(design))),
        ),
    };
    let (left, right) = (rect.loc.x + DIAG_PAD_X, rect.loc.x + rect.size.w - DIAG_PAD_X);
    let label_top = rect.loc.y + DIAG_PAD_Y;
    let label_h = super::QS_SUB.line_height_px as i32;
    c.text(
        ctx.painter,
        (left, right, label_top, label_top + label_h),
        TextAlign::Left,
        t(label),
        theme.faint,
        &super::QS_SUB,
    );
    let value_top = label_top + label_h + DIAG_GAP;
    let band = (value_top, value_top + DIAG_VALUE);
    let value = value.unwrap_or_else(|| "\u{2014}".to_string());
    let value_w = text_w(ctx.painter, &value, &super::QS_BATTERY_PCT);
    c.text(
        ctx.painter,
        (left, left + value_w, band.0, band.1),
        TextAlign::Left,
        &value,
        theme.label,
        &super::QS_BATTERY_PCT,
    );
    let after = left + value_w + DIAG_UNIT_GAP;
    if index == 0 {
        if let Some(pct) = health.wear_pct {
            let worn = pct < HEALTHY_PCT;
            let text = t(if worn { Key::QsWorn } else { Key::QsHealthy });
            let (well, ink) = if worn { (theme.danger_soft, theme.danger) } else { (theme.ok_soft, theme.ok) };
            let width = text_w(ctx.painter, text, &DIAG_PILL) + DIAG_PILL_PAD * 2;
            let pill = local_rect(after, band.0 + (DIAG_VALUE - DIAG_PILL_H) / 2, width, DIAG_PILL_H);
            c.fill(pill, well, R_ROUND);
            c.text_in(ctx.painter, pill, TextAlign::Center, text, ink, &DIAG_PILL);
        }
    } else if let Some(unit) = unit {
        c.text(ctx.painter, (after, right, band.0, band.1), TextAlign::Left, &unit, theme.faint, &TextStyle::CAP);
    }
}

const fn profile_hint(profile: PowerProfile) -> Key {
    match profile {
        PowerProfile::PowerSaver => Key::QsProfileSaverHint,
        PowerProfile::Balanced => Key::QsProfileBalancedHint,
        PowerProfile::Performance => Key::QsProfilePerformanceHint,
    }
}

fn draw_profile_card(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, profile: PowerProfile, active: bool,
) {
    let theme = ctx.theme;
    if active {
        c.fill(rect, theme.active_soft, R2);
    } else {
        c.fill(rect, theme.track_soft, R2);
        c.stroke(rect, 1.0, R2, theme.line);
    }
    let radio = local_rect(rect.loc.x + CARD_PAD_X, rect.loc.y + CARD_PAD_X + 2, RADIO, RADIO);
    c.stroke(radio, 1.5, R_ROUND, if active { theme.active_background } else { theme.line_2 });
    if active {
        let dot = local_rect(
            radio.loc.x + (RADIO - RADIO_DOT) / 2,
            radio.loc.y + (RADIO - RADIO_DOT) / 2,
            RADIO_DOT,
            RADIO_DOT,
        );
        c.fill(dot, theme.active_background, R_ROUND);
    }
    let left = radio.loc.x + RADIO + CARD_GAP;
    let right = rect.loc.x + rect.size.w - CARD_PAD_X;
    let (title_colour, hint_colour) = if active {
        (theme.active_soft_label, super::dimmed(theme.active_soft_label, super::QS_SOFT_DIM))
    } else {
        (theme.label, theme.faint)
    };
    let top = rect.loc.y + CARD_PAD_X;
    let title_h = CARD_PRIMARY.line_height_px as i32;
    c.text(
        ctx.painter,
        (left, right, top, top + title_h),
        TextAlign::Left,
        t(profile_label(profile)),
        title_colour,
        &CARD_PRIMARY,
    );
    let hint_top = top + title_h + 2;
    let hint_h = super::QS_NOTE.line_height_px as i32;
    c.text(
        ctx.painter,
        (left, right, hint_top, hint_top + hint_h),
        TextAlign::Left,
        t(profile_hint(profile)),
        hint_colour,
        &super::QS_NOTE,
    );
}

fn status_trailing(theme: &TopbarTheme, status: RowStatus) -> Option<(String, Color)> {
    let (key, danger) = match status {
        RowStatus::Idle => return None,
        RowStatus::Connecting => (Key::Connecting, false),
        RowStatus::Disconnecting => (Key::QsDisconnecting, false),
        RowStatus::Failed(ConnectError::WrongPassword) => (Key::QsWrongPassword, true),
        RowStatus::Failed(ConnectError::Failed) => (Key::QsCouldNotConnect, true),
    };
    let colour = if danger { theme.danger } else { theme.muted };
    Some((t(key).to_string(), colour))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trailing {
    Nothing,
    Tick,
    Lock { open: bool },
}

struct CardSpec<'a> {
    icon: &'a str,
    glyph: GlyphPainter,
    title: &'a str,
    subtitle: (String, Option<Color>),
    active: bool,
    action: Option<&'a str>,
    trailing: Trailing,
}

fn card_background(c: &mut Canvas<'_>, theme: &TopbarTheme, rect: LocalRect, active: bool) {
    if active {
        c.fill(rect, theme.active_soft, R2);
    } else {
        c.fill(rect, theme.track_soft, R2);
        c.stroke(rect, 1.0, R2, theme.line);
    }
}

fn draw_card(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, spec: &CardSpec<'_>) {
    let theme = ctx.theme;
    let well = local_rect(rect.loc.x + CARD_PAD_X, rect.loc.y + (rect.size.h - CARD_ICON) / 2, CARD_ICON, CARD_ICON);
    let (fill, symbol) =
        if spec.active { (theme.active_background, theme.active_label) } else { (theme.fill_2, theme.muted) };
    c.fill(well, fill, R1);
    super::qs_symbol(c, well, (spec.icon, spec.glyph), CARD_GLYPH, symbol);

    let mut right = rect.loc.x + rect.size.w - CARD_PAD_X;
    let centre_y = rect.loc.y + rect.size.h / 2;
    let accent = if spec.active { theme.active_soft_label } else { theme.faint };
    if let Some(action) = spec.action {
        let width = text_w(ctx.painter, action, &CARD_ACTION) + CARD_ACTION_PAD * 2;
        let button = local_rect(right - width, centre_y - CARD_ACTION_H / 2, width, CARD_ACTION_H);
        c.fill(button, theme.fill_2, R1);
        c.text_in(ctx.painter, button, TextAlign::Center, action, theme.muted, &CARD_ACTION);
        right = button.loc.x - CARD_GAP;
    }
    match spec.trailing {
        Trailing::Nothing => {}
        Trailing::Tick => {
            let tick = local_rect(right - CARD_TRAILING, centre_y - CARD_TRAILING / 2, CARD_TRAILING, CARD_TRAILING);
            super::qs_symbol(c, tick, ("object-select", crate::paint::draw_tick_glyph_over), CARD_TRAILING, accent);
            right = tick.loc.x - CARD_GAP;
        }
        Trailing::Lock { open } => {
            let chevron = local_rect(right - CARD_TRAILING, centre_y - CARD_TRAILING / 2, CARD_TRAILING, CARD_TRAILING);
            let icon = if open { "pan-down" } else { "go-next" };
            let colour = if open { theme.label } else { theme.faint };
            if !c.icon(icon, CARD_TRAILING, (chevron.loc.x, chevron.loc.y), colour) {
                c.text_in(ctx.painter, chevron, TextAlign::Center, "\u{203a}", colour, &TextStyle::HEAD);
            }
            let lock = local_rect(chevron.loc.x - SP1 - CARD_TRAILING, chevron.loc.y, CARD_TRAILING, CARD_TRAILING);
            super::qs_symbol(
                c,
                lock,
                ("network-wireless-encrypted", crate::paint::draw_lock_glyph_over),
                CARD_TRAILING,
                theme.faint,
            );
            right = lock.loc.x - CARD_GAP;
        }
    }

    let left = well.loc.x + CARD_ICON + CARD_GAP;
    let title_h = CARD_PRIMARY.line_height_px as i32;
    let sub_h = super::QS_NOTE.line_height_px as i32;
    let top = rect.loc.y + (rect.size.h - title_h - sub_h) / 2;
    let title_colour = if spec.active { theme.active_soft_label } else { theme.label };
    c.text(ctx.painter, (left, right, top, top + title_h), TextAlign::Left, spec.title, title_colour, &CARD_PRIMARY);
    let sub_colour =
        spec.subtitle.1.unwrap_or(if spec.active { super::dimmed(accent, super::QS_SOFT_DIM) } else { accent });
    let band = (left, right, top + title_h, top + title_h + sub_h);
    c.text(ctx.painter, band, TextAlign::Left, &spec.subtitle.0, sub_colour, &super::QS_NOTE);
}

fn subtitle_with(status: (&TopbarTheme, RowStatus), fallback: String) -> (String, Option<Color>) {
    status_trailing(status.0, status.1).map_or((fallback, None), |(text, colour)| (text, Some(colour)))
}

fn network_subtitle(network: &WifiNetwork) -> Key {
    if network.connected {
        Key::Connected
    } else if network.security == Security::Enterprise && !network.saved {
        Key::QsEnterprise
    } else if network.saved {
        Key::QsSaved
    } else if network.security.is_secured() {
        Key::QsSecured
    } else {
        Key::QsOpenNetwork
    }
}

fn draw_network_card(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, network: &WifiNetwork,
    (status, open): (RowStatus, bool),
) {
    if open {
        let failed = matches!(status, RowStatus::Failed(_));
        let block = local_rect(rect.loc.x, rect.loc.y, rect.size.w, rect.size.h + prompt_height(failed));
        c.fill(block, ctx.theme.fill_2, R2);
        c.stroke(block, 1.0, R2, ctx.theme.line);
    } else {
        card_background(c, ctx.theme, rect, network.connected);
    }
    let secured = network.security.is_secured() && !network.connected;
    let spec = CardSpec {
        icon: network.icon(),
        glyph: crate::paint::draw_wifi_waves_glyph_over,
        title: &network.ssid,
        subtitle: subtitle_with((ctx.theme, status), t(network_subtitle(network)).to_string()),
        active: network.connected && !open,
        action: network.connected.then(|| t(Key::QsDisconnect)),
        trailing: if secured { Trailing::Lock { open } } else { Trailing::Nothing },
    };
    draw_card(c, ctx, rect, &spec);
}

fn draw_device_card(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, device: &BtDevice, status: RowStatus,
) {
    card_background(c, ctx.theme, rect, device.connected);
    let fallback = if device.connected {
        device.battery.map_or_else(
            || t(Key::Connected).to_string(),
            |pct| format!("{} \u{b7} {}", t(Key::Connected), t(Key::QsBatteryLevel).replace("{}", &pct.to_string())),
        )
    } else {
        t(Key::QsPaired).to_string()
    };
    let spec = CardSpec {
        icon: device.kind.icon(),
        glyph: crate::paint::draw_bluetooth_glyph_over,
        title: &device.name,
        subtitle: subtitle_with((ctx.theme, status), fallback),
        active: device.connected,
        action: Some(t(if device.connected { Key::QsDisconnect } else { Key::QsConnect })),
        trailing: Trailing::Nothing,
    };
    draw_card(c, ctx, rect, &spec);
}

fn draw_audio_card(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, device: &AudioOutput, status: RowStatus,
) {
    card_background(c, ctx.theme, rect, device.active);
    let kind = if device.active { Key::QsDefaultDevice } else { super::qs_device_kind(device.icon) };
    let spec = CardSpec {
        icon: device.icon,
        glyph: |b, w, h, side, stroke, s, c| crate::paint::draw_speaker_glyph_over(b, w, h, side, stroke, false, s, c),
        title: &device.description,
        subtitle: subtitle_with((ctx.theme, status), t(kind).to_string()),
        active: device.active,
        action: None,
        trailing: if device.active { Trailing::Tick } else { Trailing::Nothing },
    };
    draw_card(c, ctx, rect, &spec);
}

fn draw_prompt(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, prompt: &Prompt, failed: bool) {
    let theme = ctx.theme;
    let parts = prompt_parts(rect, failed, prompt_pills(ctx.painter));
    let title = t(Key::QsPasswordFor).replace("{ssid}", &prompt.ssid);
    c.text_in(ctx.painter, parts.title, TextAlign::Left, &title, theme.label, &super::QS_TITLE);
    draw_password_field(c, ctx, (parts.field, parts.reveal), prompt);
    if let Some(error) = parts.error {
        c.text_in(ctx.painter, error, TextAlign::Left, t(Key::QsWrongPassword), theme.danger, &super::QS_NOTE);
    }
    let cancel_colour = if ctx.hovered == Some(Slot::QsPromptCancel) { theme.label } else { theme.muted };
    c.fill(parts.cancel, theme.fill_2, R_ROUND);
    c.text_in(ctx.painter, parts.cancel, TextAlign::Center, t(Key::PowerCancel), cancel_colour, &super::QS_PILL_TEXT);
    c.fill(parts.connect, theme.active_background, R_ROUND);
    c.text_in(
        ctx.painter,
        parts.connect,
        TextAlign::Center,
        t(Key::QsConnect),
        theme.active_label,
        &super::QS_PILL_TEXT,
    );
}

fn draw_eye(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, revealed: bool) {
    let colour = if ctx.hovered == Some(Slot::QsPromptReveal) { ctx.theme.label } else { ctx.theme.faint };
    let side = PROMPT_EYE;
    let (x, y) = (rect.loc.x + (rect.size.w - side) / 2, rect.loc.y + (rect.size.h - side) / 2);
    let icon = if revealed { "view-conceal" } else { "view-reveal" };
    if c.icon(icon, side, (x, y), colour) {
        return;
    }
    c.stroke(local_rect(x, y + 3, side, side - 6), 1.5, R_ROUND, colour);
    c.fill(local_rect(x + side / 2 - 2, y + side / 2 - 2, 4, 4), colour, R_ROUND);
    if revealed {
        c.fill(local_rect(x, y + side / 2, side, 1), colour, 0.0);
    }
}

fn draw_password_field(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, (boxed, reveal): (LocalRect, LocalRect), prompt: &Prompt,
) {
    let theme = ctx.theme;
    let field = &prompt.field;
    c.fill(boxed, theme.popover_bg, R2);
    c.stroke(boxed, 1.0, R2, theme.active_background);
    draw_eye(c, ctx, reveal, prompt.reveal);
    let (left, _, top, bottom) = band(boxed);
    let mut right = reveal.loc.x;
    if prompt.caps_lock {
        let cap = &TextStyle::CAP;
        let label = t(Key::CapsLockOn);
        let width = text_w(ctx.painter, label, cap);
        c.text(
            ctx.painter,
            (right - SP1 - width, right - SP1, top, bottom),
            TextAlign::Right,
            label,
            theme.danger,
            cap,
        );
        right -= width + SP1;
    }
    let (text_left, text_right) = (left + SP2, right - SP1);
    let text_band = (text_left, text_right, top, bottom);
    let style = TextStyle::CALL;
    if field.is_empty() {
        c.text(ctx.painter, text_band, TextAlign::Left, t(Key::QsPassword), theme.faint, &style);
    } else {
        let shown = prompt.shown();
        if field.selection().is_some() {
            let width = text_w(ctx.painter, &shown, &style).min(text_right - text_left);
            let selection = local_rect(text_left, top + SP1, width, boxed.size.h - SP1 * 2);
            c.fill(selection, theme.active_soft, 0.0);
        }
        c.text(ctx.painter, text_band, TextAlign::Left, &shown, theme.label, &style);
    }
    let typed = &field.text()[..field.caret()];
    let before = if prompt.reveal { typed.to_string() } else { masked(typed) };
    let caret_x = (text_left + text_w(ctx.painter, &before, &style)).min(text_right);
    let caret = local_rect(caret_x, top + SP1 + 2, 1, boxed.size.h - SP1 * 2 - 4);
    c.fill(caret, theme.active_background, 0.0);
}

fn draw_charge_card(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, limit: &ChargeLimit, status: RowStatus,
) {
    let theme = ctx.theme;
    card_background(c, theme, rect, false);
    let switch_x = rect.loc.x + rect.size.w - CARD_PAD_X - DETAIL_SWITCH_W;
    let switch =
        local_rect(switch_x, rect.loc.y + (rect.size.h - DETAIL_SWITCH_H) / 2, DETAIL_SWITCH_W, DETAIL_SWITCH_H);
    let target = if status == RowStatus::Connecting { !limit.enabled } else { limit.enabled };
    let position = f64::from(u8::from(target));
    draw_switch(
        c,
        theme,
        switch,
        (DETAIL_SWITCH_INSET, DETAIL_SWITCH_THUMB),
        SwitchMotion { thumb: position, track: position },
    );

    let (left, right) = (rect.loc.x + CARD_PAD_X, switch.loc.x - CARD_GAP);
    let end = if limit.end == 0 { DEFAULT_CHARGE_END } else { limit.end.min(100) };
    let title = t(Key::QsChargeLimit).replace("{n}", &end.to_string());
    let (note, colour) = match status {
        RowStatus::Connecting => (Key::QsWaitingForAuth, theme.muted),
        RowStatus::Failed(_) => (Key::QsCouldNotChange, theme.danger),
        RowStatus::Idle | RowStatus::Disconnecting => (Key::QsChargeLimitHint, theme.faint),
    };
    let title_h = CARD_PRIMARY.line_height_px as i32;
    let note_h = super::QS_NOTE.line_height_px as i32;
    let top = rect.loc.y + (rect.size.h - title_h - note_h) / 2;
    c.text(ctx.painter, (left, right, top, top + title_h), TextAlign::Left, &title, theme.label, &CARD_PRIMARY);
    c.text(
        ctx.painter,
        (left, right, top + title_h, top + title_h + note_h),
        TextAlign::Left,
        t(note),
        colour,
        &super::QS_NOTE,
    );
}

fn masked(text: &str) -> String {
    text.chars().map(|_| '\u{2022}').collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topbar::{TopbarAction, TopbarTheme};
    use smithay::utils::Size;

    fn output() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), Size::from((1920, 1080)))
    }

    fn relayout(bar: &mut Topbar) {
        bar.layout_slots(&crate::paint::Painter::new(14.0, None), output());
    }

    fn network(ssid: &str, secured: bool, connected: bool) -> WifiNetwork {
        let security = if secured { Security::Passphrase } else { Security::Open };
        WifiNetwork { ssid: ssid.to_string(), security, signal: 60, connected, saved: false }
    }

    fn open_wifi() -> Topbar {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.set_system_status(crate::system_info::SystemStatus {
            wifi_enabled: true,
            bluetooth_on: true,
            ..Default::default()
        });
        bar.toggle_popup(OpenPopup::QuickSettings);
        relayout(&mut bar);
        assert!(bar.open_detail(Detail::Wifi));
        bar.apply_net_event(NetEvent::Wifi(vec![
            network("Home", true, true),
            network("Cafe", false, false),
            network("Neighbour", true, false),
        ]));
        relayout(&mut bar);
        bar
    }

    fn centre(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
        Point::from((f64::from(rect.loc.x + rect.size.w / 2), f64::from(rect.loc.y + rect.size.h / 2)))
    }

    #[test]
    fn the_chevron_opens_the_wifi_panel_and_back_returns_to_the_tiles() {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.toggle_popup(OpenPopup::QuickSettings);
        relayout(&mut bar);
        let more = bar.slot_rect(Slot::QsWifiMore).expect("card slot");
        let badge = bar.slot_rect(Slot::QsWifi).expect("badge slot");
        assert!(more.contains(badge.loc), "the toggle is the badge inside the card");
        assert_eq!(bar.action_at(centre(more)), Some(TopbarAction::QsPanel(PanelAction::Open(Detail::Wifi))));

        assert!(bar.open_detail(Detail::Wifi));
        assert!(!bar.open_detail(Detail::Wifi), "already there");
        relayout(&mut bar);
        assert_eq!(bar.qs_key_mode(), QsKeyMode::Detail);
        assert!(bar.slot_rect(Slot::QsWifi).is_none(), "tiles leave with the main view");
        let back = bar.slot_rect(Slot::QsBack).expect("back button");
        assert_eq!(bar.action_at(centre(back)), Some(TopbarAction::QsPanel(PanelAction::Back)));
        assert!(bar.slot_rect(Slot::QsDetailSettings).is_some());

        assert!(bar.close_detail());
        assert!(!bar.close_detail());
        relayout(&mut bar);
        assert!(bar.slot_rect(Slot::QsWifi).is_some());
        assert_eq!(bar.qs_key_mode(), QsKeyMode::Normal);
    }

    #[test]
    fn keyboard_focus_follows_the_view_it_came_from() {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.toggle_popup(OpenPopup::QuickSettings);
        relayout(&mut bar);
        bar.set_focused(Some(Slot::QsBluetoothMore));
        bar.open_detail(Detail::Bluetooth);
        assert_eq!(bar.focused, Some(Slot::QsBack));
        bar.close_detail();
        assert_eq!(bar.focused, Some(Slot::QsBluetoothMore));
    }

    #[test]
    fn reopening_quick_settings_starts_on_the_main_view() {
        let mut bar = open_wifi();
        bar.close_popup();
        bar.toggle_popup(OpenPopup::QuickSettings);
        assert_eq!(bar.detail(), None);
        assert!(bar.qs_panel.slide_progress() <= 0.0);
    }

    #[test]
    fn rows_list_the_networks_in_order_and_resolve_back_to_them() {
        let bar = open_wifi();
        let first = bar.slot_rect(Slot::QsDetailRow(0)).expect("row 0");
        let second = bar.slot_rect(Slot::QsDetailRow(1)).expect("row 1");
        let third = bar.slot_rect(Slot::QsDetailRow(2)).expect("row 2");
        assert_eq!(first.size.h, DETAIL_CARD);
        assert!(
            second.loc.y > first.loc.y + DETAIL_CARD + DETAIL_CARD_GAP,
            "a section label parts connected from available"
        );
        assert_eq!(third.loc.y, second.loc.y + DETAIL_CARD + DETAIL_CARD_GAP, "cards in one section sit 6 px apart");
        assert_eq!(bar.detail_row(1), Some(RowTarget::Network(network("Cafe", false, false))));
        assert_eq!(bar.detail_row(9), None);
    }

    #[test]
    fn each_view_groups_its_cards_under_section_labels() {
        let bar = open_wifi();
        let geometry = detail_geometry(qs_geometry(&bar.qs_shape(None)).height, true);
        let items = detail_items(&bar.qs_panel, Detail::Wifi, bar.system_status(), geometry.body);
        let kinds: Vec<ItemKind> = items.iter().map(|item| item.kind).collect();
        assert_eq!(
            kinds,
            [
                ItemKind::Section(Key::QsConnectedNetwork),
                ItemKind::Network(0),
                ItemKind::Section(Key::QsAvailableNetworks),
                ItemKind::Network(1),
                ItemKind::Network(2),
            ]
        );
        assert!(items.iter().all(|item| geometry.body.contains(item.rect.loc)));

        let status = crate::system_info::SystemStatus {
            battery_pct: Some(70),
            power_profile: Some(PowerProfile::Balanced),
            ..Default::default()
        };
        let battery = detail_items(&bar.qs_panel, Detail::Battery, &status, geometry.body);
        let diag = battery.iter().filter(|item| matches!(item.kind, ItemKind::Diag(_))).count();
        let profiles: Vec<Option<Slot>> =
            battery.iter().filter(|item| matches!(item.kind, ItemKind::Profile(_))).map(|item| item.slot).collect();
        assert_eq!(diag, 4, "a two by two telemetry grid");
        assert_eq!(profiles, super::super::QS_PROFILES.map(|profile| Some(Slot::QsProfile(profile))));
        let off = crate::system_info::SystemStatus { wifi_enabled: false, ..status };
        let wifi_off = detail_items(&bar.qs_panel, Detail::Wifi, &off, geometry.body);
        assert_eq!(wifi_off.iter().map(|item| item.kind).collect::<Vec<_>>(), [ItemKind::Message(Key::QsWifiIsOff)]);
    }

    #[test]
    fn arrows_walk_the_rows_from_the_header_down_to_settings() {
        let mut bar = open_wifi();
        bar.set_focused(Some(Slot::QsBack));
        let mut visited = vec![Slot::QsBack];
        while bar.focus_nav(0, 1) {
            visited.push(bar.focused.expect("focus"));
        }
        assert_eq!(
            visited,
            [Slot::QsBack, Slot::QsDetailRow(0), Slot::QsDetailRow(1), Slot::QsDetailRow(2), Slot::QsDetailSettings,]
        );
    }

    #[test]
    fn a_switched_off_radio_lists_nothing() {
        let mut bar = open_wifi();
        let mut status = bar.system_status().clone();
        status.wifi_enabled = false;
        bar.set_system_status(status);
        relayout(&mut bar);
        assert!(bar.slot_rect(Slot::QsDetailRow(0)).is_none());
        assert!(bar.slot_rect(Slot::QsDetailToggle).is_some());
    }

    #[test]
    fn a_secured_network_asks_for_a_password_under_its_row() {
        let mut bar = open_wifi();
        bar.toggle_prompt("Neighbour");
        relayout(&mut bar);
        let row = bar.slot_rect(Slot::QsDetailRow(2)).expect("neighbour row");
        let field = bar.slot_rect(Slot::QsPasswordField).expect("field row");
        let cancel = bar.slot_rect(Slot::QsPromptCancel).expect("cancel pill");
        let connect = bar.slot_rect(Slot::QsPromptConnect).expect("connect pill");
        assert!(field.loc.y > row.loc.y + row.size.h, "the drawer opens under its card");
        assert!(field.loc.y + field.size.h < cancel.loc.y && cancel.loc.x + cancel.size.w < connect.loc.x);
        assert_eq!(bar.action_at(centre(connect)), Some(TopbarAction::QsPanel(PanelAction::PromptSubmit)));
        assert_eq!(bar.action_at(centre(cancel)), Some(TopbarAction::QsPanel(PanelAction::PromptCancel)));
        assert_eq!(bar.qs_key_mode(), QsKeyMode::Typing);

        for ch in "pa55".chars() {
            assert!(bar.prompt_key(FieldKey::Type(ch)) == PromptOutcome::Changed);
        }
        assert!(bar.prompt_key(FieldKey::Backspace) == PromptOutcome::Changed);
        assert!(bar.prompt_key(FieldKey::Type('w')) == PromptOutcome::Changed);
        let submitted = bar.prompt_key(FieldKey::Submit);
        assert!(submitted == PromptOutcome::Submit { ssid: "Neighbour".to_string(), password: "pa5w".to_string() });
        assert_eq!(bar.qs_key_mode(), QsKeyMode::Detail, "the field is gone");
    }

    #[test]
    fn the_password_field_warns_about_caps_lock() {
        let mut bar = open_wifi();
        assert!(!bar.set_caps_lock(true), "no field open, nothing to warn");
        bar.toggle_prompt("Neighbour");
        assert!(bar.set_caps_lock(true));
        assert!(!bar.set_caps_lock(true), "an unchanged state does not repaint");
        assert!(bar.qs_panel.prompt.as_ref().is_some_and(|prompt| prompt.caps_lock));
        assert!(bar.set_caps_lock(false));
    }

    #[test]
    fn escape_cancels_the_prompt_and_an_empty_field_never_submits() {
        let mut bar = open_wifi();
        bar.toggle_prompt("Neighbour");
        assert!(bar.prompt_key(FieldKey::Submit) == PromptOutcome::Unchanged);
        assert!(bar.prompt_key(FieldKey::Cancel) == PromptOutcome::Cancelled);
        assert!(bar.prompt_key(FieldKey::Type('x')) == PromptOutcome::Unchanged);
        assert_eq!(bar.focused, Some(Slot::QsDetailRow(2)));
    }

    #[test]
    fn a_wrong_password_reopens_the_prompt_with_the_error() {
        let mut bar = open_wifi();
        bar.begin_connect("Neighbour");
        assert_eq!(bar.qs_panel.status_for("Neighbour"), RowStatus::Connecting);
        bar.apply_net_event(NetEvent::WifiResult {
            ssid: "Neighbour".to_string(),
            result: Err(ConnectError::WrongPassword),
        });
        assert_eq!(bar.qs_panel.status_for("Neighbour"), RowStatus::Failed(ConnectError::WrongPassword));
        assert_eq!(bar.qs_key_mode(), QsKeyMode::Typing);
        bar.apply_net_event(NetEvent::WifiResult { ssid: "Neighbour".to_string(), result: Ok(()) });
        assert_eq!(bar.qs_panel.status_for("Neighbour"), RowStatus::Idle);
    }

    #[test]
    fn an_identical_scan_does_not_repaint() {
        let mut bar = open_wifi();
        let list = bar.qs_panel.wifi.clone();
        assert!(!bar.apply_net_event(NetEvent::Wifi(list)));
    }

    #[test]
    fn closing_the_popup_forgets_a_typed_password() {
        let mut bar = open_wifi();
        bar.toggle_prompt("Neighbour");
        bar.prompt_key(FieldKey::Type('s'));
        bar.close_popup();
        assert!(bar.qs_panel.prompt.is_none());
    }

    #[test]
    fn typing_keys_map_onto_field_edits() {
        let key = |sym: Keysym, ctrl: bool, typed: Option<char>| field_key(ctrl, !ctrl, typed, |want| want == sym);
        assert_eq!(key(Keysym::Escape, false, None), Some(FieldKey::Cancel));
        assert_eq!(key(Keysym::Return, false, None), Some(FieldKey::Submit));
        assert_eq!(key(Keysym::BackSpace, true, None), Some(FieldKey::DeleteWord));
        assert_eq!(key(Keysym::Left, true, None), Some(FieldKey::Caret(CaretMove::WordLeft)));
        assert_eq!(key(Keysym::a, true, Some('a')), Some(FieldKey::SelectAll));
        assert_eq!(key(Keysym::a, false, Some('a')), Some(FieldKey::Type('a')));
        assert_eq!(key(Keysym::Down, false, None), None, "arrows leave the field");
        assert_eq!(field_key(false, false, Some('q'), |_| false), None);
        assert_eq!(format!("{:?}", FieldKey::Type('s')), "Type(*)");
    }

    #[test]
    fn every_power_button_is_laid_out_and_reports_its_action() {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.toggle_popup(OpenPopup::QuickSettings);
        relayout(&mut bar);
        let mut previous_right = i32::MIN;
        for action in POWER_ROW {
            let rect = bar.slot_rect(Slot::QsPowerButton(action)).expect("power button");
            assert!(rect.loc.x > previous_right);
            previous_right = rect.loc.x + rect.size.w;
            assert_eq!(bar.action_at(centre(rect)), Some(TopbarAction::QsArmPower(action)));
        }
    }

    #[test]
    fn the_battery_time_line_joins_state_and_estimate() {
        let info = BatteryInfo {
            pct: 50,
            state: BatteryState::Discharging,
            minutes: Some(135),
            health: crate::system_info::BatteryHealth::default(),
            profile: None,
        };
        let eta = t(Key::TbBatteryTimeLeft).replace("{}", &crate::i18n::duration_label(135));
        assert_eq!(time_line(&info), format!("{} \u{b7} {eta}", t(Key::TbBatteryOnBattery)));
        assert!(crate::i18n::duration_label(135).contains("15"));
        let full = BatteryInfo { state: BatteryState::Full, minutes: Some(10), ..info };
        assert_eq!(time_line(&full), t(Key::TbBatteryFull));
    }

    #[test]
    fn the_battery_row_shows_the_estimate_alone_when_there_is_one() {
        let info = BatteryInfo {
            pct: 50,
            state: BatteryState::Discharging,
            minutes: Some(135),
            health: crate::system_info::BatteryHealth::default(),
            profile: None,
        };
        let status =
            |battery| crate::system_info::SystemStatus { battery_pct: Some(50), battery, ..Default::default() };
        let left = t(Key::TbBatteryTimeLeft).replace("{}", &crate::i18n::duration_label(135));
        assert_eq!(battery_head_line(&status(Some(info))), left);
        let charging = BatteryInfo { state: BatteryState::Charging, ..info };
        let full = t(Key::TbBatteryTimeToFull).replace("{}", &crate::i18n::duration_label(135));
        assert_eq!(battery_head_line(&status(Some(charging))), full);
        let unknown = BatteryInfo { minutes: None, ..info };
        assert_eq!(battery_head_line(&status(Some(unknown))), t(Key::TbBatteryOnBattery));
        let done = BatteryInfo { state: BatteryState::Full, ..info };
        assert_eq!(battery_head_line(&status(Some(done))), t(Key::TbBatteryFull));
        assert_eq!(battery_head_line(&status(None)), t(Key::TbBatteryOnBattery));
    }

    #[test]
    fn without_a_battery_report_the_state_falls_back_to_the_charging_flag() {
        let status =
            crate::system_info::SystemStatus { battery_pct: Some(40), battery_charging: true, ..Default::default() };
        assert_eq!(battery_state_line(&status), t(Key::TbBatteryCharging));
        assert!(radio_on(Detail::Battery, &status));
        assert!(!has_radio(Detail::Battery));
        assert_eq!(more_slot(Detail::Battery), Slot::QsBattery);
    }

    fn sink(name: &str, active: bool) -> AudioOutput {
        AudioOutput { name: name.to_string(), description: name.to_uppercase(), icon: "audio-speakers", active }
    }

    fn two_sinks() -> NetEvent {
        NetEvent::Outputs(vec![sink("speakers", true), sink("hdmi", false)])
    }

    #[test]
    fn the_volume_chevron_opens_a_list_of_audio_outputs() {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.toggle_popup(OpenPopup::QuickSettings);
        relayout(&mut bar);
        let more = bar.slot_rect(Slot::QsVolumeMore).expect("volume chevron");
        assert_eq!(bar.action_at(centre(more)), Some(TopbarAction::QsPanel(PanelAction::Open(Detail::Sound))));
        assert!(bar.open_detail(Detail::Sound));
        relayout(&mut bar);
        assert!(bar.slot_rect(Slot::QsDetailToggle).is_none(), "outputs have no radio");
        assert!(bar.slot_rect(Slot::QsDetailRow(0)).is_none(), "nothing listed before the scan");

        assert!(bar.apply_net_event(two_sinks()));
        relayout(&mut bar);
        assert!(bar.slot_rect(Slot::QsDetailRow(1)).is_some());
        assert_eq!(bar.detail_row(1), Some(RowTarget::Output(sink("hdmi", false))));
        assert!(!bar.apply_net_event(two_sinks()));

        bar.set_focused(Some(Slot::QsBack));
        bar.close_detail();
        assert_eq!(bar.focused, Some(Slot::QsVolumeMore), "focus returns to the chevron");
    }

    #[test]
    fn a_failed_output_switch_marks_its_row() {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.open_detail(Detail::Sound);
        bar.apply_net_event(two_sinks());
        bar.begin_connect("hdmi");
        assert_eq!(bar.qs_panel.status_for("hdmi"), RowStatus::Connecting);
        bar.apply_net_event(NetEvent::OutputResult { name: "hdmi".into(), result: Err(ConnectError::Failed) });
        assert_eq!(bar.qs_panel.status_for("hdmi"), RowStatus::Failed(ConnectError::Failed));
    }

    #[test]
    fn the_detail_list_stays_inside_the_popup() {
        let height = qs_geometry(&super::super::QsShape::default()).height;
        for radio in [true, false] {
            let geometry = detail_geometry(height, radio);
            assert!(geometry.body.size.h >= (DETAIL_CARD + DETAIL_CARD_GAP) * 6, "room for a useful list");
            assert!(geometry.body.loc.y + geometry.body.size.h < geometry.divider_bottom);
            assert_eq!(geometry.settings.loc.y + geometry.settings.size.h, height - QS_PAD);
            assert!(geometry.title_left < geometry.title_right);
            assert!(geometry.body.loc.y > geometry.back.loc.y + geometry.back.size.h);
        }
        assert_eq!(
            detail_geometry(height, true).toggle.size,
            smithay::utils::Size::from((DETAIL_SWITCH_W, DETAIL_SWITCH_H))
        );
        assert_eq!(prompt_height(true) - prompt_height(false), PROMPT_ERROR + SP2);
    }

    #[test]
    fn every_detail_view_draws_without_icons_at_both_scales() {
        let mut bar = open_wifi();
        bar.toggle_prompt("Neighbour");
        bar.apply_net_event(NetEvent::WifiResult {
            ssid: "Neighbour".to_string(),
            result: Err(ConnectError::WrongPassword),
        });
        assert!(bar.qs_panel.prompt.is_some() && bar.qs_panel.failed(2), "the drawer reopens with its error");
        let device = |mac: &str, connected: bool, battery: Option<u8>| BtDevice {
            mac: mac.to_string(),
            name: mac.to_uppercase(),
            kind: crate::qs_net::BtKind::Headphones,
            connected,
            battery,
        };
        bar.apply_net_event(NetEvent::Bluetooth(vec![device("aa", true, Some(80)), device("bb", false, None)]));
        bar.apply_net_event(two_sinks());
        bar.apply_net_event(NetEvent::Inputs(Vec::new()));
        bar.apply_net_event(NetEvent::ChargeLimit(Some(charge_limit(true))));
        assert!(bar.toggle_reveal());
        bar.begin_connect("hdmi");
        let status = crate::system_info::SystemStatus {
            wifi_enabled: true,
            bluetooth_on: true,
            volume_muted: true,
            battery_pct: Some(9),
            power_profile: Some(PowerProfile::PowerSaver),
            battery: Some(BatteryInfo {
                pct: 9,
                state: BatteryState::Discharging,
                minutes: None,
                health: crate::system_info::BatteryHealth { wear_pct: Some(91), ..Default::default() },
                profile: None,
            }),
            ..Default::default()
        };
        let painter = crate::paint::Painter::new(14.0, None);
        let theme = TopbarTheme::default();
        let height = qs_geometry(&bar.qs_shape(None)).height;
        for detail in [Detail::Wifi, Detail::Bluetooth, Detail::Sound, Detail::Battery] {
            for (scale, hovered) in [(1, None), (2, Some(Slot::QsPromptCancel)), (2, Some(Slot::QsDetailSettings))] {
                let ctx = QsRenderContext { painter: &painter, theme: &theme, hovered };
                let mut canvas = Canvas::new(QS_WIDTH, height, scale).expect("detail canvas");
                draw_detail(&mut canvas, &ctx, &bar.qs_panel, detail, &status);
            }
        }
    }

    fn charge_limit(enabled: bool) -> ChargeLimit {
        ChargeLimit { path: "/org/freedesktop/UPower/devices/battery_BAT1".to_string(), enabled, end: 80 }
    }

    fn open_battery() -> Topbar {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.set_system_status(crate::system_info::SystemStatus {
            battery_pct: Some(60),
            power_profile: Some(PowerProfile::Balanced),
            ..Default::default()
        });
        bar.toggle_popup(OpenPopup::QuickSettings);
        relayout(&mut bar);
        assert!(bar.open_detail(Detail::Battery));
        relayout(&mut bar);
        bar
    }

    #[test]
    fn the_charge_limit_card_shows_only_where_upower_supports_it() {
        let mut bar = open_battery();
        assert!(bar.slot_rect(Slot::QsChargeLimit).is_none(), "no supported battery, no switch");
        assert_eq!(bar.charge_limit(), None);
        assert!(bar.apply_net_event(NetEvent::ChargeLimit(Some(charge_limit(false)))));
        assert!(!bar.apply_net_event(NetEvent::ChargeLimit(Some(charge_limit(false)))), "an unchanged reading");
        relayout(&mut bar);
        let card = bar.slot_rect(Slot::QsChargeLimit).expect("charge limit card");
        let last_profile = bar.slot_rect(Slot::QsProfile(PowerProfile::Performance)).expect("profiles");
        assert!(card.loc.y > last_profile.loc.y + last_profile.size.h);
        assert!(card.loc.y + card.size.h < bar.slot_rect(Slot::QsDetailSettings).expect("settings").loc.y);
        assert_eq!(bar.action_at(centre(card)), Some(TopbarAction::QsPanel(PanelAction::ChargeLimit)));
        assert_eq!(bar.charge_limit(), Some(charge_limit(false)));

        bar.begin_connect(CHARGE_LIMIT_KEY);
        assert_eq!(bar.qs_panel.status_for(CHARGE_LIMIT_KEY), RowStatus::Connecting);
        bar.apply_net_event(NetEvent::ChargeLimitResult(Err(ConnectError::Failed)));
        assert_eq!(bar.qs_panel.status_for(CHARGE_LIMIT_KEY), RowStatus::Failed(ConnectError::Failed));
        bar.close_detail();
        assert_eq!(bar.charge_limit(), None, "only the Battery view offers it");
    }

    #[test]
    fn the_eye_shows_the_typed_password_and_a_new_prompt_starts_hidden() {
        let mut bar = open_wifi();
        bar.toggle_prompt("Neighbour");
        relayout(&mut bar);
        for ch in "pa55".chars() {
            bar.prompt_key(FieldKey::Type(ch));
        }
        let field = bar.slot_rect(Slot::QsPasswordField).expect("field");
        let eye = bar.slot_rect(Slot::QsPromptReveal).expect("eye");
        assert!(field.contains(eye.loc), "the eye sits inside the field");
        assert_eq!(
            bar.action_at(centre(eye)),
            Some(TopbarAction::QsPanel(PanelAction::PromptReveal)),
            "it wins over the field"
        );
        let prompt = || bar.qs_panel.prompt.as_ref().expect("prompt");
        assert_eq!(prompt().shown(), "\u{2022}".repeat(4));
        assert!(bar.toggle_reveal());
        assert_eq!(bar.qs_panel.prompt.as_ref().expect("prompt").shown(), "pa55");
        assert!(format!("{:?}", bar.qs_panel.prompt.as_ref().expect("prompt")).contains("len: 4"));
        assert!(
            !format!("{:?}", bar.qs_panel.prompt.as_ref().expect("prompt")).contains("pa55"),
            "Debug never prints it"
        );
        assert_eq!(bar.qs_key_mode(), QsKeyMode::Typing, "the field keeps the keyboard");
        bar.prompt_key(FieldKey::Cancel);
        bar.toggle_prompt("Neighbour");
        assert!(!bar.qs_panel.prompt.as_ref().expect("prompt").reveal);
        bar.prompt_key(FieldKey::Cancel);
        assert!(!bar.toggle_reveal(), "no field, nothing to show");
    }

    #[test]
    fn telemetry_values_read_in_tenths() {
        assert_eq!(tenths(84), "8.4");
        assert_eq!(tenths(525), "52.5");
        assert_eq!(tenths(0), "0.0");
    }
}
