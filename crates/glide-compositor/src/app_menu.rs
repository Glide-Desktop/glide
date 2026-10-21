use std::collections::HashMap;

use glide_appmenu::{Activation, AppMenuEvent, MenuItem, Source};
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, protocol::wl_surface::WlSurface,
};

use crate::{global_menu::DesktopCommand, state::GlideState};

#[allow(clippy::all, clippy::pedantic, non_upper_case_globals, non_camel_case_types, unused_imports, missing_docs)]
pub mod gtk_shell {
    use wayland_server;
    use wayland_server::protocol::*;

    pub mod __interfaces {
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/gtk-shell.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_server_code!("protocols/gtk-shell.xml");
}

#[allow(clippy::all, clippy::pedantic, non_upper_case_globals, non_camel_case_types, unused_imports, missing_docs)]
pub mod kde_appmenu {
    use wayland_server;
    use wayland_server::protocol::*;

    pub mod __interfaces {
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/appmenu.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_server_code!("protocols/appmenu.xml");
}

use gtk_shell::{gtk_shell1, gtk_surface1};
use kde_appmenu::{org_kde_kwin_appmenu, org_kde_kwin_appmenu_manager};

const GTK_SHELL_VERSION: u32 = 4;
const KDE_APPMENU_VERSION: u32 = 2;
const CAPABILITY_GLOBAL_MENU_BAR: u32 = 1 << (2 - 1);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Owner {
    #[default]
    Desktop,
    App(Source),
    None,
}

#[derive(Debug)]
pub struct AppMenus {
    pub handle: Option<glide_appmenu::AppMenuHandle>,
    pub registrar: bool,
    enabled: bool,
    shells: Vec<gtk_shell1::GtkShell1>,
    surfaces: Vec<(WlSurface, Source)>,
    by_pid: HashMap<u32, Source>,
    registrations: HashMap<u32, (Source, Option<u32>)>,
    x11: crate::x11_menus::X11Bridge,
    cache: HashMap<Source, Vec<MenuItem>>,
    owner: Owner,
}

impl Default for AppMenus {
    fn default() -> Self {
        Self {
            handle: None,
            registrar: true,
            enabled: true,
            shells: Vec::new(),
            surfaces: Vec::new(),
            by_pid: HashMap::new(),
            registrations: HashMap::new(),
            x11: crate::x11_menus::X11Bridge::default(),
            cache: HashMap::new(),
            owner: Owner::Desktop,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Identity<'a> {
    surface: Option<&'a WlSurface>,
    pid: Option<u32>,
    app_id: &'a str,
    title: &'a str,
}

impl AppMenus {
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<gtk_shell1::GtkShell1, ()>
            + GlobalDispatch<org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager, ()>
            + 'static,
    {
        display.create_global::<D, gtk_shell1::GtkShell1, _>(GTK_SHELL_VERSION, ());
        display.create_global::<D, org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager, _>(KDE_APPMENU_VERSION, ());
        Self::default()
    }

    fn capabilities(&self) -> u32 {
        if self.enabled { CAPABILITY_GLOBAL_MENU_BAR } else { 0 }
    }

    #[must_use]
    pub fn is_gtk_client(&self, client: &Client) -> bool {
        self.shells.iter().any(|shell| shell.is_alive() && shell.client().as_ref() == Some(client))
    }

    #[must_use]
    pub fn cached(&self, source: &Source) -> Option<&[MenuItem]> {
        self.cache.get(source).map(Vec::as_slice)
    }

    fn set_source(&mut self, surface: &WlSurface, source: Option<Source>) {
        self.surfaces.retain(|(known, _)| known.is_alive() && known != surface);
        if let Some(source) = source {
            self.surfaces.push((surface.clone(), source));
        }
    }

    fn source_for(&mut self, identity: &Identity<'_>) -> Option<Source> {
        let known = identity
            .surface
            .and_then(|surface| self.surfaces.iter().find(|(known, _)| known == surface))
            .map(|(_, source)| source)
            .or_else(|| identity.pid.and_then(|pid| self.by_pid.get(&pid)));
        if let Some(source) = known {
            return Some(source.clone());
        }
        if identity.app_id.is_empty() && identity.title.is_empty() {
            return None;
        }
        let windows = self.x11.windows();
        let window = crate::x11_menus::match_window(&windows, identity.app_id, identity.title)?.clone();
        self.registrations.get(&window.id).map(|(source, _)| source.clone()).or(window.gtk)
    }
}

fn gtk_source(
    menubar: Option<String>, bus: Option<String>, app: Option<String>, window: Option<String>,
) -> Option<Source> {
    let menubar = menubar.filter(|path| !path.is_empty())?;
    let bus = bus.filter(|name| !name.is_empty())?;
    Some(Source::Gtk {
        bus,
        menubar,
        app: app.filter(|path| !path.is_empty()),
        window: window.filter(|path| !path.is_empty()),
    })
}

impl GlideState {
    pub fn set_global_menu_enabled(&mut self, enabled: bool) {
        if self.app_menus.enabled == enabled {
            return;
        }
        self.app_menus.enabled = enabled;
        let capabilities = self.app_menus.capabilities();
        self.app_menus.shells.retain(Resource::is_alive);
        for shell in &self.app_menus.shells {
            shell.capabilities(capabilities);
        }
        self.refresh_app_menu();
    }

    fn set_app_menu_source(&mut self, surface: &WlSurface, source: Option<Source>) {
        tracing::info!(surface = %surface.id(), ?source, "app menu address");
        self.app_menus.set_source(surface, source);
        if self.refresh_app_menu() {
            self.queue_redraw();
        }
    }

    fn focused_menu_owner(&mut self) -> Owner {
        if !self.app_menus.enabled {
            return Owner::None;
        }
        let Some(window) = self.layout.focused_window().and_then(|id| self.window_map.get(&id)) else {
            return Owner::Desktop;
        };
        let surface = crate::state::window_surface(window);
        let pid = surface
            .client()
            .and_then(|client| client.get_credentials(&self.display_handle).ok())
            .and_then(|credentials| u32::try_from(credentials.pid).ok());
        let (app_id, title) = window.identity();
        let identity = Identity {
            surface: Some(surface),
            pid,
            app_id: app_id.as_deref().unwrap_or_default(),
            title: title.as_deref().unwrap_or_default(),
        };
        self.app_menus.source_for(&identity).map_or(Owner::None, Owner::App)
    }

    pub fn refresh_app_menu(&mut self) -> bool {
        let owner = self.focused_menu_owner();
        if owner != self.app_menus.owner {
            if let (Owner::App(old), Some(handle)) = (&self.app_menus.owner, &self.app_menus.handle) {
                handle.unwatch(old.clone());
            }
            if let (Owner::App(new), Some(handle)) = (&owner, &self.app_menus.handle) {
                handle.watch(new.clone());
            }
            self.app_menus.owner = owner.clone();
        }
        let items = match &owner {
            Owner::Desktop => crate::global_menu::desktop_menu(self.topbar.dnd(), self.topbar.caffeinated()),
            Owner::App(source) => self.app_menus.cache.get(source).cloned().unwrap_or_default(),
            Owner::None => Vec::new(),
        };
        self.topbar.set_app_menu(items)
    }

    pub fn handle_app_menu_event(&mut self, event: AppMenuEvent) {
        match event {
            AppMenuEvent::Menu { source, items } => {
                tracing::info!(?source, titles = items.len(), "app menu received");
                self.app_menus.cache.insert(source, items);
            }
            AppMenuEvent::Gone { source } => {
                tracing::info!(?source, "app menu gone");
                self.app_menus.cache.remove(&source);
            }
            AppMenuEvent::Registered { window_id, source, pid } => {
                tracing::info!(window_id, ?source, ?pid, "app menu registered");
                if let Some(pid) = pid {
                    self.app_menus.by_pid.insert(pid, source.clone());
                }
                self.app_menus.registrations.insert(window_id, (source, pid));
            }
            AppMenuEvent::Unregistered { window_id } => {
                if let Some((_, Some(pid))) = self.app_menus.registrations.remove(&window_id) {
                    self.app_menus.by_pid.remove(&pid);
                }
            }
        }
        self.sync_tray_menu();
        if self.refresh_app_menu() {
            self.queue_redraw();
        }
    }

    fn open_menu_source(&self) -> Option<Source> {
        if self.topbar.tray_menu_id().is_some() {
            return self.tray_menu_source();
        }
        match &self.app_menus.owner {
            Owner::App(source) => Some(source.clone()),
            Owner::Desktop | Owner::None => None,
        }
    }

    fn notify_menu_opened(&self, path: crate::global_menu::MenuPath) {
        let (Some(source), Some(handle)) = (self.open_menu_source(), &self.app_menus.handle) else {
            return;
        };
        if let Some(item) = self.topbar.app_menu_item(path)
            && matches!(item.activation, Activation::DbusMenu(_))
        {
            handle.opened(source, item.activation.clone());
        }
    }

    pub fn open_app_menu(&mut self, index: u16) {
        if self.topbar.open_app_menu(usize::from(index)) {
            self.notify_menu_opened(crate::global_menu::MenuPath::top(usize::from(index)));
        }
    }

    pub fn app_menu_row(&mut self, path: crate::global_menu::MenuPath) {
        match self.topbar.app_menu_row(path) {
            crate::topbar::MenuRowOutcome::Submenu => self.notify_menu_opened(path),
            crate::topbar::MenuRowOutcome::Activate(activation) => {
                self.activate_app_menu(activation);
            }
            crate::topbar::MenuRowOutcome::Ignored => {}
        }
    }

    fn activate_app_menu(&mut self, activation: Activation) {
        if self.topbar.tray_menu_id().is_some() {
            if let (Some(source), Some(handle)) = (self.tray_menu_source(), &self.app_menus.handle) {
                handle.activate(source, activation);
            }
            return;
        }
        match self.app_menus.owner.clone() {
            Owner::Desktop => {
                if let Some(command) = DesktopCommand::from_activation(&activation) {
                    self.run_desktop_command(command);
                }
            }
            Owner::App(source) => {
                if let Some(handle) = &self.app_menus.handle {
                    handle.activate(source, activation);
                }
            }
            Owner::None => {}
        }
    }

    fn run_desktop_command(&mut self, command: DesktopCommand) {
        use crate::{input::KeyAction, topbar::TopbarAction};
        match command {
            DesktopCommand::SystemSettings => crate::system_info::open_settings(),
            DesktopCommand::LockScreen => self.lock_screen(),
            DesktopCommand::NewTerminal => self.open_terminal_tab(),
            DesktopCommand::OpenHome => {
                let _ = self.spawn_user_command("xdg-open \"$HOME\"");
            }
            DesktopCommand::Screenshot => self.open_shot_after_menus(),
            DesktopCommand::Overview => self.run_key_action(KeyAction::ToggleOverview),
            DesktopCommand::Launcher => self.run_key_action(KeyAction::ToggleLauncher),
            DesktopCommand::DoNotDisturb => {
                self.run_topbar_action(TopbarAction::ToggleDoNotDisturb);
            }
            DesktopCommand::Caffeine => self.run_topbar_action(TopbarAction::ToggleCaffeine),
            DesktopCommand::PreviousWorkspace => self.run_key_action(KeyAction::PrevWorkspace),
            DesktopCommand::NextWorkspace => self.run_key_action(KeyAction::NextWorkspace),
        }
        self.refresh_app_menu();
        self.queue_redraw();
    }
}

impl GlobalDispatch<gtk_shell1::GtkShell1, ()> for GlideState {
    fn bind(
        state: &mut Self, _handle: &DisplayHandle, _client: &Client, resource: New<gtk_shell1::GtkShell1>,
        _global_data: &(), data_init: &mut DataInit<'_, Self>,
    ) {
        let shell = data_init.init(resource, ());
        shell.capabilities(state.app_menus.capabilities());
        state.app_menus.shells.retain(Resource::is_alive);
        state.app_menus.shells.push(shell);
    }
}

impl Dispatch<gtk_shell1::GtkShell1, ()> for GlideState {
    fn request(
        _state: &mut Self, _client: &Client, _shell: &gtk_shell1::GtkShell1, request: gtk_shell1::Request, _data: &(),
        _dh: &DisplayHandle, data_init: &mut DataInit<'_, Self>,
    ) {
        if let gtk_shell1::Request::GetGtkSurface { gtk_surface, surface } = request {
            data_init.init(gtk_surface, surface);
        }
    }
}

impl Dispatch<gtk_surface1::GtkSurface1, WlSurface> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, _resource: &gtk_surface1::GtkSurface1, request: gtk_surface1::Request,
        surface: &WlSurface, _dh: &DisplayHandle, _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            gtk_surface1::Request::SetDbusProperties {
                menubar_path,
                window_object_path,
                application_object_path,
                unique_bus_name,
                ..
            } => {
                let source = gtk_source(menubar_path, unique_bus_name, application_object_path, window_object_path);
                state.set_app_menu_source(surface, source);
            }
            gtk_surface1::Request::Release => state.set_app_menu_source(surface, None),
            _ => {}
        }
    }
}

impl GlobalDispatch<org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager, ()> for GlideState {
    fn bind(
        _state: &mut Self, _handle: &DisplayHandle, _client: &Client,
        resource: New<org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager>, _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager, ()> for GlideState {
    fn request(
        _state: &mut Self, _client: &Client, _manager: &org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager,
        request: org_kde_kwin_appmenu_manager::Request, _data: &(), _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let org_kde_kwin_appmenu_manager::Request::Create { id, surface } = request {
            data_init.init(id, surface);
        }
    }
}

impl Dispatch<org_kde_kwin_appmenu::OrgKdeKwinAppmenu, WlSurface> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, _resource: &org_kde_kwin_appmenu::OrgKdeKwinAppmenu,
        request: org_kde_kwin_appmenu::Request, surface: &WlSurface, _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            org_kde_kwin_appmenu::Request::SetAddress { service_name, object_path } => {
                let valid = !service_name.is_empty() && !object_path.is_empty();
                let source = valid.then_some(Source::DbusMenu { bus: service_name, path: object_path });
                state.set_app_menu_source(surface, source);
            }
            org_kde_kwin_appmenu::Request::Release => state.set_app_menu_source(surface, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capability_bit_matches_gtks_one_based_shift() {
        assert_eq!(CAPABILITY_GLOBAL_MENU_BAR, 2);
        let menus = AppMenus::default();
        assert_eq!(menus.capabilities(), 2);
        let off = AppMenus { enabled: false, ..AppMenus::default() };
        assert_eq!(off.capabilities(), 0, "a disabled menu bar must not hide in-app menus");
    }

    #[test]
    fn gtk_needs_both_a_menubar_path_and_a_bus_name() {
        let some = |s: &str| Some(s.to_string());
        assert_eq!(gtk_source(None, some(":1.4"), None, None), None);
        assert_eq!(gtk_source(some("/m"), None, None, None), None);
        assert_eq!(gtk_source(some(""), some(":1.4"), None, None), None);
        assert_eq!(
            gtk_source(some("/m"), some(":1.4"), some("/app"), some("")),
            Some(Source::Gtk { bus: ":1.4".into(), menubar: "/m".into(), app: Some("/app".into()), window: None })
        );
    }
}
