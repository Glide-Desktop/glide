use std::path::{Path, PathBuf};

use iced::widget::{button, column, container, image, row, text};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::error_banner;
use super::network::{form_row, note};
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::accounts::{self, Account, Session, UsersState, initials};
use crate::services::config::ConfigStore;
use crate::services::run::blocking;
use crate::theme::{self, ThemeState, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Unavailable => "Accounts are unavailable", "Облікові записи недоступні";
        UnavailableHint => "AccountsService is needed to show and edit users", "Щоб показувати й змінювати користувачів, потрібен AccountsService";
        Administrator => "Administrator", "Адміністратор";
        Standard => "Standard", "Звичайний";
        AutoLogin => "Signs in automatically", "Входить автоматично";
        Locked => "Locked", "Заблокований";
        Profile => "Profile", "Профіль";
        ProfileHint => "Shown on the lock screen, in the sign-in screen and in Settings", "Показується на екрані блокування, екрані входу та в налаштуваннях";
        FullName => "Full name", "Повне ім'я";
        Save => "Save", "Зберегти";
        Picture => "Picture", "Зображення";
        PictureHint => "Pick a picture or enter the path to a PNG or JPEG file", "Оберіть зображення або введіть шлях до файлу PNG чи JPEG";
        PicturePath => "~/Pictures/me.png", "~/Pictures/me.png";
        Use => "Use", "Застосувати";
        Account => "Account", "Обліковий запис";
        AccountHint => "Changing these needs an administrator", "Щоб змінити це, потрібен адміністратор";
        UserName => "User name", "Ім'я користувача";
        AccountType => "Account type", "Тип облікового запису";
        AutomaticLogin => "Automatic login", "Автоматичний вхід";
        On => "On", "Увімкнено";
        Off => "Off", "Вимкнено";
        Home => "Home folder", "Домашня тека";
        Shell => "Shell", "Оболонка";
        Groups => "Groups", "Групи";
        LastSignIn => "Last sign-in", "Останній вхід";
        SignIns => "Sign-ins", "Кількість входів";
        JustNow => "just now", "щойно";
        MinutesAgo => "min ago", "хв тому";
        HoursAgo => "h ago", "год тому";
        DaysAgo => "days ago", "дн. тому";
        Sessions => "Sessions", "Сеанси";
        SessionsHint => "Where you are signed in on this computer", "Де ви ввійшли на цьому комп'ютері";
        ThisSession => "This session", "Цей сеанс";
        Remote => "remote", "віддалений";
        End => "Sign out", "Вийти";
        Linger => "Keep my background services running", "Не зупиняти мої фонові служби";
        LingerHint => "User services keep running after you sign out", "Служби користувача працюють і після виходу";
        Others => "Other users", "Інші користувачі";
        NoOthers => "You are the only user", "Ви єдиний користувач";
        Saved => "Saved", "Збережено";
        PolkitHint => "Some changes ask for your password through polkit. Without a polkit agent they fail with “Permission denied”.", "Деякі зміни просять пароль через polkit. Без агента polkit вони завершаться помилкою «Доступ заборонено».";
    }
}

use tr::Key as T;

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Box<UsersState>),
    NameInput(String),
    SaveName,
    PickFace(PathBuf),
    PathInput(String),
    UsePath,
    Linger(bool),
    EndSession(String),
    Done(Option<Result<String, String>>),
}

#[allow(clippy::struct_excessive_bools)] // Independent loading, saving and busy flags
#[derive(Debug, Default)]
pub struct Page {
    state: UsersState,
    loaded: bool,
    loading: bool,
    name: Option<String>,
    path: String,
    error: Option<String>,
    saved: bool,
    busy: bool,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["user", "account", "avatar", "name", "session", "користувач", "обліковий", "аватар", "ім'я", "сеанс"];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.reload()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(state) => {
                self.state = *state;
                self.loaded = true;
                self.loading = false;
            }
            Msg::NameInput(name) => {
                self.saved = false;
                self.name = Some(name);
            }
            Msg::SaveName => {
                let (Some(me), Some(name)) = (self.state.me.as_ref(), self.name.clone()) else {
                    return Task::none();
                };
                let path = me.path.clone();
                return self.run(move || accounts::set_real_name(&path, &name));
            }
            Msg::PickFace(file) => return self.set_icon(file.to_string_lossy().into_owned()),
            Msg::PathInput(path) => self.path = path,
            Msg::UsePath => return self.set_icon(self.path.trim().to_string()),
            Msg::Linger(on) => {
                let user = self.state.me.as_ref().map(|me| me.user_name.clone()).unwrap_or_default();
                self.state.linger = Some(on);
                return self.run(move || accounts::set_linger(&user, on));
            }
            Msg::EndSession(id) => {
                self.state.sessions.retain(|session| session.id != id);
                return self.run(move || accounts::terminate_session(&id));
            }
            Msg::Done(result) => {
                self.busy = false;
                match result {
                    Some(Ok(_)) => {
                        self.saved = true;
                        self.name = None;
                        self.path.clear();
                    }
                    Some(Err(err)) => self.error = Some(err),
                    None => {}
                }
                return self.reload();
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let Some(me) = self.state.me.as_ref().filter(|_| self.state.available) else {
            return if self.loaded {
                widgets::empty_state(t, Icon::Users, c.t(T::Unavailable), c.t(T::UnavailableHint))
            } else {
                widgets::empty_state(t, Icon::Users, c.t(K::Loading), "")
            };
        };
        column![
            hero(&c, me),
            error_banner(t, self.error.as_deref()),
            self.profile(&c, me),
            self.account(&c, me),
            self.sessions(&c),
            self.others(&c),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

impl Page {
    fn reload(&mut self) -> Task<Msg> {
        if self.loading {
            return Task::none();
        }
        self.loading = true;
        Task::perform(blocking(accounts::read), |state| Msg::Loaded(Box::new(state.unwrap_or_default())))
    }

    fn run(&mut self, job: impl FnOnce() -> Result<String, String> + Send + 'static) -> Task<Msg> {
        self.error = None;
        self.saved = false;
        self.busy = true;
        Task::perform(blocking(job), Msg::Done)
    }

    fn set_icon(&mut self, file: String) -> Task<Msg> {
        let Some(me) = self.state.me.as_ref() else {
            return Task::none();
        };
        let path = me.path.clone();
        self.run(move || accounts::set_icon(&path, &file))
    }

    #[must_use]
    pub const fn state(&self) -> &UsersState {
        &self.state
    }

    fn profile<'a>(&'a self, c: &Ctx, me: &'a Account) -> Element<'a, Msg> {
        let t = c.theme;
        let draft = self.name.clone().unwrap_or_else(|| me.real_name.clone());
        let changed = self.name.as_ref().is_some_and(|name| name.trim() != me.real_name.trim());
        let save = (changed && !self.busy).then_some(Msg::SaveName);
        let status: Element<'a, Msg> = if self.saved && !changed {
            widgets::status_pill(t, Tone::Ok, c.t(T::Saved))
        } else {
            widgets::primary(t, c.t(T::Save), save.clone())
        };
        let faces = row(self.state.faces.iter().map(|face| {
            face_button(
                t,
                face,
                Path::new(&me.icon_file) == face.as_path(),
                (!self.busy).then(|| Msg::PickFace(face.clone())),
            )
        }))
        .spacing(8);
        let path_ready = !self.path.trim().is_empty() && !self.busy;
        let mut group = Group::new(t).title(c.t(T::Profile)).description(c.t(T::ProfileHint)).push(form_row(
            t,
            c.t(T::FullName),
            None,
            row![widgets::field(t, &me.user_name, &draft, Msg::NameInput, save, false).width(Fill), status,]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
        ));
        if !self.state.faces.is_empty() {
            group = group.push(
                container(
                    column![text(c.t(T::Picture)).size(13.5).color(t.fg_1()), faces.wrap().vertical_spacing(8),]
                        .spacing(8),
                )
                .padding([8.0, theme::ROW_PADDING_X])
                .width(Fill),
            );
        }
        group
            .push(form_row(
                t,
                if self.state.faces.is_empty() { c.t(T::Picture) } else { "" },
                Some(c.t(T::PictureHint)),
                row![
                    widgets::field(
                        t,
                        c.t(T::PicturePath),
                        &self.path,
                        Msg::PathInput,
                        path_ready.then_some(Msg::UsePath),
                        false,
                    )
                    .width(Fill),
                    widgets::secondary(t, c.t(T::Use), path_ready.then_some(Msg::UsePath)),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ))
            .push(note(t, Icon::Lock, c.t(T::PolkitHint)))
            .build()
    }

    fn account<'a>(&'a self, c: &Ctx, me: &'a Account) -> Element<'a, Msg> {
        let t = c.theme;
        let kind = if me.administrator { T::Administrator } else { T::Standard };
        let login = if me.automatic_login { T::On } else { T::Off };
        let info = [
            (T::UserName, Some(me.user_name.clone())),
            (T::AccountType, Some(c.t(kind).to_string())),
            (T::AutomaticLogin, Some(c.t(login).to_string())),
            (T::LastSignIn, (me.login_time > 0).then(|| ago(c, me.login_time))),
            (T::SignIns, (me.login_frequency > 0).then(|| me.login_frequency.to_string())),
            (T::Home, (!me.home.is_empty()).then(|| me.home.clone())),
            (T::Shell, (!me.shell.is_empty()).then(|| me.shell.clone())),
            (T::Groups, (!self.state.groups.is_empty()).then(|| self.state.groups.join(", "))),
        ]
        .into_iter()
        .filter_map(|(label, value)| Some((c.t(label).to_string(), value?)))
        .collect();
        column![
            Group::new(t).title(c.t(T::Account)).description(c.t(T::AccountHint)).build(),
            widgets::info_grid(t, info),
        ]
        .spacing(8)
        .into()
    }

    fn sessions(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Sessions)).description(c.t(T::SessionsHint));
        for session in &self.state.sessions {
            group = group.push(session_row(c, session, &self.state.current_session, self.busy));
        }
        if let Some(linger) = self.state.linger {
            group = group.push(
                Row::new(t, c.t(T::Linger))
                    .subtitle(c.t(T::LingerHint))
                    .suffix(widgets::switch(t, linger, Msg::Linger))
                    .build(),
            );
        }
        group.build()
    }

    fn others(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Others));
        if self.state.others.is_empty() {
            return group.push(widgets::list_empty(t, Icon::Users, c.t(T::NoOthers), "")).build();
        }
        for user in &self.state.others {
            let mut badges = row![].spacing(6).align_y(Alignment::Center);
            if user.administrator {
                badges = badges.push(widgets::pill(t, Tone::Accent, c.t(T::Administrator)));
            }
            if user.locked {
                badges = badges.push(widgets::pill(t, Tone::Danger, c.t(T::Locked)));
            }
            group = group.push(
                Row::new(t, user.display_name().to_string())
                    .subtitle(user.user_name.clone())
                    .prefix(portrait(t, user, 32.0))
                    .suffix(badges)
                    .build(),
            );
        }
        group.build()
    }
}

fn session_row<'a>(c: &Ctx, session: &'a Session, current: &str, busy: bool) -> Element<'a, Msg> {
    let t = c.theme;
    let this = session.id == current;
    let mut place: Vec<String> = Vec::new();
    if !session.kind.is_empty() && session.kind != "unspecified" {
        place.push(session.kind.clone());
    }
    if !session.tty.is_empty() {
        place.push(session.tty.clone());
    } else if !session.seat.is_empty() {
        place.push(session.seat.clone());
    }
    if session.remote {
        place.push(c.t(T::Remote).to_string());
    }
    if !session.since.is_empty() {
        place.push(session.since.clone());
    }
    let suffix: Element<'a, Msg> = if this {
        widgets::status_pill(t, Tone::Ok, c.t(T::ThisSession))
    } else {
        widgets::secondary(t, c.t(T::End), (!busy).then(|| Msg::EndSession(session.id.clone())))
    };
    Row::new(t, format!("{} {}", session.user, session.id))
        .subtitle(place.join(" · "))
        .prefix(widgets::icon(
            if session.remote { Icon::Globe } else { Icon::Display },
            18.0,
            if this { t.acc_text() } else { t.fg_2() },
        ))
        .suffix(suffix)
        .build()
}

fn ago(c: &Ctx, timestamp: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX));
    let seconds = (now - timestamp).max(0);
    match seconds {
        0..60 => c.t(T::JustNow).to_string(),
        60..3600 => format!("{} {}", seconds / 60, c.t(T::MinutesAgo)),
        3600..86_400 => format!("{} {}", seconds / 3600, c.t(T::HoursAgo)),
        _ => format!("{} {}", seconds / 86_400, c.t(T::DaysAgo)),
    }
}

#[must_use]
pub fn portrait<'a>(t: ThemeState, account: &Account, size: f32) -> Element<'a, Msg> {
    match account.icon() {
        Some(path) => image(image::Handle::from_path(path))
            .width(size)
            .height(size)
            .content_fit(iced::ContentFit::Cover)
            .border_radius(size / 2.0)
            .into(),
        None => widgets::avatar(t, initials(account.display_name()), size),
    }
}

#[must_use]
pub fn face_button<'a>(t: ThemeState, face: &Path, selected: bool, on_press: Option<Msg>) -> Element<'a, Msg> {
    button(
        image(image::Handle::from_path(face))
            .width(48)
            .height(48)
            .content_fit(iced::ContentFit::Cover)
            .border_radius(24.0),
    )
    .on_press_maybe(on_press)
    .padding(theme::RING_GAP)
    .style(move |_: &iced::Theme, status| theme::ring_button_style(&t, selected, status))
    .into()
}

fn hero<'a>(c: &Ctx, me: &'a Account) -> Element<'a, Msg> {
    let t = c.theme;
    let (tone, kind) = if me.administrator { (Tone::Accent, T::Administrator) } else { (Tone::Neutral, T::Standard) };
    let mut badges = row![widgets::pill(t, tone, c.t(kind))].spacing(6);
    if me.automatic_login {
        badges = badges.push(widgets::pill(t, Tone::Warn, c.t(T::AutoLogin)));
    }
    let tint = super::Panel::Users.color();
    container(
        row![
            portrait(t, me, 72.0),
            column![
                text(me.display_name().to_string()).size(22).font(theme::SEMIBOLD).color(t.fg_1()),
                text(me.user_name.clone()).size(13).color(t.fg_2()),
                badges,
            ]
            .spacing(4)
            .width(Fill),
        ]
        .spacing(18)
        .align_y(Alignment::Center),
    )
    .padding(20)
    .width(Length::Fill)
    .style(move |_: &iced::Theme| theme::hero_card_style(&t, tint))
    .into()
}
