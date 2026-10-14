use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use glide_appmenu::{Activation, MenuItem, Source};
use smithay::reexports::calloop::channel;

use crate::{
    config::Color,
    icon::IconLoader,
    state::GlideState,
    system_info::ScrollDelta,
    topbar::{IconArt, TRAY_ICON, TRAY_MAX_SCALE, TrayButton, TrayClick, TrayIcon},
};

const MENU_WAIT: Duration = Duration::from_secs(2);
const THEME_DIR_DEPTH: usize = 5;
const WHEEL_STEP: f64 = 120.0;
const PIXELS_PER_STEP: f64 = 15.0;

#[derive(Debug)]
pub enum TrayUpdate {
    Icon(Box<TrayIcon>),
    Removed(String),
}

#[derive(Debug, Default)]
pub struct TrayService {
    handle: Option<glide_tray::TrayHandle>,
    started: bool,
    icon_theme: Arc<Mutex<Option<String>>>,
    menus: HashMap<String, Source>,
    pending: Option<(u16, Source, Instant)>,
    open: Option<Source>,
}

fn find_in_theme_dir(dir: &Path, name: &str, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                found.extend(find_in_theme_dir(&path, name, depth - 1));
            }
            continue;
        }
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        let ext = path.extension().and_then(|ext| ext.to_str());
        if stem == Some(name) && matches!(ext, Some("svg" | "png")) {
            found.push(path);
        }
    }
    found
}

fn themed_icon(dir: &str, name: &str) -> Option<PathBuf> {
    if dir.trim().is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    let found = find_in_theme_dir(Path::new(dir), name, THEME_DIR_DEPTH);
    let svg = found.iter().find(|path| path.extension().is_some_and(|ext| ext == "svg"));
    svg.cloned().or_else(|| found.into_iter().max_by_key(|path| std::fs::metadata(path).map_or(0, |meta| meta.len())))
}

fn into_mask(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3];
        pixel[..3].fill(alpha);
    }
}

#[must_use]
pub fn build_icon(item: &glide_tray::TrayItem, loader: &mut IconLoader) -> TrayIcon {
    let props = &item.props;
    let (name, pixmaps) = props.shown_icon();
    let name = name.trim();
    let themed = themed_icon(&props.icon_theme_path, name);
    let declared_symbolic = name.ends_with("-symbolic");
    let white = Color([1.0; 4]);
    let mut rasters = Vec::new();
    for scale in 1..=TRAY_MAX_SCALE {
        let side = TRAY_ICON * scale;
        let named = || {
            if name.is_empty() {
                return None;
            }
            let pixels =
                if declared_symbolic { loader.load_symbolic(name, side, white) } else { loader.load_named(name, side) };
            pixels.map(<[u8]>::to_vec)
        };
        let raster = themed
            .as_deref()
            .and_then(|path| crate::icon::render(path, side, None))
            .or_else(named)
            .or_else(|| glide_tray::pixmap_square(pixmaps, side));
        if let Some(raster) = raster {
            rasters.push((scale, raster));
        }
    }
    let symbolic = declared_symbolic || rasters.first().is_some_and(|(_, raster)| glide_tray::is_monochrome(raster));
    if symbolic {
        for (_, raster) in &mut rasters {
            into_mask(raster);
        }
    }
    TrayIcon {
        key: item.key.clone(),
        bus: item.address.bus.clone(),
        label: props.label().to_string(),
        menu: props.menu.clone(),
        item_is_menu: props.item_is_menu,
        status: props.status,
        art: IconArt { symbolic, rasters },
    }
}

#[must_use]
pub fn scroll_amount(delta: ScrollDelta) -> Option<i32> {
    let steps = match delta {
        ScrollDelta::Wheel(v120) => v120,
        ScrollDelta::Continuous(pixels) => pixels * WHEEL_STEP / PIXELS_PER_STEP,
        ScrollDelta::Stop => return None,
    };
    let amount = (-steps).round() as i32;
    (amount != 0).then_some(amount)
}

#[must_use]
pub fn wants_menu(icon: &TrayIcon, button: TrayButton) -> bool {
    match button {
        TrayButton::Context => true,
        TrayButton::Primary => icon.item_is_menu,
        TrayButton::Secondary => false,
    }
}

impl GlideState {
    pub fn configure_tray(&mut self, topbar: &crate::config::TopbarConfig, icon_theme: Option<String>) {
        let enabled = topbar.enabled && topbar.tray;
        if self.topbar.set_tray_enabled(enabled) {
            self.queue_redraw();
        }
        let theme_changed = {
            let mut current = self.tray.icon_theme.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let changed = *current != icon_theme;
            *current = icon_theme;
            changed
        };
        if enabled && !self.tray.started {
            self.start_tray();
        } else if theme_changed && let Some(handle) = &self.tray.handle {
            handle.refresh();
        }
    }

    fn start_tray(&mut self) {
        self.tray.started = true;
        let (sender, receiver) = channel::channel::<TrayUpdate>();
        let sender = Mutex::new(sender);
        let theme = self.tray.icon_theme.clone();
        let loader: Mutex<Option<(Option<String>, IconLoader)>> = Mutex::new(None);
        let sink: glide_tray::Sink = Arc::new(move |event| {
            let update = match event {
                glide_tray::TrayEvent::Item(item) => {
                    let wanted = theme.lock().map(|theme| theme.clone()).unwrap_or_default();
                    let Ok(mut guard) = loader.lock() else {
                        return;
                    };
                    let loader = match guard.as_mut() {
                        Some((theme, loader)) if *theme == wanted => loader,
                        _ => {
                            let fresh = IconLoader::new(wanted.clone());
                            &mut guard.insert((wanted, fresh)).1
                        }
                    };
                    TrayUpdate::Icon(Box::new(build_icon(&item, loader)))
                }
                glide_tray::TrayEvent::Removed { key } => TrayUpdate::Removed(key),
            };
            let Ok(sender) = sender.lock() else {
                return;
            };
            if let Err(err) = sender.send(update) {
                tracing::debug!("dropping a tray update, the compositor is gone: {err}");
            }
        });
        match glide_tray::start(sink) {
            Ok(handle) => self.tray.handle = Some(handle),
            Err(err) => {
                tracing::warn!("the system tray is unavailable: {err}");
                return;
            }
        }
        let inserted = self.loop_handle.insert_source(receiver, |event, (), state| {
            if let channel::Event::Msg(update) = event {
                state.handle_tray_update(update);
            }
        });
        if let Err(err) = inserted {
            tracing::error!("failed to watch the tray channel: {err}");
        }
    }

    pub fn handle_tray_update(&mut self, update: TrayUpdate) {
        let dirty = match update {
            TrayUpdate::Icon(icon) => self.topbar.upsert_tray_item(*icon),
            TrayUpdate::Removed(key) => {
                if let Some(source) = self.tray.menus.remove(&key)
                    && let Some(handle) = &self.app_menus.handle
                {
                    handle.unwatch(source);
                }
                let id = self.topbar.remove_tray_item(&key);
                if id.is_some() && self.tray.pending.as_ref().map(|(p, _, _)| *p) == id {
                    self.tray.pending = None;
                }
                id.is_some()
            }
        };
        if dirty {
            self.queue_redraw();
        }
    }

    fn pointer_xy(&self) -> (i32, i32) {
        let pos = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        (pos.x.round() as i32, pos.y.round() as i32)
    }

    pub fn run_tray_click(&mut self, click: TrayClick) {
        let Some(icon) = self.topbar.tray_item(click.id).cloned() else {
            return;
        };
        self.topbar.suppress_tray_tooltip();
        let menu = wants_menu(&icon, click.button);
        if self.topbar.tray_menu_showing(click.id) {
            self.topbar.close_popup();
            if menu {
                return;
            }
        }
        if menu && self.open_tray_menu(click.id, &icon) {
            return;
        }
        self.topbar.close_popup();
        let Some(handle) = self.tray.handle.clone() else {
            return;
        };
        let (x, y) = self.pointer_xy();
        match click.button {
            TrayButton::Primary => handle.activate(&icon.key, x, y),
            TrayButton::Secondary => handle.secondary_activate(&icon.key, x, y),
            TrayButton::Context => handle.context_menu(&icon.key, x, y),
        }
    }

    pub fn scroll_tray(&mut self, id: u16, delta: ScrollDelta) {
        let (Some(icon), Some(handle), Some(amount)) =
            (self.topbar.tray_item(id), self.tray.handle.as_ref(), scroll_amount(delta))
        else {
            return;
        };
        handle.scroll(&icon.key, amount, true);
    }

    fn open_tray_menu(&mut self, id: u16, icon: &TrayIcon) -> bool {
        let (Some(path), Some(handle)) = (icon.menu.clone(), self.app_menus.handle.clone()) else {
            return false;
        };
        let source = Source::DbusMenu { bus: icon.bus.clone(), path };
        handle.watch(source.clone());
        handle.opened(source.clone(), Activation::DbusMenu(0));
        self.tray.menus.insert(icon.key.clone(), source.clone());
        match self.app_menus.cached(&source).map(<[MenuItem]>::to_vec) {
            Some(items) if !items.is_empty() => {
                self.tray.pending = None;
                if self.topbar.open_tray_menu(id, items) {
                    self.tray.open = Some(source);
                }
            }
            _ => self.tray.pending = Some((id, source, Instant::now())),
        }
        true
    }

    pub fn sync_tray_menu(&mut self) {
        if let Some((id, source, since)) = self.tray.pending.clone() {
            if since.elapsed() > MENU_WAIT {
                self.tray.pending = None;
            } else if let Some(items) =
                self.app_menus.cached(&source).filter(|items| !items.is_empty()).map(<[MenuItem]>::to_vec)
            {
                self.tray.pending = None;
                if self.topbar.open_tray_menu(id, items) {
                    self.tray.open = Some(source);
                    self.queue_redraw();
                }
                return;
            }
        }
        if let (Some(id), Some(source)) = (self.topbar.tray_menu_id(), self.tray.open.clone())
            && let Some(items) = self.app_menus.cached(&source).map(<[MenuItem]>::to_vec)
            && self.topbar.set_tray_menu_items(id, items)
        {
            self.queue_redraw();
        }
    }

    #[must_use]
    pub fn tray_menu_source(&self) -> Option<Source> {
        self.topbar.tray_menu_id()?;
        self.tray.open.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(item_is_menu: bool) -> TrayIcon {
        TrayIcon {
            key: ":1.2/StatusNotifierItem".into(),
            bus: ":1.2".into(),
            label: "App".into(),
            menu: Some("/Menu".into()),
            item_is_menu,
            status: glide_tray::Status::Active,
            art: IconArt::default(),
        }
    }

    #[test]
    fn right_click_always_asks_for_the_menu_and_left_only_for_menu_items() {
        assert!(wants_menu(&icon(false), TrayButton::Context));
        assert!(!wants_menu(&icon(false), TrayButton::Primary));
        assert!(wants_menu(&icon(true), TrayButton::Primary));
        assert!(!wants_menu(&icon(true), TrayButton::Secondary));
    }

    #[test]
    fn wheel_notches_become_sni_angle_deltas_with_up_positive() {
        assert_eq!(scroll_amount(ScrollDelta::Wheel(120.0)), Some(-120));
        assert_eq!(scroll_amount(ScrollDelta::Wheel(-240.0)), Some(240));
        assert_eq!(scroll_amount(ScrollDelta::Continuous(15.0)), Some(-120));
        assert_eq!(scroll_amount(ScrollDelta::Stop), None);
        assert_eq!(scroll_amount(ScrollDelta::Wheel(0.2)), None);
    }

    #[test]
    fn a_mask_keeps_only_coverage() {
        let mut pixels = vec![10, 20, 30, 200, 0, 0, 0, 0];
        into_mask(&mut pixels);
        assert_eq!(pixels, vec![200, 200, 200, 200, 0, 0, 0, 0]);
    }

    #[test]
    fn icon_theme_paths_are_searched_before_the_theme() {
        let dir = std::env::temp_dir().join(format!("glide-tray-theme-{}", std::process::id()));
        let nested = dir.join("hicolor").join("22x22").join("apps");
        std::fs::create_dir_all(&nested).expect("temp dir");
        std::fs::write(nested.join("my-app.png"), b"png").expect("write");
        std::fs::write(nested.join("my-app.svg"), b"svg").expect("write");
        std::fs::write(nested.join("other.svg"), b"svg").expect("write");
        let found = themed_icon(dir.to_str().expect("utf-8"), "my-app");
        assert_eq!(found, Some(nested.join("my-app.svg")), "vector art wins");
        assert_eq!(themed_icon(dir.to_str().expect("utf-8"), "missing"), None);
        assert_eq!(themed_icon("", "my-app"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pixmap_only_items_get_a_raster_per_scale() {
        let props = glide_tray::ItemProps {
            id: "app".into(),
            icon_pixmaps: vec![glide_tray::Pixmap {
                width: 2,
                height: 2,
                argb: [255u8, 200, 30, 30, 255, 30, 200, 30].repeat(2),
            }],
            ..glide_tray::ItemProps::default()
        };
        let item = glide_tray::TrayItem {
            key: ":1.2/StatusNotifierItem".into(),
            address: glide_tray::Address { bus: ":1.2".into(), path: "/StatusNotifierItem".into() },
            props,
        };
        let built = build_icon(&item, &mut IconLoader::new(None));
        assert_eq!(built.label, "app");
        assert!(!built.art.symbolic, "coloured art is drawn as-is");
        let scales: Vec<i32> = built.art.rasters.iter().map(|(scale, _)| *scale).collect();
        assert_eq!(scales, (1..=TRAY_MAX_SCALE).collect::<Vec<_>>());
        for (scale, raster) in &built.art.rasters {
            let side = usize::try_from(TRAY_ICON * scale).expect("positive");
            assert_eq!(raster.len(), side * side * 4);
        }
    }
}
