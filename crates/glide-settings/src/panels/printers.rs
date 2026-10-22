use iced::widget::{column, row, text};
use iced::{Alignment, Element, Task};

use super::common::error_banner;
use super::{Ctx, Pane, Panel};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::printers::{self, Job, PrintState, Printer, PrinterState};
use crate::services::run::{Generation, blocking};
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        NotInstalled => "Printing is not set up", "Друк не налаштовано";
        NotInstalledHint => "Install CUPS to add and manage printers", "Установіть CUPS, щоб додавати принтери й керувати ними";
        NotRunning => "The CUPS print service is not running, so printers and jobs may be missing.", "Служба друку CUPS не запущена, тому принтери й завдання можуть не відображатися.";
        NoDefault => "No default printer", "Типовий принтер не вибрано";
        PrintersCount => "Printers", "Принтерів";
        JobsCount => "Jobs in queue", "Завдань у черзі";
        OpenCups => "Printing settings…", "Параметри друку…";
        NoPrinters => "No printers", "Немає принтерів";
        NoPrintersHint => "Add a printer in the CUPS web interface", "Додайте принтер у вебінтерфейсі CUPS";
        Printers => "Printers", "Принтери";
        PrintersHint => "Click a printer for details. The default printer is remembered for your account.", "Натисніть на принтер, щоб побачити подробиці. Типовий принтер запам'ятовується для вашого облікового запису.";
        Ready => "Ready", "Готовий";
        Printing => "Printing", "Друкує";
        Paused => "Paused", "Призупинено";
        DefaultBadge => "Default", "Типовий";
        MakeDefault => "Make default", "Зробити типовим";
        DefaultPrinter => "Default printer", "Типовий принтер";
        DefaultPrinterHint => "Used when an app does not ask", "Використовується, коли програма не питає";
        Connection => "Connection", "Під'єднання";
        ConnUsb => "USB", "USB";
        ConnNetwork => "Network", "Мережа";
        ConnFile => "File", "Файл";
        ConnOther => "Other", "Інше";
        AcceptJobs => "Print jobs", "Друкувати завдання";
        AcceptJobsHint => "Pausing and resuming may require administrator rights", "Для призупинення й відновлення можуть знадобитися права адміністратора";
        TestPage => "Test page", "Пробна сторінка";
        TestPageHint => "Prints the CUPS test page", "Друкує пробну сторінку CUPS";
        Print => "Print", "Друкувати";
        TestPageSent => "The test page was sent to the printer", "Пробну сторінку надіслано на принтер";
        Queue => "Print queue", "Черга друку";
        QueueHint => "Documents waiting for a printer", "Документи, що очікують на друк";
        QueueEmpty => "No documents are waiting", "Немає документів у черзі";
    }
}

use tr::Key as T;

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, PrintState),
    Refresh,
    Select(String),
    MakeDefault(String),
    SetEnabled(String, bool),
    TestPage(String),
    CancelJob(String),
    OpenCups,
    Done(Option<Result<String, String>>),
    TestSent(Option<Result<String, String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    state: PrintState,
    loading: bool,
    loaded: bool,
    selected: Option<String>,
    error: Option<String>,
    info: Option<String>,
    pending: u32,
    generation: Generation,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &["printer", "print", "cups", "queue", "принтер", "друк", "черга"];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        if self.loaded || self.loading {
            return Task::none();
        }
        self.refresh()
    }

    fn update(&mut self, msg: Msg, c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, _) if !self.generation.is_current(generation) => {}
            Msg::Loaded(_, state) => {
                self.state = state;
                self.loading = false;
                self.loaded = true;
                if let Some(name) = &self.selected
                    && !self.state.printers.iter().any(|printer| &printer.name == name)
                {
                    self.selected = None;
                }
            }
            Msg::Refresh => return self.refresh(),
            Msg::Select(name) => {
                self.selected = (self.selected.as_ref() != Some(&name)).then_some(name);
            }
            Msg::MakeDefault(name) => {
                for printer in &mut self.state.printers {
                    printer.is_default = printer.name == name;
                }
                return self.run(move || printers::set_default(&name), Msg::Done);
            }
            Msg::SetEnabled(name, enabled) => {
                return self.run(move || printers::set_enabled(&name, enabled), Msg::Done);
            }
            Msg::TestPage(name) => {
                return self.run(move || printers::print_test_page(&name), Msg::TestSent);
            }
            Msg::CancelJob(id) => {
                self.state.jobs.retain(|job| job.id != id);
                return self.run(move || printers::cancel(&id), Msg::Done);
            }
            Msg::OpenCups => {
                return Task::perform(blocking(printers::open_web_interface), |result| {
                    Msg::Done(result.filter(Result::is_err))
                });
            }
            Msg::Done(result) | Msg::TestSent(result @ Some(Err(_))) => {
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
                return self.finish();
            }
            Msg::TestSent(_) => {
                self.info = Some(c.t(T::TestPageSent).to_string());
                return self.finish();
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        if !self.state.available {
            let (title, hint) =
                if self.loaded { (c.t(T::NotInstalled), c.t(T::NotInstalledHint)) } else { (c.t(K::Loading), "") };
            return widgets::empty_state(t, Icon::Printer, title, hint);
        }
        let not_running = (!self.state.running)
            .then(|| widgets::banner(t, crate::theme::Tone::Warn, Icon::Warning, c.t(T::NotRunning), None));
        let info = self.info.as_deref().map(|info| widgets::banner(t, crate::theme::Tone::Ok, Icon::Check, info, None));
        let page = column![self.hero(&c), not_running, error_banner(t, self.error.as_deref()), info]
            .spacing(theme::SECTION_GAP);
        if self.state.printers.is_empty() {
            return page
                .push(column![
                    widgets::empty_state(t, Icon::Printer, c.t(T::NoPrinters), c.t(T::NoPrintersHint),),
                    iced::widget::container(widgets::primary(t, c.t(T::OpenCups), Some(Msg::OpenCups),))
                        .center_x(iced::Fill),
                ])
                .into();
        }
        page.push(self.printers_group(&c)).push(self.queue_group(&c)).into()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.refresh()
    }
}

impl Page {
    fn refresh(&mut self) -> Task<Msg> {
        if self.loading || self.pending > 0 {
            return Task::none();
        }
        self.loading = true;
        let generation = self.generation.current();
        Task::perform(blocking(printers::read), move |state| Msg::Loaded(generation, state.unwrap_or_default()))
    }

    fn run(
        &mut self, job: impl FnOnce() -> Result<String, String> + Send + 'static,
        done: fn(Option<Result<String, String>>) -> Msg,
    ) -> Task<Msg> {
        self.generation.bump();
        self.loading = false;
        self.pending += 1;
        self.error = None;
        self.info = None;
        Task::perform(blocking(job), done)
    }

    fn finish(&mut self) -> Task<Msg> {
        self.pending = self.pending.saturating_sub(1);
        self.generation.bump();
        self.loading = false;
        self.refresh()
    }

    fn hero(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let heading = self
            .state
            .printers
            .iter()
            .find(|printer| printer.is_default)
            .map_or_else(|| c.t(T::NoDefault).to_string(), |printer| printer.name.clone());
        let summary = format!(
            "{}: {} · {}: {}",
            c.t(T::PrintersCount),
            self.state.printers.len(),
            c.t(T::JobsCount),
            self.state.jobs.len()
        );
        widgets::section_hero(
            t,
            Icon::Printer,
            Panel::Printers.color(),
            heading,
            summary,
            Some(
                row![
                    widgets::icon_button(t, Icon::Refresh, Some(Msg::Refresh)),
                    widgets::secondary(t, c.t(T::OpenCups), Some(Msg::OpenCups)),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ),
        )
    }

    fn printers_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Printers)).description(c.t(T::PrintersHint));
        for printer in &self.state.printers {
            let open = self.selected.as_ref() == Some(&printer.name);
            let tone = match printer.state {
                PrinterState::Idle => t.ok(),
                PrinterState::Printing => t.acc_text(),
                PrinterState::Disabled => t.warn(),
            };
            let mut suffix = row![].spacing(8).align_y(Alignment::Center);
            if printer.is_default {
                suffix = suffix.push(widgets::pill(t, crate::theme::Tone::Accent, c.t(T::DefaultBadge)));
            }
            let chevron = if open { Icon::ChevronDown } else { Icon::ChevronRight };
            group = group.push(
                Row::new(t, printer.name.clone())
                    .subtitle(subtitle(c, printer))
                    .prefix(widgets::tile(Icon::Printer, tone, theme::SMALL_CONTROL_HEIGHT))
                    .suffix(suffix.push(widgets::icon(chevron, 16.0, t.fg_3())))
                    .on_press(Msg::Select(printer.name.clone()))
                    .build(),
            );
            if open {
                group = group.extend(self.details(c, printer));
            }
        }
        group.build()
    }

    fn details<'a>(&'a self, c: &Ctx, printer: &'a Printer) -> Vec<Element<'a, Msg>> {
        let t = c.theme;
        let busy = self.pending > 0;
        let name = printer.name.clone();
        let mut rows = Vec::new();
        if let Some(uri) = &printer.device {
            rows.push(Row::new(t, c.t(T::Connection)).suffix(text(uri.as_str()).size(12).color(t.fg_2())).build());
        }
        rows.push(
            Row::new(t, c.t(T::DefaultPrinter))
                .subtitle(c.t(T::DefaultPrinterHint))
                .suffix(if printer.is_default {
                    widgets::icon(Icon::Check, 18.0, t.acc_text())
                } else {
                    widgets::secondary(t, c.t(T::MakeDefault), (!busy).then(|| Msg::MakeDefault(name.clone())))
                })
                .build(),
        );
        let enabled = printer.state != PrinterState::Disabled;
        let toggle_name = name.clone();
        rows.push(
            Row::new(t, c.t(T::AcceptJobs))
                .subtitle(c.t(T::AcceptJobsHint))
                .suffix(if busy {
                    widgets::switch_disabled(t, enabled)
                } else {
                    widgets::switch(t, enabled, move |on| Msg::SetEnabled(toggle_name.clone(), on))
                })
                .build(),
        );
        if self.state.test_page {
            rows.push(
                Row::new(t, c.t(T::TestPage))
                    .subtitle(c.t(T::TestPageHint))
                    .suffix(widgets::secondary(t, c.t(T::Print), (!busy && enabled).then_some(Msg::TestPage(name))))
                    .build(),
            );
        }
        rows
    }

    fn queue_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let group = Group::new(t).title(c.t(T::Queue)).description(c.t(T::QueueHint));
        if self.state.jobs.is_empty() {
            return group.push(Row::new(t, c.t(T::QueueEmpty)).dimmed(true).build()).build();
        }
        let busy = self.pending > 0;
        group.extend(self.state.jobs.iter().map(|job| job_row(c, job, busy))).build()
    }
}

fn subtitle(c: &Ctx, printer: &Printer) -> String {
    let mut label = match (printer.state, &printer.job) {
        (PrinterState::Idle, _) => c.t(T::Ready).to_string(),
        (PrinterState::Printing, Some(job)) => format!("{} {job}", c.t(T::Printing)),
        (PrinterState::Printing, None) => c.t(T::Printing).to_string(),
        (PrinterState::Disabled, _) => c.t(T::Paused).to_string(),
    };
    if printer.state == PrinterState::Disabled
        && let Some(reason) = &printer.reason
    {
        label = format!("{label} · {reason}");
    }
    if let Some(uri) = printer.device.as_deref() {
        let connection = match printers::connection_label(uri) {
            "USB" => T::ConnUsb,
            "network" => T::ConnNetwork,
            "file" => T::ConnFile,
            _ => T::ConnOther,
        };
        label = format!("{label} · {}", c.t(connection));
    }
    label
}

fn job_row<'a>(c: &Ctx, job: &'a Job, busy: bool) -> Element<'a, Msg> {
    let t = c.theme;
    let mut details = vec![job.user.clone(), crate::services::system::format_bytes(job.size, c.lang)];
    if !job.submitted.is_empty() {
        details.push(job.submitted.clone());
    }
    Row::new(t, format!("{} · {}", job.printer, job.id))
        .subtitle(details.join(" · "))
        .prefix(widgets::icon(Icon::Printer, 20.0, t.fg_2()))
        .suffix(widgets::danger(t, c.t(K::Cancel), (!busy).then(|| Msg::CancelJob(job.id.clone()))))
        .build()
}
