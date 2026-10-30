use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

pub use glide_app::chooser::{Filter, Mode, Pattern, Request, Response};

use crate::fs::mime::{MimeDb, glob_match};

static REQUEST: OnceLock<Request> = OnceLock::new();
static ACCEPTED: AtomicBool = AtomicBool::new(false);

pub fn install(request: Request) {
    let _ = REQUEST.set(request);
}

#[must_use]
pub fn request() -> Option<&'static Request> {
    REQUEST.get()
}

#[must_use]
pub fn accepted() -> bool {
    ACCEPTED.load(Ordering::Relaxed)
}

pub fn finish(response: &Response) {
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{}", response.to_json()).and_then(|()| out.flush()).is_ok() {
        ACCEPTED.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub request: Request,
    pub name: String,
    pub filter: Option<usize>,
    pub confirm: Option<PathBuf>,
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub index: usize,
    pub id: String,
    pub label: String,
}

impl std::fmt::Display for Pick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

impl State {
    #[must_use]
    pub fn new(request: Request) -> Self {
        let filter = request
            .current_filter
            .filter(|index| *index < request.filters.len())
            .or_else(|| (!request.filters.is_empty()).then_some(0));
        let name = request.current_name.clone().unwrap_or_default();
        let choices: Vec<String> = request.choices.iter().map(glide_app::chooser::Choice::initial).collect();
        Self { request, name, filter, confirm: None, choices }
    }

    pub(crate) fn set_choice(&mut self, index: usize, value: String) {
        let known = self.request.choices.get(index).is_some_and(|choice| {
            if choice.is_toggle() {
                matches!(value.as_str(), "true" | "false")
            } else {
                choice.options.iter().any(|(id, _)| *id == value)
            }
        });
        if known && let Some(slot) = self.choices.get_mut(index) {
            *slot = value;
        }
    }

    pub(crate) fn set_filter(&mut self, index: usize) {
        if index < self.request.filters.len() {
            self.filter = Some(index);
        }
    }

    #[must_use]
    pub(crate) fn filter_options(&self) -> Vec<Pick> {
        self.request
            .filters
            .iter()
            .enumerate()
            .map(|(index, filter)| Pick { index, id: index.to_string(), label: filter.name.clone() })
            .collect()
    }

    #[must_use]
    pub(crate) fn choice_options(&self, index: usize) -> Vec<Pick> {
        self.request.choices.get(index).map_or_else(Vec::new, |choice| {
            choice.options.iter().map(|(id, label)| Pick { index, id: id.clone(), label: label.clone() }).collect()
        })
    }

    #[must_use]
    pub(crate) fn new_folder_offered(&self) -> bool {
        matches!(self.request.mode, Mode::Save | Mode::SaveFiles) || self.picks_folders()
    }

    #[must_use]
    pub(crate) fn name_stem(&self) -> usize {
        let name = self.name.trim_end();
        Path::new(name).file_stem().map_or(0, |stem| stem.to_string_lossy().chars().count())
    }

    #[must_use]
    pub(crate) fn choice_is_on(&self, index: usize) -> bool {
        self.request.choices.get(index).is_some_and(glide_app::chooser::Choice::is_toggle)
            && self.choices.get(index).is_some_and(|value| value == "true")
    }

    #[must_use]
    pub(crate) fn answered_choices(&self) -> Vec<(String, String)> {
        self.request
            .choices
            .iter()
            .zip(&self.choices)
            .map(|(choice, value)| (choice.id.clone(), value.clone()))
            .collect()
    }

    #[must_use]
    pub(crate) const fn picks_folders(&self) -> bool {
        matches!(self.request.mode, Mode::Open) && self.request.directory
    }

    #[must_use]
    pub(crate) fn active_filter(&self) -> Option<&Filter> {
        self.filter.and_then(|index| self.request.filters.get(index))
    }

    #[must_use]
    pub fn shows(&self, path: &Path, is_dir: bool, db: &MimeDb) -> bool {
        if is_dir {
            return true;
        }
        if self.picks_folders() {
            return false;
        }
        let Some(filter) = self.active_filter() else {
            return true;
        };
        let name = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
        matches(filter, &name, &db.of_name(path, false), db)
    }
}

#[must_use]
pub fn matches(filter: &Filter, name: &str, mime: &str, db: &MimeDb) -> bool {
    filter.patterns.iter().any(|pattern| match pattern {
        Pattern::Glob(glob) => glob_match(glob, name) || glob_match(&glob.to_lowercase(), &name.to_lowercase()),
        Pattern::Mime(wanted) => {
            db.is_a(mime, wanted)
                || wanted.strip_suffix("/*").is_some_and(|family| mime.split('/').next() == Some(family))
        }
    })
}

#[must_use]
pub fn valid_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn images() -> Request {
        Request {
            filters: vec![
                Filter { name: "Images".into(), patterns: vec![Pattern::Glob("*.png".into())] },
                Filter { name: "Text".into(), patterns: vec![Pattern::Mime("text/*".into())] },
            ],
            ..Request::default()
        }
    }

    #[test]
    fn folders_always_show_and_files_follow_the_active_filter() {
        let db = MimeDb::default();
        let state = State::new(images());
        assert_eq!(state.filter, Some(0), "the first filter is active when none is named");
        assert!(state.shows(Path::new("/x/sub"), true, &db));
        assert!(state.shows(Path::new("/x/Photo.PNG"), false, &db), "globs match regardless of case");
        assert!(!state.shows(Path::new("/x/notes.txt"), false, &db));
    }

    #[test]
    fn the_filter_list_picks_any_filter_and_ignores_unknown_ones() {
        let mut state = State::new(images());
        assert_eq!(
            state.filter_options().iter().map(|pick| pick.label.as_str()).collect::<Vec<_>>(),
            ["Images", "Text"]
        );
        state.set_filter(1);
        assert_eq!(state.filter, Some(1));
        state.set_filter(7);
        assert_eq!(state.filter, Some(1));
    }

    #[test]
    fn the_save_name_preselects_its_stem_and_new_folders_are_offered_where_folders_are_picked() {
        let mut save = State::new(Request { mode: Mode::Save, ..Request::default() });
        save.name = "report.final.pdf".into();
        assert_eq!(save.name_stem(), "report.final".chars().count());
        assert!(save.new_folder_offered());
        assert!(State::new(Request { directory: true, ..Request::default() }).new_folder_offered());
        assert!(!State::new(Request::default()).new_folder_offered());
    }

    #[test]
    fn a_folder_picker_hides_files() {
        let state = State::new(Request { directory: true, ..Request::default() });
        assert!(state.picks_folders());
        assert!(!state.shows(Path::new("/x/a.txt"), false, &MimeDb::default()));
        assert!(state.shows(Path::new("/x/dir"), true, &MimeDb::default()));
    }

    #[test]
    fn choices_start_at_their_default_and_are_answered_by_id() {
        let request = Request {
            choices: vec![
                glide_app::chooser::Choice {
                    id: "ro".into(),
                    label: "Read only".into(),
                    options: Vec::new(),
                    default: "true".into(),
                },
                glide_app::chooser::Choice {
                    id: "enc".into(),
                    label: "Encoding".into(),
                    options: vec![("a".into(), "A".into()), ("b".into(), "B".into())],
                    default: "a".into(),
                },
            ],
            ..Request::default()
        };
        let mut state = State::new(request);
        assert!(state.choice_is_on(0));
        state.set_choice(0, "false".into());
        state.set_choice(1, "b".into());
        state.set_choice(1, "z".into());
        assert!(!state.choice_is_on(0));
        assert_eq!(state.choice_options(1).iter().map(|pick| pick.label.as_str()).collect::<Vec<_>>(), ["A", "B"]);
        assert_eq!(state.answered_choices(), [("ro".to_string(), "false".to_string()), ("enc".into(), "b".into())]);
    }

    #[test]
    fn names_that_cannot_be_files_are_refused() {
        for bad in ["", "  ", ".", "..", "a/b"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        assert!(valid_name("report.pdf"));
    }
}
