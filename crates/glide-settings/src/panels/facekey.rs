use glide_config::facekey::{Attempt, Event, PamService, Reason, Source};
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::scan::{self, Cue, Mood, Step, StepMark, StepState, Tick};
use glide_sdk::widget::{self as sdk};
use iced::widget::{Column, column, container, image, row};
use iced::{Alignment, Element, Fill, Length, Subscription, Task};

use super::common::{gated, toggle};
use super::{Ctx, Pane};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::facekey::{self as service, Run, Session, Snapshot, Update};
use crate::services::run::blocking;
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        NotInstalled => "FaceKey is not installed", "FaceKey не встановлено";
        NotInstalledHint => "Install the facekey package to unlock with your face.", "Установіть пакет facekey, щоб розблоковувати комп’ютер обличчям.";
        StaleTitle => "Add your face again", "Додайте обличчя ще раз";
        StaleText => "FaceKey was updated and the old face no longer fits. Use the password for now.", "FaceKey оновився, і старе обличчя більше не підходить. Поки що входьте паролем.";
        CameraBadTitle => "This camera won’t do", "Ця камера не підійде";
        CameraBadText => "A software camera is selected, not a real one. Choose the built-in or a connected camera below.", "Вибрано програмну камеру, а не справжню. Виберіть вбудовану або підключену камеру нижче.";
        Added => "Face added", "Обличчя додано";
        AddAgainTitle => "Add your face again", "Обличчя треба додати ще раз";
        Working => "Working", "Працює";
        Outdated => "Outdated", "Застаріло";
        Paused => "Paused", "Пауза";
        KeptHere => "Kept only on this computer", "Зберігається лише на цьому комп’ютері";
        StaleSub => "The old face doesn’t fit the new FaceKey", "Старе обличчя не підходить до нової версії FaceKey";
        PausedSub => "Waiting for you to choose a real camera", "Чекає, поки ви виберете справжню камеру";
        AddedOn => "Added {date}", "Додано {date}";
        Check => "Check", "Перевірити";
        AddAgain => "Add again", "Додати заново";
        NoFaceTitle => "No face added yet", "Обличчя ще не додано";
        NoFaceText => "It takes about a minute: look at the camera and turn your head a little.", "Це займе близько хвилини: подивіться в камеру й трохи покрутіть головою.";
        CameraReady => "Camera: {name}, ready", "Камера: {name}, готова";
        NoCamera => "No camera found", "Камеру не знайдено";
        AddFace => "Add face", "Додати обличчя";
        When => "When to recognise your face", "Коли впізнавати обличчя";
        WhenHint => "The password always works too", "Пароль теж завжди працює";
        Lock => "Unlocking the screen", "Розблокування екрана";
        LockHint => "When you come back to the computer", "Коли повертаєтеся до комп’ютера";
        Polkit => "Confirming for apps", "Підтвердження для програм";
        PolkitHint => "When an app asks whether it is really you, for example before installing updates", "Коли програма питає, чи це справді ви, наприклад перед встановленням оновлень";
        Login => "Signing in after switching on", "Вхід після ввімкнення комп’ютера";
        LoginHint => "The first sign-in, right after the computer starts", "Перший вхід, коли комп’ютер щойно увімкнули";
        LoginWarning => "Some apps, like Chrome, will still ask for the password once to open the passwords saved in them.", "Деякі програми, як-от Chrome, однаково попросять пароль один раз, щоб відкрити збережені в них паролі.";
        Sudo => "Terminal commands", "Команди в терміналі";
        SudoHint => "For those who use the sudo command", "Для тих, хто користується командою sudo";
        Camera => "Camera", "Камера";
        CameraRow => "Which camera to use", "Якою камерою дивитися";
        CameraHint => "It turns on only for a few seconds while recognising you; a green dot glows at the top of the screen then", "Вмикається лише на кілька секунд, поки впізнає вас; угорі екрана тоді світиться зелена точка";
        Recent => "Recent attempts", "Останні спроби";
        Recognised => "Recognised", "Впізнано";
        TooDarkAttempt => "Not recognised: it was too dark", "Не впізнано: було затемно";
        SpoofAttempt => "Someone showed a photo or a screen", "Хтось показав фото або екран";
        MismatchAttempt => "Not recognised: it wasn’t you", "Не впізнано: це були не ви";
        Failed => "Not recognised", "Не впізнано";
        Yours => "Your face", "Ваше обличчя";
        Delete => "Delete face", "Видалити обличчя";
        DeleteHint => "After this you can sign in with the password only", "Після цього входити можна буде лише паролем";
        DeleteButton => "Delete…", "Видалити…";
        DeleteConfirm => "The face will be deleted and can’t be restored.", "Обличчя буде видалено, і повернути його не вийде.";
        Cancel => "Cancel", "Скасувати";
        DeleteNow => "Delete", "Видалити";
        PasswordNote => "Adding or deleting the face asks for your password.", "Додати чи видалити обличчя можна лише після введення вашого пароля.";
        Today => "Today at {time}", "Сьогодні о {time}";
        Yesterday => "Yesterday at {time}", "Вчора о {time}";
        OnDay => "{date} at {time}", "{date} о {time}";
        SourceLock => "unlocking the screen", "розблокування екрана";
        SourcePolkit => "confirming for an app", "підтвердження для програм";
        SourceSudo => "terminal command", "команди в терміналі";
        SourceLogin => "signing in", "вхід після ввімкнення";
        StepOf => "Step {n} of 5", "Крок {n} з 5";
        IntroTitle => "How to add your face", "Як додати обличчя";
        IntroText => "FaceKey will ask you to look in several directions: straight, left, right, up and down. It takes about a minute.", "FaceKey попросить подивитися в кілька боків: прямо, ліворуч, праворуч, угору й униз. Це займе близько хвилини.";
        Start => "Start", "Почати";
        PasswordTitle => "Type your password first", "Спершу введіть пароль";
        PasswordText => "This way nobody else can add their face instead of yours. The camera turns on after that.", "Так ніхто інший не зможе додати своє обличчя замість вашого. Камера ввімкнеться після цього.";
        PasswordWhere => "The password field is at the top of the screen", "Поле для пароля — угорі екрана";
        Step0 => "Look straight at the camera", "Подивіться прямо в камеру";
        Step0Text => "Put your face in the frame and hold your head level.", "Розмістіть обличчя в рамці й тримайте голову рівно.";
        Step1 => "Turn your head a little to the left", "Трохи поверніть голову ліворуч";
        Step1Text => "Just a little, as if looking at the edge of the screen.", "Зовсім трохи, наче дивитеся на край екрана.";
        Step2 => "Now a little to the right", "Тепер трохи праворуч";
        Step2Text => "Just as smoothly, no hurry.", "Так само плавно, без поспіху.";
        Step3 => "Raise or lower your chin", "Підніміть або опустіть підборіддя";
        Step3Text => "A small nod is enough.", "Достатньо невеликого кивка.";
        Step4 => "And straight again", "І знову прямо";
        Step4Text => "Last step: look at the camera once more.", "Останній крок: подивіться в камеру ще раз.";
        DarkHint => "A bit dark: light in front of you helps.", "Трохи затемно: світло спереду допоможе.";
        BrightHint => "Too bright behind or in front of you.", "Засвітло: приберіть яскраве світло.";
        FaceHint => "Move so your whole face is in the circle.", "Розташуйтеся так, щоб усе обличчя було в колі.";
        StillHint => "Hold still for a moment.", "Затримайтеся на мить.";
        DoneTitle => "Face added", "Обличчя додано";
        DoneText => "Now you only need to look at the camera to unlock the screen.", "Тепер, щоб розблокувати екран, досить подивитися в камеру.";
        FailTitle => "Couldn’t add the face", "Не вдалося додати обличчя";
        OtherPerson => "Another person appeared in front of the camera. Try again when you are alone.", "Перед камерою з’явилася інша людина. Спробуйте ще раз, коли будете самі.";
        Timeout => "Time ran out. Try again in a calm moment.", "Час вийшов. Спробуйте ще раз, коли буде зручно.";
        CameraFailed => "The camera couldn’t be used. Check that no other app is holding it.", "Камерою не вдалося скористатися. Перевірте, чи її не тримає інша програма.";
        Generic => "Something went wrong. Try again.", "Щось пішло не так. Спробуйте ще раз.";
        Retry => "Try again", "Спробувати ще раз";
        Done => "Done", "Готово";
        StepFront => "Straight", "Прямо";
        StepLeft => "Left", "Ліворуч";
        StepRight => "Right", "Праворуч";
        StepTilt => "Up and down", "Угору й униз";
        StepAgain => "Straight again", "Ще раз прямо";
        LookTitle => "Look at the camera", "Дивіться в камеру";
        LookText => "Hold your head level for a few seconds.", "Тримайте голову рівно кілька секунд.";
        ItsYou => "It’s you!", "Це ви!";
        ItsYouText => "Everything works. The computer recognises you on the lock screen the same way.", "Усе працює. Так само комп’ютер впізнає вас на екрані блокування.";
        NoFace => "Face not visible", "Обличчя не видно";
        NoFaceTest => "The camera can’t see your whole face.", "Камера не бачить вашого обличчя повністю.";
        TooDark => "Too dark", "Затемно";
        TooDarkText => "The camera needs more light to see your face.", "Камері бракує світла, щоб роздивитися обличчя.";
        TooBright => "Too bright", "Засвітло";
        TooBrightText => "Bright light washes out your face.", "Яскраве світло засвічує обличчя.";
        Spoof => "Looks like a photo, not a person", "Схоже на фото, а не на людину";
        SpoofText => "FaceKey doesn’t open the computer for a photo or a video.", "FaceKey не відкриває комп’ютер за фотографією чи відео.";
        NotYou => "It’s not you", "Це не ви";
        NotYouText => "The face in the camera doesn’t look like the one added.", "Обличчя в камері не схоже на додане.";
        Late => "Didn’t make it in time", "Не вийшло вчасно";
        LateText => "The face was moving, and FaceKey couldn’t see you clearly.", "Обличчя рухалося, і FaceKey не встиг вас роздивитися.";
        CameraBusy => "The camera is busy", "Камера зайнята";
        CameraBusyText => "Another app is using the camera right now.", "Камерою зараз користується інша програма.";
        Unavailable => "Couldn’t check", "Не вдалося перевірити";
        UnavailableText => "FaceKey couldn’t start the camera or read the face.", "FaceKey не зміг увімкнути камеру або прочитати обличчя.";
        WhatToDo => "What you can do", "Що можна зробити";
        TipSit => "Sit straight in front of the camera", "Сядьте прямо навпроти камери";
        TipCover => "Check that nothing covers the camera", "Перевірте, чи камера нічим не закрита";
        TipLight => "Turn on a light in front of you", "Увімкніть світло перед собою";
        TipWindow => "Don’t sit with your back to a window", "Не сідайте спиною до вікна";
        TipGlasses => "If it really is you, take off glasses with glare and try again", "Якщо це справді ви, зніміть окуляри з відблиском і спробуйте ще раз";
        TipReenroll => "If it is you and you look different now, add the face again", "Якщо це все ж ви, а зовнішність змінилася, додайте обличчя заново";
        TipHold => "Hold your head still for a few seconds", "Тримайте голову нерухомо кілька секунд";
        TipLook => "Look straight at the camera", "Дивіться прямо в камеру";
        TipClose => "Close the app that uses the camera", "Закрийте програму, яка користується камерою";
        AddAgainButton => "Add face again", "Додати обличчя заново";
        Again => "Again", "Ще раз";
        JustCheck => "This is only a check: nothing gets unlocked.", "Це лише перевірка: нічого не розблоковується.";
    }
}

use tr::Key as T;

const MONTHS_UK: [&str; 12] = [
    "січня",
    "лютого",
    "березня",
    "квітня",
    "травня",
    "червня",
    "липня",
    "серпня",
    "вересня",
    "жовтня",
    "листопада",
    "грудня",
];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const SHEET_WIDTH: f32 = 600.0;
const ATTEMPTS_SHOWN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollStage {
    Intro,
    Password,
    Capturing,
    Done,
    Failed(Reason),
}

#[derive(Debug, Clone)]
pub struct Enroll {
    pub stage: EnrollStage,
    pub step: u8,
    pub hold: u8,
    pub required: u8,
    pub hint: Option<Reason>,
    pub frame: Option<image::Handle>,
}

impl Enroll {
    const fn new() -> Self {
        Self { stage: EnrollStage::Intro, step: 0, hold: 0, required: 4, hint: None, frame: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStage {
    Running,
    Finished(Reason),
}

#[derive(Debug, Clone)]
pub enum Sheet {
    Enroll(Enroll),
    Test(TestStage),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Box<Snapshot>),
    OpenEnroll,
    BeginCapture,
    OpenTest,
    Update(Update),
    CloseSheet,
    Lock(bool),
    Pam(PamService, bool),
    PamDone(Result<String, String>),
    Camera(CameraChoice),
    AskDelete(bool),
    Delete,
    Deleted(Result<String, String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraChoice {
    path: String,
    name: String,
}

impl std::fmt::Display for CameraChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Default)]
pub struct Page {
    snapshot: Option<Snapshot>,
    sheet: Option<Sheet>,
    generation: u64,
    pending: Option<(PamService, bool)>,
    confirm_delete: bool,
    error: Option<String>,
}

fn reload() -> Task<Msg> {
    Task::perform(blocking(service::snapshot), |snapshot| Msg::Loaded(Box::new(snapshot.unwrap_or_default())))
}

#[must_use]
pub fn ticks(stage: EnrollStage, step: u8) -> Vec<Tick> {
    let sector = |angle: f32| {
        if (225.0..315.0).contains(&angle) {
            1
        } else if (45.0..135.0).contains(&angle) {
            2
        } else {
            3
        }
    };
    scan::ring_ticks(|angle| match stage {
        EnrollStage::Done => Tick::Done,
        EnrollStage::Failed(_) => match sector(angle).cmp(&step) {
            std::cmp::Ordering::Less => Tick::Done,
            std::cmp::Ordering::Equal => Tick::Failed,
            std::cmp::Ordering::Greater => Tick::Idle,
        },
        EnrollStage::Capturing if step == 0 || step >= 4 => Tick::Active,
        EnrollStage::Capturing => match sector(angle).cmp(&step) {
            std::cmp::Ordering::Less => Tick::Done,
            std::cmp::Ordering::Equal => Tick::Active,
            std::cmp::Ordering::Greater => Tick::Idle,
        },
        _ => Tick::Idle,
    })
}

#[must_use]
pub const fn cue(step: u8) -> Option<Cue> {
    match step {
        1 => Some(Cue::Left),
        2 => Some(Cue::Right),
        3 => Some(Cue::Tilt),
        _ => None,
    }
}

#[must_use]
pub fn steps(c: &Ctx, stage: EnrollStage, current: u8) -> Vec<Step> {
    let marks = [
        (StepMark::Front, T::StepFront),
        (StepMark::Left, T::StepLeft),
        (StepMark::Right, T::StepRight),
        (StepMark::Tilt, T::StepTilt),
        (StepMark::Front, T::StepAgain),
    ];
    marks
        .iter()
        .enumerate()
        .map(|(position, (mark, label))| {
            let position = position as u8;
            let progress = match stage {
                EnrollStage::Done => StepState::Done,
                EnrollStage::Failed(_) if position == current => StepState::Failed,
                EnrollStage::Capturing if position == current => StepState::Current,
                EnrollStage::Capturing | EnrollStage::Failed(_) if position < current => StepState::Done,
                _ => StepState::Pending,
            };
            Step { mark: *mark, label: c.t(*label).to_string(), state: progress }
        })
        .collect()
}

fn month(c: &Ctx, index: u8) -> &'static str {
    let index = usize::from(index.clamp(1, 12) - 1);
    match c.lang {
        crate::i18n::Lang::Uk => MONTHS_UK[index],
        crate::i18n::Lang::En => MONTHS_EN[index],
    }
}

fn day_month(c: &Ctx, day: u8, month_index: u8) -> String {
    format!("{day} {}", month(c, month_index))
}

#[must_use]
pub fn added_on(c: &Ctx, created: &str) -> Option<String> {
    let date = created.get(..10)?;
    let mut parts = date.split('-');
    let (_, month_index, day) = (parts.next()?, parts.next()?.parse().ok()?, parts.next()?.parse::<u8>().ok()?);
    Some(c.t(T::AddedOn).replace("{date}", &day_month(c, day, month_index)))
}

#[must_use]
pub fn when(c: &Ctx, at: i64, now: i64) -> String {
    let Some(moment) = u64::try_from(at).ok().and_then(glide_config::local_time_at) else { return String::new() };
    let time = format!("{:02}:{:02}", moment.hour, moment.minute);
    let today = u64::try_from(now).ok().and_then(glide_config::local_time_at);
    let yesterday = u64::try_from(now - 86_400).ok().and_then(glide_config::local_time_at);
    let same = |other: Option<glide_config::LocalTime>| {
        other.is_some_and(|o| (o.year, o.month, o.day) == (moment.year, moment.month, moment.day))
    };
    if same(today) {
        c.t(T::Today).replace("{time}", &time)
    } else if same(yesterday) {
        c.t(T::Yesterday).replace("{time}", &time)
    } else {
        c.t(T::OnDay).replace("{date}", &day_month(c, moment.day, moment.month)).replace("{time}", &time)
    }
}

const fn source_label(source: Source) -> T {
    match source {
        Source::Lock | Source::Test => T::SourceLock,
        Source::Polkit => T::SourcePolkit,
        Source::Sudo => T::SourceSudo,
        Source::Login => T::SourceLogin,
    }
}

#[must_use]
pub const fn attempt_title(attempt: &Attempt) -> T {
    match attempt.reason {
        _ if attempt.success => T::Recognised,
        Reason::TooDark | Reason::TooBright => T::TooDarkAttempt,
        Reason::Spoof => T::SpoofAttempt,
        Reason::Mismatch => T::MismatchAttempt,
        _ => T::Failed,
    }
}

#[must_use]
pub const fn verdict(reason: Reason) -> (T, T, &'static [T], bool) {
    match reason {
        Reason::Passed => (T::ItsYou, T::ItsYouText, &[], false),
        Reason::NoFace | Reason::OffFrame | Reason::TooFar => {
            (T::NoFace, T::NoFaceTest, &[T::TipSit, T::TipCover], false)
        }
        Reason::TooDark => (T::TooDark, T::TooDarkText, &[T::TipLight, T::TipWindow], false),
        Reason::TooBright => (T::TooBright, T::TooBrightText, &[T::TipWindow], false),
        Reason::Spoof => (T::Spoof, T::SpoofText, &[T::TipGlasses], false),
        Reason::Mismatch | Reason::OtherPerson => (T::NotYou, T::NotYouText, &[T::TipReenroll], true),
        Reason::Timeout | Reason::Moving | Reason::HeadTurned | Reason::Blurry => {
            (T::Late, T::LateText, &[T::TipHold, T::TipLook], false)
        }
        Reason::CameraBusy => (T::CameraBusy, T::CameraBusyText, &[T::TipClose], false),
        Reason::StaleProfile => (T::NotYou, T::StaleText, &[], true),
        _ => (T::Unavailable, T::UnavailableText, &[], false),
    }
}

const fn hint_text(hint: Reason) -> T {
    match hint {
        Reason::TooDark => T::DarkHint,
        Reason::TooBright => T::BrightHint,
        Reason::NoFace | Reason::OffFrame | Reason::TooFar => T::FaceHint,
        _ => T::StillHint,
    }
}

const fn failure_text(reason: Reason) -> T {
    match reason {
        Reason::OtherPerson => T::OtherPerson,
        Reason::Timeout => T::Timeout,
        Reason::CameraBusy | Reason::CameraUntrusted | Reason::NoCamera => T::CameraFailed,
        _ => T::Generic,
    }
}

impl Page {
    fn session(&self) -> Option<(Run, u64)> {
        match &self.sheet {
            Some(Sheet::Enroll(enroll)) if matches!(enroll.stage, EnrollStage::Password | EnrollStage::Capturing) => {
                Some((Run::Enroll, self.generation))
            }
            Some(Sheet::Test(TestStage::Running)) => Some((Run::Verify, self.generation)),
            _ => None,
        }
    }

    fn camera_untrusted(&self, config: &ConfigStore) -> bool {
        let Some(snapshot) = &self.snapshot else { return false };
        let chosen = config.get_str("facekey", "camera");
        match chosen {
            Some(path) => snapshot.cameras.iter().any(|camera| camera.path == path && !camera.trusted),
            None => !snapshot.cameras.is_empty() && snapshot.cameras.iter().all(|camera| !camera.trusted),
        }
    }

    fn apply(&mut self, update: Update) -> Task<Msg> {
        match (&mut self.sheet, update) {
            (Some(Sheet::Enroll(enroll)), Update::Frame(frame)) => enroll.frame = Some(frame),
            (Some(Sheet::Enroll(enroll)), Update::Event(event)) => match event {
                Event::Started { .. } => enroll.stage = EnrollStage::Capturing,
                Event::Enroll { step, hold, required, hint, .. } => {
                    enroll.stage = EnrollStage::Capturing;
                    enroll.step = step.min(5);
                    enroll.hold = hold;
                    enroll.required = required.max(1);
                    enroll.hint = hint;
                }
                Event::Finished { success, reason } => {
                    enroll.stage = if success { EnrollStage::Done } else { EnrollStage::Failed(reason) };
                    return reload();
                }
                Event::Failed { reason, .. } => enroll.stage = EnrollStage::Failed(reason),
                Event::Verify { .. } | Event::Frame { .. } => {}
            },
            (Some(Sheet::Enroll(enroll)), Update::Ended(_)) => match enroll.stage {
                EnrollStage::Password => enroll.stage = EnrollStage::Intro,
                EnrollStage::Capturing => enroll.stage = EnrollStage::Failed(Reason::Error),
                _ => {}
            },
            (Some(Sheet::Test(stage)), Update::Event(Event::Finished { success, reason })) => {
                *stage = TestStage::Finished(if success { Reason::Passed } else { reason });
            }
            (Some(Sheet::Test(stage)), Update::Ended(_)) if *stage == TestStage::Running => {
                *stage = TestStage::Finished(Reason::Error);
            }
            _ => {}
        }
        Task::none()
    }

    #[must_use]
    pub const fn sheet_state(&self) -> Option<&Sheet> {
        self.sheet.as_ref()
    }

    #[must_use]
    pub const fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "face",
        "face unlock",
        "facekey",
        "camera",
        "biometric",
        "обличчя",
        "розблокування обличчям",
        "камера",
        "біометрія",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        reload()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(snapshot) => self.snapshot = Some(*snapshot),
            Msg::OpenEnroll => {
                self.generation += 1;
                self.sheet = Some(Sheet::Enroll(Enroll::new()));
            }
            Msg::BeginCapture => {
                self.generation += 1;
                self.sheet = Some(Sheet::Enroll(Enroll { stage: EnrollStage::Password, ..Enroll::new() }));
            }
            Msg::OpenTest => {
                self.generation += 1;
                self.sheet = Some(Sheet::Test(TestStage::Running));
            }
            Msg::Update(update) => return self.apply(update),
            Msg::CloseSheet => {
                self.sheet = None;
                return reload();
            }
            Msg::Lock(on) => config.set_bool("facekey", "lock-screen", on),
            Msg::Camera(choice) => config.set_str("facekey", "camera", &choice.path),
            Msg::Pam(service, on) => {
                self.pending = Some((service, on));
                self.error = None;
                return Task::perform(blocking(move || service::set_pam(service, on)), |result| {
                    Msg::PamDone(result.unwrap_or_else(|| Err(String::new())))
                });
            }
            Msg::PamDone(result) => {
                self.pending = None;
                self.error = result.err().filter(|error| !error.is_empty());
                return reload();
            }
            Msg::AskDelete(ask) => self.confirm_delete = ask,
            Msg::Delete => {
                self.confirm_delete = false;
                return Task::perform(blocking(service::delete), |result| {
                    Msg::Deleted(result.unwrap_or_else(|| Err(String::new())))
                });
            }
            Msg::Deleted(result) => {
                self.error = result.err().filter(|error| !error.is_empty());
                return reload();
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Msg> {
        let Some((run, generation)) = self.session() else { return Subscription::none() };
        let camera = None;
        Subscription::run_with(Session { run, camera, generation }, service::stream).map(Msg::Update)
    }

    fn sidebar_status(&self, _c: Ctx) -> Option<String> {
        None
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let Some(snapshot) = &self.snapshot else {
            return widgets::spinner_label(t, crate::i18n::Key::Loading.text(c.lang));
        };
        if !snapshot.installed {
            return widgets::empty_state(t, Icon::Face, c.t(T::NotInstalled), c.t(T::NotInstalledHint));
        }
        let mut page = Column::new().spacing(theme::SECTION_GAP);
        if let Some(error) = &self.error {
            page = page.push(widgets::banner(t, Tone::Danger, Icon::Warning, error.as_str(), None));
        }
        let stale = snapshot.stale();
        let camera_bad = self.camera_untrusted(config);
        if stale {
            let action = widgets::primary(t, c.t(T::AddFace), Some(Msg::OpenEnroll));
            page = page.push(widgets::titled_banner(
                t,
                Tone::Warn,
                Icon::Warning,
                c.t(T::StaleTitle),
                c.t(T::StaleText),
                Some(action),
            ));
        }
        if camera_bad {
            page = page.push(widgets::titled_banner(
                t,
                Tone::Danger,
                Icon::Camera,
                c.t(T::CameraBadTitle),
                c.t(T::CameraBadText),
                None,
            ));
        }
        page = page.push(hero(&c, snapshot, stale, camera_bad));
        page = page.push(self.places(&c, config, snapshot));
        page = page.push(camera_group(&c, config, snapshot));
        if snapshot.enrolled() {
            if !snapshot.attempts.is_empty() {
                page = page.push(attempts_group(&c, &snapshot.attempts));
            }
            page = page.push(self.yours(&c));
        }
        page.into()
    }

    fn sheet<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Option<Element<'a, Msg>> {
        let body = match self.sheet.as_ref()? {
            Sheet::Enroll(enroll) => enroll_sheet(&c, enroll),
            Sheet::Test(stage) => test_sheet(&c, *stage),
        };
        Some(scan::sheet_card(&c.theme.paint, SHEET_WIDTH, body))
    }
}

impl Page {
    fn places<'a>(&'a self, c: &Ctx, config: &'a ConfigStore, snapshot: &Snapshot) -> Element<'a, Msg> {
        let t = c.theme;
        let usable = snapshot.enrolled() && !snapshot.stale();
        let lock = config.get_bool("facekey", "lock-screen").unwrap_or(true);
        let mut group = Group::new(t).title(c.t(T::When)).description(c.t(T::WhenHint)).push(
            Row::new(t, c.t(T::Lock))
                .subtitle(c.t(T::LockHint))
                .prefix(small_tile(c, Icon::Lock))
                .dimmed(!usable)
                .suffix(gated(*c, widgets::inert(t, usable, widgets::switch(t, lock && usable, Msg::Lock))))
                .build(),
        );
        for (service, glyph, title, hint) in [
            (PamService::Polkit, Icon::Shield, T::Polkit, T::PolkitHint),
            (PamService::Login, Icon::SignIn, T::Login, T::LoginHint),
            (PamService::Sudo, Icon::Terminal, T::Sudo, T::SudoHint),
        ] {
            let Some(enabled) = snapshot.pam(service) else { continue };
            let shown = self.pending.filter(|(pending, _)| *pending == service).map_or(enabled, |(_, on)| on);
            let busy = self.pending.is_some();
            let control =
                widgets::inert(t, usable && !busy, toggle(*c, shown && usable, move |on| Msg::Pam(service, on)));
            group = group.push(
                Row::new(t, c.t(title))
                    .subtitle(c.t(hint))
                    .prefix(small_tile(c, glyph))
                    .dimmed(!usable)
                    .suffix(control)
                    .build(),
            );
            if service == PamService::Login && shown && usable {
                group = group.push(widgets::sample_well(
                    t,
                    widgets::banner(t, Tone::Warn, Icon::Warning, c.t(T::LoginWarning), None),
                ));
            }
        }
        group.build()
    }

    fn yours(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let row = if self.confirm_delete {
            Row::new(t, c.t(T::DeleteConfirm))
                .suffix(
                    row![
                        widgets::secondary(t, c.t(T::Cancel), Some(Msg::AskDelete(false))),
                        widgets::danger(t, c.t(T::DeleteNow), Some(Msg::Delete)),
                    ]
                    .spacing(8),
                )
                .build()
        } else {
            Row::new(t, c.t(T::Delete))
                .subtitle(c.t(T::DeleteHint))
                .suffix(widgets::danger(t, c.t(T::DeleteButton), Some(Msg::AskDelete(true))))
                .build()
        };
        column![
            Group::new(t).title(c.t(T::Yours)).push(row).build(),
            sdk::text(c.t(T::PasswordNote), TextStyle::CAP).color(t.fg_3()),
        ]
        .spacing(8)
        .into()
    }
}

fn small_tile<'a>(c: &Ctx, glyph: Icon) -> Element<'a, Msg> {
    let t = c.theme;
    sdk::inked_tile(glyph, t.paint.fill_1(), t.paint.fg_2(), 28.0)
}

fn hero<'a>(c: &Ctx, snapshot: &Snapshot, stale: bool, camera_bad: bool) -> Element<'a, Msg> {
    let t = c.theme;
    let paint = t.paint;
    if !snapshot.enrolled() {
        let camera = snapshot
            .cameras
            .iter()
            .find(|camera| camera.trusted)
            .map_or_else(|| c.t(T::NoCamera).to_string(), |camera| c.t(T::CameraReady).replace("{name}", &camera.name));
        let labels = column![
            sdk::text(c.t(T::NoFaceTitle), TextStyle::T2),
            sdk::text(c.t(T::NoFaceText), TextStyle::CALL).color(paint.fg_2()),
            sdk::text(camera, TextStyle::CAP).color(paint.fg_3()),
        ]
        .spacing(4)
        .width(Fill);
        return sdk::card(
            &paint,
            row![
                scan::face_tile(&paint, 104.0, Mood::Idle),
                labels,
                widgets::primary(t, c.t(T::AddFace), Some(Msg::OpenEnroll))
            ]
            .spacing(18)
            .align_y(Alignment::Center),
        );
    }
    let (badge, tone, subtitle) = if stale {
        (T::Outdated, Tone::Warn, T::StaleSub)
    } else if camera_bad {
        (T::Paused, Tone::Danger, T::PausedSub)
    } else {
        (T::Working, Tone::Ok, T::KeptHere)
    };
    let title = if stale { T::AddAgainTitle } else { T::Added };
    let mut labels = column![
        row![sdk::text(c.t(title), TextStyle::T2), widgets::pill(t, tone, c.t(badge))]
            .spacing(8)
            .align_y(Alignment::Center),
        sdk::text(c.t(subtitle), TextStyle::CALL).color(paint.fg_2()),
    ]
    .spacing(4)
    .width(Fill);
    if let Some(added) = snapshot
        .status
        .as_ref()
        .and_then(|status| status.created_at.as_deref())
        .and_then(|created| added_on(c, created))
    {
        labels = labels.push(sdk::text(added, TextStyle::CAP).color(paint.fg_3()));
    }
    let mood = if stale || camera_bad { Mood::Idle } else { Mood::Ready };
    let buttons = column![
        widgets::tonal(t, c.t(T::Check), (!stale).then_some(Msg::OpenTest)),
        widgets::secondary(t, c.t(T::AddAgain), Some(Msg::OpenEnroll)),
    ]
    .spacing(8)
    .width(Length::Shrink);
    sdk::card(
        &paint,
        row![scan::face_tile(&paint, 104.0, mood), labels, buttons].spacing(18).align_y(Alignment::Center),
    )
}

fn camera_group<'a>(c: &Ctx, config: &'a ConfigStore, snapshot: &Snapshot) -> Element<'a, Msg> {
    let t = c.theme;
    let choices: Vec<CameraChoice> = snapshot
        .cameras
        .iter()
        .map(|camera| CameraChoice { path: camera.path.clone(), name: camera.name.clone() })
        .collect();
    let row = if choices.is_empty() {
        Row::new(t, c.t(T::CameraRow)).subtitle(c.t(T::NoCamera)).build()
    } else {
        let chosen = config.get_str("facekey", "camera");
        let selected = choices
            .iter()
            .find(|choice| Some(choice.path.as_str()) == chosen)
            .or_else(|| {
                let trusted = snapshot.cameras.iter().find(|camera| camera.trusted)?;
                choices.iter().find(|choice| choice.path == trusted.path)
            })
            .cloned();
        Row::new(t, c.t(T::CameraRow))
            .subtitle(c.t(T::CameraHint))
            .suffix(gated(*c, widgets::dropdown(t, choices, selected, Msg::Camera, 220.0)))
            .build()
    };
    Group::new(t).title(c.t(T::Camera)).push(row).build()
}

fn attempts_group<'a>(c: &Ctx, attempts: &[Attempt]) -> Element<'a, Msg> {
    let t = c.theme;
    let paint = t.paint;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(0));
    let rows = attempts.iter().take(ATTEMPTS_SHOWN).map(|attempt| {
        let (glyph, fill, ink) = match attempt.reason {
            _ if attempt.success => (Icon::Check, paint.ok_soft(), paint.ok()),
            Reason::Spoof | Reason::Mismatch | Reason::OtherPerson => {
                (Icon::Close, paint.danger_soft(), paint.danger())
            }
            _ => (Icon::Warning, paint.warn_soft(), paint.warn()),
        };
        let subtitle = format!("{} · {}", when(c, attempt.at, now), c.t(source_label(attempt.source)));
        Row::new(t, c.t(attempt_title(attempt)))
            .subtitle(subtitle)
            .prefix(sdk::inked_tile(glyph, fill, ink, 28.0))
            .build()
    });
    Group::new(t).title(c.t(T::Recent)).extend(rows).build()
}

fn sheet_frame<'a>(
    c: &Ctx, label: Option<String>, visual: Element<'a, Msg>, title: T, text: String, extra: Vec<Element<'a, Msg>>,
    buttons: Vec<Element<'a, Msg>>,
) -> Element<'a, Msg> {
    let t = c.theme;
    let paint = t.paint;
    let top = row![
        sdk::text(label.unwrap_or_default(), TextStyle::CAP).color(paint.fg_3()).width(Fill),
        widgets::icon_button(t, Icon::Close, Some(Msg::CloseSheet)),
    ]
    .align_y(Alignment::Center);
    let mut body = column![
        top,
        container(visual).center_x(Fill),
        column![
            sdk::text(c.t(title), TextStyle::T1).align_x(Alignment::Center),
            sdk::text(text, TextStyle::BODY).color(paint.fg_2()).align_x(Alignment::Center),
        ]
        .spacing(8)
        .align_x(Alignment::Center)
        .width(Fill),
    ]
    .spacing(20)
    .align_x(Alignment::Center);
    for element in extra {
        body = body.push(element);
    }
    if !buttons.is_empty() {
        body = body.push(row(buttons).spacing(10));
    }
    body.into()
}

fn enroll_sheet<'a>(c: &Ctx, enroll: &Enroll) -> Element<'a, Msg> {
    let t = c.theme;
    let paint = t.paint;
    let step = enroll.step.min(4);
    let (title, text) = match enroll.stage {
        EnrollStage::Intro => (T::IntroTitle, c.t(T::IntroText).to_string()),
        EnrollStage::Password => (T::PasswordTitle, c.t(T::PasswordText).to_string()),
        EnrollStage::Capturing => {
            let (title, text) = [
                (T::Step0, T::Step0Text),
                (T::Step1, T::Step1Text),
                (T::Step2, T::Step2Text),
                (T::Step3, T::Step3Text),
                (T::Step4, T::Step4Text),
            ][usize::from(step)];
            (title, c.t(enroll.hint.map_or(text, hint_text)).to_string())
        }
        EnrollStage::Done => (T::DoneTitle, c.t(T::DoneText).to_string()),
        EnrollStage::Failed(reason) => (T::FailTitle, c.t(failure_text(reason)).to_string()),
    };
    let visual: Element<'a, Msg> = match enroll.stage {
        EnrollStage::Intro | EnrollStage::Password => scan::face_tile(&paint, 180.0, Mood::Idle),
        stage => scan::scan(
            &paint,
            scan::Scan {
                ticks: ticks(stage, step),
                frame: enroll.frame.clone(),
                dimmed: matches!(stage, EnrollStage::Failed(_)),
                cue: if stage == EnrollStage::Capturing { cue(step) } else { None },
                done: stage == EnrollStage::Done,
            },
        ),
    };
    let label =
        (enroll.stage == EnrollStage::Capturing).then(|| c.t(T::StepOf).replace("{n}", &(step + 1).to_string()));
    let mut extra = Vec::new();
    if enroll.stage == EnrollStage::Capturing {
        extra.push(scan::hold_bar(&paint, f32::from(enroll.hold) / f32::from(enroll.required)));
    }
    if enroll.stage == EnrollStage::Password {
        extra.push(sdk::text(c.t(T::PasswordWhere), TextStyle::CAP).color(paint.fg_3()).into());
    } else {
        extra.push(scan::step_trail(&paint, steps(c, enroll.stage, step)));
    }
    let buttons = match enroll.stage {
        EnrollStage::Intro => vec![widgets::primary(t, c.t(T::Start), Some(Msg::BeginCapture))],
        EnrollStage::Failed(_) => vec![
            widgets::secondary(t, c.t(T::Cancel), Some(Msg::CloseSheet)),
            widgets::primary(t, c.t(T::Retry), Some(Msg::BeginCapture)),
        ],
        EnrollStage::Done => vec![
            widgets::secondary(t, c.t(T::Check), Some(Msg::OpenTest)),
            widgets::primary(t, c.t(T::Done), Some(Msg::CloseSheet)),
        ],
        EnrollStage::Password | EnrollStage::Capturing => Vec::new(),
    };
    sheet_frame(c, label, visual, title, text, extra, buttons)
}

fn test_sheet<'a>(c: &Ctx, stage: TestStage) -> Element<'a, Msg> {
    let t = c.theme;
    let paint = t.paint;
    let (mood, title, text, tips, reenroll) = match stage {
        TestStage::Running => (Mood::Scanning, T::LookTitle, T::LookText, &[][..], false),
        TestStage::Finished(reason) => {
            let (title, text, tips, reenroll) = verdict(reason);
            let mood = if reason == Reason::Passed { Mood::Recognised } else { Mood::Rejected };
            (mood, title, text, tips, reenroll)
        }
    };
    let mut extra: Vec<Element<'a, Msg>> = Vec::new();
    if !tips.is_empty() {
        let list = Column::with_children(tips.iter().map(|tip| {
            row![sdk::dot(paint.fg_3(), 6.0), sdk::text(c.t(*tip), TextStyle::CALL).color(paint.fg_2())]
                .spacing(10)
                .align_y(Alignment::Center)
                .into()
        }))
        .spacing(8);
        extra.push(sdk::sample_well(&paint, column![sdk::text(c.t(T::WhatToDo), TextStyle::HEAD), list].spacing(8)));
    }
    let mut buttons = Vec::new();
    if reenroll {
        buttons.push(widgets::secondary(t, c.t(T::AddAgainButton), Some(Msg::OpenEnroll)));
    }
    match stage {
        TestStage::Running => buttons.push(widgets::secondary(t, c.t(T::Cancel), Some(Msg::CloseSheet))),
        TestStage::Finished(Reason::Passed) => buttons.push(widgets::primary(t, c.t(T::Done), Some(Msg::CloseSheet))),
        TestStage::Finished(_) => buttons.push(widgets::primary(t, c.t(T::Again), Some(Msg::OpenTest))),
    }
    extra.push(sdk::text(c.t(T::JustCheck), TextStyle::CAP).color(paint.fg_3()).into());
    sheet_frame(c, None, scan::face_tile(&paint, 160.0, mood), title, c.t(text).to_string(), extra, buttons)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_fills_the_side_of_each_turn_and_pulses_on_frontal_steps() {
        let left = ticks(EnrollStage::Capturing, 1);
        assert_eq!(left[54], Tick::Active, "270° is the left arc");
        assert_eq!(left[18], Tick::Idle, "90° is still to come");
        let right = ticks(EnrollStage::Capturing, 2);
        assert_eq!((right[54], right[18]), (Tick::Done, Tick::Active));
        assert!(ticks(EnrollStage::Capturing, 0).iter().all(|tick| *tick == Tick::Active));
        assert!(ticks(EnrollStage::Done, 4).iter().all(|tick| *tick == Tick::Done));
        let failed = ticks(EnrollStage::Failed(Reason::OtherPerson), 2);
        assert_eq!((failed[54], failed[18], failed[0]), (Tick::Done, Tick::Failed, Tick::Idle));
    }

    #[test]
    fn every_turn_has_its_cue() {
        assert_eq!(
            [cue(0), cue(1), cue(2), cue(3), cue(4)],
            [None, Some(Cue::Left), Some(Cue::Right), Some(Cue::Tilt), None]
        );
    }

    #[test]
    fn verdicts_offer_a_new_enrollment_only_when_the_face_did_not_match() {
        assert!(verdict(Reason::Mismatch).3);
        assert!(!verdict(Reason::TooDark).3);
        assert!(verdict(Reason::Passed).2.is_empty());
        assert_eq!(verdict(Reason::Timeout).0, T::Late);
    }
}
