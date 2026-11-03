#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Network,
    Appearance,
    IslandAndDock,
    Gestures,
    Phone,
    NightLight,
    Photos,
    QuickLook,
    Snapping,
    Apps,
    Screenshot,
    Finish,
}

impl Step {
    pub const ALL: [Self; 13] = [
        Self::Welcome,
        Self::Network,
        Self::Appearance,
        Self::IslandAndDock,
        Self::Gestures,
        Self::Phone,
        Self::NightLight,
        Self::Photos,
        Self::QuickLook,
        Self::Snapping,
        Self::Apps,
        Self::Screenshot,
        Self::Finish,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub fn at(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    Light,
    #[default]
    Dark,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Accent {
    Blue,
    #[default]
    Teal,
    Green,
    Amber,
    Orange,
    Red,
    Purple,
    Graphite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Island {
    Music,
    Notification,
    Call,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Snap {
    Left,
    Right,
    Maximized,
    #[default]
    Floating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoApp {
    Chrome,
    Telegram,
    Spotify,
    LibreOffice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wifi {
    Off,
    #[default]
    Available,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Network {
    #[default]
    Home,
    Guest,
}

impl Network {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Home => "Glide-Home-5G",
            Self::Guest => "Studio_Guest",
        }
    }

    #[must_use]
    pub const fn security(self) -> &'static str {
        match self {
            Self::Home => "WPA3",
            Self::Guest => "WPA2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phone {
    #[default]
    Unpaired,
    Pairing,
    Paired,
}

#[derive(Debug, Clone)]
pub struct Welcome {
    pub step: Step,
    pub theme: Theme,
    pub accent: Accent,
    pub sound: bool,
    pub night_temperature: u16,
    pub wifi: Wifi,
    pub network: Network,
    pub wifi_password: String,
    pub wifi_password_visible: bool,
    pub phone: Phone,
    pub island: Island,
    pub workspace: usize,
    pub photo: Option<usize>,
    pub file: usize,
    pub quick_look_open: bool,
    pub snap: Snap,
    pub installed_apps: Vec<DemoApp>,
    pub installing_apps: Vec<DemoApp>,
}

impl Default for Welcome {
    fn default() -> Self {
        Self {
            step: Step::Welcome,
            theme: Theme::Dark,
            accent: Accent::Teal,
            sound: true,
            night_temperature: 4600,
            wifi: Wifi::Available,
            network: Network::Home,
            wifi_password: "GlideFastNetwork2026".into(),
            wifi_password_visible: false,
            phone: Phone::Unpaired,
            island: Island::Music,
            workspace: 0,
            photo: None,
            file: 0,
            quick_look_open: false,
            snap: Snap::Floating,
            installed_apps: Vec::new(),
            installing_apps: Vec::new(),
        }
    }
}

impl Welcome {
    pub fn go_to(&mut self, step: Step) {
        self.step = step;
    }

    pub fn next(&mut self) {
        if let Some(step) = Step::at(self.step.index() + 1) {
            self.go_to(step);
        }
    }

    pub fn back(&mut self) {
        if let Some(step) = self.step.index().checked_sub(1).and_then(Step::at) {
            self.go_to(step);
        }
    }

    pub fn confirm_skip(&mut self) {
        self.go_to(Step::Finish);
    }

    pub fn set_temperature(&mut self, kelvin: u16) {
        self.night_temperature = kelvin.clamp(3200, 6500);
    }

    pub fn swipe_workspace(&mut self, distance: f32) {
        if distance < -40.0 {
            self.workspace = (self.workspace + 1).min(2);
        } else if distance > 40.0 {
            self.workspace = self.workspace.saturating_sub(1);
        }
    }

    pub fn install_demo_app(&mut self, app: DemoApp) {
        if !self.installed_apps.contains(&app) {
            self.installed_apps.push(app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_covers_all_thirteen_steps_and_stays_in_bounds() {
        let mut welcome = Welcome::default();
        welcome.back();
        assert_eq!(welcome.step, Step::Welcome);
        for expected in Step::ALL.into_iter().skip(1) {
            welcome.next();
            assert_eq!(welcome.step, expected);
        }
        welcome.next();
        assert_eq!(welcome.step, Step::Finish);
        welcome.go_to(Step::Photos);
        welcome.back();
        assert_eq!(welcome.step, Step::NightLight);
    }

    #[test]
    fn skip_leads_to_summary_without_losing_choices() {
        let mut welcome = Welcome { accent: Accent::Purple, ..Welcome::default() };
        welcome.confirm_skip();
        assert_eq!(welcome.step, Step::Finish);
        assert_eq!(welcome.accent, Accent::Purple);
    }

    #[test]
    fn demonstration_state_respects_the_prototypes_limits() {
        let mut welcome = Welcome::default();
        welcome.set_temperature(1000);
        assert_eq!(welcome.night_temperature, 3200);
        welcome.set_temperature(9000);
        assert_eq!(welcome.night_temperature, 6500);
        for _ in 0..4 {
            welcome.swipe_workspace(-41.0);
        }
        assert_eq!(welcome.workspace, 2);
        welcome.swipe_workspace(40.0);
        assert_eq!(welcome.workspace, 2);
        welcome.swipe_workspace(41.0);
        assert_eq!(welcome.workspace, 1);
        welcome.install_demo_app(DemoApp::Chrome);
        welcome.install_demo_app(DemoApp::Chrome);
        assert_eq!(welcome.installed_apps, [DemoApp::Chrome]);
    }

    #[test]
    fn completion_creates_the_marker_after_the_tour_is_finished() {
        let dir = std::env::temp_dir().join(format!("glide-welcome-complete-{}", std::process::id()));
        let marker = dir.join("glide").join("welcomed");
        assert!(!marker.exists());
        super::write_completion(&marker).expect("completion is saved");
        assert!(marker.is_file());
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
mod calculator;
mod island;
mod snap;
mod sound;
mod stepper;
pub mod ui;

pub fn mark_complete() -> std::io::Result<()> {
    let path = glide_config::welcome::marker()
        .ok_or_else(|| std::io::Error::other("cannot locate the user state directory"))?;
    write_completion(&path)
}

fn write_completion(path: &std::path::Path) -> std::io::Result<()> {
    glide_config::welcome::mark_complete(path)
}
