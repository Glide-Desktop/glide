use std::path::{Path, PathBuf};

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Fill, Task};

use super::common::error_banner;
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::accounts::{self, Account, Session, UsersState};
use crate::services::config::ConfigStore;
use crate::services::run::blocking;
use crate::theme::{self, ThemeState, Tone};
use crate::widgets::{self, Group, Row};
use crate::widgets::{form_row, note};

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
        PolkitHint => "Some changes ask for your password in the status island at the top of the screen.", "Деякі зміни просять пароль в острові вгорі екрана.";
        Password => "Password", "Пароль";
        ChangePassword => "Change…", "Змінити…";
        SetPassword => "Password…", "Пароль…";
        NewPassword => "New password", "Новий пароль";
        ConfirmPassword => "Repeat it", "Повторіть його";
        AddUser => "Add user…", "Додати користувача…";
        NewUser => "New user", "Новий користувач";
        Create => "Create", "Створити";
        Cancel => "Cancel", "Скасувати";
        Remove => "Remove…", "Видалити…";
        RemoveAsk => "Remove {}? Their sign-in goes away; choose what happens to their files.", "Видалити {}? Вхід цього користувача зникне; оберіть, що буде з файлами.";
        KeepFiles => "Remove, keep files", "Видалити, лишити файли";
        DeleteFiles => "Remove with files", "Видалити разом із файлами";
        MakeAdmin => "Make administrator", "Зробити адміністратором";
        MakeStandard => "Make standard", "Зробити звичайним";
        NameEmpty => "Enter a user name", "Введіть ім'я користувача";
        NameTooLong => "At most 32 characters", "Не більше 32 символів";
        NameBadStart => "Start with a lowercase Latin letter", "Почніть з малої латинської літери";
        NameBadCharacter => "Only a–z, 0–9, - and _", "Лише a–z, 0–9, - і _";
        PasswordEmpty => "Enter a password", "Введіть пароль";
        PasswordMismatch => "The passwords differ", "Паролі не збігаються";
        PasswordLineBreak => "No line breaks", "Без переносів рядка";
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Form {
    AddUser { full_name: String, user_name: String, named: bool, password: String, confirm: String, admin: bool },
    Password { path: String, password: String, confirm: String },
    Remove { uid: u64, name: String },
}

fn name_hint(problem: accounts::UserNameProblem) -> T {
    match problem {
        accounts::UserNameProblem::Empty => T::NameEmpty,
        accounts::UserNameProblem::TooLong => T::NameTooLong,
        accounts::UserNameProblem::BadStart => T::NameBadStart,
        accounts::UserNameProblem::BadCharacter => T::NameBadCharacter,
    }
}

fn password_hint(problem: accounts::PasswordProblem) -> T {
    match problem {
        accounts::PasswordProblem::Empty => T::PasswordEmpty,
        accounts::PasswordProblem::Mismatch => T::PasswordMismatch,
        accounts::PasswordProblem::LineBreak => T::PasswordLineBreak,
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
    OpenAddUser,
    OpenPassword(String),
    AskRemove(u64, String),
    CloseForm,
    FullName(String),
    UserName(String),
    NewPassword(String),
    ConfirmPassword(String),
    Admin(bool),
    Create,
    SavePassword,
    Remove(bool),
    AccountType(String, bool),
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
    form: Option<Form>,
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
            Msg::OpenAddUser
            | Msg::OpenPassword(_)
            | Msg::AskRemove(..)
            | Msg::CloseForm
            | Msg::FullName(_)
            | Msg::UserName(_)
            | Msg::NewPassword(_)
            | Msg::ConfirmPassword(_)
            | Msg::Admin(_) => self.edit_form(msg),
            Msg::Create | Msg::SavePassword | Msg::Remove(_) => return self.submit_form(msg),
            Msg::AccountType(path, admin) => return self.run(move || accounts::set_account_type(&path, admin)),
            Msg::Done(result) => {
                self.busy = false;
                match result {
                    Some(Ok(_)) => {
                        self.saved = true;
                        self.name = None;
                        self.path.clear();
                        self.form = None;
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
    fn edit_form(&mut self, msg: Msg) {
        match (msg, self.form.as_mut()) {
            (Msg::OpenAddUser, _) => {
                self.form = Some(Form::AddUser {
                    full_name: String::new(),
                    user_name: String::new(),
                    named: false,
                    password: String::new(),
                    confirm: String::new(),
                    admin: false,
                });
            }
            (Msg::OpenPassword(path), _) => {
                self.form = Some(Form::Password { path, password: String::new(), confirm: String::new() });
            }
            (Msg::AskRemove(uid, name), _) => self.form = Some(Form::Remove { uid, name }),
            (Msg::CloseForm, _) => self.form = None,
            (Msg::FullName(value), Some(Form::AddUser { full_name, user_name, named, .. })) => {
                if !*named {
                    *user_name = accounts::suggest_user_name(&value);
                }
                *full_name = value;
            }
            (Msg::UserName(value), Some(Form::AddUser { user_name, named, .. })) => {
                *named = !value.is_empty();
                *user_name = value;
            }
            (Msg::NewPassword(value), Some(Form::AddUser { password, .. } | Form::Password { password, .. })) => {
                *password = value;
            }
            (Msg::ConfirmPassword(value), Some(Form::AddUser { confirm, .. } | Form::Password { confirm, .. })) => {
                *confirm = value;
            }
            (Msg::Admin(on), Some(Form::AddUser { admin, .. })) => *admin = on,
            _ => {}
        }
    }

    fn submit_form(&mut self, msg: Msg) -> Task<Msg> {
        match (msg, self.form.clone()) {
            (Msg::Create, Some(Form::AddUser { full_name, user_name, password, confirm, admin, .. }))
                if accounts::user_name_problem(&user_name).is_none()
                    && accounts::password_problem(&password, &confirm).is_none() =>
            {
                self.run(move || accounts::create_user(&user_name, &full_name, admin, &password))
            }
            (Msg::SavePassword, Some(Form::Password { path, password, confirm }))
                if accounts::password_problem(&password, &confirm).is_none() =>
            {
                let own = self.state.me.as_ref().is_some_and(|me| me.path == path);
                self.run(move || {
                    let changed = accounts::set_password(&path, &password)?;
                    if own {
                        accounts::tell_the_shell_about(password);
                    }
                    Ok(changed)
                })
            }
            (Msg::Remove(remove_files), Some(Form::Remove { uid, .. })) => {
                self.run(move || accounts::delete_user(uid, remove_files))
            }
            _ => Task::none(),
        }
    }

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
                    column![
                        text(c.t(T::Picture), TextStyle::MENU_ITEM).color(t.fg_1()),
                        faces.wrap().vertical_spacing(8)
                    ]
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
        let mut password = Group::new(t).push(
            Row::new(t, c.t(T::Password))
                .suffix(widgets::secondary(
                    t,
                    c.t(T::ChangePassword),
                    (!self.busy).then(|| Msg::OpenPassword(me.path.clone())),
                ))
                .build(),
        );
        if let Some(form) = self.password_form(c, &me.path) {
            password = password.push(form);
        }
        column![
            Group::new(t).title(c.t(T::Account)).description(c.t(T::AccountHint)).build(),
            widgets::info_grid(t, info),
            password.build(),
        ]
        .spacing(8)
        .into()
    }

    fn password_form<'a>(&'a self, c: &Ctx, owner: &str) -> Option<Element<'a, Msg>> {
        let Some(Form::Password { path, password, confirm }) = self.form.as_ref() else {
            return None;
        };
        if path != owner {
            return None;
        }
        let t = c.theme;
        let problem = accounts::password_problem(password, confirm);
        let ready = problem.is_none() && !self.busy;
        let save = ready.then_some(Msg::SavePassword);
        let hint = problem.filter(|_| !confirm.is_empty()).map(|problem| c.t(password_hint(problem)));
        Some(
            column![
                form_row(
                    t,
                    c.t(T::NewPassword),
                    None,
                    widgets::field(t, "", password, Msg::NewPassword, None, false).secure(true).into(),
                ),
                form_row(
                    t,
                    c.t(T::ConfirmPassword),
                    hint,
                    widgets::field(t, "", confirm, Msg::ConfirmPassword, save.clone(), hint.is_some())
                        .secure(true)
                        .into(),
                ),
                form_buttons(c, widgets::primary(t, c.t(K::Apply), save)),
            ]
            .spacing(4)
            .into(),
        )
    }

    fn add_user_form(&self, c: &Ctx) -> Option<Element<'_, Msg>> {
        let Some(Form::AddUser { full_name, user_name, password, confirm, admin, .. }) = self.form.as_ref() else {
            return None;
        };
        let t = c.theme;
        let name_problem = accounts::user_name_problem(user_name);
        let password_problem = accounts::password_problem(password, confirm);
        let ready = name_problem.is_none() && password_problem.is_none() && !self.busy;
        let create = ready.then_some(Msg::Create);
        let name_hint = name_problem.filter(|_| !user_name.is_empty()).map(|problem| c.t(name_hint(problem)));
        let password_hint = password_problem.filter(|_| !confirm.is_empty()).map(|problem| c.t(password_hint(problem)));
        Some(
            column![
                text(c.t(T::NewUser), TextStyle::WIN_TITLE).color(t.fg_1()),
                form_row(
                    t,
                    c.t(T::FullName),
                    None,
                    widgets::field(t, "", full_name, Msg::FullName, None, false).into()
                ),
                form_row(
                    t,
                    c.t(T::UserName),
                    name_hint,
                    widgets::field(t, "", user_name, Msg::UserName, None, name_hint.is_some()).into(),
                ),
                form_row(
                    t,
                    c.t(T::NewPassword),
                    None,
                    widgets::field(t, "", password, Msg::NewPassword, None, false).secure(true).into(),
                ),
                form_row(
                    t,
                    c.t(T::ConfirmPassword),
                    password_hint,
                    widgets::field(t, "", confirm, Msg::ConfirmPassword, create.clone(), password_hint.is_some())
                        .secure(true)
                        .into(),
                ),
                form_row(t, c.t(T::Administrator), None, widgets::switch(t, *admin, Msg::Admin)),
                form_buttons(c, widgets::primary(t, c.t(T::Create), create)),
            ]
            .spacing(4)
            .padding([8.0, theme::ROW_PADDING_X])
            .into(),
        )
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
            group = group.push(widgets::list_empty(t, Icon::Users, c.t(T::NoOthers), ""));
        }
        let idle = !self.busy;
        for user in &self.state.others {
            let mut badges = row![].spacing(6).align_y(Alignment::Center);
            if user.administrator {
                badges = badges.push(widgets::pill(t, Tone::Accent, c.t(T::Administrator)));
            }
            if user.locked {
                badges = badges.push(widgets::pill(t, Tone::Danger, c.t(T::Locked)));
            }
            let kind = if user.administrator { T::MakeStandard } else { T::MakeAdmin };
            let toggle = Msg::AccountType(user.path.clone(), !user.administrator);
            badges = badges
                .push(widgets::secondary(t, c.t(kind), idle.then_some(toggle)))
                .push(widgets::secondary(t, c.t(T::SetPassword), idle.then(|| Msg::OpenPassword(user.path.clone()))))
                .push(widgets::danger(
                    t,
                    c.t(T::Remove),
                    idle.then(|| Msg::AskRemove(user.uid, user.display_name().to_string())),
                ));
            group = group.push(
                Row::new(t, user.display_name().to_string())
                    .subtitle(user.user_name.clone())
                    .prefix(portrait(t, user, 32.0))
                    .suffix(badges)
                    .stacked(t.compact)
                    .build(),
            );
            if let Some(form) = self.password_form(c, &user.path) {
                group = group.push(form);
            }
            if let Some(Form::Remove { uid, name }) = self.form.as_ref()
                && *uid == user.uid
            {
                group = group.push(remove_form(c, name, idle));
            }
        }
        match self.add_user_form(c) {
            Some(form) => group = group.push(form),
            None => {
                group = group.push(
                    container(widgets::secondary(t, c.t(T::AddUser), idle.then_some(Msg::OpenAddUser)))
                        .padding([8.0, theme::ROW_PADDING_X]),
                );
            }
        }
        group.build()
    }
}

fn form_buttons<'a>(c: &Ctx, confirm: Element<'a, Msg>) -> Element<'a, Msg> {
    container(
        row![
            iced::widget::space().width(Fill),
            widgets::secondary(c.theme, c.t(T::Cancel), Some(Msg::CloseForm)),
            confirm
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([4.0, theme::ROW_PADDING_X])
    .into()
}

fn remove_form<'a>(c: &Ctx, name: &str, idle: bool) -> Element<'a, Msg> {
    let t = c.theme;
    column![
        container(text(c.t(T::RemoveAsk).replace("{}", name), TextStyle::CALL).color(t.fg_2()))
            .padding([4.0, theme::ROW_PADDING_X]),
        container(
            row![
                iced::widget::space().width(Fill),
                widgets::secondary(t, c.t(T::Cancel), Some(Msg::CloseForm)),
                widgets::secondary(t, c.t(T::KeepFiles), idle.then_some(Msg::Remove(false))),
                widgets::danger(t, c.t(T::DeleteFiles), idle.then_some(Msg::Remove(true))),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([4.0, theme::ROW_PADDING_X]),
    ]
    .into()
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
    if let Some(started) = session.started {
        place.push(ago(c, started));
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
pub fn portrait<'a, M: 'a>(t: ThemeState, account: &Account, size: f32) -> Element<'a, M> {
    match account.icon() {
        Some(path) => glide_sdk::widget::photo(&path, size, size / 2.0),
        None => widgets::avatar(t, account.display_name(), size),
    }
}

#[must_use]
pub fn face_button<'a>(t: ThemeState, face: &Path, selected: bool, on_press: Option<Msg>) -> Element<'a, Msg> {
    glide_sdk::widget::photo_button(&widgets::paint(&t), face, 48.0, selected, on_press)
}

fn hero<'a>(c: &Ctx, me: &'a Account) -> Element<'a, Msg> {
    let t = c.theme;
    let (tone, kind) = if me.administrator { (Tone::Accent, T::Administrator) } else { (Tone::Neutral, T::Standard) };
    let mut badges = row![widgets::pill(t, tone, c.t(kind))].spacing(6);
    if me.automatic_login {
        badges = badges.push(widgets::pill(t, Tone::Warn, c.t(T::AutoLogin)));
    }
    let body = row![
        portrait(t, me, 72.0),
        column![
            text(me.display_name().to_string(), TextStyle::T2).color(t.fg_1()),
            text(me.user_name.clone(), TextStyle::CALL).color(t.fg_2()),
            badges,
        ]
        .spacing(4)
        .width(Fill),
    ]
    .spacing(18)
    .align_y(Alignment::Center);
    glide_sdk::widget::tinted_card(&widgets::paint(&t), super::Panel::Users.color(), 20, body)
}
