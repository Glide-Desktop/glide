use std::time::Duration;

use glide_ipc::{Request, Response};
use glide_sdk::theme::TextStyle;
use glide_sdk::widget as sdk;
use iced::widget::column;
use iced::{Element, Task};

use super::common::error_banner;
use super::{Ctx, Pane, Panel};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::run::{self, blocking};
use crate::services::{ipc, keybinds};
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Hero => "Show your screen on a TV", "Покажіть екран на телевізорі";
        HeroHint => "Turn on Screen Mirroring or Miracast on the TV, then search. Found displays appear in a notification at the top of the screen.", "Увімкніть на телевізорі Screen Mirroring чи Miracast, тоді шукайте. Знайдені дисплеї з’являться в сповіщенні вгорі екрана.";
        Search => "Search for displays", "Шукати дисплеї";
        Searching => "Searching for 30 seconds…", "Пошук триває 30 секунд…";
        Offline => "Glide is not running, so it cannot search for displays", "Glide не запущено, тож шукати дисплеї неможливо";
        What => "What the TV shows", "Що показує телевізор";
        Display => "Display", "Дисплей";
        DisplayValue => "The one under the pointer", "Той, де вказівник";
        DisplayHint => "A copy of it, scaled to the TV", "Його копія, масштабована під телевізор";
        Sound => "Sound on the TV", "Звук на телевізорі";
        SoundHint => "What plays through the default output goes along", "Те, що грає через типовий вихід, іде разом із зображенням";
        Quality => "Picture", "Зображення";
        Best => "Best", "Найкраще";
        Smooth => "Smoother", "Плавніше";
        QualityHint => "Up to 1080p at 60 frames with a hardware encoder, 30 without", "До 1080p 60 кадрів з апаратним кодуванням, 30 без нього";
        SmoothHint => "At most 720p, easier on a weak Wi-Fi link or an old computer", "Не більше 720p, легше для слабкого Wi-Fi чи старого комп’ютера";
        During => "While mirroring", "Під час трансляції";
        Awake => "Keep the screen on", "Не гасити екран";
        AwakeHint => "The screen does not go dark or lock while the TV shows it", "Екран не гасне й не блокується, поки його показує телевізор";
        Banners => "Notification banners", "Банери сповіщень";
        BannersHint => "Set on the Screen Sharing page", "Налаштовуються на сторінці «Трансляція екрана»";
        Shortcut => "Keyboard shortcut", "Комбінація клавіш";
        ShortcutHint => "Also the Mirror the screen command in the launcher", "Також команда «Дублювання екрана» в лаунчері";
        NoShortcut => "Not set", "Не призначено";
        Ready => "Ready to mirror", "Готовність";
        ReadyHint => "Checked on this computer", "Перевірено на цьому комп’ютері";
        Direct => "Wi-Fi Direct", "Wi-Fi Direct";
        DirectYes => "The Wi-Fi adapter can connect to a TV without a router", "Wi-Fi-адаптер може з’єднатися з телевізором без маршрутизатора";
        DirectNo => "NetworkManager reports no Wi-Fi Direct device", "NetworkManager не бачить пристрою Wi-Fi Direct";
        Encoder => "Video encoding", "Кодування відео";
        Hardware => "Hardware", "Апаратне";
        Software => "Software", "Програмне";
        NoEncoder => "No H.264 encoder: install gst-plugins-bad or gst-plugin-va", "Немає кодувальника H.264: встановіть gst-plugins-bad або gst-plugin-va";
        Capture => "Sound capture", "Захоплення звуку";
        CaptureYes => "PipeWire can send the sound to the TV", "PipeWire може передати звук на телевізор";
        CaptureNo => "The GStreamer PipeWire plugin is missing, so the TV gets no sound", "Бракує плагіна PipeWire для GStreamer, тож телевізор лишиться без звуку";
        Yes => "Available", "Доступно";
        No => "Missing", "Бракує";
    }
}

use tr::Key as T;

const HARDWARE: [&str; 3] = ["vah264enc", "vah264lpenc", "vaapih264enc"];
const SOFTWARE: [&str; 2] = ["x264enc", "openh264enc"];
const PROBE: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoder {
    Hardware,
    Software,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readiness {
    pub direct: bool,
    pub encoder: Encoder,
    pub sound: bool,
}

#[must_use]
pub fn has_p2p(devices: &str) -> bool {
    devices.lines().any(|line| line.trim() == "wifi-p2p")
}

fn element(name: &str) -> bool {
    run::output_within("gst-inspect-1.0", &[name], PROBE).is_some()
}

fn readiness() -> Readiness {
    let direct = run::output_within("nmcli", &["-t", "-f", "TYPE", "device"], PROBE).is_some_and(|text| has_p2p(&text));
    let encoder = if HARDWARE.iter().any(|name| element(name)) {
        Encoder::Hardware
    } else if SOFTWARE.iter().any(|name| element(name)) {
        Encoder::Software
    } else {
        Encoder::Missing
    };
    Readiness { direct, encoder, sound: element("pipewiresrc") }
}

fn start_search() -> Result<(), String> {
    match ipc::request(&Request::Action { name: String::from("screen-mirroring") })? {
        Response::Error { message } => Err(message),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Checked(Readiness),
    Search,
    Started(Option<Result<(), String>>),
    Smooth(bool),
    Audio(bool),
    Awake(bool),
    Open(Panel),
}

const TABLE: &str = "cast";

#[derive(Debug, Default)]
pub struct Page {
    readiness: Option<Readiness>,
    searching: bool,
    error: Option<String>,
}

fn readiness_row<'a>(c: &Ctx, title: T, ok: bool, detail: T, value: &'a str) -> Element<'a, Msg> {
    let t = c.theme;
    let (tone, glyph, ink) = if ok { (Tone::Ok, Icon::Check, t.ok()) } else { (Tone::Warn, Icon::Warning, t.warn()) };
    Row::new(t, c.t(title))
        .subtitle(c.t(detail))
        .prefix(widgets::icon(glyph, 18.0, ink))
        .suffix(widgets::status_pill(t, tone, value))
        .build()
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "miracast",
        "wireless display",
        "mirror",
        "mirroring",
        "cast",
        "tv",
        "projector",
        "бездротовий дисплей",
        "дублювання",
        "телевізор",
        "проєктор",
        "трансляція на тв",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        Task::perform(blocking(readiness), |found| {
            Msg::Checked(found.unwrap_or(Readiness { direct: false, encoder: Encoder::Missing, sound: false }))
        })
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Smooth(true) => config.set_str(TABLE, "quality", "smooth"),
            Msg::Smooth(false) => config.remove(TABLE, "quality"),
            Msg::Audio(true) => config.remove(TABLE, "audio"),
            Msg::Audio(false) => config.set_bool(TABLE, "audio", false),
            Msg::Awake(true) => config.remove(TABLE, "keep-screen-on"),
            Msg::Awake(false) => config.set_bool(TABLE, "keep-screen-on", false),
            Msg::Checked(readiness) => self.readiness = Some(readiness),
            Msg::Search => {
                self.error = None;
                self.searching = true;
                return Task::perform(blocking(start_search), Msg::Started);
            }
            Msg::Started(result) => {
                if let Some(Err(err)) = result {
                    self.searching = false;
                    self.error = Some(err);
                }
            }
            Msg::Open(_) => {}
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let action: Element<'a, Msg> = if !c.online {
            widgets::primary(t, c.t(T::Search), None)
        } else if self.searching {
            widgets::spinner_label(t, c.t(T::Searching))
        } else {
            widgets::primary(t, c.t(T::Search), Some(Msg::Search))
        };
        let hint = if c.online { c.t(T::HeroHint) } else { c.t(T::Offline) };
        let hero = widgets::section_hero(t, Icon::Tv, Panel::Miracast.color(), c.t(T::Hero), hint, Some(action));
        let value = |text: T| sdk::text(c.t(text), TextStyle::CALL).color(t.fg_2());
        let smooth = config.str_or(TABLE, "quality", "best") == "smooth";
        let audio = config.bool_or(TABLE, "audio", true);
        let awake = config.bool_or(TABLE, "keep-screen-on", true);
        let qualities = vec![(false, c.t(T::Best).to_string()), (true, c.t(T::Smooth).to_string())];
        let what = Group::new(t)
            .title(c.t(T::What))
            .push(Row::new(t, c.t(T::Display)).subtitle(c.t(T::DisplayHint)).suffix(value(T::DisplayValue)).build())
            .push(
                Row::new(t, c.t(T::Sound))
                    .subtitle(c.t(T::SoundHint))
                    .changed(!audio)
                    .suffix(super::common::toggle(c, audio, Msg::Audio))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Quality))
                    .subtitle(c.t(if smooth { T::SmoothHint } else { T::QualityHint }))
                    .changed(smooth)
                    .suffix(super::common::gated(c, widgets::segmented(t, qualities, smooth, Msg::Smooth)))
                    .stacked(t.compact)
                    .build(),
            );
        let during = Group::new(t)
            .title(c.t(T::During))
            .push(
                Row::new(t, c.t(T::Awake))
                    .subtitle(c.t(T::AwakeHint))
                    .changed(!awake)
                    .suffix(super::common::toggle(c, awake, Msg::Awake))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Banners))
                    .subtitle(c.t(T::BannersHint))
                    .suffix(widgets::disclosure(t, None))
                    .on_press(Msg::Open(Panel::ScreenShare))
                    .build(),
            );
        let combo = keybinds::combo_for(&keybinds::current(config), "screen-mirroring");
        let shortcut = Row::new(t, c.t(T::Shortcut))
            .subtitle(c.t(T::ShortcutHint))
            .suffix(match combo {
                Some(combo) => widgets::key_combo(t, &combo),
                None => widgets::disclosure(t, Some(c.t(T::NoShortcut).to_string())),
            })
            .on_press(Msg::Open(Panel::Shortcuts))
            .build();
        let mut page =
            column![hero, error_banner(t, self.error.as_deref()), what.build(), during.push(shortcut).build()]
                .spacing(theme::SECTION_GAP);
        if let Some(ready) = self.readiness {
            let (encoder_ok, encoder_value, encoder_hint) = match ready.encoder {
                Encoder::Hardware => (true, c.t(T::Hardware), T::QualityHint),
                Encoder::Software => (true, c.t(T::Software), T::QualityHint),
                Encoder::Missing => (false, c.t(T::No), T::NoEncoder),
            };
            let yes_no = |ok: bool| c.t(if ok { T::Yes } else { T::No });
            page = page.push(
                Group::new(t)
                    .title(c.t(T::Ready))
                    .description(c.t(T::ReadyHint))
                    .push(readiness_row(
                        &c,
                        T::Direct,
                        ready.direct,
                        if ready.direct { T::DirectYes } else { T::DirectNo },
                        yes_no(ready.direct),
                    ))
                    .push(readiness_row(&c, T::Encoder, encoder_ok, encoder_hint, encoder_value))
                    .push(readiness_row(
                        &c,
                        T::Capture,
                        ready.sound,
                        if ready.sound { T::CaptureYes } else { T::CaptureNo },
                        yes_no(ready.sound),
                    ))
                    .build(),
            );
        }
        page.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_direct_is_read_from_the_device_types() {
        assert!(has_p2p("ethernet\nwifi\nwifi-p2p\nloopback\n"));
        assert!(!has_p2p("ethernet\nwifi\n"));
    }
}
