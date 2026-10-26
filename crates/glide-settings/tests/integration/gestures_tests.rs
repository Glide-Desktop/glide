use glide_ipc::{Event, GestureAction, GestureKind, GesturePhase, GestureSample};
use glide_settings::app::{App, Message};
use glide_settings::i18n::Lang;
use glide_settings::panels::gestures::{
    Direction, Goal, Msg, Page, direction, finger_offset, fingers, fingers_text, sensitivity, verdict_tone,
};
use glide_settings::panels::{Ctx, Pane, Panel};
use glide_settings::services::config::ConfigStore;
use glide_settings::services::ipc::{PracticeEvent, practice_sample};
use glide_settings::theme::Tone;

fn sample(kind: GestureKind, phase: GesturePhase, action: GestureAction, dx: f64, dy: f64) -> GestureSample {
    GestureSample {
        kind,
        phase,
        fingers: 3,
        dx,
        dy,
        scale: 1.0,
        progress: 0.5,
        action,
        committed: phase == GesturePhase::End,
    }
}

fn ctx(online: bool) -> Ctx {
    Ctx { theme: App::default().theme, lang: Lang::Uk, writable: true, online }
}

#[test]
fn gesture_settings_write_the_keys_the_compositor_reads() {
    let mut app = App::with_config(ConfigStore::parse(""));
    for msg in [
        Msg::Enabled(false),
        Msg::Fingers(4),
        Msg::WorkspaceSwipe(false),
        Msg::OverviewSwipe(true),
        Msg::Natural(false),
        Msg::Sensitivity(1.337),
        Msg::CommitSensitivity,
    ] {
        let _ = app.update(Message::Gestures(msg));
    }
    let config = app.config();
    assert_eq!(config.get_bool("gestures", "enabled"), Some(false));
    assert_eq!(config.get_int("gestures", "fingers"), Some(4));
    assert_eq!(config.get_bool("gestures", "workspace-swipe"), Some(false));
    assert_eq!(config.get_bool("gestures", "overview-swipe"), Some(true));
    assert_eq!(config.get_bool("gestures", "natural"), Some(false));
    assert_eq!(config.get_float("gestures", "sensitivity"), Some(1.35), "the slider snaps to its 0.05 step");
}

#[test]
fn out_of_range_gesture_values_read_back_clamped() {
    let config = ConfigStore::parse("[gestures]\nfingers = 7\nsensitivity = 9.0\n");
    assert_eq!(fingers(&config), 4);
    assert!((sensitivity(&config) - 2.0).abs() < f64::EPSILON);
    let empty = ConfigStore::parse("");
    assert_eq!(fingers(&empty), 3);
    assert!((sensitivity(&empty) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn swipe_direction_is_where_the_fingers_went() {
    let swipe = |dx, dy| sample(GestureKind::Swipe, GesturePhase::Update, GestureAction::None, dx, dy);
    assert_eq!(direction(&swipe(-80.0, 5.0), true), Direction::Left);
    assert_eq!(
        direction(&swipe(-80.0, 5.0), false),
        Direction::Right,
        "reversed swipes arrive negated, so the fingers moved the other way"
    );
    assert_eq!(direction(&swipe(3.0, -90.0), true), Direction::Up);
    assert_eq!(direction(&swipe(4.0, 4.0), true), Direction::Still);

    let (x, y) = finger_offset(&swipe(-10_000.0, 0.0), true);
    assert!(x < 0.0 && y.abs() < f32::EPSILON);
    assert!(x.abs() < glide_settings::panels::gestures::PAD_WIDTH / 2.0);
}

#[test]
fn practice_ticks_off_each_gesture_once_it_would_have_worked() {
    let c = ctx(true);
    let mut config = ConfigStore::parse("");
    let mut page = Page::default();
    let mut send = |page: &mut Page, msg| {
        let _ = page.update(msg, c, &mut config);
    };
    send(&mut page, Msg::StartPractice);
    assert!(page.practising());
    send(&mut page, Msg::Practice(PracticeEvent::Started));

    let end = |action, committed| GestureSample {
        committed,
        ..sample(GestureKind::Swipe, GesturePhase::End, action, -300.0, 0.0)
    };
    send(&mut page, Msg::Practice(PracticeEvent::Sample(end(GestureAction::NextWorkspace, false))));
    assert!(page.done().is_empty(), "a swipe that fell short counts for nothing");
    for _ in 0..2 {
        send(&mut page, Msg::Practice(PracticeEvent::Sample(end(GestureAction::NextWorkspace, true))));
    }
    send(
        &mut page,
        Msg::Practice(PracticeEvent::Sample(GestureSample {
            progress: 0.4,
            ..sample(GestureKind::Pinch, GesturePhase::End, GestureAction::App, 0.0, 0.0)
        })),
    );
    assert_eq!(page.done(), [Goal::Next, Goal::Pinch]);
    assert_eq!(page.last().map(|last| last.kind), Some(GestureKind::Pinch));

    send(&mut page, Msg::ResetPractice);
    assert!(page.done().is_empty());
    send(&mut page, Msg::Practice(PracticeEvent::Stopped));
    assert!(!page.practising());
    assert!(!page.failed(), "it had connected, so stopping is not a failure");
}

#[test]
fn a_practice_that_never_connects_reports_a_failure() {
    let c = ctx(true);
    let mut config = ConfigStore::parse("");
    let mut page = Page::default();
    let _ = page.update(Msg::StartPractice, c, &mut config);
    let _ = page.update(Msg::Practice(PracticeEvent::Stopped), c, &mut config);
    assert!(page.failed());
    let _ = page.update(Msg::StartPractice, c, &mut config);
    assert!(!page.failed());
}

#[test]
fn verdicts_separate_success_short_swipes_and_app_gestures() {
    let done = |action, committed| GestureSample {
        committed,
        ..sample(GestureKind::Swipe, GesturePhase::End, action, 0.0, 0.0)
    };
    assert_eq!(verdict_tone(&done(GestureAction::OpenOverview, true), false), Tone::Ok);
    assert_eq!(verdict_tone(&done(GestureAction::OpenOverview, false), false), Tone::Warn);
    assert_eq!(verdict_tone(&done(GestureAction::App, true), false), Tone::Neutral);
    assert_eq!(verdict_tone(&done(GestureAction::Disabled, false), false), Tone::Neutral);
    assert_eq!(verdict_tone(&done(GestureAction::None, false), true), Tone::Accent);
}

#[test]
fn finger_counts_use_ukrainian_plural_forms() {
    assert_eq!(fingers_text(1, Lang::Uk), "1 палець");
    assert_eq!(fingers_text(3, Lang::Uk), "3 пальці");
    assert_eq!(fingers_text(5, Lang::Uk), "5 пальців");
    assert_eq!(fingers_text(4, Lang::En), "4 fingers");
    assert_eq!(fingers_text(1, Lang::En), "1 finger");
}

#[test]
fn practice_lines_are_parsed_and_other_events_ignored() {
    let gesture = sample(GestureKind::Hold, GesturePhase::Begin, GestureAction::App, 0.0, 0.0);
    let line = serde_json::to_string(&Event::Gesture { sample: gesture }).expect("serializes");
    assert_eq!(practice_sample(&line), Some(gesture));
    assert_eq!(practice_sample(r#"{"type":"output-changed"}"#), None);
    assert_eq!(practice_sample("garbage"), None);
}

#[test]
fn the_gestures_panel_is_findable() {
    assert_eq!(Panel::from_slug("gestures"), Some(Panel::Gestures));
    assert_eq!(Panel::from_slug("swipe"), Some(Panel::Gestures));
    assert!(Panel::Gestures.matches("жест"));
    assert!(Panel::Gestures.matches("pinch"));
    assert_eq!(glide_settings::Category::of(Panel::Gestures), glide_settings::Category::Mouse);
}
