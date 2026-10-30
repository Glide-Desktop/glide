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
    Field(usize),
    CardNumber,
    CardCode,
}

impl Copied {
    const fn is_secret(self) -> bool {
        !matches!(self, Self::Username | Self::Link)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Placed,
    Refused,
    Absent,
}

#[must_use]
pub fn placement(result: &zbus::Result<()>) -> Placement {
    match result {
        Ok(()) => Placement::Placed,
        Err(zbus::Error::MethodError(name, ..)) if name.as_str() == ACCESS_DENIED => Placement::Refused,
        Err(zbus::Error::FDO(inner)) if matches!(**inner, zbus::fdo::Error::AccessDenied(_)) => Placement::Refused,
        Err(_) => Placement::Absent,
    }
}

const ACCESS_DENIED: &str = "org.freedesktop.DBus.Error.AccessDenied";

#[must_use]
pub fn seconds_until(expires: u64, now: u64, period: u32) -> u32 {
    u32::try_from(expires.saturating_sub(now)).unwrap_or(u32::MAX).min(period)
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
    ShellCopied(u64, Placement),
    ClipboardRead(u64, Option<String>),
    Tick,
    CodeLoaded(ItemRef, Result<Response, String>),
    CodeForCopy(ItemRef, Result<Response, String>),
    Sync,
    LockAll,
    RequestUnlock,
    New(ItemKind),
    Edit,
    Editor(editor::Event),
    EditorLoaded(Box<Editor>),
    EditorUnavailable,
    Saved(Result<Response, String>),
    Trash,
    MoreMenu,
    Restore,
    AskPurge,
    Purge(ItemRef),
    Done(Result<Response, String>),
    Generator(generator::Event),
    Sends(sends::Event),
    Wizard(wizard::Event),
    Transfer(crate::transfer::Event),
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
    Frame,
    Step(isize),
    FocusSearch,
    ShowShortcuts,
    Quit,
    Reload,
}

const RETRY: Duration = Duration::from_secs(3);
const PAGE: isize = 8;

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
    pub code_expires: u64,
    pub editor: Option<Editor>,
    pub generator: Option<Generating>,
    pub generated: Vec<zeroize::Zeroizing<String>>,
    pub send: SendForm,
    pub wizard: Option<Wizard>,
    pub create: Option<wizard::Create>,
    pub settings: bool,
    pub transfer: Option<crate::transfer::Transfer>,
    pub password_change: Option<crate::settings::PasswordChange>,
    pub edit_when_loaded: bool,
    pub generate_on_edit: bool,
    pub conflict_pick: usize,
    pub conflict_open: bool,
    pub check_filter: usize,
    pub clipboard: u64,
    pub copied: Option<Hidden>,
    pub now: u64,
    pub modal: glide_sdk::anim::Fade,
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

pub(crate) fn open_detached(link: String) {
    let _ = std::thread::Builder::new().name("glide-passwords-open".into()).spawn(move || {
        let _ = glide_sdk::system::open(&link, crate::APP_ID);
    });
}

fn copy_here(text: String, token: u64, clears: u32) -> Task<Message> {
    let write = iced::clipboard::write(text);
    if clears == 0 {
        return write;
    }
    let wait =
        Task::perform(async move { tokio::time::sleep(Duration::from_secs(u64::from(clears))).await }, move |()| {
            Message::ClipboardCheck(token)
        });
    Task::batch([write, wait])
}

pub const ITEM_ARGUMENT: &str = "--item";

#[must_use]
pub fn item_argument(mut args: impl Iterator<Item = String>) -> Option<ItemRef> {
    let value = loop {
        let arg = args.next()?;
        if arg == ITEM_ARGUMENT {
            break args.next()?;
        }
        if let Some(value) = arg.strip_prefix("--item=") {
            break value.to_string();
        }
    };
    let (vault, id) = value.split_once('/')?;
    (!vault.is_empty() && !id.is_empty()).then(|| ItemRef::new(vault, id))
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
            selected: item_argument(std::env::args().skip(1)),
            detail: None,
            revealed: None,
            code: None,
            code_expires: 0,
            editor: None,
            generator: None,
            generated: Vec::new(),
            send: SendForm::default(),
            wizard: None,
            create: None,
            settings: false,
            transfer: None,
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
            modal: glide_sdk::anim::Fade::asymmetric(
                false,
                glide_sdk::anim::Speed::EXPAND,
                glide_sdk::anim::Speed::FADE,
            ),
        }
    }

    pub fn t(&self, key: Key) -> &'static str {
        key.text(self.lang)
    }

    #[must_use]
    pub fn modal_shown(&self) -> bool {
        self.settings
            || self.wizard.is_some()
            || self.create.is_some()
            || self.transfer.is_some()
            || self.password_change.is_some()
            || self.conflict_open
    }

    #[must_use]
    pub fn modal_progress(&self) -> f32 {
        self.modal.progress(std::time::Instant::now())
    }

    fn step_selection(&mut self, step: isize) -> Task<Message> {
        let visible = crate::model::visible(
            &self.data.items,
            self.vault.as_deref(),
            self.section,
            &self.query,
            self.prefs.sort_by_name,
        );
        let Some(last) = visible.len().checked_sub(1) else {
            return Task::none();
        };
        let current = self.selected.as_ref().and_then(|item| visible.iter().position(|summary| &summary.item == item));
        let next = match current {
            Some(index) => index.saturating_add_signed(step).min(last),
            None if step < 0 => last,
            None => 0,
        };
        let item = visible[next].item.clone();
        if current == Some(next) {
            return Task::none();
        }
        self.select(item)
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

    fn set_code(&mut self, code: Code) {
        self.code_expires = unix_now() + u64::from(code.seconds_left);
        self.code = Some(code);
    }

    fn code_left(&self) -> u32 {
        self.code.as_ref().map_or(0, |code| seconds_until(self.code_expires, unix_now(), code.period))
    }

    fn code_for(item: ItemRef, done: fn(ItemRef, Result<Response, String>) -> Message) -> Task<Message> {
        ask(Request::Totp { item: item.clone() }, move |result| done(item.clone(), result))
    }

    fn select(&mut self, item: ItemRef) -> Task<Message> {
        self.selected = Some(item.clone());
        self.revealed = None;
        self.code = None;
        self.detail = self.detail.take().filter(|detail| detail.summary.item == item);
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

    pub(crate) fn copy(&mut self, what: Copied, secret: Hidden, shell: &mut Shell<Message>) -> Task<Message> {
        self.clipboard += 1;
        let token = self.clipboard;
        let text = secret.expose().to_string();
        self.copied = Some(secret);
        let label = match what {
            Copied::Username => Key::CopiedUsername,
            Copied::Password => Key::CopiedPassword,
            Copied::Code => Key::CopiedCode,
            Copied::Link => Key::CopiedLink,
            Copied::Field(_) | Copied::CardNumber | Copied::CardCode => Key::CopiedSecret,
        };
        let clears = self.prefs.clear_after;
        let note = if clears > 0 && what != Copied::Link {
            format!("{} · {}", self.t(label), self.t(Key::ClearsIn).replace("{n}", &clears.to_string()))
        } else {
            self.t(label).to_string()
        };
        shell.toast_with(note, self.t(Key::Okay), Message::Ignored);
        if what.is_secret() {
            let secret = zeroize::Zeroizing::new(text);
            return Task::perform(
                async move { placement(&glide_sdk::system::copy_secret(&secret, clears)) },
                move |placed| Message::ShellCopied(token, placed),
            );
        }
        copy_here(text, token, if what == Copied::Link { 0 } else { clears })
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
                let read = |secret: SecretKind| {
                    let item = item.clone();
                    async move {
                        match client::call(Request::Secret { item, secret }).await {
                            Ok(Response::Secret { secret }) => Some(secret),
                            _ => None,
                        }
                    }
                };
                let password = read(SecretKind::Password).await?;
                let totp = read(SecretKind::Totp).await?;
                let mut hidden = Vec::new();
                for (index, field) in detail.fields.iter().enumerate() {
                    if field.hidden {
                        hidden.push((index, read(SecretKind::FieldAt(index)).await?));
                    }
                }
                Some(Box::new(Editor::from_detail(&detail, &password, &totp, &hidden)))
            },
            |editor| editor.map_or(Message::EditorUnavailable, Message::EditorLoaded),
        )
    }

    pub(crate) fn change_prefs(&mut self, change: impl Fn(&mut Prefs)) {
        match Prefs::update(&change) {
            Ok(fresh) => self.prefs = fresh,
            Err(_) => change(&mut self.prefs),
        }
    }

    pub fn check_leaks(&self) -> Task<Message> {
        if self.prefs.leak_check { ask(Request::CheckLeaks, Message::Done) } else { Task::none() }
    }

    pub(crate) fn reply_notice(&self, result: &Result<Response, String>) -> Option<String> {
        match result {
            Ok(Response::WrongPassword) => Some(self.t(Key::WrongPassword).to_string()),
            Ok(Response::Unsupported) => Some(self.t(Key::NotAvailable).to_string()),
            Ok(Response::Error { message }) => Some(humane(self.lang, message)),
            Ok(Response::Locked) => Some(self.t(Key::VaultIsLocked).to_string()),
            Ok(_) => None,
            Err(err) => Some(format!("{}: {err}", self.t(Key::DaemonMissing))),
        }
    }

    fn after(&self, result: &Result<Response, String>, shell: &mut Shell<Message>) {
        if let Some(text) = self.reply_notice(result) {
            shell.toast(text);
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
                    return self.check_leaks();
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
                let by_name = !self.prefs.sort_by_name;
                self.change_prefs(move |prefs| prefs.sort_by_name = by_name);
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
                    self.set_code(code);
                }
                Task::none()
            }
            Message::CodeForCopy(item, Ok(Response::Code { code })) => {
                let text = Hidden::new(code.code.clone());
                if self.selected.as_ref() == Some(&item) {
                    self.set_code(code);
                }
                self.copy(Copied::Code, text, shell)
            }
            Message::Ignored | Message::Frame => Task::none(),
            Message::Step(step) => self.step_selection(step),
            Message::FocusSearch => iced::widget::operation::focus(crate::view::SEARCH_ID),
            Message::Copy(what) => {
                let Some(item) = self.selected.clone() else {
                    return Task::none();
                };
                match what {
                    Copied::Username => {
                        let name = self
                            .detail
                            .as_ref()
                            .filter(|detail| detail.summary.item == item)
                            .map(|detail| detail.summary.username.clone())
                            .unwrap_or_default();
                        if name.is_empty() {
                            return Task::none();
                        }
                        self.copy(what, Hidden::new(name), shell)
                    }
                    Copied::Code => {
                        let code = self.code.as_ref().map(|code| code.code.clone()).unwrap_or_default();
                        if code.is_empty() {
                            return Task::none();
                        }
                        if self.code_left() == 0 {
                            return Self::code_for(item, Message::CodeForCopy);
                        }
                        self.copy(what, Hidden::new(code), shell)
                    }
                    Copied::Password | Copied::CardNumber | Copied::CardCode | Copied::Field(_) => {
                        let secret = match what {
                            Copied::CardNumber => SecretKind::CardNumber,
                            Copied::CardCode => SecretKind::CardCode,
                            Copied::Field(index) => {
                                let Some(field) = self.detail.as_ref().and_then(|detail| detail.fields.get(index))
                                else {
                                    return Task::none();
                                };
                                if !field.hidden {
                                    return self.copy(what, Hidden::new(field.value.clone()), shell);
                                }
                                SecretKind::FieldAt(index)
                            }
                            _ => SecretKind::Password,
                        };
                        ask(Request::Secret { item, secret }, move |result| Message::CopyText(what, result))
                    }
                    Copied::Link => Task::none(),
                }
            }
            Message::CopyText(what, Ok(Response::Secret { secret })) => self.copy(what, secret, shell),
            Message::CodeForCopy(_, result) | Message::CopyText(_, result) | Message::Done(result) => {
                self.after(&result, shell);
                Task::none()
            }
            Message::ShellCopied(token, placed) => {
                if token != self.clipboard {
                    return Task::none();
                }
                match placed {
                    Placement::Placed => Task::none(),
                    Placement::Absent => match &self.copied {
                        Some(secret) => copy_here(secret.expose().to_string(), token, self.prefs.clear_after),
                        None => Task::none(),
                    },
                    Placement::Refused => {
                        self.copied = None;
                        shell.toast(self.t(Key::CopyRefused));
                        Task::none()
                    }
                }
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
                let left = self.code_left();
                let Some(code) = self.code.as_mut() else {
                    return Task::none();
                };
                code.seconds_left = left;
                if left > 0 {
                    return Task::none();
                }
                let Some(item) = self.selected.clone() else {
                    return Task::none();
                };
                Self::code_for(item, Message::CodeLoaded)
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
                if editor.item.is_some() && editor.item != self.selected {
                    self.generate_on_edit = false;
                    return Task::none();
                }
                self.editor = Some(*editor);
                self.generator = std::mem::take(&mut self.generate_on_edit).then(Generating::fresh);
                Task::none()
            }
            Message::EditorUnavailable => {
                shell.toast(self.t(Key::EditorUnavailable));
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
                self.after(&result, shell);
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
                let Some(detail) =
                    self.detail.as_ref().filter(|detail| self.selected.as_ref() == Some(&detail.summary.item))
                else {
                    return Task::none();
                };
                let item = detail.summary.item.clone();
                shell.dialog(
                    glide_sdk::Dialog::new(
                        self.t(Key::DeleteForeverTitle).replace("{name}", &detail.summary.name),
                        self.t(Key::DeleteForeverBody),
                    )
                    .choice(self.t(Key::DeleteForever), Tone::Danger, Message::Purge(item))
                    .cancel(self.t(Key::Cancel)),
                );
                Task::none()
            }
            Message::Purge(item) => {
                if self.selected.as_ref() == Some(&item) {
                    self.selected = None;
                }
                ask(Request::Purge { item }, Message::Done)
            }
            Message::Generator(event) => generator::update(self, event, shell),
            Message::Sends(event) => sends::update(self, event, shell),
            Message::Wizard(event) => wizard::update(self, event, shell),
            Message::Transfer(event) => crate::transfer::update(self, event, shell),
            Message::OpenWizard => {
                self.settings = false;
                self.wizard = Some(Wizard::default());
                Task::none()
            }
            Message::CreateVault(event) => wizard::create(self, event, shell),
            Message::Settings(event) => settings::update(self, event, shell),
            Message::ShowShortcuts => {
                shell.dialog(
                    glide_sdk::Dialog::new(self.t(Key::Shortcuts), self.t(Key::ShortcutsBody))
                        .cancel(self.t(Key::Okay)),
                );
                Task::none()
            }
            Message::OpenSettings => {
                self.prefs = Prefs::load();
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
                    open_detached(with_scheme(&site));
                }
                self.edit_when_loaded = true;
                self.generate_on_edit = true;
                select
            }
            Message::OpenLink(link) => {
                open_detached(with_scheme(&link));
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
            if self.password_change.is_some() {
                return Task::done(Message::Settings(crate::settings::Event::CancelPassword));
            }
            if self.transfer.is_some() {
                return Task::done(Message::Transfer(crate::transfer::Event::Close));
            }
            if self.conflict_open {
                return Task::done(Message::CloseConflict);
            }
        }
        let browsing = !captured && self.editor.is_none() && !self.modal_shown() && self.generator.is_none();
        if modifiers.control() && !modifiers.alt() && letter(keyboard::key::Code::KeyF) && self.editor.is_none() {
            return Task::done(Message::FocusSearch);
        }
        if !browsing || modifiers.control() || modifiers.alt() || modifiers.logo() {
            return Task::none();
        }
        let keyboard::Key::Named(named) = key else {
            return Task::none();
        };
        match named {
            keyboard::key::Named::ArrowDown => Task::done(Message::Step(1)),
            keyboard::key::Named::ArrowUp => Task::done(Message::Step(-1)),
            keyboard::key::Named::PageDown => Task::done(Message::Step(PAGE)),
            keyboard::key::Named::PageUp => Task::done(Message::Step(-PAGE)),
            keyboard::key::Named::Home => Task::done(Message::Step(isize::MIN / 2)),
            keyboard::key::Named::End => Task::done(Message::Step(isize::MAX / 2)),
            keyboard::key::Named::Enter if self.selected.is_some() => Task::done(Message::Copy(Copied::Password)),
            keyboard::key::Named::Delete if self.selected.is_some() => Task::done(Message::Trash),
            _ => Task::none(),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![Subscription::run(client::changes).map(|()| Message::Changed)];
        if self.code.is_some() || self.section == Section::Sends {
            subscriptions.push(iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick));
        }
        if self.failure.is_some() {
            subscriptions.push(iced::time::every(RETRY).map(|_| Message::Reload));
        }
        if self.modal.is_animating(std::time::Instant::now()) {
            subscriptions.push(iced::window::frames().map(|_| Message::Frame));
        }
        Subscription::batch(subscriptions)
    }
}

#[must_use]
pub fn humane(lang: Lang, message: &str) -> String {
    match message.trim() {
        "no such item" => Key::ItemGone.text(lang).to_string(),
        "the vault is locked" => Key::VaultIsLocked.text(lang).to_string(),
        "" => Key::ActionFailed.text(lang).to_string(),
        detail => format!("{} ({detail})", Key::ActionFailed.text(lang)),
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
        let task = Self::update(self, message, shell);
        let shown = self.modal_shown();
        self.modal.set(shown, std::time::Instant::now());
        task
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
    fn the_item_argument_names_a_vault_and_an_id() {
        let args = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<_>>().into_iter();
        assert_eq!(item_argument(args(&["--item", "bw-1/5f3c"])), Some(ItemRef::new("bw-1", "5f3c")));
        assert_eq!(item_argument(args(&["--item=local/a"])), Some(ItemRef::new("local", "a")));
        assert_eq!(item_argument(args(&["--item", "nothing"])), None);
        assert_eq!(item_argument(args(&["--item"])), None);
        assert_eq!(item_argument(args(&[])), None);
    }

    #[test]
    fn delete_forever_purges_the_item_the_dialog_named_even_after_the_selection_moved() {
        use glide_sdk::app::Event;
        use glide_sdk::overlay::Answer;

        let mut harness =
            glide_sdk::Harness::<App>::new(crate::APP_ID, glide_sdk::Look::default(), iced::Size::new(1100.0, 720.0));
        let named = ItemRef::new(LOCAL, "named");
        let other = ItemRef::new(LOCAL, "other");
        let detail: Detail = serde_json::from_value(serde_json::json!({
            "summary": {
                "item": { "vault": LOCAL, "id": "named" }, "kind": "login", "name": "Named", "username": "",
                "folder": "", "sites": [], "apps": [], "has_password": false, "has_totp": false,
                "has_passkey": false, "favorite": false, "created": 0, "modified": 0, "deleted": true,
                "strength": 0, "reused": 0
            },
            "notes": "", "sites": [], "apps": [], "fields": [], "passkeys": [], "history": 0, "card": null,
            "password_length": 0
        }))
        .unwrap();

        harness.message(Message::Select(named.clone()));
        harness.message(Message::Detail(named, Ok(Response::Item { detail: Box::new(detail) })));
        harness.message(Message::AskPurge);
        assert!(harness.shell().has_dialog());

        harness.message(Message::Select(other.clone()));
        harness.send(Event::Dialog(Answer::Chose(0)));
        assert!(!harness.shell().has_dialog());
        assert_eq!(
            harness.app().selected,
            Some(other),
            "the purge was for the named item, so the moved selection stays"
        );
    }

    fn harness() -> glide_sdk::Harness<App> {
        glide_sdk::Harness::<App>::new(crate::APP_ID, glide_sdk::Look::default(), iced::Size::new(1100.0, 720.0))
    }

    fn with_code(seconds_left: u32) -> (glide_sdk::Harness<App>, ItemRef) {
        let mut harness = harness();
        let item = ItemRef::new(LOCAL, "mail");
        harness.message(Message::Select(item.clone()));
        let code = Code { code: "123456".into(), seconds_left, period: 30 };
        harness.message(Message::CodeLoaded(item.clone(), Ok(Response::Code { code })));
        (harness, item)
    }

    #[test]
    fn the_countdown_is_the_wall_clock_so_a_suspend_cannot_leave_it_behind() {
        assert_eq!(seconds_until(1_000 + 20, 1_000, 30), 20);
        assert_eq!(seconds_until(1_000 + 20, 1_000 + 19, 30), 1);
        assert_eq!(seconds_until(1_000 + 20, 1_000 + 20, 30), 0);
        assert_eq!(seconds_until(1_000 + 20, 1_000 + 600, 30), 0, "ten minutes asleep expire the code");
        assert_eq!(seconds_until(1_000 + 20, 400, 30), 30, "a clock set back never shows more than a period");
    }

    #[test]
    fn ticks_follow_the_clock_not_their_own_count() {
        let (mut harness, _) = with_code(25);
        for _ in 0..5 {
            harness.message(Message::Tick);
        }
        let left = harness.app().code.as_ref().map(|code| code.seconds_left).expect("a code");
        assert!(left >= 24, "five ticks inside one second took {} seconds off", 25 - left);
    }

    #[test]
    fn an_expired_code_is_fetched_again_instead_of_being_copied() {
        let (mut harness, _) = with_code(0);
        harness.message(Message::Copy(Copied::Code));
        assert!(harness.app().copied.is_none(), "the stale code must not reach the clipboard");

        let (mut harness, _) = with_code(25);
        harness.message(Message::Copy(Copied::Code));
        assert_eq!(harness.app().copied.as_ref().map(Hidden::expose), Some("123456"));
    }

    #[test]
    fn the_fresh_code_that_replaces_an_expired_one_is_what_gets_copied() {
        let (mut harness, item) = with_code(0);
        let fresh = Code { code: "654321".into(), seconds_left: 25, period: 30 };
        harness.message(Message::CodeForCopy(item, Ok(Response::Code { code: fresh })));
        assert_eq!(harness.app().copied.as_ref().map(Hidden::expose), Some("654321"));
        assert_eq!(harness.app().code.as_ref().map(|code| code.code.as_str()), Some("654321"));
        assert!(harness.app().code_left() > 0);
    }

    #[test]
    fn a_missing_shell_falls_back_to_the_clipboard_and_keeps_the_secret_to_clear_it_later() {
        let mut harness = harness();
        let secret = Hidden::new("hunter2".to_string());
        harness.message(Message::CopyText(Copied::Password, Ok(Response::Secret { secret })));
        let token = harness.app().clipboard;

        harness.message(Message::ShellCopied(token, Placement::Absent));
        assert_eq!(harness.app().copied.as_ref().map(Hidden::expose), Some("hunter2"));

        harness.message(Message::ShellCopied(token + 1, Placement::Refused));
        assert!(harness.app().copied.is_some(), "an answer for an older copy is ignored");

        harness.message(Message::ShellCopied(token, Placement::Refused));
        assert!(harness.app().copied.is_none(), "an explicit refusal drops the secret");
    }

    #[test]
    fn only_an_explicit_access_denied_counts_as_a_refusal() {
        assert_eq!(placement(&Ok(())), Placement::Placed);
        let denied: zbus::Error = zbus::fdo::Error::AccessDenied("not focused".into()).into();
        assert_eq!(placement(&Err(denied)), Placement::Refused);
        let missing: zbus::Error = zbus::fdo::Error::ServiceUnknown("org.glide.Shell".into()).into();
        assert_eq!(placement(&Err(missing)), Placement::Absent);
        assert_eq!(placement(&Err(zbus::Error::Failure("no session bus".into()))), Placement::Absent);
    }

    #[test]
    fn an_access_denied_reply_from_the_bus_counts_as_a_refusal() {
        let call = zbus::Message::method_call("/org/glide/Shell1", "CopySecret").unwrap().build(&()).unwrap();
        let reply = zbus::Message::error(&call.header(), ACCESS_DENIED).unwrap().build(&()).unwrap();
        let name = zbus::names::OwnedErrorName::try_from(ACCESS_DENIED).unwrap();
        assert_eq!(placement(&Err(zbus::Error::MethodError(name, None, reply))), Placement::Refused);
        let other = zbus::names::OwnedErrorName::try_from("org.freedesktop.DBus.Error.ServiceUnknown").unwrap();
        let reply = zbus::Message::error(&call.header(), "org.freedesktop.DBus.Error.ServiceUnknown")
            .unwrap()
            .build(&())
            .unwrap();
        assert_eq!(placement(&Err(zbus::Error::MethodError(other, None, reply))), Placement::Absent);
    }

    #[test]
    fn revealing_a_password_in_one_form_leaves_every_other_form_masked() {
        use crate::{sends, settings, transfer, wizard};

        let mut harness = harness();
        harness.message(Message::Settings(settings::Event::ChangePassword("bw".into())));
        harness.message(Message::OpenWizard);
        harness.message(Message::Wizard(wizard::Event::TogglePassword));
        assert!(harness.app().wizard.as_ref().is_some_and(|wizard| wizard.shown));
        assert!(
            harness.app().password_change.as_ref().is_some_and(|change| !change.shown),
            "the wizard's eye must not reveal the vault password form"
        );

        harness.message(Message::Wizard(wizard::Event::Close));
        harness.message(Message::OpenWizard);
        assert!(harness.app().wizard.as_ref().is_some_and(|wizard| !wizard.shown), "a new wizard starts masked");

        harness.message(Message::CreateVault(wizard::CreateEvent::Open));
        harness.message(Message::CreateVault(wizard::CreateEvent::Reveal));
        harness.message(Message::CreateVault(wizard::CreateEvent::Close));
        harness.message(Message::CreateVault(wizard::CreateEvent::Open));
        assert!(harness.app().create.as_ref().is_some_and(|form| !form.shown));

        harness.message(Message::Transfer(transfer::Event::Export));
        harness.message(Message::Transfer(transfer::Event::ToggleSecrets));
        assert!(matches!(harness.app().transfer, Some(crate::transfer::Transfer::Export { shown: true, .. })));
        harness.message(Message::Transfer(transfer::Event::Close));
        harness.message(Message::Transfer(transfer::Event::Export));
        assert!(matches!(harness.app().transfer, Some(crate::transfer::Transfer::Export { shown: false, .. })));
        assert!(harness.app().create.as_ref().is_some_and(|form| !form.shown));

        harness.message(Message::Sends(sends::Event::Reveal));
        assert!(harness.app().send.shown);
        harness.message(Message::Sends(sends::Event::New));
        assert!(!harness.app().send.shown);
    }

    #[test]
    fn sites_get_a_scheme_when_they_lack_one() {
        assert_eq!(with_scheme("github.com"), "https://github.com");
        assert_eq!(with_scheme("http://192.168.1.1"), "http://192.168.1.1");
    }
}
