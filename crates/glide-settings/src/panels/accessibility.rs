use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::{error_banner, gated};
use super::{Ctx, Pane};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::gsettings::{self, Snapshot};
use crate::services::run::{Debounce, blocking};
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Hero => "Make Glide easier to use", "Зробіть Glide зручнішим";
        HeroHint => "Settings for seeing, moving and pointing. Apps built with GTK follow them too.", "Налаштування для зору, руху й керування вказівником. Програми на GTK теж їх враховують.";
        Seeing => "Seeing", "Зір";
        HighContrast => "High contrast", "Висока контрастність";
        HighContrastHint => "Stronger outlines and colours in GTK and libadwaita apps", "Чіткіші контури й кольори в програмах GTK і libadwaita";
        TextSize => "Text size", "Розмір тексту";
        TextSizeHint => "Scales text in apps that follow GNOME settings", "Масштабує текст у програмах, що наслідують налаштування GNOME";
        TextPreview => "The quick brown fox jumps over the lazy dog", "Чуєш їх, доцю, га? Кумедна ж ти, прощайся без ґольфів!";
        Scrollbars => "Always show scrollbars", "Завжди показувати смуги прокрутки";
        ScrollbarsHint => "Instead of revealing them only while scrolling", "Замість того, щоб показувати їх лише під час прокрутки";
        Pointer => "Pointer", "Вказівник";
        CursorSize => "Cursor size", "Розмір курсора";
        CursorSizeHint => "Applies to the Glide pointer at once and to apps as they restart", "Діє на вказівник Glide одразу, а на програми — після їх перезапуску";
        Default => "Default", "Типовий";
        Medium => "Medium", "Середній";
        Large => "Large", "Великий";
        Larger => "Larger", "Більший";
        Largest => "Largest", "Найбільший";
        DoubleClick => "Double-click speed", "Швидкість подвійного клацання";
        DoubleClickHint => "Longest pause between the two clicks, in GTK apps", "Найдовша пауза між двома клацаннями в програмах GTK";
        Milliseconds => "ms", "мс";
        Motion => "Motion", "Рух";
        ReduceMotion => "Reduce motion", "Зменшити рух";
        ReduceMotionHint => "Turns off animations in Glide and in apps", "Вимикає анімації в Glide і в програмах";
        NotFollowing => "Glide is set to ignore this switch. Turn on “Follow reduced motion” so the shell stops animating too.", "Glide налаштовано ігнорувати цей перемикач. Увімкніть «Зважати на зменшення руху», щоб оболонка теж припинила анімації.";
        Follow => "Follow it", "Зважати";
        CursorBlink => "Blinking text cursor", "Блимання текстового курсора";
        CursorBlinkHint => "The caret in text fields blinks", "Курсор у текстових полях блимає";
        NoGsettings => "gsettings is not available, so only the Glide pointer size can be changed", "gsettings недоступний, тож можна змінити лише розмір вказівника Glide";
        Test => "Double-click here to test", "Двічі клацніть тут для перевірки";
        Tested => "Double-click recognised", "Подвійне клацання розпізнано";
    }
}

use tr::Key as T;

const INTERFACE: &str = "org.gnome.desktop.interface";
const A11Y: &str = "org.gnome.desktop.a11y.interface";
const MOUSE: &str = "org.gnome.desktop.peripherals.mouse";

pub const TEXT_SCALES: [f64; 4] = [1.0, 1.25, 1.5, 2.0];
pub const CURSOR_SIZES: [i64; 5] = [24, 32, 48, 64, 96];

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Snapshot),
    HighContrast(bool),
    TextScale(usize),
    Scrollbars(bool),
    CursorSize(i64),
    DoubleClick(f64),
    CommitDoubleClick,
    DoubleClickSettled(u64),
    ReduceMotion(bool),
    FollowSystem,
    CursorBlink(bool),
    TestClick,
    Done(Option<Result<String, String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    snapshot: Snapshot,
    loaded: bool,
    error: Option<String>,
    double_click: Option<f64>,
    debounce: Debounce,
    last_click: Option<std::time::Instant>,
    recognised: bool,
}

#[must_use]
pub fn nearest_scale(value: f64) -> usize {
    TEXT_SCALES
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (**a - value).abs().total_cmp(&(**b - value).abs()))
        .map_or(0, |(index, _)| index)
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "accessibility",
        "a11y",
        "contrast",
        "text size",
        "large text",
        "cursor",
        "motion",
        "animation",
        "доступність",
        "контраст",
        "розмір тексту",
        "курсор",
        "анімац",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        load()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(snapshot) => {
                self.snapshot = snapshot;
                self.loaded = true;
            }
            Msg::HighContrast(on) => return self.set(A11Y, "high-contrast", on.to_string()),
            Msg::TextScale(index) => {
                let scale = TEXT_SCALES.get(index).copied().unwrap_or(1.0);
                return self.set(INTERFACE, "text-scaling-factor", format!("{scale:.2}"));
            }
            Msg::Scrollbars(always) => {
                return self.set(INTERFACE, "overlay-scrolling", (!always).to_string());
            }
            Msg::CursorSize(size) => {
                if size == CURSOR_SIZES[0] {
                    config.remove("accessibility", "cursor-size");
                } else {
                    config.set_int("accessibility", "cursor-size", size);
                }
                if self.snapshot.has_schema(INTERFACE) {
                    return self.set(INTERFACE, "cursor-size", size.to_string());
                }
            }
            Msg::DoubleClick(value) => {
                self.double_click = Some(value);
                return self.debounce.schedule(Msg::DoubleClickSettled);
            }
            Msg::DoubleClickSettled(generation) if !self.debounce.is_current(generation) => {}
            Msg::DoubleClickSettled(_) | Msg::CommitDoubleClick => {
                self.debounce.cancel();
                if let Some(value) = self.double_click.take() {
                    let millis = value.round() as i64;
                    return self.set(MOUSE, "double-click", millis.to_string());
                }
            }
            Msg::ReduceMotion(on) => {
                return self.set(INTERFACE, "enable-animations", (!on).to_string());
            }
            Msg::FollowSystem => config.set_bool("animations", "respect-system", true),
            Msg::CursorBlink(on) => return self.set(INTERFACE, "cursor-blink", on.to_string()),
            Msg::TestClick => {
                let now = std::time::Instant::now();
                let limit = self.snapshot.int(MOUSE, "double-click").unwrap_or(400);
                self.recognised = self
                    .last_click
                    .is_some_and(|last| now.duration_since(last).as_millis() <= u128::try_from(limit).unwrap_or(400));
                self.last_click = if self.recognised { None } else { Some(now) };
            }
            Msg::Done(result) => {
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                    return load();
                }
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let gnome = self.snapshot.has_schema(INTERFACE);
        column![
            widgets::section_hero(
                t,
                Icon::Accessibility,
                super::Panel::Accessibility.color(),
                c.t(T::Hero),
                c.t(T::HeroHint),
                None,
            ),
            error_banner(t, self.error.as_deref()),
            (self.loaded && !gnome).then(|| widgets::banner(t, t.warn(), Icon::Info, c.t(T::NoGsettings), None)),
            gnome.then(|| self.seeing(&c)),
            self.pointer(&c, config),
            gnome.then(|| self.motion(&c, config)),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

fn load() -> Task<Msg> {
    Task::perform(blocking(|| Snapshot::read(&[INTERFACE, A11Y, MOUSE])), |snapshot| {
        Msg::Loaded(snapshot.unwrap_or_default())
    })
}

impl Page {
    fn set(&mut self, schema: &'static str, key: &'static str, value: String) -> Task<Msg> {
        self.error = None;
        self.snapshot.insert(schema, key, &value);
        Task::perform(blocking(move || gsettings::set(schema, key, &value)), Msg::Done)
    }

    #[must_use]
    pub fn text_scale(&self) -> f64 {
        self.snapshot.double(INTERFACE, "text-scaling-factor").unwrap_or(1.0)
    }

    fn seeing(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let scale = nearest_scale(self.text_scale());
        let options =
            TEXT_SCALES.iter().enumerate().map(|(index, value)| (index, format!("{:.0}%", value * 100.0))).collect();
        let preview_size = (13.0 * TEXT_SCALES[scale]) as f32;
        Group::new(t)
            .title(c.t(T::Seeing))
            .push(
                Row::new(t, c.t(T::HighContrast))
                    .subtitle(c.t(T::HighContrastHint))
                    .prefix(widgets::icon(Icon::Contrast, 18.0, t.fg_2()))
                    .suffix(widgets::switch(
                        t,
                        self.snapshot.bool(A11Y, "high-contrast").unwrap_or(false),
                        Msg::HighContrast,
                    ))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::TextSize))
                    .subtitle(c.t(T::TextSizeHint))
                    .prefix(widgets::icon(Icon::TextSize, 18.0, t.fg_2()))
                    .suffix(widgets::segmented(t, options, scale, Msg::TextScale))
                    .wide()
                    .build(),
            )
            .push(widgets::sample_well(t, text(c.t(T::TextPreview)).size(preview_size).color(t.fg_1())))
            .push(
                Row::new(t, c.t(T::Scrollbars))
                    .subtitle(c.t(T::ScrollbarsHint))
                    .suffix(widgets::switch(
                        t,
                        !self.snapshot.bool(INTERFACE, "overlay-scrolling").unwrap_or(true),
                        Msg::Scrollbars,
                    ))
                    .build(),
            )
            .build()
    }

    fn pointer<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let current = config
            .get_int("accessibility", "cursor-size")
            .or_else(|| self.snapshot.int(INTERFACE, "cursor-size"))
            .unwrap_or(CURSOR_SIZES[0]);
        let labels = [T::Default, T::Medium, T::Large, T::Larger, T::Largest];
        let cards = CURSOR_SIZES
            .into_iter()
            .zip(labels)
            .map(|(size, label)| {
                let glyph = (size as f32 * 0.5).clamp(12.0, 40.0);
                container(widgets::radio_card(
                    t,
                    container(widgets::icon(Icon::Pointer, glyph, t.fg_1()))
                        .center_x(Fill)
                        .center_y(Length::Fixed(44.0)),
                    c.t(label),
                    Some(format!("{size} px")),
                    current == size,
                    Msg::CursorSize(size),
                ))
                .width(Fill)
                .into()
            })
            .collect();
        let cards = widgets::grid(cards, if t.compact { 3 } else { CURSOR_SIZES.len() }, 8.0);
        let mut group = Group::new(t)
            .title(c.t(T::Pointer))
            .push(
                Row::new(t, c.t(T::CursorSize))
                    .subtitle(c.t(T::CursorSizeHint))
                    .prefix(widgets::icon(Icon::Pointer, 18.0, t.fg_2()))
                    .build(),
            )
            .push(container(gated(*c, cards)).padding([4.0, theme::ROW_PADDING_X - 4.0]));
        if let Some(saved) = self.snapshot.int(MOUSE, "double-click") {
            let value = self.double_click.unwrap_or(saved as f64);
            group = group
                .push(
                    Row::new(t, format!("{} · {value:.0} {}", c.t(T::DoubleClick), c.t(T::Milliseconds)))
                        .subtitle(c.t(T::DoubleClickHint))
                        .suffix(widgets::flexible(
                            widgets::value_slider(
                                t,
                                100.0..=1000.0,
                                value,
                                50.0,
                                Msg::DoubleClick,
                                Some(Msg::CommitDoubleClick),
                            ),
                            240.0,
                        ))
                        .build(),
                )
                .push(
                    Row::new(t, c.t(T::Test))
                        .prefix(widgets::icon(Icon::Mouse, 18.0, t.fg_2()))
                        .suffix(if self.recognised {
                            widgets::status_pill(t, Tone::Ok, c.t(T::Tested))
                        } else {
                            column![].into()
                        })
                        .on_press(Msg::TestClick)
                        .build(),
                );
        }
        group.build()
    }

    fn motion<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let reduced = !self.snapshot.bool(INTERFACE, "enable-animations").unwrap_or(true);
        let mut group = Group::new(t).title(c.t(T::Motion)).push(
            Row::new(t, c.t(T::ReduceMotion))
                .subtitle(c.t(T::ReduceMotionHint))
                .prefix(widgets::icon(Icon::Sparkle, 18.0, t.fg_2()))
                .suffix(widgets::switch(t, reduced, Msg::ReduceMotion))
                .build(),
        );
        if reduced && !config.respect_system_motion() {
            group = group.push(
                container(
                    row![
                        widgets::icon(Icon::Info, 16.0, t.warn()),
                        text(c.t(T::NotFollowing)).size(12.5).color(t.fg_2()).width(Fill),
                        gated(*c, widgets::tonal(t, c.t(T::Follow), Some(Msg::FollowSystem))),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([8.0, theme::ROW_PADDING_X]),
            );
        }
        group
            .push(
                Row::new(t, c.t(T::CursorBlink))
                    .subtitle(c.t(T::CursorBlinkHint))
                    .suffix(widgets::switch(
                        t,
                        self.snapshot.bool(INTERFACE, "cursor-blink").unwrap_or(true),
                        Msg::CursorBlink,
                    ))
                    .build(),
            )
            .build()
    }
}
