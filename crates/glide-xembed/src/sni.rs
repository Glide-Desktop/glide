use std::sync::{Arc, Mutex, PoisonError};

use zbus::object_server::SignalEmitter;

use crate::pixmap::Icon;

pub const PATH: &str = "/StatusNotifierItem";
pub const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

type Pixmaps = Vec<(i32, i32, Vec<u8>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    Primary,
    Middle,
    Secondary,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

impl Click {
    #[must_use]
    pub const fn button(self) -> u8 {
        match self {
            Self::Primary => 1,
            Self::Middle => 2,
            Self::Secondary => 3,
            Self::ScrollUp => 4,
            Self::ScrollDown => 5,
            Self::ScrollLeft => 6,
            Self::ScrollRight => 7,
        }
    }

    #[must_use]
    pub fn scroll(delta: i32, orientation: &str) -> Option<Self> {
        let horizontal = orientation.eq_ignore_ascii_case("horizontal");
        match (delta.signum(), horizontal) {
            (0, _) => None,
            (-1, false) => Some(Self::ScrollUp),
            (_, false) => Some(Self::ScrollDown),
            (-1, true) => Some(Self::ScrollLeft),
            (_, true) => Some(Self::ScrollRight),
        }
    }
}

pub trait Clicker: Send + Sync {
    fn click(&self, window: u32, click: Click);
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Look {
    pub id: String,
    pub title: String,
    pub icon: Icon,
}

impl Look {
    fn pixmaps(&self) -> Pixmaps {
        if self.icon.argb.is_empty() {
            return Vec::new();
        }
        vec![(self.icon.width, self.icon.height, self.icon.argb.clone())]
    }
}

pub struct Item {
    window: u32,
    look: Arc<Mutex<Look>>,
    clicker: Arc<dyn Clicker>,
}

impl Item {
    fn look(&self) -> Look {
        self.look.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn press(&self, click: Click) {
        self.clicker.click(self.window, click);
    }
}

// zbus requires `&self` and owned arguments on served methods even where they go unused.
#[allow(clippy::unused_self, clippy::needless_pass_by_value)]
#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> String {
        "ApplicationStatus".into()
    }

    #[zbus(property)]
    fn id(&self) -> String {
        self.look().id
    }

    #[zbus(property)]
    fn title(&self) -> String {
        self.look().title
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "Active".into()
    }

    #[zbus(property)]
    fn window_id(&self) -> i32 {
        i32::try_from(self.window).unwrap_or_default()
    }

    #[zbus(property)]
    fn icon_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Pixmaps {
        self.look().pixmaps()
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn tool_tip(&self) -> (String, Pixmaps, String, String) {
        (String::new(), Vec::new(), self.look().title, String::new())
    }

    fn activate(&self, x: i32, y: i32) {
        tracing::debug!(window = self.window, x, y, "tray icon activated");
        self.press(Click::Primary);
    }

    fn secondary_activate(&self, x: i32, y: i32) {
        tracing::debug!(window = self.window, x, y, "tray icon middle-clicked");
        self.press(Click::Middle);
    }

    fn context_menu(&self, x: i32, y: i32) {
        tracing::debug!(window = self.window, x, y, "tray icon menu asked for");
        self.press(Click::Secondary);
    }

    fn scroll(&self, delta: i32, orientation: String) {
        if let Some(click) = Click::scroll(delta, &orientation) {
            self.press(click);
        }
    }

    fn provide_xdg_activation_token(&self, token: String) {
        tracing::trace!(window = self.window, len = token.len(), "activation token offered; X11 icons cannot use it");
    }

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

pub struct Exported {
    connection: zbus::blocking::Connection,
    name: String,
    look: Arc<Mutex<Look>>,
}

impl Exported {
    pub fn on(
        builder: zbus::blocking::connection::Builder<'_>, name: String, window: u32, look: Look,
        clicker: Arc<dyn Clicker>,
    ) -> zbus::Result<Self> {
        let look = Arc::new(Mutex::new(look));
        let item = Item { window, look: Arc::clone(&look), clicker };
        let connection = builder.name(name.as_str())?.serve_at(PATH, item)?.build()?;
        Ok(Self { connection, name, look })
    }

    pub fn session(name: String, window: u32, look: Look, clicker: Arc<dyn Clicker>) -> zbus::Result<Self> {
        Self::on(zbus::blocking::connection::Builder::session()?, name, window, look, clicker)
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn update(&self, look: Look) -> zbus::Result<()> {
        let before = {
            let mut current = self.look.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::replace(&mut *current, look)
        };
        let look = self.look.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let item = self.connection.object_server().interface::<_, Item>(PATH)?;
        let emitter = item.signal_emitter();
        if before.icon != look.icon {
            zbus::block_on(Item::new_icon(emitter))?;
        }
        if before.title != look.title {
            zbus::block_on(Item::new_title(emitter))?;
            zbus::block_on(Item::new_tool_tip(emitter))?;
        }
        Ok(())
    }
}

pub fn register(connection: &zbus::blocking::Connection, service: &str) -> zbus::Result<()> {
    connection.call_method(Some(WATCHER), WATCHER_PATH, Some(WATCHER), "RegisterStatusNotifierItem", &(service,))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_maps_to_the_x_wheel_buttons() {
        assert_eq!(Click::scroll(-120, "vertical"), Some(Click::ScrollUp));
        assert_eq!(Click::scroll(120, "vertical"), Some(Click::ScrollDown));
        assert_eq!(Click::scroll(-1, "Horizontal"), Some(Click::ScrollLeft));
        assert_eq!(Click::scroll(3, "horizontal"), Some(Click::ScrollRight));
        assert_eq!(Click::scroll(0, "vertical"), None);
        assert_eq!(Click::ScrollDown.button(), 5);
        assert_eq!(Click::Secondary.button(), 3);
    }

    #[test]
    fn an_icon_that_was_never_captured_offers_no_pixmap() {
        assert!(Look::default().pixmaps().is_empty());
        let look = Look { icon: Icon { width: 1, height: 1, argb: vec![255, 1, 2, 3] }, ..Look::default() };
        assert_eq!(look.pixmaps(), vec![(1, 1, vec![255, 1, 2, 3])]);
    }
}
