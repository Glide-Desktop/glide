use super::{
    Anim, Canvas, Color, E_OUT, GlyphPainter, LocalRect, Logical, MemoryRenderBufferRenderElement, OpenPopup, Placed,
    Point, QS_PAD, QS_SWITCH_H, QS_SWITCH_W, QS_WIDTH, QsRenderContext, Rectangle, Scale, Slot, SwitchMotion,
    TextAlign, TextStyle, Topbar, TopbarTheme, band, draw_qs_switch, local_rect, motion, push_placed, qs_geometry,
    text_w,
};

use crate::i18n::{Key, t};
use crate::launcher::field::{CaretMove, TextField};
use crate::power::PowerAction;
use crate::qs_net::{AudioOutput, BtDevice, ConnectError, NetEvent, Security, WifiNetwork};
use crate::system_info::{BatteryInfo, BatteryState, PowerProfile};
use glide_theme::shape::{R_ROUND, R1};
use glide_theme::spacing::{SP1, SP2};
use smithay::input::keyboard::Keysym;

const DETAIL_HEADER: i32 = 32;
const DETAIL_ROW: i32 = 32;
const DETAIL_SECTION: i32 = 16;
const DETAIL_ICON: i32 = 16;
const TRAILING_ICON: i32 = 14;
const TRAILING_GLYPH: f32 = 12.0;
const BATTERY_BAR: i32 = 6;

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

const fn profile_icon(profile: PowerProfile) -> &'static str {
    match profile {
        PowerProfile::PowerSaver => "power-profile-power-saver",
        PowerProfile::Balanced => "power-profile-balanced",
        PowerProfile::Performance => "power-profile-performance",
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
}

impl std::fmt::Debug for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prompt")
            .field("ssid", &self.ssid)
            .field("len", &self.field.text().chars().count())
            .field("caps_lock", &self.caps_lock)
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

    pub(super) fn active_output(&self) -> Option<&AudioOutput> {
        self.outputs.iter().find(|output| output.active)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowKind {
    Network(usize),
    Device(usize),
    Output(usize),
    Prompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DetailGeometry {
    pub(super) back: LocalRect,
    pub(super) toggle: LocalRect,
    pub(super) title_left: i32,
    pub(super) title_right: i32,
    pub(super) divider_top: i32,
    pub(super) section: LocalRect,
    pub(super) list: LocalRect,
    pub(super) divider_bottom: i32,
    pub(super) settings: LocalRect,
}

pub(super) fn detail_geometry(height: i32) -> DetailGeometry {
    let row_w = QS_WIDTH - QS_PAD * 2;
    let back = local_rect(QS_PAD, QS_PAD, DETAIL_HEADER, DETAIL_HEADER);
    let toggle = local_rect(
        QS_WIDTH - QS_PAD - QS_SWITCH_W,
        QS_PAD + (DETAIL_HEADER - QS_SWITCH_H) / 2,
        QS_SWITCH_W,
        QS_SWITCH_H,
    );
    let divider_top = QS_PAD + DETAIL_HEADER + SP2;
    let section = local_rect(QS_PAD + SP2, divider_top + 1 + SP2, row_w - SP2 * 2, DETAIL_SECTION);
    let settings = local_rect(QS_PAD, height - QS_PAD - DETAIL_ROW, row_w, DETAIL_ROW);
    let divider_bottom = settings.loc.y - SP1 - 1;
    let list_y = section.loc.y + section.size.h + SP1;
    let list = local_rect(QS_PAD, list_y, row_w, (divider_bottom - SP1 - list_y).max(0));
    DetailGeometry {
        back,
        toggle,
        title_left: back.loc.x + back.size.w + SP2,
        title_right: toggle.loc.x - SP2,
        divider_top,
        section,
        list,
        divider_bottom,
        settings,
    }
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

pub(super) fn detail_rows(
    panel: &DetailPanel, detail: Detail, status: &crate::system_info::SystemStatus, list: LocalRect,
) -> Vec<(Slot, LocalRect, RowKind)> {
    if !radio_on(detail, status) {
        return Vec::new();
    }
    let capacity = usize::try_from(list.size.h / DETAIL_ROW).unwrap_or(0);
    let mut kinds = Vec::new();
    match detail {
        Detail::Wifi => {
            let room = if panel.prompt.is_some() { capacity.saturating_sub(1) } else { capacity };
            for (index, network) in panel.wifi.iter().enumerate().take(room) {
                kinds.push(RowKind::Network(index));
                if panel.prompt.as_ref().is_some_and(|prompt| prompt.ssid == network.ssid) {
                    kinds.push(RowKind::Prompt);
                }
            }
        }
        Detail::Bluetooth => {
            kinds.extend((0..panel.devices.len().min(capacity)).map(RowKind::Device));
        }
        Detail::Sound => {
            kinds.extend((0..panel.outputs.len().min(capacity)).map(RowKind::Output));
        }
        Detail::Battery => {}
    }
    let mut y = list.loc.y;
    kinds
        .into_iter()
        .filter_map(|kind| {
            let slot = match kind {
                RowKind::Network(index) | RowKind::Device(index) | RowKind::Output(index) => {
                    Slot::QsDetailRow(u16::try_from(index).ok()?)
                }
                RowKind::Prompt => Slot::QsPasswordField,
            };
            let rect = local_rect(list.loc.x, y, list.size.w, DETAIL_ROW);
            y += DETAIL_ROW;
            Some((slot, rect, kind))
        })
        .collect()
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

    fn row_slot_for(&self, key: &str) -> Option<Slot> {
        let index = match self.qs_panel.view? {
            Detail::Wifi => self.qs_panel.wifi.iter().position(|n| n.ssid == key)?,
            Detail::Bluetooth => self.qs_panel.devices.iter().position(|d| d.mac == key)?,
            Detail::Sound => self.qs_panel.outputs.iter().position(|o| o.name == key)?,
            Detail::Battery => return None,
        };
        Some(Slot::QsDetailRow(u16::try_from(index).ok()?))
    }

    fn settle_focus_on_row(&mut self, key: &str) {
        if self.focused.is_some() {
            let slot = self.row_slot_for(key);
            self.set_focused(slot);
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
            self.qs_panel.prompt =
                Some(Prompt { ssid: ssid.to_string(), field: TextField::default(), caps_lock: false });
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
                    self.qs_panel.prompt = Some(Prompt { ssid, field: TextField::default(), caps_lock: false });
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
            NetEvent::OutputResult { name, result } => self.settle_pending(&name, result),
        }
        self.qs_panel.bump();
        self.touch();
        true
    }

    pub(super) fn detail_slots(&self) -> Option<Vec<(Slot, LocalRect)>> {
        let detail = self.qs_panel.view?;
        let geometry = detail_geometry(qs_geometry(&self.qs_shape(None)).height);
        let mut slots = vec![(Slot::QsBack, geometry.back)];
        if has_radio(detail) {
            slots.push((Slot::QsDetailToggle, geometry.toggle));
        }
        slots.extend(
            detail_rows(&self.qs_panel, detail, &self.system_status, geometry.list)
                .into_iter()
                .map(|(slot, rect, _)| (slot, rect)),
        );
        slots.push((Slot::QsDetailSettings, geometry.settings));
        Some(slots)
    }
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
    let geometry = detail_geometry(c.h);
    draw_detail_header(c, ctx, &geometry, detail, status);

    let divider = |c: &mut Canvas<'_>, y: i32| {
        c.fill(local_rect(QS_PAD, y, c.w - QS_PAD * 2, 1), ctx.theme.border, 0.0);
    };
    divider(c, geometry.divider_top);

    if detail == Detail::Battery {
        draw_battery_body(c, ctx, &geometry, status);
    } else {
        draw_list_body(c, ctx, &geometry, (panel, detail), status);
    }

    divider(c, geometry.divider_bottom);
    let settings = geometry.settings;
    let settings_label = t(match detail {
        Detail::Wifi => Key::QsWifiSettings,
        Detail::Bluetooth => Key::QsBluetoothSettings,
        Detail::Battery => Key::TbBatterySettings,
        Detail::Sound => Key::QsSoundSettings,
    });
    let (left, right, top, bottom) = band(settings);
    c.text(
        ctx.painter,
        (left + SP2, right - SP2, top, bottom),
        TextAlign::Left,
        settings_label,
        ctx.theme.label,
        &TextStyle::CALL,
    );
}

fn battery_low(theme: &TopbarTheme, status: &crate::system_info::SystemStatus) -> bool {
    status.battery_pct.is_some_and(|pct| !status.battery_charging && pct <= theme.battery_low_pct)
}

fn draw_battery_body(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, geometry: &DetailGeometry, status: &crate::system_info::SystemStatus,
) {
    let theme = ctx.theme;
    let pct = status.battery_pct.unwrap_or(0);
    let section = geometry.section;
    let track =
        local_rect(section.loc.x, section.loc.y + (section.size.h - BATTERY_BAR) / 2, section.size.w, BATTERY_BAR);
    c.fill(track, theme.track, R_ROUND);
    let fill_colour = if battery_low(theme, status) {
        theme.danger
    } else if status.battery_charging {
        theme.ok
    } else {
        theme.active_background
    };
    let level_w = track.size.w * i32::from(pct.min(100)) / 100;
    if level_w > 0 {
        let level = local_rect(track.loc.x, track.loc.y, level_w.max(BATTERY_BAR), BATTERY_BAR);
        c.fill(level, fill_colour, R_ROUND);
    }

    let list = geometry.list;
    let row = |index: i32| local_rect(list.loc.x, list.loc.y + DETAIL_ROW * index, list.size.w, DETAIL_ROW);
    let state = battery_state_line(status);
    let icon = super::battery_icon_name(pct, status.battery_charging);
    let spec = RowSpec {
        icon: &icon,
        glyph: crate::paint::draw_power_glyph_over,
        title: &state,
        active: false,
        trailing: None,
    };
    draw_row_body(c, ctx, row(0), &spec);
    if let Some(profile) = status.battery.as_ref().and_then(|info| info.profile) {
        let spec = RowSpec {
            icon: profile_icon(profile),
            title: t(Key::TbPowerMode),
            trailing: Some((t(profile_label(profile)).to_string(), theme.muted)),
            ..spec
        };
        draw_row_body(c, ctx, row(1), &spec);
    }
}

fn draw_list_body(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, geometry: &DetailGeometry, (panel, detail): (&DetailPanel, Detail),
    status: &crate::system_info::SystemStatus,
) {
    let section_label = t(match detail {
        Detail::Wifi => Key::QsNetworks,
        Detail::Bluetooth | Detail::Battery => Key::QsDevices,
        Detail::Sound => Key::QsOutput,
    });
    let muted = ctx.theme.muted;
    let section = band(geometry.section);
    c.text(ctx.painter, section, TextAlign::Left, section_label, muted, &TextStyle::MICRO);

    let rows = detail_rows(panel, detail, status, geometry.list);
    if rows.is_empty() {
        let message = t(empty_message(panel, detail, status));
        let (left, right, top, _) = band(geometry.list);
        let area = (left, right, top, top + DETAIL_ROW * 2);
        c.text(ctx.painter, area, TextAlign::Center, message, muted, &TextStyle::CAP);
    }
    for (_, rect, kind) in rows {
        match kind {
            RowKind::Network(index) => {
                if let Some(network) = panel.wifi.get(index) {
                    draw_network_row(c, ctx, rect, network, panel.status_for(&network.ssid));
                }
            }
            RowKind::Device(index) => {
                if let Some(device) = panel.devices.get(index) {
                    draw_device_row(c, ctx, rect, device, panel.status_for(&device.mac));
                }
            }
            RowKind::Output(index) => {
                if let Some(output) = panel.outputs.get(index) {
                    draw_output_row(c, ctx, rect, output, panel.status_for(&output.name));
                }
            }
            RowKind::Prompt => {
                if let Some(prompt) = panel.prompt.as_ref() {
                    draw_password_row(c, ctx, rect, &prompt.field, prompt.caps_lock);
                }
            }
        }
    }
}

fn empty_message(panel: &DetailPanel, detail: Detail, status: &crate::system_info::SystemStatus) -> Key {
    match detail {
        Detail::Wifi if !status.wifi_enabled => Key::QsWifiIsOff,
        Detail::Bluetooth if !status.bluetooth_on => Key::QsBluetoothIsOff,
        Detail::Wifi if !panel.wifi_ready => Key::QsSearching,
        Detail::Bluetooth if !panel.devices_ready => Key::QsSearching,
        Detail::Sound if !panel.outputs_ready => Key::QsSearching,
        Detail::Wifi => Key::QsNoNetworks,
        Detail::Bluetooth | Detail::Battery => Key::QsNoDevices,
        Detail::Sound => Key::QsNoOutputs,
    }
}

fn draw_detail_header(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, geometry: &DetailGeometry, detail: Detail,
    status: &crate::system_info::SystemStatus,
) {
    let back = geometry.back;
    c.fill(back, ctx.theme.track_soft, R_ROUND);
    let icon_at = (back.loc.x + (back.size.w - DETAIL_ICON) / 2, back.loc.y + (back.size.h - DETAIL_ICON) / 2);
    let label = ctx.theme.label;
    let head = &TextStyle::HEAD;
    if !c.icon("go-previous", DETAIL_ICON, icon_at, label) {
        c.text(ctx.painter, band(back), TextAlign::Center, "\u{2039}", label, head);
    }
    let title = t(match detail {
        Detail::Wifi => Key::WiFi,
        Detail::Bluetooth => Key::Bluetooth,
        Detail::Battery => Key::TbBattery,
        Detail::Sound => Key::SettingsSound,
    });
    let (top, bottom) = (back.loc.y, back.loc.y + back.size.h);
    let title_band = (geometry.title_left, geometry.title_right, top, bottom);
    c.text(ctx.painter, title_band, TextAlign::Left, title, label, head);
    let toggle = geometry.toggle;
    if !has_radio(detail) {
        let (pct, colour) = if detail == Detail::Sound {
            let colour = if status.volume_muted { ctx.theme.muted } else { label };
            (format!("{}%", status.volume_pct), colour)
        } else {
            let pct = status.battery_pct.map_or_else(String::new, |pct| format!("{pct}%"));
            (pct, if battery_low(ctx.theme, status) { ctx.theme.danger } else { label })
        };
        let pct_band = (geometry.title_right, toggle.loc.x + toggle.size.w, top, bottom);
        c.text(ctx.painter, pct_band, TextAlign::Right, &pct, colour, &TextStyle::NUM);
        return;
    }
    let position = f64::from(u8::from(radio_on(detail, status)));
    let switch = SwitchMotion { thumb: position, track: position };
    draw_qs_switch(c, ctx.theme, (toggle.loc.x, toggle.loc.y), switch);
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

struct RowSpec<'a> {
    icon: &'a str,
    glyph: GlyphPainter,
    title: &'a str,
    active: bool,
    trailing: Option<(String, Color)>,
}

fn icon_or_glyph(c: &mut Canvas<'_>, rect: LocalRect, (name, glyph): (&str, GlyphPainter), side: f32, colour: Color) {
    if !c.icon(name, rect.size.w, (rect.loc.x, rect.loc.y), colour) {
        c.glyph(rect, |b, w, h, s| glyph(b, w, h, side, 1.4, s, colour));
    }
}

fn draw_row_body(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, spec: &RowSpec<'_>) {
    let icon_colour = if spec.active { ctx.theme.active_background } else { ctx.theme.muted };
    let icon_rect =
        local_rect(rect.loc.x + SP2, rect.loc.y + (rect.size.h - DETAIL_ICON) / 2, DETAIL_ICON, DETAIL_ICON);
    icon_or_glyph(c, icon_rect, (spec.icon, spec.glyph), 14.0, icon_colour);
    let (_, mut right, top, bottom) = band(rect);
    right -= SP2;
    if let Some((text, colour)) = spec.trailing.as_ref() {
        let cap = &TextStyle::CAP;
        let width = text_w(ctx.painter, text, cap);
        c.text(ctx.painter, (right - width, right, top, bottom), TextAlign::Right, text, *colour, cap);
        right -= width + SP2;
    }
    let style = if spec.active { TextStyle::HEAD } else { TextStyle::CALL };
    c.text(
        ctx.painter,
        (icon_rect.loc.x + DETAIL_ICON + SP2, right, top, bottom),
        TextAlign::Left,
        spec.title,
        ctx.theme.label,
        &style,
    );
}

fn trailing_icon(c: &mut Canvas<'_>, rect: LocalRect, right: i32, icon: (&str, GlyphPainter), colour: Color) -> i32 {
    let at =
        local_rect(right - TRAILING_ICON, rect.loc.y + (rect.size.h - TRAILING_ICON) / 2, TRAILING_ICON, TRAILING_ICON);
    icon_or_glyph(c, at, icon, TRAILING_GLYPH, colour);
    right - TRAILING_ICON - SP2
}

fn draw_network_row(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, network: &WifiNetwork, status: RowStatus,
) {
    let secured = network.security.is_secured();
    let icons_w = (TRAILING_ICON + SP2) * (i32::from(secured) + i32::from(network.connected));
    let inset = local_rect(rect.loc.x, rect.loc.y, rect.size.w - icons_w, rect.size.h);
    let enterprise = (network.security == Security::Enterprise && !network.connected && !network.saved)
        .then(|| (t(Key::QsEnterprise).to_string(), ctx.theme.muted));
    let spec = RowSpec {
        icon: network.icon(),
        glyph: crate::paint::draw_wifi_waves_glyph_over,
        title: &network.ssid,
        active: network.connected,
        trailing: status_trailing(ctx.theme, status).or(enterprise),
    };
    draw_row_body(c, ctx, inset, &spec);
    let mut right = rect.loc.x + rect.size.w - SP2;
    if network.connected {
        let tick = ("object-select", crate::paint::draw_tick_glyph_over as GlyphPainter);
        right = trailing_icon(c, rect, right, tick, ctx.theme.active_background);
    }
    if secured {
        let lock = ("network-wireless-encrypted", crate::paint::draw_lock_glyph_over as GlyphPainter);
        trailing_icon(c, rect, right, lock, ctx.theme.muted);
    }
}

fn draw_device_row(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, device: &BtDevice, status: RowStatus,
) {
    let trailing = status_trailing(ctx.theme, status).or_else(|| {
        device.connected.then(|| {
            let text = device.battery.map_or_else(|| t(Key::Connected).to_string(), |pct| format!("{pct}%"));
            (text, ctx.theme.muted)
        })
    });
    let spec = RowSpec {
        icon: device.kind.icon(),
        glyph: crate::paint::draw_bluetooth_glyph_over,
        title: &device.name,
        active: device.connected,
        trailing,
    };
    draw_row_body(c, ctx, rect, &spec);
}

fn draw_output_row(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, output: &AudioOutput, status: RowStatus,
) {
    let tick_w = if output.active { TRAILING_ICON + SP2 } else { 0 };
    let inset = local_rect(rect.loc.x, rect.loc.y, rect.size.w - tick_w, rect.size.h);
    let spec = RowSpec {
        icon: output.icon,
        glyph: |b, w, h, side, stroke, s, c| crate::paint::draw_speaker_glyph_over(b, w, h, side, stroke, false, s, c),
        title: &output.description,
        active: output.active,
        trailing: status_trailing(ctx.theme, status),
    };
    draw_row_body(c, ctx, inset, &spec);
    if output.active {
        let tick = ("object-select", crate::paint::draw_tick_glyph_over as GlyphPainter);
        trailing_icon(c, rect, rect.loc.x + rect.size.w - SP2, tick, ctx.theme.active_background);
    }
}

fn draw_password_row(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, field: &TextField, caps_lock: bool,
) {
    let theme = ctx.theme;
    let boxed = local_rect(
        rect.loc.x + SP2 + DETAIL_ICON + SP2,
        rect.loc.y + SP1,
        rect.size.w - DETAIL_ICON - SP2 * 3,
        rect.size.h - SP1 * 2,
    );
    c.fill(boxed, theme.track_soft, R1);
    c.stroke(boxed, 1.0, R1, theme.active_background);
    let (left, mut right, top, bottom) = band(boxed);
    if caps_lock {
        let cap = &TextStyle::CAP;
        let label = t(Key::CapsLockOn);
        let width = text_w(ctx.painter, label, cap);
        c.text(
            ctx.painter,
            (right - SP2 - width, right - SP2, top, bottom),
            TextAlign::Right,
            label,
            theme.danger,
            cap,
        );
        right -= width + SP2;
    }
    let (text_left, text_right) = (left + SP2, right - SP2);
    let text_band = (text_left, text_right, top, bottom);
    let style = TextStyle::CALL;
    if field.is_empty() {
        c.text(ctx.painter, text_band, TextAlign::Left, t(Key::QsPassword), theme.faint, &style);
    } else {
        let bullets = masked(field.text());
        if field.selection().is_some() {
            let width = text_w(ctx.painter, &bullets, &style).min(text_right - text_left);
            let selection = local_rect(text_left, top + SP1, width, boxed.size.h - SP1 * 2);
            c.fill(selection, theme.active_soft, 0.0);
        }
        c.text(ctx.painter, text_band, TextAlign::Left, &bullets, theme.label, &style);
    }
    let before = masked(&field.text()[..field.caret()]);
    let caret_x = (text_left + text_w(ctx.painter, &before, &style)).min(text_right);
    let caret = local_rect(caret_x, top + SP1 + 2, 1, boxed.size.h - SP1 * 2 - 4);
    c.fill(caret, theme.active_background, 0.0);
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
        assert_eq!(second.loc.y, first.loc.y + DETAIL_ROW);
        assert_eq!(bar.detail_row(1), Some(RowTarget::Network(network("Cafe", false, false))));
        assert_eq!(bar.detail_row(9), None);
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
        assert_eq!(field.loc.y, row.loc.y + DETAIL_ROW);
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
        let info = BatteryInfo { pct: 50, state: BatteryState::Discharging, minutes: Some(135), profile: None };
        let eta = t(Key::TbBatteryTimeLeft).replace("{}", &crate::i18n::duration_label(135));
        assert_eq!(time_line(&info), format!("{} \u{b7} {eta}", t(Key::TbBatteryOnBattery)));
        assert!(crate::i18n::duration_label(135).contains("15"));
        let full = BatteryInfo { state: BatteryState::Full, minutes: Some(10), ..info };
        assert_eq!(time_line(&full), t(Key::TbBatteryFull));
    }

    #[test]
    fn the_battery_row_shows_the_estimate_alone_when_there_is_one() {
        let info = BatteryInfo { pct: 50, state: BatteryState::Discharging, minutes: Some(135), profile: None };
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
        let geometry = detail_geometry(height);
        assert!(geometry.list.size.h >= DETAIL_ROW * 5);
        assert!(geometry.list.loc.y + geometry.list.size.h < geometry.divider_bottom);
        assert_eq!(geometry.settings.loc.y + geometry.settings.size.h, height - QS_PAD);
        assert!(geometry.title_left < geometry.title_right);
    }
}
