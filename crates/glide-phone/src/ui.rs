use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Mutex;

use glide_sdk::iced::widget::{column, row};
use glide_sdk::prelude::*;
use glide_sdk::widget::{Icon, Severity, Size, Tone};

use crate::client::PhoneDaemonProxyBlocking;
use crate::plugins::battery::BatteryState;
use crate::plugins::notifications::ActiveNotification;
use crate::protocol::identity::DeviceType;
use crate::protocol::pair::PairState;
use crate::state::PhoneManager;

static GLOBAL_MANAGER: OnceLock<Arc<Mutex<PhoneManager>>> = OnceLock::new();

pub fn set_global_manager(manager: Arc<Mutex<PhoneManager>>) {
    let _ = GLOBAL_MANAGER.set(manager);
}

#[must_use]
pub fn global_manager() -> Option<&'static Arc<Mutex<PhoneManager>>> {
    GLOBAL_MANAGER.get()
}

fn get_proxy() -> Option<PhoneDaemonProxyBlocking<'static>> {
    static PROXY: OnceLock<Option<PhoneDaemonProxyBlocking<'static>>> = OnceLock::new();
    PROXY
        .get_or_init(|| {
            let conn = zbus::blocking::Connection::session().ok()?;
            PhoneDaemonProxyBlocking::new(&conn).ok()
        })
        .clone()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneCardData {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub pair_state: PairState,
    pub battery: Option<BatteryState>,
    pub notifications: Vec<ActiveNotification>,
    #[serde(default)]
    pub peer_fingerprint: Option<String>,
    #[serde(default)]
    pub connected: bool,
}

fn short_verification_code(fp: &str) -> String {
    fp.chars().filter(char::is_ascii_hexdigit).take(8).collect::<String>().to_uppercase()
}

pub struct PhoneApp {
    pub devices: Vec<PhoneCardData>,
    pub selected: Option<String>,
    pub ringing_local: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectDevice(String),
    Pair(String),
    Unpair(String),
    RingPhone(String),
    StopAlarm,
    DismissNotification(String, String),
    ClearNotifications(String),
    Refresh,
}

impl PhoneApp {
    fn pair(&self, id: &str, shell: &mut Shell<Message>) {
        let accepted = if let Some(proxy) = get_proxy() {
            proxy.accept_pair(id).unwrap_or(false)
        } else if let Some(m) = global_manager() {
            m.try_lock().is_ok_and(|mut guard| guard.accept_pair(id, String::new()).is_ok())
        } else {
            false
        };
        if !accepted {
            shell.toast(shell.t("Не вдалося надіслати запит", "Couldn't send pairing request"));
        } else if self.devices.iter().any(|device| device.id == id && !device.connected) {
            shell.toast(shell.t("Запит буде надіслано після підключення", "Pairing request queued until connected"));
        } else {
            shell.toast(shell.t("Запит на з'єднання надіслано", "Pairing request sent"));
        }
    }

    fn view_alarm_banner(shell: &Shell<Message>) -> Element<'_, Message> {
        let paint = shell.paint();
        let stop_btn = widget::compact_button(
            paint,
            shell.t("Зупинити сигнал", "Stop Alarm"),
            Tone::Danger,
            Some(Message::StopAlarm),
        );
        widget::noted_banner(
            paint,
            Severity::Danger,
            Icon::Bell,
            shell.t("Телефон шукає цей комп'ютер!", "Phone is ringing this PC!"),
            shell.t(
                "На телефоні активовано пошук пристрою. Натисніть кнопку праворуч, щоб вимкнути звуковий сигнал.",
                "Find My Device was triggered from your phone. Click the button to silence the alarm.",
            ),
            vec![stop_btn],
        )
    }

    fn device_status_badge<'a>(dev: &'a PhoneCardData, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let paint = shell.paint();
        let (status_text, status_sev) = match dev.pair_state {
            PairState::Paired if dev.connected => (shell.t("Підключено", "Connected"), Severity::Ok),
            PairState::Paired => (shell.t("Офлайн", "Offline"), Severity::Neutral),
            PairState::RequestedByPeer => (shell.t("Запит на з'єднання", "Pairing requested"), Severity::Warn),
            PairState::RequestedByLocal => (shell.t("Очікування…", "Waiting…"), Severity::Accent),
            PairState::Unpaired => (shell.t("Доступний", "Available"), Severity::Neutral),
        };
        widget::status_badge(paint, status_text, status_sev)
    }

    fn device_subtitle(dev: &PhoneCardData, shell: &Shell<Message>) -> String {
        let kind_label = match dev.device_type {
            DeviceType::Phone => shell.t("Смартфон", "Smartphone"),
            DeviceType::Tablet => shell.t("Планшет", "Tablet"),
            DeviceType::Laptop => shell.t("Ноутбук", "Laptop"),
            DeviceType::Desktop => shell.t("Комп'ютер", "Desktop"),
            DeviceType::Tv => shell.t("Телевізор", "TV"),
            DeviceType::Unknown => shell.t("Пристрій", "Device"),
        };

        if matches!(dev.pair_state, PairState::RequestedByPeer | PairState::RequestedByLocal)
            && let Some(fp) = &dev.peer_fingerprint
        {
            let code = short_verification_code(fp);
            if !code.is_empty() {
                return format!("{kind_label} · {}: {code}", shell.t("Код перевірки", "Verification key"));
            }
        }

        if let Some(b) = &dev.battery {
            let state = if b.is_charging {
                shell.t("заряджається", "charging")
            } else if b.is_low || b.charge <= 15 {
                shell.t("низький заряд", "low battery")
            } else {
                shell.t("від батареї", "on battery")
            };
            format!("{kind_label} · {}% ({state})", b.charge)
        } else if dev.connected {
            format!("{kind_label} · {}", shell.t("Активне з'єднання LAN", "Active LAN session"))
        } else {
            kind_label.to_string()
        }
    }

    fn device_prefix<'a>(dev: &PhoneCardData, paint: &Paint) -> Element<'a, Message> {
        let active = dev.pair_state == PairState::Paired && dev.connected;
        if active {
            widget::icon_tile(Icon::Phone, paint.acc_bg(), 28.0)
        } else if matches!(dev.pair_state, PairState::RequestedByPeer | PairState::RequestedByLocal) {
            widget::icon_tile(Icon::Phone, paint.warn(), 28.0)
        } else {
            container(widget::icon(Icon::Phone, 18.0, paint.fg_2())).center_x(28.0).center_y(28.0).into()
        }
    }

    fn view_paired_device<'a>(dev: &'a PhoneCardData, shell: &'a Shell<Message>) -> Vec<Element<'a, Message>> {
        let paint = shell.paint();
        let mut actions = row![].spacing(SP2).align_y(Alignment::Center);

        if dev.connected {
            actions = actions.push(widget::glyph_button(
                paint,
                Icon::Bell,
                shell.t("Знайти", "Ring"),
                Tone::Tonal,
                Size::Small,
                Some(Message::RingPhone(dev.id.clone())),
            ));
        }
        actions = actions.push(widget::compact_button(
            paint,
            shell.t("Від'єднати", "Unpair"),
            Tone::Neutral,
            Some(Message::Unpair(dev.id.clone())),
        ));

        let main_row: Element<'a, Message> = widget::ListRow::new(paint, &dev.name)
            .strong()
            .prefix(Self::device_prefix(dev, paint))
            .beside(Self::device_status_badge(dev, shell))
            .subtitle(Self::device_subtitle(dev, shell))
            .suffix(actions)
            .into();

        let mut rows = vec![main_row];

        if let Some(battery) = &dev.battery {
            let clamped = u8::try_from(battery.charge.clamp(0, 100)).unwrap_or(0);
            let ratio = f32::from(clamped) / 100.0;
            let (batt_sev, batt_color) = if battery.is_low || battery.charge <= 15 {
                (Severity::Danger, paint.danger())
            } else if battery.is_charging {
                (Severity::Ok, paint.ok())
            } else {
                (Severity::Neutral, paint.acc_bg())
            };
            let charge_str =
                if battery.is_charging { format!("{}% ⚡", battery.charge) } else { format!("{}%", battery.charge) };
            let batt_sub = if battery.is_charging {
                shell.t("Підключено до живлення", "Plugged in and charging")
            } else if battery.is_low || battery.charge <= 15 {
                shell.t("Рекомендується підключити зарядний пристрій", "Low battery level")
            } else {
                shell.t("Працює від акумулятора", "Discharging")
            };
            let bar_and_badge = row![
                container(widget::usage_bar(paint, &[(ratio, batt_color)], 6.0)).width(120.0),
                widget::badge(paint, charge_str, batt_sev),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center);

            let batt_row: Element<'a, Message> = widget::ListRow::new(paint, shell.t("Заряд батареї", "Battery"))
                .subtitle(batt_sub)
                .suffix(bar_and_badge)
                .into();
            rows.push(batt_row);
        }

        let mut group = widget::Group::new(paint).title(shell.t("Підключений пристрій", "Connected Device"));
        for r in rows {
            group = group.push(r);
        }

        vec![group.into(), Self::view_device_notifications(dev, shell)]
    }

    fn view_device_notifications<'a>(dev: &'a PhoneCardData, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let paint = shell.paint();
        let count = u32::try_from(dev.notifications.len()).unwrap_or(99);
        let mut notif_group = widget::Group::new(paint).title(shell.t("Сповіщення", "Notifications"));

        if dev.notifications.is_empty() {
            notif_group = notif_group.push(widget::empty_note(
                paint,
                Icon::Bell,
                shell.t("Немає нових сповіщень", "No active notifications"),
                shell.t(
                    "Нові сповіщення з вашого смартфона автоматично з'являться тут.",
                    "Notifications from your phone will appear here as they arrive.",
                ),
            ));
        } else {
            let header_suffix = row![
                widget::count_badge(paint, count),
                widget::compact_button(
                    paint,
                    shell.t("Очистити", "Clear all"),
                    Tone::Neutral,
                    Some(Message::ClearNotifications(dev.id.clone())),
                ),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center);
            notif_group = notif_group.suffix(header_suffix);

            for n in &dev.notifications {
                let app_label = if n.app_name.is_empty() { "Phone" } else { n.app_name.as_str() };
                let title_line = if n.title.is_empty() {
                    app_label.to_string()
                } else if n.title == app_label {
                    n.title.clone()
                } else {
                    format!("{app_label} · {}", n.title)
                };
                let dismiss_label = shell.t("Закрити сповіщення", "Dismiss notification");
                let notif_row: Element<'a, Message> = widget::ListRow::new(paint, title_line)
                    .prefix(widget::letter_tile(paint, app_label, 28.0))
                    .subtitle(n.body.as_str())
                    .suffix(widget::icon_button(
                        paint,
                        Icon::Close,
                        dismiss_label,
                        Some(Message::DismissNotification(dev.id.clone(), n.id.clone())),
                        false,
                    ))
                    .into();
                notif_group = notif_group.push(notif_row);
            }
        }

        notif_group.into()
    }

    fn view_available_device_row<'a>(dev: &'a PhoneCardData, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let paint = shell.paint();
        let mut actions = row![].spacing(SP2).align_y(Alignment::Center);
        match dev.pair_state {
            PairState::RequestedByPeer => {
                actions = actions.push(widget::compact_button(
                    paint,
                    shell.t("Прийняти", "Accept"),
                    Tone::Accent,
                    Some(Message::Pair(dev.id.clone())),
                ));
                actions = actions.push(widget::compact_button(
                    paint,
                    shell.t("Відхилити", "Reject"),
                    Tone::Neutral,
                    Some(Message::Unpair(dev.id.clone())),
                ));
            }
            PairState::RequestedByLocal => {
                actions = actions.push(widget::compact_button(
                    paint,
                    shell.t("Скасувати", "Cancel"),
                    Tone::Neutral,
                    Some(Message::Unpair(dev.id.clone())),
                ));
            }
            PairState::Unpaired | PairState::Paired => {
                actions = actions.push(widget::compact_button(
                    paint,
                    shell.t("З'єднати", "Pair"),
                    Tone::Accent,
                    Some(Message::Pair(dev.id.clone())),
                ));
            }
        }

        widget::ListRow::new(paint, &dev.name)
            .strong()
            .prefix(Self::device_prefix(dev, paint))
            .beside(Self::device_status_badge(dev, shell))
            .subtitle(Self::device_subtitle(dev, shell))
            .suffix(actions)
            .into()
    }
}

impl App for PhoneApp {
    type Message = Message;

    fn new(_shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        (Self { devices: Vec::new(), selected: None, ringing_local: false }, Task::none())
    }

    fn title(&self, shell: &Shell<Message>) -> String {
        shell.t("Підключення телефону", "Phone Link").into()
    }

    fn subscription(&self) -> Subscription<Message> {
        glide_sdk::iced::time::every(Duration::from_millis(500)).map(|_| Message::Refresh)
    }

    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::SelectDevice(id) => {
                self.selected = Some(id);
            }
            Message::Pair(id) => self.pair(&id, shell),
            Message::Unpair(id) => {
                if let Some(proxy) = get_proxy() {
                    let _ = proxy.reject_pair(&id);
                } else if let Some(m) = global_manager() {
                    let mut mgr = m.try_lock();
                    if let Ok(ref mut guard) = mgr {
                        let _ = guard.reject_pair(&id);
                    }
                }
                shell.toast(shell.t("Пристрій відключено", "Device unpaired"));
            }
            Message::RingPhone(id) => {
                if let Some(proxy) = get_proxy() {
                    let _ = proxy.ring_phone(&id);
                } else if let Some(m) = global_manager() {
                    let mgr = m.try_lock();
                    if let Ok(ref guard) = mgr {
                        let _ = guard.ring_phone(&id);
                    }
                }
                let text = shell.t("Сигнал надіслано на телефон", "Ringing phone...");
                shell.toast(text);
            }
            Message::StopAlarm => {
                self.ringing_local = false;
                if let Some(proxy) = get_proxy() {
                    let _ = proxy.stop_alarm("");
                } else if let Some(m) = global_manager() {
                    let mut mgr = m.try_lock();
                    if let Ok(ref mut guard) = mgr {
                        let ids: Vec<String> = guard.devices.keys().cloned().collect();
                        for id in ids {
                            guard.stop_alarm(&id);
                        }
                    }
                }
                shell.toast(shell.t("Сигнал зупинено", "Alarm stopped"));
            }
            Message::DismissNotification(dev_id, notif_id) => {
                if let Some(dev) = self.devices.iter_mut().find(|d| d.id == dev_id) {
                    dev.notifications.retain(|n| n.id != notif_id);
                }
                if let Some(proxy) = get_proxy() {
                    let _ = proxy.dismiss_notification(&dev_id, &notif_id);
                } else if let Some(m) = global_manager() {
                    let mut mgr = m.try_lock();
                    if let Ok(ref mut guard) = mgr {
                        let _ = guard.dismiss_notification(&dev_id, &notif_id);
                    }
                }
            }
            Message::ClearNotifications(dev_id) => {
                let ids: Vec<String> = self
                    .devices
                    .iter()
                    .find(|d| d.id == dev_id)
                    .map(|d| d.notifications.iter().map(|n| n.id.clone()).collect())
                    .unwrap_or_default();
                if let Some(dev) = self.devices.iter_mut().find(|d| d.id == dev_id) {
                    dev.notifications.clear();
                }
                for notif_id in ids {
                    if let Some(proxy) = get_proxy() {
                        let _ = proxy.dismiss_notification(&dev_id, &notif_id);
                    } else if let Some(m) = global_manager() {
                        let mut mgr = m.try_lock();
                        if let Ok(ref mut guard) = mgr {
                            let _ = guard.dismiss_notification(&dev_id, &notif_id);
                        }
                    }
                }
            }
            Message::Refresh => {
                if let Some(proxy) = get_proxy()
                    && let Ok(json_str) = proxy.list_devices_json()
                    && let Ok(cards) = serde_json::from_str::<Vec<PhoneCardData>>(&json_str)
                {
                    self.devices = cards;
                    self.ringing_local = proxy.is_ringing().unwrap_or(false);
                    return Task::none();
                }

                if let Some(m) = global_manager() {
                    let mgr = m.try_lock();
                    if let Ok(ref guard) = mgr {
                        self.ringing_local = guard.is_ringing();
                        self.devices = guard.sorted_cards();
                    }
                }
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let paint = shell.paint();

        let header = widget::page_header(
            paint,
            Icon::Phone,
            paint.acc_bg(),
            Some(shell.t("ПРИСТРОЇ", "DEVICES")),
            shell.t("Підключення телефону", "Phone Link"),
            Some(shell.t(
                "Синхронізація батареї, сповіщень, кодів 2FA та пошук пристроїв у локальній мережі",
                "Battery sync, notifications, 2FA auto-copy, and Find My Device over LAN",
            )),
            None,
        );

        let mut list_col = column![header].spacing(SP4);

        if self.ringing_local {
            list_col = list_col.push(Self::view_alarm_banner(shell));
        }

        if self.devices.is_empty() {
            let empty = widget::empty_note(
                paint,
                Icon::Phone,
                shell.t("Не знайдено пристроїв поблизу", "No nearby devices found"),
                shell.t(
                    "Переконайтеся, що ваш телефон підключено до тієї ж Wi-Fi мережі з відкритим додатком KDE Connect.",
                    "Make sure your phone is on the same Wi-Fi network with the KDE Connect app open.",
                ),
            );
            list_col = list_col.push(widget::Group::new(paint).title(shell.t("Пристрої", "Devices")).push(empty));
        } else {
            let (paired, available): (Vec<&PhoneCardData>, Vec<&PhoneCardData>) =
                self.devices.iter().partition(|d| d.pair_state == PairState::Paired);

            for dev in paired {
                for section in Self::view_paired_device(dev, shell) {
                    list_col = list_col.push(section);
                }
            }

            if !available.is_empty() {
                let mut avail_group = widget::Group::new(paint)
                    .title(shell.t("Доступні пристрої", "Available Devices"))
                    .description(shell.t(
                        "Пристрої KDE Connect, виявлені у вашій локальній мережі",
                        "KDE Connect devices discovered on your local network",
                    ));
                for dev in available {
                    avail_group = avail_group.push(Self::view_available_device_row(dev, shell));
                }
                list_col = list_col.push(avail_group);
            }
        }

        let clamp =
            container(list_col.max_width(720.0)).padding(Padding::from([24.0, 32.0]).bottom(48.0)).center_x(Fill);
        widget::pane(paint.win_bg(), 0.0, widget::scrollable(paint, clamp).height(Fill)).into()
    }
}
