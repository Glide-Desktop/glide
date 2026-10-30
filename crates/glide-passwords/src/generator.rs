use glide_sdk::theme::{SP1, SP2, SP3, TextStyle};
use glide_sdk::widget::{self as ui, Size as ButtonSize, Strength, SwitchSize, Tone};
use glide_sdk::{Lang, Shell};
use iced::widget::{Column, Space, column, container, row};
use iced::{Alignment, Element, Fill, Padding, Task};

use crate::app::{App, Message};
use crate::i18n::{Key, plural};
use crate::icons::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Upper,
    Digits,
    Symbols,
    Similar,
    Capitalise,
    Number,
}

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Mode(Mode),
    Amount(f32),
    Toggle(Toggle, bool),
    Again,
    Copy,
    Use,
    Close,
    History,
    Pick(usize),
}

const HISTORY: usize = 10;

fn remember(app: &mut App, value: &str) {
    app.generated.retain(|known| known.as_str() != value);
    app.generated.insert(0, zeroize::Zeroizing::new(value.to_string()));
    app.generated.truncate(HISTORY);
}

pub fn update(app: &mut App, event: Event, shell: &mut Shell<Message>) -> Task<Message> {
    let Some(state) = app.generator.as_mut() else {
        return Task::none();
    };
    match event {
        Event::Mode(mode) => {
            state.recipe.mode = mode;
            state.again();
        }
        Event::Amount(amount) => {
            let amount = amount.round() as u32;
            if amount != state.recipe.amount() {
                state.recipe.set_amount(amount);
                state.again();
            }
        }
        Event::Toggle(which, on) => {
            let recipe = &mut state.recipe;
            match which {
                Toggle::Upper => recipe.upper = on,
                Toggle::Digits => recipe.digits = on,
                Toggle::Symbols => recipe.symbols = on,
                Toggle::Similar => recipe.avoid_similar = on,
                Toggle::Capitalise => recipe.capitalise = on,
                Toggle::Number => recipe.number = on,
            }
            state.again();
        }
        Event::Again => state.again(),
        Event::History => state.history = !state.history,
        Event::Pick(index) => {
            if let Some(value) = app.generated.get(index) {
                state.value = value.to_string();
                state.history = false;
            }
        }
        Event::Copy => {
            let value = state.value.clone();
            remember(app, &value);
            return app.copy(crate::app::Copied::Password, glide_ipc::keychain::Hidden::new(value), shell);
        }
        Event::Use => {
            let value = state.value.clone();
            remember(app, &value);
            app.generator = None;
            if let Some(editor) = app.editor.as_mut() {
                editor.password = value;
                editor.shown = true;
            }
        }
        Event::Close => app.generator = None,
    }
    Task::none()
}

fn history_rows(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let rows = app.generated.iter().enumerate().map(|(index, past)| -> Element<'_, Message> {
        iced::widget::button(
            container(ui::password_text(&paint, past.as_str(), ui::SECRET))
                .padding(Padding::from([SP1, SP2]))
                .width(Fill),
        )
        .on_press(Message::Generator(Event::Pick(index)))
        .padding(0)
        .width(Fill)
        .style(move |_, status| {
            glide_sdk::glide_iced::style::flat(
                paint.layered(iced::Color::TRANSPARENT, status),
                paint.fg_1(),
                glide_sdk::theme::R2,
            )
        })
        .into()
    });
    Column::with_children(rows).spacing(2).into()
}

#[must_use]
pub fn amount_text(lang: Lang, mode: Mode, amount: u32) -> String {
    let forms = match mode {
        Mode::Password => [Key::CharsOne, Key::CharsFew, Key::CharsMany],
        Mode::Phrase => [Key::WordsOne, Key::WordsFew, Key::WordsMany],
        Mode::Pin => [Key::DigitsOne, Key::DigitsFew, Key::DigitsMany],
    };
    plural(lang, u64::from(amount), forms)
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(state) = app.generator.as_ref() else {
        return Space::new().into();
    };
    let t = |key: Key| key.text(app.lang);
    let recipe = &state.recipe;
    let value = container(
        row![
            container(ui::password_text(
                &paint,
                &state.value,
                glide_sdk::theme::TextStyle { size_px: 17.0, line_height_px: 24.0, ..ui::SECRET }
            ))
            .width(Fill),
            ui::icon_button(&paint, Icon::Sync, t(Key::Another), Some(Message::Generator(Event::Again)), false),
            ui::icon_button(&paint, Icon::Copy, t(Key::Copy), Some(Message::Generator(Event::Copy)), false),
        ]
        .spacing(SP1)
        .align_y(Alignment::Start),
    )
    .padding(SP3)
    .width(Fill)
    .style(move |_| glide_sdk::glide_iced::style::rounded(paint.fill_1(), glide_sdk::theme::R2));
    let strength = recipe.strength();
    let label = format!(
        "{} · {}",
        crate::view::strength_name(app.lang, strength),
        amount_text(app.lang, recipe.mode, recipe.amount())
    );
    let modes = ui::segmented_fill(
        &paint,
        &[(Mode::Password, t(Key::ModePassword)), (Mode::Phrase, t(Key::ModePhrase)), (Mode::Pin, t(Key::ModePin))],
        recipe.mode,
        |mode| Message::Generator(Event::Mode(mode)),
    );
    let (low, high) = recipe.range();
    let amount_label = match recipe.mode {
        Mode::Password => t(Key::Length),
        Mode::Phrase => t(Key::WordCount),
        Mode::Pin => t(Key::DigitCount),
    };
    let amount = column![
        row![
            ui::text(amount_label, TextStyle::CALL).width(Fill),
            ui::text(recipe.amount().to_string(), TextStyle::NUM)
        ],
        ui::stepped_slider(
            &paint,
            low as f32..=high as f32,
            recipe.amount() as f32,
            1.0,
            Some(|value| Message::Generator(Event::Amount(value))),
            None,
        ),
    ]
    .spacing(6)
    .padding(Padding::from([0.0, SP1]));
    let switches = switches(app, recipe);
    let history = ui::glyph_button(
        &paint,
        Icon::History,
        t(Key::HistoryButton),
        if state.history { Tone::Tonal } else { Tone::Ghost },
        ButtonSize::Small,
        (!app.generated.is_empty() || state.history).then_some(Message::Generator(Event::History)),
    );
    let footer = row![
        history,
        Space::new().width(Fill),
        ui::sized_button(
            &paint,
            t(Key::Cancel),
            Tone::Neutral,
            ButtonSize::Small,
            Some(Message::Generator(Event::Close))
        ),
        ui::sized_button(&paint, t(Key::Use), Tone::Accent, ButtonSize::Small, Some(Message::Generator(Event::Use))),
    ]
    .spacing(SP2)
    .align_y(Alignment::Center);
    let mut content = Column::new()
        .spacing(SP3)
        .push(value)
        .push(container(ui::strength_line(&paint, strength, label)).padding(Padding::from([0.0, SP1])));
    if state.history {
        content = content.push(history_rows(app));
    } else {
        content = content.push(modes).push(amount).push(Column::with_children(switches));
    }
    let content = content.push(ui::separator(&paint)).push(footer);
    container(ui::popover(&paint, container(content).padding(SP2).width(Fill))).width(POPOVER).into()
}

fn switches<'a>(app: &'a App, recipe: &Recipe) -> Vec<Element<'a, Message>> {
    let paint = app.paint;
    let switch = |label: Key, sample: &'static str, on: bool, which: Toggle| -> Element<'a, Message> {
        row![
            ui::text(label.text(app.lang), TextStyle::CALL).width(Fill),
            ui::text(sample, glide_sdk::theme::TextStyle { size_px: 12.0, line_height_px: 16.0, ..ui::SECRET })
                .color(paint.fg_3()),
            ui::sized_switch(&paint, on, SwitchSize::Bar, Some(move |on| Message::Generator(Event::Toggle(which, on)))),
        ]
        .spacing(SP3)
        .align_y(Alignment::Center)
        .height(36)
        .padding(Padding::from([0.0, SP1]))
        .into()
    };
    match recipe.mode {
        Mode::Password => vec![
            switch(Key::Upper, "A–Z", recipe.upper, Toggle::Upper),
            switch(Key::Digits, "0–9", recipe.digits, Toggle::Digits),
            switch(Key::Symbols, "!@#$", recipe.symbols, Toggle::Symbols),
            switch(Key::AvoidSimilar, "l 1 O 0", recipe.avoid_similar, Toggle::Similar),
        ],
        Mode::Phrase => vec![
            switch(Key::Capitalise, "Orbit", recipe.capitalise, Toggle::Capitalise),
            switch(Key::AddDigit, "orbit7", recipe.number, Toggle::Number),
        ],
        Mode::Pin => Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    #[default]
    Password,
    Phrase,
    Pin,
}

#[allow(clippy::struct_excessive_bools)] // One switch per character class, as in the generator popover
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub mode: Mode,
    pub length: u32,
    pub words: u32,
    pub digits_only: u32,
    pub upper: bool,
    pub digits: bool,
    pub symbols: bool,
    pub avoid_similar: bool,
    pub capitalise: bool,
    pub number: bool,
}

impl Default for Recipe {
    fn default() -> Self {
        Self {
            mode: Mode::Password,
            length: 20,
            words: 5,
            digits_only: 6,
            upper: true,
            digits: true,
            symbols: true,
            avoid_similar: true,
            capitalise: true,
            number: true,
        }
    }
}

pub const POPOVER: f32 = 376.0;
pub const LENGTH: (u32, u32) = (8, 64);
pub const WORDS: (u32, u32) = (3, 10);
pub const PIN: (u32, u32) = (4, 12);

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const LOWER_CLEAR: &str = "abcdefghijkmnpqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const UPPER_CLEAR: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const DIGITS_CLEAR: &str = "23456789";
const SYMBOLS: &str = "!@#$%^&*-_+=?";

pub trait Source {
    fn below(&mut self, bound: u32) -> u32;
}

pub struct System;

impl Source for System {
    fn below(&mut self, bound: u32) -> u32 {
        if bound <= 1 {
            return 0;
        }
        let zone = u32::MAX - u32::MAX % bound;
        loop {
            let mut bytes = [0; 4];
            getrandom::fill(&mut bytes).expect("the system random source refuses to give randomness");
            let value = u32::from_le_bytes(bytes);
            if value < zone {
                return value % bound;
            }
        }
    }
}

impl Recipe {
    #[must_use]
    pub const fn amount(&self) -> u32 {
        match self.mode {
            Mode::Password => self.length,
            Mode::Phrase => self.words,
            Mode::Pin => self.digits_only,
        }
    }

    #[must_use]
    pub const fn range(&self) -> (u32, u32) {
        match self.mode {
            Mode::Password => LENGTH,
            Mode::Phrase => WORDS,
            Mode::Pin => PIN,
        }
    }

    pub const fn set_amount(&mut self, amount: u32) {
        let (low, high) = self.range();
        let amount = if amount < low {
            low
        } else if amount > high {
            high
        } else {
            amount
        };
        match self.mode {
            Mode::Password => self.length = amount,
            Mode::Phrase => self.words = amount,
            Mode::Pin => self.digits_only = amount,
        }
    }

    fn alphabet(&self) -> String {
        let mut set = String::from(if self.avoid_similar { LOWER_CLEAR } else { LOWER });
        if self.upper {
            set.push_str(if self.avoid_similar { UPPER_CLEAR } else { UPPER });
        }
        if self.digits {
            set.push_str(if self.avoid_similar { DIGITS_CLEAR } else { DIGITS });
        }
        if self.symbols {
            set.push_str(SYMBOLS);
        }
        set
    }

    #[must_use]
    pub fn bits(&self) -> f64 {
        match self.mode {
            Mode::Password => f64::from(self.length) * (self.alphabet().chars().count() as f64).log2(),
            Mode::Phrase => f64::from(self.words) * 11.0 + if self.number { 3.3 } else { 0.0 },
            Mode::Pin => f64::from(self.digits_only) * 10_f64.log2(),
        }
    }

    #[must_use]
    pub fn strength(&self) -> Strength {
        Strength::of_bits(self.bits())
    }

    pub fn generate(&self, source: &mut impl Source) -> String {
        match self.mode {
            Mode::Pin => (0..self.digits_only).map(|_| char::from(b'0' + source.below(10) as u8)).collect(),
            Mode::Phrase => {
                let list = bip39::Language::English.word_list();
                let mut words: Vec<String> = (0..self.words)
                    .map(|_| {
                        let word = list[source.below(list.len() as u32) as usize];
                        if self.capitalise {
                            let mut chars = word.chars();
                            chars
                                .next()
                                .map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect::<String>())
                        } else {
                            word.to_string()
                        }
                    })
                    .collect();
                if self.number && !words.is_empty() {
                    let at = source.below(words.len() as u32) as usize;
                    words[at].push(char::from(b'0' + source.below(10) as u8));
                }
                words.join("-")
            }
            Mode::Password => {
                let alphabet: Vec<char> = self.alphabet().chars().collect();
                let mut groups: Vec<Vec<char>> = Vec::new();
                if self.upper {
                    groups.push(if self.avoid_similar { UPPER_CLEAR } else { UPPER }.chars().collect());
                }
                if self.digits {
                    groups.push(if self.avoid_similar { DIGITS_CLEAR } else { DIGITS }.chars().collect());
                }
                if self.symbols {
                    groups.push(SYMBOLS.chars().collect());
                }
                loop {
                    let candidate: Vec<char> =
                        (0..self.length).map(|_| alphabet[source.below(alphabet.len() as u32) as usize]).collect();
                    let complete = self.length < groups.len() as u32 + 1
                        || groups.iter().all(|group| candidate.iter().any(|ch| group.contains(ch)));
                    if complete {
                        return candidate.into_iter().collect();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter(u32);

    impl Source for Counter {
        fn below(&mut self, bound: u32) -> u32 {
            self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (self.0 >> 8) % bound
        }
    }

    #[test]
    fn a_password_has_its_length_and_every_class_asked_for() {
        let recipe = Recipe::default();
        let password = recipe.generate(&mut Counter(7));
        assert_eq!(password.chars().count(), 20);
        assert!(password.chars().any(|ch| ch.is_ascii_uppercase()));
        assert!(password.chars().any(|ch| ch.is_ascii_digit()));
        assert!(password.chars().any(|ch| SYMBOLS.contains(ch)));
        assert!(!password.chars().any(|ch| "l1O0I".contains(ch)), "similar glyphs are avoided: {password}");
    }

    #[test]
    fn a_phrase_joins_capitalised_words_with_one_number() {
        let recipe = Recipe { mode: Mode::Phrase, words: 4, ..Recipe::default() };
        let phrase = recipe.generate(&mut Counter(3));
        let words: Vec<&str> = phrase.split('-').collect();
        assert_eq!(words.len(), 4);
        assert!(words.iter().all(|word| word.chars().next().is_some_and(char::is_uppercase)));
        assert_eq!(phrase.chars().filter(char::is_ascii_digit).count(), 1);
    }

    #[test]
    fn a_pin_is_only_digits() {
        let recipe = Recipe { mode: Mode::Pin, digits_only: 8, ..Recipe::default() };
        let pin = recipe.generate(&mut Counter(1));
        assert_eq!(pin.len(), 8);
        assert!(pin.chars().all(|ch| ch.is_ascii_digit()));
    }

    #[test]
    fn amounts_are_clamped_to_their_mode() {
        let mut recipe = Recipe::default();
        recipe.set_amount(2);
        assert_eq!(recipe.length, 8);
        recipe.mode = Mode::Phrase;
        recipe.set_amount(40);
        assert_eq!(recipe.words, 10);
    }

    #[test]
    fn strength_grows_with_length() {
        let short = Recipe { length: 8, symbols: false, upper: false, ..Recipe::default() };
        assert!(short.strength() < Recipe::default().strength());
        assert_eq!(Recipe::default().strength(), Strength::VeryStrong);
    }

    #[test]
    fn the_system_source_stays_below_its_bound() {
        let mut source = System;
        assert!((0..200).all(|_| source.below(7) < 7));
    }
}
