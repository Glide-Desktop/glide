use std::sync::{Arc, Mutex, PoisonError};

use anyhow::{Context, anyhow, bail};
use x11rb::{
    CURRENT_TIME, NONE,
    connection::Connection,
    protocol::{
        Event,
        damage::{self, ConnectionExt as _},
        xfixes::ConnectionExt as _,
        xproto::{
            Atom, AtomEnum, ButtonPressEvent, ButtonReleaseEvent, ChangeWindowAttributesAux, ClientMessageEvent,
            ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, EventMask, ImageFormat, KeyButMask,
            PropMode, Window, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use crate::{
    pixmap::{self, Icon},
    sni::{self, Click, Clicker, Exported, Look},
};

pub const CONTAINER_CLASS: &str = "glide-xembed";
pub const ICON_SIZE: u16 = 32;
const SYSTEM_TRAY_REQUEST_DOCK: u32 = 0;
const XEMBED_EMBEDDED_NOTIFY: u32 = 0;
const XEMBED_MAPPED: u32 = 1;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_SYSTEM_TRAY_OPCODE,
        _NET_SYSTEM_TRAY_ORIENTATION,
        _NET_SYSTEM_TRAY_VISUAL,
        _NET_WM_NAME,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NORMAL,
        _XEMBED,
        _XEMBED_INFO,
        MANAGER,
        UTF8_STRING,
    }
}

pub type Registered = Arc<Mutex<Vec<String>>>;

struct XClicker {
    conn: Arc<RustConnection>,
    root: Window,
}

impl XClicker {
    fn send(&self, window: Window, click: Click) -> anyhow::Result<()> {
        let half = i16::try_from(ICON_SIZE / 2).unwrap_or(0);
        let at = self.conn.translate_coordinates(window, self.root, half, half)?.reply()?;
        let button = click.button();
        let press = ButtonPressEvent {
            response_type: x11rb::protocol::xproto::BUTTON_PRESS_EVENT,
            detail: button,
            sequence: 0,
            time: CURRENT_TIME,
            root: self.root,
            event: window,
            child: NONE,
            root_x: at.dst_x,
            root_y: at.dst_y,
            event_x: half,
            event_y: half,
            state: KeyButMask::default(),
            same_screen: true,
        };
        let held = match button {
            1 => KeyButMask::BUTTON1,
            2 => KeyButMask::BUTTON2,
            3 => KeyButMask::BUTTON3,
            4 => KeyButMask::BUTTON4,
            _ => KeyButMask::BUTTON5,
        };
        let release =
            ButtonReleaseEvent { response_type: x11rb::protocol::xproto::BUTTON_RELEASE_EVENT, state: held, ..press };
        self.conn.send_event(false, window, EventMask::BUTTON_PRESS, press)?;
        self.conn.send_event(false, window, EventMask::BUTTON_RELEASE, release)?;
        self.conn.flush()?;
        Ok(())
    }
}

impl Clicker for XClicker {
    fn click(&self, window: u32, click: Click) {
        if let Err(err) = self.send(window, click) {
            tracing::debug!(window, ?click, "could not pass a click to a tray icon: {err}");
        }
    }
}

struct Docked {
    window: Window,
    damage: u32,
    look: Look,
    exported: Option<Exported>,
}

pub struct Host {
    conn: Arc<RustConnection>,
    root: Window,
    atoms: Atoms,
    selection: Atom,
    owner: Window,
    container: Window,
    clicker: Arc<XClicker>,
    control: zbus::blocking::Connection,
    bus: Option<String>,
    registered: Registered,
    icons: Vec<Docked>,
    serial: u32,
}

fn argb_visual(screen: &x11rb::protocol::xproto::Screen) -> Option<u32> {
    screen
        .allowed_depths
        .iter()
        .filter(|depth| depth.depth == 32)
        .flat_map(|depth| depth.visuals.iter())
        .find(|visual| visual.class == x11rb::protocol::xproto::VisualClass::TRUE_COLOR)
        .map(|visual| visual.visual_id)
}

fn text_property(conn: &RustConnection, window: Window, property: Atom, kind: Atom) -> Option<String> {
    let reply = conn.get_property(false, window, property, kind, 0, 1024).ok()?.reply().ok()?;
    let text = String::from_utf8_lossy(&reply.value).trim_end_matches('\0').trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn wm_class(conn: &RustConnection, window: Window) -> Option<String> {
    let reply = conn.get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024).ok()?.reply().ok()?;
    let mut fields = reply.value.split(|byte| *byte == 0).filter(|field| !field.is_empty());
    let instance = fields.next();
    let class = fields.next().or(instance)?;
    Some(String::from_utf8_lossy(class).into_owned())
}

#[must_use]
pub fn xembed_mapped(info: Option<&[u32]>) -> bool {
    info.and_then(|info| info.get(1)).is_none_or(|flags| flags & XEMBED_MAPPED != 0)
}

impl Host {
    pub fn connect(
        display: Option<&str>, bus: Option<String>, control: zbus::blocking::Connection, registered: Registered,
    ) -> anyhow::Result<Self> {
        let (conn, screen_num) = x11rb::connect(display).context("no X display")?;
        let conn = Arc::new(conn);
        let screen = conn.setup().roots.get(screen_num).ok_or_else(|| anyhow!("no X screen {screen_num}"))?.clone();
        let root = screen.root;
        let atoms = Atoms::new(conn.as_ref())?.reply()?;
        let selection = conn.intern_atom(false, format!("_NET_SYSTEM_TRAY_S{screen_num}").as_bytes())?.reply()?.atom;
        let current = conn.get_selection_owner(selection)?.reply()?.owner;
        if current != NONE {
            bail!("another tray owns the X11 system tray");
        }
        conn.damage_query_version(1, 1)?.reply().context("the X server has no DAMAGE extension")?;
        conn.xfixes_query_version(5, 0)?.reply().context("the X server has no XFIXES extension")?;

        let visual = argb_visual(&screen).ok_or_else(|| anyhow!("the X server offers no 32-bit visual"))?;
        let colormap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual)?;
        let container = conn.generate_id()?;
        conn.create_window(
            32,
            container,
            root,
            0,
            0,
            ICON_SIZE,
            ICON_SIZE,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
        )?;
        let class = format!("{CONTAINER_CLASS}\0{CONTAINER_CLASS}\0");
        conn.change_property8(PropMode::REPLACE, container, AtomEnum::WM_CLASS, AtomEnum::STRING, class.as_bytes())?;
        conn.change_property8(
            PropMode::REPLACE,
            container,
            atoms._NET_WM_NAME,
            atoms.UTF8_STRING,
            CONTAINER_CLASS.as_bytes(),
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            container,
            atoms._NET_WM_WINDOW_TYPE,
            AtomEnum::ATOM,
            &[atoms._NET_WM_WINDOW_TYPE_NORMAL],
        )?;

        let owner = conn.generate_id()?;
        conn.create_window(
            0,
            owner,
            root,
            -1,
            -1,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new().override_redirect(1),
        )?;
        conn.change_property32(PropMode::REPLACE, owner, atoms._NET_SYSTEM_TRAY_ORIENTATION, AtomEnum::CARDINAL, &[0])?;
        conn.change_property32(PropMode::REPLACE, owner, atoms._NET_SYSTEM_TRAY_VISUAL, AtomEnum::VISUALID, &[visual])?;
        conn.set_selection_owner(owner, selection, CURRENT_TIME)?;
        if conn.get_selection_owner(selection)?.reply()?.owner != owner {
            bail!("could not take the X11 system tray selection");
        }
        let announce = ClientMessageEvent::new(32, root, atoms.MANAGER, [CURRENT_TIME, selection, owner, 0, 0]);
        conn.send_event(false, root, EventMask::STRUCTURE_NOTIFY, announce)?;
        conn.flush()?;
        tracing::info!(screen = screen_num, "owning the X11 system tray");

        let clicker = Arc::new(XClicker { conn: Arc::clone(&conn), root });
        Ok(Self {
            conn,
            root,
            atoms,
            selection,
            owner,
            container,
            clicker,
            control,
            bus,
            registered,
            icons: Vec::new(),
            serial: 0,
        })
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        loop {
            let event = self.conn.wait_for_event()?;
            self.handle(event)?;
            while let Some(event) = self.conn.poll_for_event()? {
                self.handle(event)?;
            }
            self.conn.flush()?;
        }
    }

    fn handle(&mut self, event: Event) -> anyhow::Result<()> {
        match event {
            Event::ClientMessage(message) if message.window == self.owner => {
                let data = message.data.as_data32();
                if message.type_ == self.atoms._NET_SYSTEM_TRAY_OPCODE && data[1] == SYSTEM_TRAY_REQUEST_DOCK {
                    self.dock(data[2]);
                }
            }
            Event::SelectionClear(clear) if clear.selection == self.selection => {
                bail!("another tray took the X11 system tray");
            }
            Event::DestroyNotify(destroyed) => self.undock(destroyed.window),
            Event::ReparentNotify(moved) if moved.parent != self.container => self.undock(moved.window),
            Event::PropertyNotify(changed) => {
                if changed.atom == self.atoms._XEMBED_INFO {
                    self.sync_visibility(changed.window);
                } else if changed.atom == self.atoms._NET_WM_NAME || changed.atom == u32::from(AtomEnum::WM_NAME) {
                    self.refresh(changed.window);
                }
            }
            Event::ConfigureNotify(configured)
                if self.position(configured.window).is_some()
                    && (configured.width != ICON_SIZE || configured.height != ICON_SIZE) =>
            {
                let size = ConfigureWindowAux::new().width(u32::from(ICON_SIZE)).height(u32::from(ICON_SIZE));
                self.conn.configure_window(configured.window, &size)?;
            }
            Event::DamageNotify(notice) => {
                self.conn.damage_subtract(notice.damage, NONE, NONE)?;
                self.refresh(notice.drawable);
            }
            Event::Error(err) => tracing::debug!(?err, "X11 request failed"),
            _ => {}
        }
        Ok(())
    }

    fn position(&self, window: Window) -> Option<usize> {
        self.icons.iter().position(|icon| icon.window == window)
    }

    fn xembed_info(&self, window: Window) -> Option<Vec<u32>> {
        let reply = self
            .conn
            .get_property(false, window, self.atoms._XEMBED_INFO, self.atoms._XEMBED_INFO, 0, 2)
            .ok()?
            .reply()
            .ok()?;
        let values: Vec<u32> = reply.value32()?.collect();
        (!values.is_empty()).then_some(values)
    }

    fn dock(&mut self, window: Window) {
        if window == NONE || self.position(window).is_some() {
            return;
        }
        if let Err(err) = self.embed(window) {
            tracing::info!(window, "could not embed a tray icon: {err}");
        }
    }

    fn embed(&mut self, window: Window) -> anyhow::Result<()> {
        let events = EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY;
        self.conn.change_window_attributes(window, &ChangeWindowAttributesAux::new().event_mask(events))?.check()?;
        let info = self.xembed_info(window);
        let x = i16::try_from(self.icons.len() * usize::from(ICON_SIZE)).unwrap_or(i16::MAX);
        self.conn.reparent_window(window, self.container, x, 0)?.check()?;
        let size = ConfigureWindowAux::new().width(u32::from(ICON_SIZE)).height(u32::from(ICON_SIZE));
        self.conn.configure_window(window, &size)?;
        let notify = ClientMessageEvent::new(
            32,
            window,
            self.atoms._XEMBED,
            [CURRENT_TIME, XEMBED_EMBEDDED_NOTIFY, 0, self.container, 0],
        );
        self.conn.send_event(false, window, EventMask::NO_EVENT, notify)?;
        if xembed_mapped(info.as_deref()) {
            self.conn.map_window(window)?;
        }
        let damage = self.conn.generate_id()?;
        self.conn.damage_create(damage, window, damage::ReportLevel::NON_EMPTY)?;
        self.serial += 1;
        let id = wm_class(&self.conn, window).unwrap_or_else(|| format!("xembed-{}", self.serial));
        self.icons.push(Docked { window, damage, look: Look { id, ..Look::default() }, exported: None });
        self.arrange()?;
        tracing::info!(window, "embedded an X11 tray icon");
        self.refresh(window);
        Ok(())
    }

    fn undock(&mut self, window: Window) {
        let Some(at) = self.position(window) else {
            return;
        };
        let icon = self.icons.remove(at);
        let _ = self.conn.damage_destroy(icon.damage);
        if let Some(exported) = icon.exported {
            self.forget(exported.name());
        }
        tracing::info!(window, "an X11 tray icon went away");
        if let Err(err) = self.arrange() {
            tracing::debug!("could not rearrange the tray container: {err}");
        }
    }

    fn arrange(&self) -> anyhow::Result<()> {
        for (index, icon) in self.icons.iter().enumerate() {
            let x = i32::try_from(index * usize::from(ICON_SIZE)).unwrap_or(i32::MAX);
            self.conn.configure_window(icon.window, &ConfigureWindowAux::new().x(x).y(0))?;
        }
        if self.icons.is_empty() {
            self.conn.unmap_window(self.container)?;
            return Ok(());
        }
        let width = u32::try_from(self.icons.len()).unwrap_or(1) * u32::from(ICON_SIZE);
        self.conn.configure_window(self.container, &ConfigureWindowAux::new().width(width))?;
        self.conn.map_window(self.container)?;
        Ok(())
    }

    fn sync_visibility(&mut self, window: Window) {
        if self.position(window).is_none() {
            return;
        }
        let mapped = xembed_mapped(self.xembed_info(window).as_deref());
        let result =
            if mapped { self.conn.map_window(window).map(drop) } else { self.conn.unmap_window(window).map(drop) };
        if let Err(err) = result {
            tracing::debug!(window, "could not follow the icon's XEMBED state: {err}");
        }
        if mapped {
            self.refresh(window);
        } else if let Some(at) = self.position(window)
            && let Some(exported) = self.icons[at].exported.take()
        {
            self.forget(exported.name());
        }
    }

    fn capture(&self, window: Window) -> Option<Icon> {
        let reply = self
            .conn
            .get_image(ImageFormat::Z_PIXMAP, window, 0, 0, ICON_SIZE, ICON_SIZE, u32::MAX)
            .ok()?
            .reply()
            .ok()?;
        pixmap::from_zpixmap(&reply.data, ICON_SIZE, ICON_SIZE, reply.depth)
    }

    fn refresh(&mut self, window: Window) {
        let Some(at) = self.position(window) else {
            return;
        };
        if !xembed_mapped(self.xembed_info(window).as_deref()) {
            return;
        }
        let title = text_property(&self.conn, window, self.atoms._NET_WM_NAME, self.atoms.UTF8_STRING)
            .or_else(|| text_property(&self.conn, window, AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
            .unwrap_or_default();
        let icon =
            self.capture(window).filter(|icon| !icon.is_blank()).unwrap_or_else(|| self.icons[at].look.icon.clone());
        let look = Look { id: self.icons[at].look.id.clone(), title, icon };
        if look.icon.argb.is_empty() {
            return;
        }
        if self.icons[at].exported.is_none() {
            self.export(at, look);
            return;
        }
        if self.icons[at].look == look {
            return;
        }
        self.icons[at].look = look.clone();
        if let Some(exported) = self.icons[at].exported.as_ref()
            && let Err(err) = exported.update(look)
        {
            tracing::debug!(window, "could not announce a tray icon change: {err}");
        }
    }

    fn export(&mut self, at: usize, look: Look) {
        self.serial += 1;
        let name = format!("org.kde.StatusNotifierItem-{}-{}", std::process::id(), self.serial);
        let clicker: Arc<dyn Clicker> = Arc::clone(&self.clicker) as Arc<dyn Clicker>;
        let window = self.icons[at].window;
        let builder = match self.bus.as_deref() {
            Some(address) => zbus::blocking::connection::Builder::address(address),
            None => zbus::blocking::connection::Builder::session(),
        };
        let exported = builder.and_then(|builder| Exported::on(builder, name.clone(), window, look.clone(), clicker));
        match exported {
            Ok(exported) => {
                self.icons[at].look = look;
                self.icons[at].exported = Some(exported);
                self.registered.lock().unwrap_or_else(PoisonError::into_inner).push(name.clone());
                if let Err(err) = sni::register(&self.control, &name) {
                    tracing::info!(name, "no StatusNotifierWatcher yet: {err}");
                }
            }
            Err(err) => tracing::warn!(window, "could not export a tray icon: {err}"),
        }
    }

    fn forget(&self, name: &str) {
        self.registered.lock().unwrap_or_else(PoisonError::into_inner).retain(|known| known != name);
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        for icon in self.icons.drain(..) {
            let _ = self.conn.reparent_window(icon.window, self.root, 0, 0);
            if let Some(exported) = icon.exported {
                self.registered.lock().unwrap_or_else(PoisonError::into_inner).retain(|known| known != exported.name());
            }
        }
        let _ = self.conn.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_is_shown_unless_its_xembed_info_says_it_is_unmapped() {
        assert!(xembed_mapped(None), "no _XEMBED_INFO: map it, as every tray does");
        assert!(xembed_mapped(Some(&[0, 1])));
        assert!(!xembed_mapped(Some(&[0, 0])));
        assert!(xembed_mapped(Some(&[0])), "a truncated property is not a request to hide");
    }
}
