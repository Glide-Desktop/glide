use std::time::Duration;

use glide_ipc::keychain::{
    Code, Conflict, Detail, Folder, Hidden, ItemKind, ItemRef, Keep, LOCAL, Report, Request, Response, SecretKind,
    Send, Summary, Vault, VaultKind,
};
use glide_sdk::window::{Action, Window};
use glide_sdk::{Lang, Paint, Shell, Tone};
use iced::{Subscription, Task, keyboard};

use crate::editor::Editor;
use crate::generator::{Recipe, System};
use crate::i18n::Key;
use crate::model::Section;
use crate::prefs::Prefs;
use crate::sends::SendForm;
use crate::wizard::Wizard;
use crate::{client, editor, generator, sends, settings, wizard};

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub vaults: Vec<Vault>,
    pub default: String,
    pub items: Vec<Summary>,
    pub folders: Vec<Folder>,
    pub report: Report,
    pub sends: Vec<Send>,
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copied {
    Username,
    Password,
    Code,
    Link,
}

#[derive(Debug, Clone)]
pub enum Message {
    Window(Action),
    Changed,
    Loaded(Result<Snapshot, String>),
    Vault(Option<String>),
    Section(Section),
    Query(String),
    ClearQuery,
    ToggleSort,
    Select(ItemRef),
    Detail(ItemRef, Result<Response, String>),
    Reveal,
    Revealed(ItemRef, Result<Response, String>),
    Copy(Copied),
    CopyText(Copied, Result<Response, String>),
    ClipboardCheck(u64),
    ClipboardRead(u64, Option<String>),
    Tick,
    CodeLoaded(ItemRef, Result<Response, String>),
    Sync,
    LockAll,
    RequestUnlock,
    New(ItemKind),
    Edit,
    Editor(editor::Event),
    EditorLoaded(Box<Editor>),
    Saved(Result<Response, String>),
    Trash,
    MoreMenu,
    Restore,
    AskPurge,
    Purge,
    Done(Result<Response, String>),
    Generator(generator::Event),
    Sends(sends::Event),
    Wizard(wizard::Event),
    OpenWizard,
    CreateVault(wizard::CreateEvent),
    Settings(settings::Event),
    OpenSettings,
    CloseSettings,
    Resolve(ItemRef, Keep),
    ConflictPick(usize),
    CloseConflict,
    CheckFilter(usize),
    CheckAgain,
    ChangePasswordOf(ItemRef),
    OpenLink(String),
    Ignored,
    Quit,
    Reload,
}

const RETRY: Duration = Duration::from_secs(3);

#[must_use]
pub fn daemon_absent(failure: &str) -> bool {
    failure.contains("ServiceUnknown") || failure.contains("NameHasNoOwner") || failure.contains("not activatable")
}

#[derive(Debug, Clone, Default)]
pub struct Generating {
    pub recipe: Recipe,
    pub value: String,
    pub history: bool,
}

impl Generating {
    pub fn fresh() -> Self {
        let recipe = Recipe::default();
        let value = recipe.generate(&mut System);
        Self { recipe, value, history: false }
    }

    pub fn again(&mut self) {
        self.value = self.recipe.generate(&mut System);
    }
}

#[allow(clippy::struct_excessive_bools)] // Independent pieces of window state, each driven by its own message
pub struct App {
    pub lang: Lang,
    pub paint: Paint,
    pub window: Window,
    pub prefs: Prefs,
    pub data: Snapshot,
    pub loaded: bool,
    pub failure: Option<String>,
    pub vault: Option<String>,
    pub section: Section,
    pub query: String,
    pub selected: Option<ItemRef>,
    pub detail: Option<Detail>,
    pub revealed: Option<Hidden>,
    pub code: Option<Code>,
    pub editor: Option<Editor>,
    pub generator: Option<Generating>,
    pub generated: Vec<zeroize::Zeroizing<String>>,
    pub send: SendForm,
    pub wizard: Option<Wizard>,
    pub create: Option<wizard::Create>,
    pub settings: bool,
    pub password_change: Option<crate::settings::PasswordChange>,
    pub edit_when_loaded: bool,
    pub generate_on_edit: bool,
    pub conflict_pick: usize,
    pub conflict_open: bool,
    pub check_filter: usize,
    pub clipboard: u64,
    pub copied: Option<Hidden>,
    pub now: u64,
}

#[must_use]
pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

pub async fn load() -> Result<Snapshot, String> {
    let mut snapshot = Snapshot::default();
    if let Response::Vaults { list, default } = client::call(Request::Vaults).await? {
        snapshot.vaults = list;
        snapshot.default = default;
    }
    if let Response::Items { list } = client::call(Request::Items).await? {
        snapshot.items = list;
    }
    if let Response::Folders { list } = client::call(Request::Folders).await? {
        snapshot.folders = list;
    }
    if let Response::Report { report } = client::call(Request::Report).await? {
        snapshot.report = report;
    }
    if let Response::Sends { list } = client::call(Request::Sends).await? {
        snapshot.sends = list;
    }
    if let Response::Conflicts { list } = client::call(Request::Conflicts).await? {
        snapshot.conflicts = list;
    }
    Ok(snapshot)
}

pub fn ask(
    request: Request, done: impl Fn(Result<Response, String>) -> Message + std::marker::Send + 'static,
) -> Task<Message> {
    Task::perform(client::call(request), done)
}

impl App {
    #[must_use]
    pub fn new(shell: &Shell<Message>) -> Self {
        let prefs = Prefs::load();
        Self {
            lang: shell.lang(),
            paint: *shell.paint(),
            window: *shell.window(),
            data: Snapshot::default(),
            loaded: false,
            failure: None,
            vault: None,
            section: Section::Logins,
            query: String::new(),
            selected: None,
            detail: None,
            revealed: None,
            code: None,
            editor: None,
            generator: None,
            generated: Vec::new(),
            send: SendForm::default(),
            wizard: None,
            create: None,
            settings: false,
            password_change: None,
            edit_when_loaded: false,
            generate_on_edit: false,
            conflict_pick: 1,
            conflict_open: false,
            check_filter: 1,
            clipboard: 0,
            copied: None,
            now: unix_now(),
            prefs,
        }
    }

    pub fn t(&self, key: Key) -> &'static str {
        key.text(self.lang)
    }

    #[must_use]
    pub fn any_vault(&self) -> bool {
        self.data.vaults.iter().any(|vault| vault.exists)
    }

    #[must_use]
    pub fn all_locked(&self) -> bool {
        self.any_vault() && !self.data.vaults.iter().any(|vault| vault.exists && vault.unlocked)
    }

    #[must_use]
    pub fn has_server(&self) -> bool {
        self.data.vaults.iter().any(|vault| vault.kind == VaultKind::Bitwarden && vault.unlocked)
    }

    #[must_use]
    pub fn summary(&self, item: &ItemRef) -> Option<&Summary> {
        self.data.items.iter().find(|summary| &summary.item == item)
    }

    #[must_use]
    pub fn vault_name<'a>(&'a self, id: &'a str) -> &'a str {
        crate::model::vault_name(&self.data.vaults, id)
    }

    #[must_use]
    pub fn default_vault(&self) -> String {
        let usable = |id: &str| self.data.vaults.iter().any(|vault| vault.id == id && vault.unlocked);
        if usable(&self.data.default) {
            return self.data.default.clone();
        }
        self.data.vaults.iter().find(|vault| vault.unlocked).map_or_else(|| LOCAL.to_string(), |vault| vault.id.clone())
    }

    pub fn reload(&self) -> Task<Message> {
        Task::perform(load(), Message::Loaded)
    }

    fn select(&mut self, item: ItemRef) -> Task<Message> {
        self.selected = Some(item.clone());
        self.revealed = None;
        self.code = None;
        let detail = ask(Request::Item { item: item.clone() }, {
            let item = item.clone();
            move |result| Message::Detail(item.clone(), result)
        });
        let code = if self.summary(&item).is_some_and(|summary| summary.has_totp) {
            ask(Request::Totp { item: item.clone() }, move |result| Message::CodeLoaded(item.clone(), result))
        } else {
            Task::none()
        };
        Task::batch([detail, code])
    }

    fn keep_selection(&mut self) -> Task<Message> {
        let visible = crate::model::visible(
            &self.data.items,
            self.vault.as_deref(),
            self.section,
            &self.query,
            self.prefs.sort_by_name,
        );
        let still = self.selected.as_ref().is_some_and(|item| visible.iter().any(|summary| &summary.item == item));
        if still {
            let item = self.selected.clone().expect("checked above");
            return self.select(item);
        }
        if let Some(first) = visible.first().map(|summary| summary.item.clone()) {
            return self.select(first);
        }
        self.selected = None;
        self.detail = None;
        self.code = None;
        Task::none()
    }

    fn copy(&mut self, what: Copied, secret: Hidden, shell: &mut Shell<Message>) -> Task<Message> {
        self.clipboard += 1;
        let token = self.clipboard;
        let text = secret.expose().to_string();
        self.copied = Some(secret);
        let label = match what {
            Copied::Username => Key::CopiedUsername,
            Copied::Password => Key::CopiedPassword,
            Copied::Code => Key::CopiedCode,
            Copied::Link => Key::CopiedLink,
        };
        let clears = self.prefs.clear_after;
        let note = if clears > 0 && what != Copied::Link {
            format!("{} · {}", self.t(label), self.t(Key::ClearsIn).replace("{n}", &clears.to_string()))
        } else {
            self.t(label).to_string()
        };
        shell.toast_with(note, self.t(Key::Okay), Message::Ignored);
        let write = iced::clipboard::write(text);
        if clears == 0 || what == Copied::Link {
            return write;
        }
        let wait = Task::perform(tokio::time::sleep(Duration::from_secs(u64::from(clears))), move |()| {
            Message::ClipboardCheck(token)
        });
        Task::batch([write, wait])
    }

    pub fn open_editor(&self, item: Option<&ItemRef>, kind: ItemKind) -> Task<Message> {
        let vault = self.default_vault();
        let Some(item) = item.cloned() else {
            let editor = Editor::blank(kind, vault);
            return Task::done(Message::EditorLoaded(Box::new(editor)));
        };
        let Some(detail) = self.detail.clone().filter(|detail| detail.summary.item == item) else {
            return Task::none();
        };
        Task::perform(
            async move {
                let password =
                    match client::call(Request::Secret { item: item.clone(), secret: SecretKind::Password }).await {
                        Ok(Response::Secret { secret }) => secret,
                        _ => Hidden::default(),
                    };
                let totp = match client::call(Request::Secret { item: item.clone(), secret: SecretKind::Totp }).await {
                    Ok(Response::Secret { secret }) => secret,
                    _ => Hidden::default(),
                };
                Box::new(Editor::from_detail(&detail, &password, &totp))
            },
            Message::EditorLoaded,
        )
    }

    fn after(&mut self, result: Result<Response, String>, shell: &mut Shell<Message>) {
        match result {
            Ok(Response::WrongPassword) => shell.toast(self.t(Key::WrongPassword)),
            Ok(Response::Unsupported) => shell.toast(self.t(Key::NotAvailable)),
            Ok(Response::Error { message }) => shell.toast(message),
            Ok(_) => {}
            Err(err) => shell.toast(format!("{}: {err}", self.t(Key::DaemonMissing))),
        }
    }

    #[allow(clippy::too_many_lines)] // One arm per message; each arm is a short hand-off
    pub fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::Window(action) => {
                shell.window_action(action);
                Task::none()
            }
            Message::Changed | Message::CheckAgain => self.reload(),
            Message::Loaded(Ok(snapshot)) => {
                let first = !self.loaded;
                self.data = snapshot;
                self.loaded = true;
                self.failure = None;
                self.now = unix_now();
                if self.vault.as_ref().is_some_and(|vault| !self.data.vaults.iter().any(|known| &known.id == vault)) {
                    self.vault = None;
                }
                if first && !self.data.conflicts.is_empty() {
                    self.conflict_open = true;
                }
                if self.data.conflicts.is_empty() {
                    self.conflict_open = false;
                }
                self.keep_selection()
            }
            Message::Loaded(Err(err)) => {
                self.failure = Some(err);
                self.loaded = true;
                Task::none()
            }
            Message::Reload => Task::perform(load(), Message::Loaded),
            Message::Vault(vault) => {
                self.vault = vault;
                self.keep_selection()
            }
            Message::Section(section) => {
                self.section = section;
                self.editor = None;
                if section == Section::Check {
                    return Task::none();
                }
                self.keep_selection()
            }
            Message::Query(query) => {
                self.query = query;
                self.keep_selection()
            }
            Message::ClearQuery => {
                self.query.clear();
                self.keep_selection()
            }
            Message::ToggleSort => {
                self.prefs.sort_by_name = !self.prefs.sort_by_name;
                self.prefs.save();
                Task::none()
            }
            Message::Select(item) => {
                self.editor = None;
                self.select(item)
            }
            Message::Detail(item, result) => {
                if self.selected.as_ref() == Some(&item)
                    && let Ok(Response::Item { detail }) = result
                {
                    self.detail = Some(*detail);
                    if std::mem::take(&mut self.edit_when_loaded) {
                        let kind = self.detail.as_ref().map_or(ItemKind::Login, |detail| detail.summary.kind);
                        return self.open_editor(Some(&item), kind);
                    }
                }
                Task::none()
            }
            Message::Reveal => {
                if self.revealed.take().is_some() {
                    return Task::none();
                }
                let Some(item) = self.selected.clone() else {
                    return Task::none();
                };
                ask(Request::Secret { item: item.clone(), secret: SecretKind::Password }, move |result| {
                    Message::Revealed(item.clone(), result)
                })
            }
            Message::Revealed(item, result) => {
                if self.selected.as_ref() == Some(&item)
                    && let Ok(Response::Secret { secret }) = result
                {
                    self.revealed = Some(secret);
                }
                Task::none()
            }
            Message::CodeLoaded(item, result) => {
                if self.selected.as_ref() == Some(&item)
                    && let Ok(Response::Code { code }) = result
                {
                    self.code = Some(code);
                }
                Task::none()
            }
            Message::Ignored => Task::none(),
            Message::Copy(what) => {
                let Some(item) = self.selected.clone() else {
                    return Task::none();
                };
                match what {
                    Copied::Username => {
                        let name =
                            self.detail.as_ref().map(|detail| detail.summary.username.clone()).unwrap_or_default();
                        self.copy(what, Hidden::new(name), shell)
                    }
                    Copied::Code => {
                        let code = self.code.as_ref().map(|code| code.code.clone()).unwrap_or_default();
                        self.copy(what, Hidden::new(code), shell)
                    }
                    Copied::Password => ask(Request::Secret { item, secret: SecretKind::Password }, move |result| {
                        Message::CopyText(what, result)
                    }),
                    Copied::Link => Task::none(),
                }
            }
            Message::CopyText(what, Ok(Response::Secret { secret })) => self.copy(what, secret, shell),
            Message::CopyText(_, result) | Message::Done(result) => {
                self.after(result, shell);
                Task::none()
            }
            Message::ClipboardCheck(token) => {
                if token != self.clipboard {
                    return Task::none();
                }
                iced::clipboard::read().map(move |text| Message::ClipboardRead(token, text))
            }
            Message::ClipboardRead(token, text) => {
                let ours = self.copied.as_ref().is_some_and(|copied| text.as_deref() == Some(copied.expose()));
                if token == self.clipboard && ours {
                    self.copied = None;
                    return iced::clipboard::write(String::new());
                }
                Task::none()
            }
            Message::Tick => {
                self.now = unix_now();
                let Some(code) = self.code.as_mut() else {
                    return Task::none();
                };
                if code.seconds_left > 1 {
                    code.seconds_left -= 1;
                    return Task::none();
                }
                let Some(item) = self.selected.clone() else {
                    return Task::none();
                };
                ask(Request::Totp { item: item.clone() }, move |result| Message::CodeLoaded(item.clone(), result))
            }
            Message::Sync => ask(Request::Sync { vault: None }, Message::Done),
            Message::LockAll => ask(Request::Lock { vault: None }, Message::Done),
            Message::RequestUnlock => ask(Request::RequestUnlock { vault: None }, Message::Done),
            Message::New(kind) => {
                if self.section == Section::Sends || self.section == Section::Check || self.section == Section::Trash {
                    self.section = Section::All;
                }
                self.open_editor(None, kind)
            }
            Message::Edit => {
                let item = self.selected.clone();
                let kind = self.detail.as_ref().map_or(ItemKind::Login, |detail| detail.summary.kind);
                self.open_editor(item.as_ref(), kind)
            }
            Message::EditorLoaded(editor) => {
                self.editor = Some(*editor);
                self.generator = std::mem::take(&mut self.generate_on_edit).then(Generating::fresh);
                Task::none()
            }
            Message::Editor(event) => editor::update(self, event, shell),
            Message::Saved(result) => {
                if let Ok(Response::Saved { item }) = result {
                    self.editor = None;
                    self.generator = None;
                    shell.toast(self.t(Key::Saved));
                    self.selected = Some(item);
                    return self.reload();
                }
                self.after(result, shell);
                Task::none()
            }
            Message::Trash => {
                let Some(item) = self.selected.clone() else {
                    return Task::none();
                };
                self.editor = None;
                shell.toast(self.t(Key::MovedToTrash));
                ask(Request::Trash { item }, Message::Done)
            }
            Message::MoreMenu => {
                let mut items = vec![
                    glide_sdk::Item::action(self.t(Key::CopyUsername), Message::Copy(Copied::Username)),
                    glide_sdk::Item::action(self.t(Key::CopyPassword), Message::Copy(Copied::Password)),
                ];
                if self.code.is_some() {
                    items.push(glide_sdk::Item::action(self.t(Key::CopyCode), Message::Copy(Copied::Code)));
                }
                items.push(glide_sdk::Item::separator());
                items.push(glide_sdk::Item::action(self.t(Key::ToTrash), Message::Trash));
                shell.context_menu(items);
                Task::none()
            }
            Message::Restore => match self.selected.clone() {
                Some(item) => ask(Request::Restore { item }, Message::Done),
                None => Task::none(),
            },
            Message::AskPurge => {
                let name = self.detail.as_ref().map(|detail| detail.summary.name.clone()).unwrap_or_default();
                shell.dialog(
                    glide_sdk::Dialog::new(
                        self.t(Key::DeleteForeverTitle).replace("{name}", &name),
                        self.t(Key::DeleteForeverBody),
                    )
                    .choice(self.t(Key::DeleteForever), Tone::Danger, Message::Purge)
                    .cancel(self.t(Key::Cancel)),
                );
                Task::none()
            }
            Message::Purge => match self.selected.clone() {
                Some(item) => {
                    self.selected = None;
                    ask(Request::Purge { item }, Message::Done)
                }
                None => Task::none(),
            },
            Message::Generator(event) => generator::update(self, event, shell),
            Message::Sends(event) => sends::update(self, event, shell),
            Message::Wizard(event) => wizard::update(self, event, shell),
            Message::OpenWizard => {
                self.settings = false;
                self.wizard = Some(Wizard::default());
                Task::none()
            }
            Message::CreateVault(event) => wizard::create(self, event, shell),
            Message::Settings(event) => settings::update(self, event, shell),
            Message::OpenSettings => {
                self.settings = true;
                Task::none()
            }
            Message::CloseSettings => {
                self.settings = false;
                Task::none()
            }
            Message::Resolve(item, keep) => {
                self.conflict_open = false;
                ask(Request::Resolve { item, keep }, Message::Done)
            }
            Message::ConflictPick(index) => {
                self.conflict_pick = index;
                Task::none()
            }
            Message::CloseConflict => {
                self.conflict_open = false;
                Task::none()
            }
            Message::CheckFilter(index) => {
                self.check_filter = index;
                Task::none()
            }
            Message::ChangePasswordOf(item) => {
                let site = self.summary(&item).and_then(|summary| summary.sites.first().cloned());
                self.section = Section::All;
                let select = self.select(item);
                if let Some(site) = site {
                    let _ = glide_sdk::system::open(&with_scheme(&site), crate::APP_ID);
                }
                self.edit_when_loaded = true;
                self.generate_on_edit = true;
                select
            }
            Message::OpenLink(link) => {
                let _ = glide_sdk::system::open(&with_scheme(&link), crate::APP_ID);
                Task::none()
            }
            Message::Quit => {
                shell.exit();
                Task::none()
            }
        }
    }

    pub fn key(&mut self, event: &keyboard::Event, captured: bool) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, modifiers, physical_key, .. } = event else {
            return Task::none();
        };
        let letter = |code: keyboard::key::Code| matches!(physical_key, keyboard::key::Physical::Code(pressed) if *pressed == code);
        if modifiers.control() && !modifiers.alt() && !modifiers.logo() {
            if letter(keyboard::key::Code::KeyS) && self.editor.is_some() {
                return Task::done(Message::Editor(editor::Event::Save));
            }
            if letter(keyboard::key::Code::KeyN) {
                return Task::done(Message::New(ItemKind::Login));
            }
            if letter(keyboard::key::Code::KeyL) && !modifiers.shift() {
                return Task::done(Message::LockAll);
            }
            if letter(keyboard::key::Code::KeyC) && !captured && self.editor.is_none() && modifiers.shift() {
                return Task::done(Message::Copy(Copied::Password));
            }
            if letter(keyboard::key::Code::KeyB) && !captured && self.editor.is_none() {
                return Task::done(Message::Copy(Copied::Username));
            }
            if letter(keyboard::key::Code::KeyE) && self.editor.is_none() && self.selected.is_some() {
                return Task::done(Message::Edit);
            }
            if letter(keyboard::key::Code::KeyQ) {
                return Task::done(Message::Quit);
            }
            if letter(keyboard::key::Code::Comma) {
                return Task::done(Message::OpenSettings);
            }
        }
        if *key == keyboard::Key::Named(keyboard::key::Named::Escape) {
            if self.generator.is_some() {
                return Task::done(Message::Generator(generator::Event::Close));
            }
            if self.editor.is_some() {
                return Task::done(Message::Editor(editor::Event::Cancel));
            }
            if self.settings {
                return Task::done(Message::CloseSettings);
            }
            if self.wizard.is_some() {
                return Task::done(Message::Wizard(wizard::Event::Close));
            }
            if self.create.is_some() {
                return Task::done(Message::CreateVault(wizard::CreateEvent::Close));
            }
        }
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![Subscription::run(client::changes).map(|()| Message::Changed)];
        if self.code.is_some() || self.section == Section::Sends {
            subscriptions.push(iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick));
        }
        if self.failure.is_some() {
            subscriptions.push(iced::time::every(RETRY).map(|_| Message::Reload));
        }
        Subscription::batch(subscriptions)
    }
}

#[must_use]
pub fn with_scheme(site: &str) -> String {
    if site.contains("://") { site.to_string() } else { format!("https://{site}") }
}

impl glide_sdk::App for App {
    type Message = Message;

    fn new(shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        let app = Self::new(shell);
        let task = app.reload();
        (app, task)
    }

    fn title(&self, _: &Shell<Message>) -> String {
        self.t(Key::AppName).to_string()
    }

    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        Self::update(self, message, shell)
    }

    fn view<'a>(&'a self, _: &'a Shell<Message>) -> iced::Element<'a, Message> {
        crate::view::view(self)
    }

    fn subscription(&self) -> Subscription<Message> {
        Self::subscription(self)
    }

    fn menus(&self, _: &Shell<Message>) -> Vec<glide_sdk::Menu<Message>> {
        crate::view::menus(self)
    }

    fn window_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.window = *shell.window();
        Task::none()
    }

    fn key(&mut self, event: &keyboard::Event, captured: bool, _: &mut Shell<Message>) -> Task<Message> {
        Self::key(self, event, captured)
    }

    fn paint(&self, _: &Shell<Message>) -> Paint {
        self.paint
    }

    fn look_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.lang = shell.lang();
        self.paint = *shell.paint();
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_get_a_scheme_when_they_lack_one() {
        assert_eq!(with_scheme("github.com"), "https://github.com");
        assert_eq!(with_scheme("http://192.168.1.1"), "http://192.168.1.1");
    }
}
