use glide_config::permissions::Access;
use glide_sdk::theme::TextStyle;
use glide_sdk::widget as sdk;
use iced::widget::{Column, column};
use iced::{Element, Task};

use super::common::error_banner;
use super::{Ctx, Pane, Panel};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::privacy::{self, PrivacyState};
use crate::services::run::blocking;
use crate::theme::{self, Accent, Tone, ramp_color};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Idle => "Nobody can see your screen", "Зараз екран ніхто не бачить";
        IdleHint => "Apps ask before they show or record it", "Програми питають, перш ніж показати чи записати його";
        Busy => "Your screen is being shared", "Ваш екран зараз бачать";
        Live => "Live", "Наживо";
        Quiet => "Not shared", "Не транслюється";
        Always => "Always on", "Завжди";
        WhileSharing => "While the screen is shared", "Поки екран транслюється";
        HideBanners => "Hide notification banners", "Приховувати банери сповіщень";
        HideBannersHint => "They wait in the notification centre. Urgent ones still show", "Вони чекають у центрі сповіщень. Термінові все одно показуються";
        Secrets => "Passwords stay covered", "Паролі лишаються закритими";
        SecretsHint => "Password prompts, vault unlocking and the Passwords menu reach the stream as a closed panel", "Запити паролів, розблокування сховищ і меню «Паролі» потрапляють у трансляцію закритою панеллю";
        Dot => "A purple dot in the top bar", "Фіолетова крапка у верхній панелі";
        DotHint => "Lit while anything sees the screen. Click it to see which apps", "Горить, поки щось бачить екран. Клацніть її, щоб побачити, які програми";
        Apps => "Programs with access", "Програми з доступом";
        AppsHint => "Programs that record the screen directly ask once; your answer is kept until you change it here", "Програми, що записують екран напряму, питають один раз; відповідь зберігається, доки ви не зміните її тут";
        NoApps => "No program has asked to record the screen", "Жодна програма ще не просила записувати екран";
        Allowed => "Allowed", "Дозволено";
        Denied => "Denied", "Заборонено";
        Forget => "Remove", "Прибрати";
        How => "How apps get the screen", "Як програми отримують екран";
        Portal => "Through the desktop portal", "Через портал робочого столу";
        PortalHint => "Browsers, video calls and OBS. You choose the whole screen or one window each time, and nothing is remembered", "Браузери, відеодзвінки й OBS. Щоразу ви вибираєте весь екран або одне вікно, і нічого не запам’ятовується";
        Direct => "Directly", "Напряму";
        DirectHint => "Tools such as wf-recorder and grim. Glide asks once and keeps an Allow until you remove it", "Інструменти на кшталт wf-recorder і grim. Glide питає один раз і пам’ятає «Дозволити», доки ви не приберете його";
        Mirroring => "Show the screen on a TV", "Показати екран на телевізорі";
        MirroringHint => "Wireless display", "Бездротовий дисплей";
    }
}

use tr::Key as T;

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Box<PrivacyState>),
    Forget(String),
    Forgotten(Option<Result<String, String>>),
    HideBanners(bool),
    Open(Panel),
}

#[derive(Debug, Default)]
pub struct Page {
    state: PrivacyState,
    loading: bool,
    error: Option<String>,
}

impl Page {
    fn reload(&mut self) -> Task<Msg> {
        if self.loading {
            return Task::none();
        }
        self.loading = true;
        Task::perform(blocking(privacy::read), |state| Msg::Loaded(Box::new(state.unwrap_or_default())))
    }

    fn programs(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let rule = self.state.permissions.rule(Access::Screen);
        let mut group = Group::new(t).title(c.t(T::Apps)).description(c.t(T::AppsHint));
        let entries =
            rule.allow.iter().map(|program| (program, true)).chain(rule.deny.iter().map(|program| (program, false)));
        let mut empty = true;
        for (program, allowed) in entries {
            empty = false;
            let name = std::path::Path::new(program).file_name().and_then(std::ffi::OsStr::to_str).unwrap_or(program);
            let (tone, verdict) = if allowed { (Tone::Ok, T::Allowed) } else { (Tone::Danger, T::Denied) };
            group = group.push(
                Row::new(t, name)
                    .subtitle(program.clone())
                    .prefix(widgets::icon(Icon::Display, 18.0, t.fg_2()))
                    .suffix(
                        iced::widget::row![
                            widgets::status_pill(t, tone, c.t(verdict)),
                            widgets::secondary(t, c.t(T::Forget), Some(Msg::Forget(program.clone()))),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .build(),
            );
        }
        if empty {
            group = group.push(widgets::note(t, Icon::Shield, c.t(T::NoApps)));
        }
        group.build()
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "screen sharing",
        "screencast",
        "share screen",
        "record screen",
        "recording",
        "portal",
        "трансляція",
        "демонстрація екрана",
        "запис екрана",
        "поділитися екраном",
    ];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.reload()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.reload()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::HideBanners(true) => config.remove("notifications", "hide-while-sharing"),
            Msg::HideBanners(false) => config.set_bool("notifications", "hide-while-sharing", false),
            Msg::Loaded(state) => {
                self.state = *state;
                self.loading = false;
            }
            Msg::Forget(program) => {
                self.error = None;
                self.state.permissions.forget(Access::Screen, &program);
                return Task::perform(
                    blocking(move || privacy::forget_permission(Access::Screen, &program)),
                    Msg::Forgotten,
                );
            }
            Msg::Forgotten(result) => {
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
                return self.reload();
            }
            Msg::Open(_) => {}
        }
        Task::none()
    }

    fn sidebar_status(&self, c: Ctx) -> Option<String> {
        (!self.state.usage.screen.is_empty()).then(|| c.t(T::Live).to_string())
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let sharing = &self.state.usage.screen;
        let purple = ramp_color(Accent::Amethyst, 3);
        let hero = if sharing.is_empty() {
            widgets::section_hero(
                t,
                Icon::Cast,
                Panel::ScreenShare.color(),
                c.t(T::Idle),
                c.t(T::IdleHint),
                Some(widgets::status_pill(t, Tone::Neutral, c.t(T::Quiet))),
            )
        } else {
            widgets::section_hero(
                t,
                Icon::Cast,
                purple,
                c.t(T::Busy),
                sharing.join(", "),
                Some(widgets::status_pill(t, Tone::Accent, c.t(T::Live))),
            )
        };
        let quiet = config.bool_or("notifications", "hide-while-sharing", true);
        let facts = Group::new(t)
            .title(c.t(T::WhileSharing))
            .push(
                Row::new(t, c.t(T::HideBanners))
                    .subtitle(c.t(T::HideBannersHint))
                    .changed(!quiet)
                    .prefix(widgets::icon(Icon::Bell, 18.0, t.fg_2()))
                    .suffix(super::common::toggle(c, quiet, Msg::HideBanners))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Secrets))
                    .subtitle(c.t(T::SecretsHint))
                    .prefix(widgets::icon(Icon::Lock, 18.0, t.ok()))
                    .suffix(sdk::text(c.t(T::Always), TextStyle::CALL).color(t.fg_2()))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Dot))
                    .subtitle(c.t(T::DotHint))
                    .prefix(widgets::status_dot(purple, 10.0))
                    .suffix(sdk::text(c.t(T::Always), TextStyle::CALL).color(t.fg_2()))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Mirroring))
                    .subtitle(c.t(T::MirroringHint))
                    .prefix(widgets::icon(Icon::Tv, 18.0, t.fg_2()))
                    .suffix(widgets::disclosure(t, None))
                    .on_press(Msg::Open(Panel::Miracast))
                    .build(),
            );
        let how =
            Column::new().spacing(8).push(sdk::section_heading(&widgets::paint(&t), c.t(T::How))).push(sdk::grid(
                vec![
                    sdk::Glance::new(&widgets::paint(&t), Icon::Window, ramp_color(Accent::Azurit, 3), c.t(T::Portal))
                        .detail(c.t(T::PortalHint))
                        .build(),
                    sdk::Glance::new(
                        &widgets::paint(&t),
                        Icon::Terminal,
                        ramp_color(Accent::Graphite, 3),
                        c.t(T::Direct),
                    )
                    .detail(c.t(T::DirectHint))
                    .build(),
                ],
                if t.compact { 1 } else { 2 },
                12.0,
            ));
        column![hero, error_banner(t, self.error.as_deref()), facts.build(), self.programs(&c), how]
            .spacing(theme::SECTION_GAP)
            .into()
    }
}
