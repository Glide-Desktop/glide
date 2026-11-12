use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use glide_sdk::theme::TextStyle;
use glide_sdk::widget as sdk;
use iced::widget::{column, container, mouse_area, row, space};
use iced::{Alignment, Element, Fill, Task, mouse};

use super::common::{error_banner, switch_row};
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::audio::{self, AudioState, Device, MeterEvent, SINK, SOURCE, Stream, TestSound};
use crate::services::config::{ConfigStore, DEFAULT_MAX_VOLUME};
use crate::services::run::{Debounce, Generation, blocking};
use crate::theme::{self, ThemeState};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Balance => "Balance", "Баланс";
        BalanceHint => "Level of the left and right speakers", "Рівень лівого й правого динаміків";
        Left => "Left", "Ліворуч";
        Right => "Right", "Праворуч";
        Centre => "Centred", "По центру";
        PauseOnDisconnect => "Pause when headphones disconnect", "Пауза, коли навушники від'єднано";
        PauseOnDisconnectHint => "Stops playback instead of switching it to the speakers when Bluetooth headphones disconnect or a cable is pulled out", "Зупиняє відтворення, а не перемикає його на динаміки, коли Bluetooth-навушники від'єднуються або виймається кабель";
        TestSound => "Test sound", "Перевірка звуку";
        TestSoundHint => "Plays a short sample through the current output", "Відтворює короткий зразок через поточний вихід";
        Play => "Play", "Відтворити";
        Playing => "Playing…", "Відтворення…";
        Applications => "Applications", "Програми";
        ApplicationsHint => "Volume of each app that is playing sound right now", "Гучність кожної програми, що зараз відтворює звук";
        NoStreams => "No apps are playing sound", "Жодна програма зараз не відтворює звук";
        InputLevel => "Input level", "Рівень сигналу";
        InputLevelHint => "Press Test and speak: the microphone is monitored for 10 seconds", "Натисніть «Перевірити» й говоріть: мікрофон прослуховується 10 секунд";
        Listening => "Listening… speak into the microphone", "Слухаю… говоріть у мікрофон";
        TestMic => "Test", "Перевірити";
        Stop => "Stop", "Зупинити";
        MeterFailed => "Could not read from the microphone", "Не вдалося отримати звук із мікрофона";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Output,
    Input,
}

impl Channel {
    const fn target(self) -> &'static str {
        match self {
            Self::Output => SINK,
            Self::Input => SOURCE,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(AudioState),
    Refresh,
    Volume(Channel, f64),
    Release(Channel),
    Settle(Channel, u64),
    Applied(Channel),
    Mute(Channel, bool),
    Device(Channel, Device),
    Done(Option<Result<String, String>>),
    OverAmplify(bool),
    PauseOnDisconnect(bool),
    Balance(f64),
    CommitBalance,
    SettleBalance(u64),
    StreamVolume(u32, f64),
    StreamRelease,
    StreamApplied,
    StreamMute(u32, bool),
    Play(TestSound),
    Played(Option<Result<String, String>>),
    MeterStart,
    MeterStop,
    Meter(u64, MeterEvent),
}

#[must_use]
pub fn max_volume(config: &ConfigStore) -> f32 {
    let pct = config.int_or("topbar", "max-volume-pct", DEFAULT_MAX_VOLUME).clamp(100, 150) as f32;
    pct / 100.0
}

#[derive(Debug, Default, Clone, Copy)]
struct Slider {
    value: Option<f32>,
    in_flight: bool,
    pending: bool,
    dragging: bool,
    settle: Debounce,
}

#[derive(Debug, Default, Clone, Copy)]
struct StreamSlider {
    index: u32,
    value: Option<f32>,
    in_flight: bool,
    pending: bool,
    dragging: bool,
}

#[derive(Debug, Default)]
struct Meter {
    running: bool,
    level: f32,
    peak: f32,
    failed: bool,
    stop: Arc<AtomicBool>,
    generation: Generation,
}

#[derive(Debug, Default)]
pub struct Page {
    state: AudioState,
    loading: bool,
    loaded: bool,
    output: Slider,
    input: Slider,
    balance: Option<f32>,
    balance_settle: Debounce,
    stream: StreamSlider,
    playing: Option<TestSound>,
    meter: Meter,
    error: Option<String>,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "audio",
        "volume",
        "speaker",
        "microphone",
        "input",
        "output",
        "mute",
        "balance",
        "звук",
        "гучність",
        "динамік",
        "мікрофон",
        "вихід",
        "вхід",
        "баланс",
    ];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        if self.loaded || self.loading {
            return Task::none();
        }
        self.refresh()
    }

    fn leave(&mut self, _config: &mut ConfigStore) {
        self.stop_meter();
    }

    #[allow(clippy::too_many_lines)] // One match arm per message; splitting would only scatter the state machine
    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::PauseOnDisconnect(on) => config.set_bool("general", "pause-media-on-disconnect", on),
            Msg::OverAmplify(on) => {
                config.set_int("topbar", "max-volume-pct", if on { 150 } else { 100 });
                let over = self.state.output.is_some_and(|level| level.volume > 1.0);
                if !on && over {
                    self.output.value = Some(1.0);
                    return self.push(Channel::Output);
                }
            }
            Msg::Loaded(state) => {
                self.loading = false;
                self.loaded = true;
                self.state = state;
                for channel in [Channel::Output, Channel::Input] {
                    let slider = self.slider(channel);
                    if !slider.dragging && !slider.in_flight {
                        slider.value = None;
                    }
                }
                if !self.stream.dragging && !self.stream.in_flight {
                    self.stream.value = None;
                }
            }
            Msg::Refresh => return self.refresh(),
            Msg::Volume(channel, value) => {
                let value = value as f32;
                let slider = self.slider(channel);
                slider.value = Some(value);
                slider.dragging = true;
                let settle = slider.settle.schedule(move |generation| Msg::Settle(channel, generation));
                return Task::batch([self.push(channel), settle]);
            }
            Msg::Settle(channel, generation) if !self.slider(channel).settle.is_current(generation) => {}
            Msg::Release(channel) | Msg::Settle(channel, _) => {
                let slider = self.slider(channel);
                slider.settle.cancel();
                slider.dragging = false;
                if !slider.in_flight {
                    return self.refresh();
                }
            }
            Msg::Applied(channel) => {
                let slider = self.slider(channel);
                slider.in_flight = false;
                if slider.pending {
                    return self.push(channel);
                }
                if !slider.dragging {
                    return self.refresh();
                }
            }
            Msg::Mute(channel, muted) => {
                return Task::perform(blocking(move || audio::set_muted(channel.target(), muted)), Msg::Done);
            }
            Msg::Device(channel, device) => {
                if channel == Channel::Input {
                    self.stop_meter();
                }
                return Task::perform(blocking(move || audio::set_default(device.id)), Msg::Done);
            }
            Msg::Done(result) => {
                self.error = match result {
                    Some(Err(err)) => Some(err),
                    _ => None,
                };
                return self.refresh();
            }
            Msg::Balance(value) => {
                let value = value as f32;
                self.balance = Some(if value.abs() < 0.05 { 0.0 } else { value });
                return self.balance_settle.schedule(Msg::SettleBalance);
            }
            Msg::SettleBalance(generation) if !self.balance_settle.is_current(generation) => {}
            Msg::CommitBalance | Msg::SettleBalance(_) => {
                self.balance_settle.cancel();
                let (Some(balance), Some(stereo)) = (self.balance.take(), self.state.stereo) else {
                    return Task::none();
                };
                let volume = stereo.left.max(stereo.right);
                let (left, right) = audio::split_balance(volume, balance);
                self.state.stereo = Some(audio::Stereo { left, right });
                return Task::perform(blocking(move || audio::set_balance(volume, balance)), Msg::Done);
            }
            Msg::StreamVolume(index, value) => {
                let value = value as f32;
                if self.stream.index != index && self.stream.in_flight {
                    return Task::none();
                }
                self.stream.index = index;
                self.stream.value = Some(value);
                self.stream.dragging = true;
                if let Some(stream) = self.state.streams.iter_mut().find(|s| s.index == index) {
                    stream.volume = value;
                }
                return self.push_stream();
            }
            Msg::StreamRelease => {
                self.stream.dragging = false;
                if !self.stream.in_flight {
                    return self.refresh();
                }
            }
            Msg::StreamApplied => {
                self.stream.in_flight = false;
                if self.stream.pending {
                    return self.push_stream();
                }
                if !self.stream.dragging {
                    return self.refresh();
                }
            }
            Msg::StreamMute(index, muted) => {
                if let Some(stream) = self.state.streams.iter_mut().find(|s| s.index == index) {
                    stream.muted = muted;
                }
                return Task::perform(blocking(move || audio::set_stream_muted(index, muted)), Msg::Done);
            }
            Msg::Play(sound) => {
                let Some(player) = self.state.player else {
                    return Task::none();
                };
                if self.playing.is_some() {
                    return Task::none();
                }
                self.playing = Some(sound);
                return Task::perform(blocking(move || audio::play(player, sound)), Msg::Played);
            }
            Msg::Played(result) => {
                self.playing = None;
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
            }
            Msg::MeterStart => {
                if self.state.meter {
                    return self.start_meter();
                }
            }
            Msg::MeterStop => self.stop_meter(),
            Msg::Meter(generation, _) if !self.meter.generation.is_current(generation) => {}
            Msg::Meter(_, event) => match event {
                MeterEvent::Level(level) => {
                    self.meter.level = level;
                    self.meter.peak = level.max(self.meter.peak * 0.92);
                }
                MeterEvent::Failed => {
                    self.meter.running = false;
                    self.meter.failed = true;
                    self.meter.level = 0.0;
                }
                MeterEvent::Finished => {
                    self.meter.running = false;
                    self.meter.level = 0.0;
                }
            },
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let max = max_volume(config);
        let t = c.theme;
        if !self.state.available {
            return if self.loading || !self.loaded {
                widgets::empty_state(t, Icon::Volume, c.t(K::Loading), "")
            } else {
                widgets::empty_state(t, Icon::VolumeMute, c.t(K::SoundUnavailable), c.t(K::SoundUnavailableHint))
            };
        }
        let mut page = column![error_banner(t, self.error.as_deref())].spacing(theme::SECTION_GAP);
        let mut extras = Vec::new();
        if let Some(balance) = self.balance_value() {
            extras.push(Self::balance_row(&c, balance));
        }
        extras.push(switch_row(
            c,
            c.t(K::OverAmplification),
            c.t(K::OverAmplificationHint),
            max > 1.0,
            Msg::OverAmplify,
        ));
        extras.push(switch_row(
            c,
            c.t(tr::Key::PauseOnDisconnect),
            c.t(tr::Key::PauseOnDisconnectHint),
            config.bool_or("general", "pause-media-on-disconnect", true),
            Msg::PauseOnDisconnect,
        ));
        extras.extend(self.test_sound_row(&c));
        page = page.push(self.device_group(&c, Channel::Output)).push(self.channel_group(
            &c,
            Channel::Output,
            max,
            extras,
        ));
        let input_extras =
            (self.state.meter && self.state.input.is_some()).then(|| self.meter_row(&c)).into_iter().collect();
        page = page.push(self.device_group(&c, Channel::Input)).push(self.channel_group(
            &c,
            Channel::Input,
            1.0,
            input_extras,
        ));
        if self.state.streams_supported {
            page = page.push(self.streams_group(&c));
        }
        page.into()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.refresh()
    }
}

impl Page {
    fn refresh(&mut self) -> Task<Msg> {
        if self.loading || self.output.dragging || self.input.dragging || self.stream.dragging || self.balance.is_some()
        {
            return Task::none();
        }
        self.loading = true;
        Task::perform(blocking(audio::read), |state| Msg::Loaded(state.unwrap_or_default()))
    }

    fn slider(&mut self, channel: Channel) -> &mut Slider {
        match channel {
            Channel::Output => &mut self.output,
            Channel::Input => &mut self.input,
        }
    }

    fn balance_value(&self) -> Option<f32> {
        self.balance.or_else(|| self.state.stereo.map(audio::Stereo::balance))
    }

    fn push(&mut self, channel: Channel) -> Task<Msg> {
        let balance = self.state.stereo.map(audio::Stereo::balance);
        let slider = self.slider(channel);
        let Some(value) = claim(slider.value, &mut slider.in_flight, &mut slider.pending) else {
            return Task::none();
        };
        Task::perform(
            blocking(move || match channel {
                Channel::Output => audio::set_output_volume(value, balance),
                Channel::Input => audio::set_volume(channel.target(), value),
            }),
            move |_| Msg::Applied(channel),
        )
    }

    fn push_stream(&mut self) -> Task<Msg> {
        let slider = &mut self.stream;
        let Some(value) = claim(slider.value, &mut slider.in_flight, &mut slider.pending) else {
            return Task::none();
        };
        let index = slider.index;
        Task::perform(blocking(move || audio::set_stream_volume(index, value)), |_| Msg::StreamApplied)
    }

    fn stop_meter(&mut self) {
        self.meter.stop.store(true, Ordering::Relaxed);
        self.meter.generation.bump();
        self.meter.running = false;
        self.meter.level = 0.0;
    }

    fn start_meter(&mut self) -> Task<Msg> {
        self.meter.stop.store(true, Ordering::Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        self.meter.stop = Arc::clone(&stop);
        let generation = self.meter.generation.bump();
        self.meter.running = true;
        self.meter.failed = false;
        self.meter.level = 0.0;
        self.meter.peak = 0.0;
        Task::run(audio::meter(stop), move |event| Msg::Meter(generation, event))
    }

    fn channel_group<'a>(
        &'a self, c: &Ctx, channel: Channel, max: f32, extras: Vec<Element<'a, Msg>>,
    ) -> Element<'a, Msg> {
        let t = c.theme;
        let (level, slider, title, glyph, muted_glyph) = match channel {
            Channel::Output => (self.state.output, self.output, K::Output, Icon::Volume, Icon::VolumeMute),
            Channel::Input => (self.state.input, self.input, K::Input, Icon::Mic, Icon::MicMute),
        };
        let row = match level {
            Some(level) => volume_row(
                c,
                channel,
                (slider.value.unwrap_or(level.volume), max),
                level.muted,
                if level.muted { muted_glyph } else { glyph },
            ),
            None => Row::new(t, c.t(K::NoDevices)).dimmed(true).build(),
        };
        let group = Group::new(t);
        let group = if channel == Channel::Input { group.title(c.t(title)) } else { group };
        group.push(row).extend(extras).build()
    }

    fn balance_row<'a>(c: &Ctx, balance: f32) -> Element<'a, Msg> {
        let t = c.theme;
        let subtitle = if balance.abs() < 0.005 {
            format!("{} · {}", c.t(tr::Key::Centre), c.t(tr::Key::BalanceHint))
        } else if balance < 0.0 {
            format!("{} {:.0}%", c.t(tr::Key::Left), -balance * 100.0)
        } else {
            format!("{} {:.0}%", c.t(tr::Key::Right), balance * 100.0)
        };
        let control = row![
            sdk::text(c.t(tr::Key::Left), TextStyle::CAP).color(t.fg_3()),
            widgets::value_slider(t, -1.0..=1.0, f64::from(balance), 0.05, Msg::Balance, Some(Msg::CommitBalance),),
            sdk::text(c.t(tr::Key::Right), TextStyle::CAP).color(t.fg_3()),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        Row::new(t, c.t(tr::Key::Balance)).subtitle(subtitle).suffix(widgets::flexible(control, 360.0)).build()
    }

    fn test_sound_row(&self, c: &Ctx) -> Option<Element<'_, Msg>> {
        let t = c.theme;
        self.state.player?;
        let busy = self.playing.is_some();
        let button = |sound: TestSound, label: &'static str| {
            sound.path().map(|_| widgets::secondary(t, label, (!busy).then_some(Msg::Play(sound))))
        };
        let sides: &[_] = if self.state.stereo.is_some() {
            &[(TestSound::Left, tr::Key::Left), (TestSound::Right, tr::Key::Right)]
        } else {
            &[]
        };
        let buttons: Vec<_> = sides
            .iter()
            .copied()
            .chain([(TestSound::Both, tr::Key::Play)])
            .filter_map(|(sound, key)| button(sound, c.t(key)))
            .collect();
        (!buttons.is_empty()).then(|| {
            let buttons = iced::widget::Row::with_children(buttons).spacing(8).align_y(Alignment::Center);
            Row::new(t, c.t(tr::Key::TestSound))
                .subtitle(if busy { c.t(tr::Key::Playing) } else { c.t(tr::Key::TestSoundHint) })
                .suffix(buttons)
                .stacked(t.compact)
                .build()
        })
    }

    fn meter_row(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let meter = &self.meter;
        let subtitle = if meter.running {
            c.t(tr::Key::Listening)
        } else if meter.failed {
            c.t(tr::Key::MeterFailed)
        } else {
            c.t(tr::Key::InputLevelHint)
        };
        let action = if meter.running {
            widgets::secondary(t, c.t(tr::Key::Stop), Some(Msg::MeterStop))
        } else {
            widgets::tonal(t, c.t(tr::Key::TestMic), Some(Msg::MeterStart))
        };
        let tone = if meter.level > 0.9 {
            theme::Tone::Danger
        } else if meter.level > 0.75 {
            theme::Tone::Warn
        } else {
            theme::Tone::Ok
        };
        let meter = sdk::level_meter(&widgets::paint(&t), meter.level, meter.peak, tone);
        let control = row![meter, action].spacing(12).align_y(Alignment::Center);
        Row::new(t, c.t(tr::Key::InputLevel)).subtitle(subtitle).suffix(widgets::flexible(control, 360.0)).build()
    }

    fn streams_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t)
            .title(c.t(tr::Key::Applications))
            .description(c.t(tr::Key::ApplicationsHint))
            .stacked_header(t.compact);
        if self.state.streams.is_empty() {
            return group
                .push(
                    Row::new(t, c.t(tr::Key::NoStreams))
                        .prefix(widgets::icon(Icon::Apps, 20.0, t.fg_3()))
                        .dimmed(true)
                        .build(),
                )
                .build();
        }
        for stream in &self.state.streams {
            group = group.push(stream_row(c, stream, self.stream));
        }
        group.build()
    }

    fn device_group(&self, c: &Ctx, channel: Channel) -> Element<'_, Msg> {
        let t = c.theme;
        let (devices, current, title, fallback) = match channel {
            Channel::Output => (&self.state.sinks, self.state.default_sink(), K::OutputDevice, Icon::Volume),
            Channel::Input => (&self.state.sources, self.state.default_source(), K::InputDevice, Icon::Mic),
        };
        let mut group = Group::new(t).title(c.t(title));
        if devices.is_empty() {
            return group.push(Row::new(t, c.t(K::NoDevices)).dimmed(true).build()).build();
        }
        if devices.len() > DEVICE_LIST_LIMIT {
            return group
                .push(
                    Row::new(t, c.t(title))
                        .suffix(widgets::dropdown(
                            t,
                            devices.clone(),
                            current.cloned(),
                            move |device| Msg::Device(channel, device),
                            300.0,
                        ))
                        .build(),
                )
                .build();
        }
        if channel == Channel::Output {
            let cards = devices
                .iter()
                .map(|device| {
                    let selected = device.is_default;
                    let paint = t.paint;
                    let face = container(sdk::text(device.name.clone(), TextStyle::HEAD))
                        .padding(16)
                        .width(Fill)
                        .height(80)
                        .style(move |_| container::Style {
                            background: Some(paint.card_bg().into()),
                            border: iced::Border {
                                color: if selected { paint.acc_bg() } else { paint.line() },
                                width: if selected { 2.0 } else { 1.0 },
                                radius: glide_sdk::theme::R3.into(),
                            },
                            ..Default::default()
                        });
                    if selected {
                        face.into()
                    } else {
                        let message = Msg::Device(channel, device.clone());
                        let card =
                            mouse_area(face).on_release(message.clone()).interaction(mouse::Interaction::Pointer);
                        sdk::focusable(&paint, card, glide_sdk::theme::R3).on_activate(Some(message)).into()
                    }
                })
                .collect();
            return column![
                sdk::text(c.t(K::Output), TextStyle::HEAD),
                sdk::grid(cards, if t.compact { 1 } else { 3 }, 12.0),
            ]
            .spacing(8)
            .into();
        }
        for device in devices {
            let active = device.is_default;
            let mark: Element<'_, Msg> =
                if active { widgets::icon(Icon::Check, 18.0, t.acc_text()) } else { space().width(18).into() };
            let mut line = Row::new(t, device.name.clone())
                .prefix(widgets::icon(
                    device_icon(&device.name, fallback),
                    20.0,
                    if active { t.acc_text() } else { t.fg_2() },
                ))
                .suffix(mark);
            if !active {
                line = line.on_press(Msg::Device(channel, device.clone()));
            }
            group = group.push(line.build());
        }
        group.build()
    }
}

fn claim(value: Option<f32>, in_flight: &mut bool, pending: &mut bool) -> Option<f32> {
    let value = value?;
    *pending = *in_flight;
    if *in_flight {
        return None;
    }
    *in_flight = true;
    Some(value)
}

pub(crate) const DEVICE_LIST_LIMIT: usize = 6;

#[must_use]
pub fn device_icon(name: &str, fallback: Icon) -> Icon {
    let name = name.to_lowercase();
    if ["headphone", "headset", "earbud", "airpods", "buds"].iter().any(|word| name.contains(word)) {
        Icon::Headphones
    } else if ["hdmi", "displayport", "monitor"].iter().any(|word| name.contains(word)) {
        Icon::Display
    } else if name.contains("bluetooth") {
        Icon::Bluetooth
    } else {
        fallback
    }
}

fn stream_row<'a>(c: &Ctx, stream: &'a Stream, slider: StreamSlider) -> Element<'a, Msg> {
    let t = c.theme;
    let index = stream.index;
    let volume = if slider.index == index { slider.value.unwrap_or(stream.volume) } else { stream.volume };
    let muted = stream.muted;
    let control = mixer(
        &t,
        if muted { Icon::VolumeMute } else { Icon::Volume },
        Msg::StreamMute(index, !muted),
        widgets::value_slider(
            t,
            0.0..=1.5,
            f64::from(volume.min(1.5)),
            0.01,
            move |value| Msg::StreamVolume(index, value),
            Some(Msg::StreamRelease),
        ),
        volume,
        muted,
    );
    Row::new(t, stream.app.clone())
        .subtitle(if muted { c.t(K::Muted).to_string() } else { stream.title.clone() })
        .prefix(super::apps::app_icon(&t, None, &stream.app, theme::SMALL_CONTROL_HEIGHT))
        .suffix(widgets::flexible(control, 320.0))
        .build()
}

fn mixer<'a>(
    t: &ThemeState, glyph: Icon, mute: Msg, slider: Element<'a, Msg>, volume: f32, muted: bool,
) -> Element<'a, Msg> {
    sdk::level_control(&widgets::paint(t), glyph, mute, slider, volume, muted)
}

fn volume_row<'a>(c: &Ctx, channel: Channel, (volume, max): (f32, f32), muted: bool, glyph: Icon) -> Element<'a, Msg> {
    let t = c.theme;
    let volume = volume.min(max);
    let control = mixer(
        &t,
        glyph,
        Msg::Mute(channel, !muted),
        widgets::value_slider(
            t,
            0.0..=f64::from(max),
            f64::from(volume),
            0.01,
            move |value| Msg::Volume(channel, value),
            Some(Msg::Release(channel)),
        ),
        volume,
        muted,
    );
    Row::new(t, c.t(K::Volume))
        .subtitle(if muted { c.t(K::Muted) } else { "" })
        .suffix(widgets::flexible(control, 360.0))
        .build()
}
