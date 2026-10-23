use iced::widget::{column, container, row};
use iced::{Alignment, Element, Fill, Task};

use super::common::error_banner;
use super::{Ctx, Pane};
use crate::icons::Icon;
use crate::services::accounts::{self, PasswordProblem};
use crate::services::config::ConfigStore;
use crate::services::keychain::{self, Draft, DraftProblem, KeychainState, Login};
use crate::services::run::blocking;
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row, form_row, note};

mod tr {
    crate::i18n::catalogue! {
        Hero => "Passwords", "Паролі";
        HeroOpen => "Saved passwords fill in when you focus a password field", "Збережені паролі вставляються, коли ви переходите в поле пароля";
        HeroLocked => "The keychain is locked", "Зв'язка ключів заблокована";
        HeroNew => "Keep app passwords in an encrypted keychain", "Зберігайте паролі програм у зашифрованій зв'язці ключів";
        Open => "Unlocked", "Відкрита";
        Closed => "Locked", "Заблокована";
        Unavailable => "The keychain is not running", "Зв'язка ключів не запущена";
        UnavailableHint => "It runs inside a Glide session; sign in to Glide to manage passwords", "Вона працює в сеансі Glide; увійдіть у Glide, щоб керувати паролями";
        Create => "Create the keychain", "Створити зв'язку ключів";
        CreateHint => "Everything you save is encrypted with this password. If you forget it, the saved passwords cannot be recovered.", "Усе збережене шифрується цим паролем. Якщо ви його забудете, збережені паролі не відновити.";
        KeychainPassword => "Keychain password", "Пароль зв'язки ключів";
        Repeat => "Repeat it", "Повторіть його";
        CreateButton => "Create", "Створити";
        Empty => "Enter a password", "Введіть пароль";
        Mismatch => "The passwords differ", "Паролі не збігаються";
        LineBreak => "No line breaks", "Без переносів рядка";
        Locked => "Unlock to see your passwords", "Розблокуйте, щоб побачити паролі";
        LockedHint => "Glide asks for the keychain password in its own prompt, not in this window", "Glide попросить пароль зв'язки ключів у власному запиті, а не в цьому вікні";
        Unlock => "Unlock", "Розблокувати";
        Lock => "Lock", "Заблокувати";
        Saved => "Saved passwords", "Збережені паролі";
        SavedHint => "Matched to an app by its app ID or by its program", "Прив'язані до програми за її ID або за файлом програми";
        Search => "Search passwords", "Шукати паролі";
        Add => "Add…", "Додати…";
        NoneSaved => "No passwords saved yet", "Ще немає збережених паролів";
        NoneFound => "Nothing matches the search", "Нічого не знайдено";
        Edit => "Edit…", "Змінити…";
        Delete => "Delete", "Видалити";
        DeleteAsk => "Delete “{}”? This cannot be undone.", "Видалити «{}»? Це не можна скасувати.";
        Name => "Name", "Назва";
        Username => "User name", "Ім'я користувача";
        Password => "Password", "Пароль";
        KeepPassword => "Leave empty to keep the saved one", "Залиште порожнім, щоб не змінювати";
        Show => "Show", "Показати";
        App => "App ID", "ID програми";
        AppHint => "As the app's window reports it", "Як його повідомляє вікно програми";
        Program => "Program", "Програма";
        ProgramHint => "Or the path of the program file", "Або шлях до файлу програми";
        Keychain => "Keychain", "Зв'язка ключів";
        NoLabel => "Give it a name", "Дайте назву";
        NoTarget => "Fill in an app ID or a program", "Вкажіть ID програми або програму";
        Save => "Save", "Зберегти";
        Cancel => "Cancel", "Скасувати";
    }
}

use tr::Key as T;

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Box<KeychainState>),
    NewPassword(String),
    ConfirmPassword(String),
    Create,
    Unlock,
    Lock,
    Search(String),
    Add,
    Edit(u64),
    Label(String),
    Username(String),
    Password(String),
    App(String),
    Program(String),
    Reveal,
    Revealed(Option<Result<String, String>>),
    Save,
    Cancel,
    AskDelete(u64),
    Delete(u64),
    Done(Option<Result<String, String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    state: KeychainState,
    loading: bool,
    new_password: String,
    confirm: String,
    search: String,
    draft: Option<Draft>,
    revealed: bool,
    deleting: Option<u64>,
    busy: bool,
    error: Option<String>,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["passwords", "keychain", "logins", "autofill", "паролі", "зв'язка ключів", "автозаповнення", "логіни"];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.reload()
    }

    fn leave(&mut self) {
        self.new_password.clear();
        self.confirm.clear();
        self.draft = None;
        self.revealed = false;
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(state) => {
                self.state = *state;
                self.loading = false;
                if !matches!(self.state, KeychainState::Open(_)) {
                    self.draft = None;
                }
            }
            Msg::NewPassword(value) => self.new_password = value,
            Msg::ConfirmPassword(value) => self.confirm = value,
            Msg::Create if accounts::password_problem(&self.new_password, &self.confirm).is_none() => {
                let password = std::mem::take(&mut self.new_password);
                self.confirm.clear();
                return self.run(move || keychain::create(&password));
            }
            Msg::Unlock => return self.run(keychain::request_unlock),
            Msg::Lock => return self.run(keychain::lock),
            Msg::Search(value) => self.search = value,
            Msg::Add => {
                self.revealed = false;
                self.draft = Some(Draft::default());
            }
            Msg::Edit(id) => {
                self.revealed = false;
                self.draft = self.state.logins().iter().find(|login| login.id == id).map(|login| Draft {
                    id: Some(login.id),
                    label: login.label.clone(),
                    username: login.username.clone(),
                    password: String::new(),
                    app: login.app.clone(),
                    program: login.program.clone(),
                });
            }
            Msg::Label(value) => self.edit_draft(|draft| draft.label = value),
            Msg::Username(value) => self.edit_draft(|draft| draft.username = value),
            Msg::Password(value) => self.edit_draft(|draft| draft.password = value),
            Msg::App(value) => self.edit_draft(|draft| draft.app = value),
            Msg::Program(value) => self.edit_draft(|draft| draft.program = value),
            Msg::Reveal => {
                if let Some(id) = self.draft.as_ref().and_then(|draft| draft.id) {
                    return Task::perform(blocking(move || keychain::password(id)), Msg::Revealed);
                }
                self.revealed = true;
            }
            Msg::Revealed(Some(Ok(password))) => {
                self.revealed = true;
                self.edit_draft(|draft| draft.password = password);
            }
            Msg::Revealed(Some(Err(err))) => self.error = Some(err),
            Msg::Save => {
                if let Some(draft) = self.draft.take().filter(|draft| draft.problem().is_none()) {
                    self.revealed = false;
                    return self.run(move || keychain::save(&draft));
                }
            }
            Msg::Cancel => {
                self.draft = None;
                self.revealed = false;
                self.deleting = None;
            }
            Msg::AskDelete(id) => self.deleting = Some(id),
            Msg::Delete(id) => {
                self.deleting = None;
                return self.run(move || keychain::delete(id));
            }
            Msg::Done(result) => {
                self.busy = false;
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
                return self.reload();
            }
            Msg::Create | Msg::Revealed(None) => {}
        }
        Task::none()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.reload()
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let (subtitle, pill) = match &self.state {
            KeychainState::Open(_) => (T::HeroOpen, Some(widgets::status_pill(t, Tone::Ok, c.t(T::Open)))),
            KeychainState::Locked => (T::HeroLocked, Some(widgets::status_pill(t, Tone::Neutral, c.t(T::Closed)))),
            _ => (T::HeroNew, None),
        };
        let body: Element<'a, Msg> = match &self.state {
            KeychainState::Loading => column![].into(),
            KeychainState::Unreachable => {
                Group::new(t).title(c.t(T::Unavailable)).push(note(t, Icon::Info, c.t(T::UnavailableHint))).build()
            }
            KeychainState::Missing => self.create_form(&c),
            KeychainState::Locked => locked(&c, self.busy),
            KeychainState::Open(logins) => self.logins(&c, logins),
        };
        column![
            widgets::section_hero(t, Icon::Key, super::Panel::Passwords.color(), c.t(T::Hero), c.t(subtitle), pill),
            error_banner(t, self.error.as_deref()),
            body,
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

fn password_hint(problem: PasswordProblem) -> T {
    match problem {
        PasswordProblem::Empty => T::Empty,
        PasswordProblem::Mismatch => T::Mismatch,
        PasswordProblem::LineBreak => T::LineBreak,
    }
}

const fn draft_hint(problem: DraftProblem) -> T {
    match problem {
        DraftProblem::NoLabel => T::NoLabel,
        DraftProblem::NoTarget => T::NoTarget,
        DraftProblem::NoPassword => T::Empty,
        DraftProblem::LineBreak => T::LineBreak,
    }
}

fn locked<'a>(c: &Ctx, busy: bool) -> Element<'a, Msg> {
    let t = c.theme;
    Group::new(t)
        .push(
            Row::new(t, c.t(T::Locked))
                .prefix(widgets::icon(Icon::Lock, 18.0, t.fg_2()))
                .suffix(widgets::primary(t, c.t(T::Unlock), (!busy).then_some(Msg::Unlock)))
                .build(),
        )
        .push(note(t, Icon::Info, c.t(T::LockedHint)))
        .build()
}

fn buttons<'a>(c: &Ctx, confirm: Element<'a, Msg>) -> Element<'a, Msg> {
    button_row(vec![widgets::secondary(c.theme, c.t(T::Cancel), Some(Msg::Cancel)), confirm])
}

fn button_row(buttons: Vec<Element<'_, Msg>>) -> Element<'_, Msg> {
    let mut line = row![iced::widget::space().width(Fill)].spacing(8).align_y(Alignment::Center);
    for button in buttons {
        line = line.push(button);
    }
    container(line).padding([4.0, theme::ROW_PADDING_X]).into()
}

impl Page {
    fn reload(&mut self) -> Task<Msg> {
        if self.loading {
            return Task::none();
        }
        self.loading = true;
        Task::perform(blocking(keychain::read), |state| Msg::Loaded(Box::new(state.unwrap_or_default())))
    }

    fn run(&mut self, job: impl FnOnce() -> Result<String, String> + Send + 'static) -> Task<Msg> {
        self.error = None;
        self.busy = true;
        Task::perform(blocking(job), Msg::Done)
    }

    fn edit_draft(&mut self, change: impl FnOnce(&mut Draft)) {
        if let Some(draft) = self.draft.as_mut() {
            change(draft);
        }
    }

    fn create_form(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let problem = accounts::password_problem(&self.new_password, &self.confirm);
        let create = (problem.is_none() && !self.busy).then_some(Msg::Create);
        let hint = problem.filter(|_| !self.confirm.is_empty()).map(|problem| c.t(password_hint(problem)));
        Group::new(t)
            .title(c.t(T::Create))
            .description(c.t(T::CreateHint))
            .push(form_row(
                t,
                c.t(T::KeychainPassword),
                None,
                widgets::field(t, "", &self.new_password, Msg::NewPassword, None, false).secure(true).into(),
            ))
            .push(form_row(
                t,
                c.t(T::Repeat),
                hint,
                widgets::field(t, "", &self.confirm, Msg::ConfirmPassword, create.clone(), hint.is_some())
                    .secure(true)
                    .into(),
            ))
            .push(button_row(vec![widgets::primary(t, c.t(T::CreateButton), create)]))
            .build()
    }

    fn logins<'a>(&'a self, c: &Ctx, logins: &'a [Login]) -> Element<'a, Msg> {
        let t = c.theme;
        let mut group = Group::new(t)
            .title(c.t(T::Saved))
            .description(c.t(T::SavedHint))
            .push(
                Row::new(t, c.t(T::Keychain))
                    .prefix(widgets::icon(Icon::Key, 18.0, t.fg_2()))
                    .suffix(
                        row![
                            widgets::secondary(t, c.t(T::Lock), (!self.busy).then_some(Msg::Lock)),
                            widgets::primary(t, c.t(T::Add), self.draft.is_none().then_some(Msg::Add)),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .build(),
            )
            .push(
                container(widgets::field(t, c.t(T::Search), &self.search, Msg::Search, None, false))
                    .padding([4.0, theme::ROW_PADDING_X]),
            );
        if let Some(form) = self.draft.as_ref().filter(|draft| draft.id.is_none()).map(|draft| self.editor(c, draft)) {
            group = group.push(form);
        }
        let shown: Vec<&Login> = logins.iter().filter(|login| login.matches(&self.search)).collect();
        if shown.is_empty() {
            let empty = if logins.is_empty() { T::NoneSaved } else { T::NoneFound };
            group = group.push(note(t, Icon::Info, c.t(empty)));
        }
        for login in shown {
            group = group.push(self.login_row(c, login));
            if let Some(draft) = self.draft.as_ref().filter(|draft| draft.id == Some(login.id)) {
                group = group.push(self.editor(c, draft));
            }
            if self.deleting == Some(login.id) {
                let question = c.t(T::DeleteAsk).replace("{}", &login.label);
                group = group.push(column![
                    container(note(t, Icon::Warning, &question)).padding([4.0, 0.0]),
                    buttons(c, widgets::danger(t, c.t(T::Delete), Some(Msg::Delete(login.id)))),
                ]);
            }
        }
        group.build()
    }

    fn login_row(&self, c: &Ctx, login: &Login) -> Element<'_, Msg> {
        let t = c.theme;
        let target = if login.app.is_empty() { &login.program } else { &login.app };
        let subtitle = [login.username.as_str(), target.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let idle = self.draft.is_none() && self.deleting.is_none() && !self.busy;
        Row::new(t, &login.label)
            .subtitle(subtitle)
            .prefix(widgets::icon(Icon::Key, 18.0, t.fg_3()))
            .suffix(
                row![
                    widgets::secondary(t, c.t(T::Edit), idle.then_some(Msg::Edit(login.id))),
                    widgets::secondary(t, c.t(T::Delete), idle.then_some(Msg::AskDelete(login.id))),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .build()
    }

    fn editor<'a>(&'a self, c: &Ctx, draft: &'a Draft) -> Element<'a, Msg> {
        let t = c.theme;
        let problem = draft.problem();
        let save = (problem.is_none() && !self.busy).then_some(Msg::Save);
        let hint = problem.map(|problem| c.t(draft_hint(problem)));
        let label_hint = hint.filter(|_| problem == Some(DraftProblem::NoLabel) && !draft.label.is_empty());
        let target_hint = hint.filter(|_| problem == Some(DraftProblem::NoTarget));
        let password_hint = match (draft.id, self.revealed) {
            (Some(_), false) => Some(c.t(T::KeepPassword)),
            _ => hint.filter(|_| matches!(problem, Some(DraftProblem::LineBreak))),
        };
        let password = row![
            widgets::field(t, "", &draft.password, Msg::Password, save.clone(), false).secure(!self.revealed),
            widgets::secondary(t, c.t(T::Show), (!self.revealed).then_some(Msg::Reveal)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        column![
            form_row(
                t,
                c.t(T::Name),
                label_hint,
                widgets::field(t, "", &draft.label, Msg::Label, None, label_hint.is_some()).into()
            ),
            form_row(
                t,
                c.t(T::Username),
                None,
                widgets::field(t, "", &draft.username, Msg::Username, None, false).into()
            ),
            form_row(t, c.t(T::Password), password_hint, password.into()),
            form_row(
                t,
                c.t(T::App),
                Some(target_hint.unwrap_or(c.t(T::AppHint))),
                widgets::field(t, "org.telegram.desktop", &draft.app, Msg::App, None, target_hint.is_some()).into()
            ),
            form_row(
                t,
                c.t(T::Program),
                Some(c.t(T::ProgramHint)),
                widgets::field(t, "/usr/bin/telegram-desktop", &draft.program, Msg::Program, save.clone(), false)
                    .into()
            ),
            buttons(c, widgets::primary(t, c.t(T::Save), save)),
        ]
        .spacing(4)
        .into()
    }
}
