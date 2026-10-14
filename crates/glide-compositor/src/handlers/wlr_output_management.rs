use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use smithay::desktop::Space;
use smithay::output::{Mode, Output};
use smithay::reexports::wayland_protocols_wlr::output_management::v1::server::{
    zwlr_output_configuration_head_v1::{self, ZwlrOutputConfigurationHeadV1},
    zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
    zwlr_output_head_v1::{self, AdaptiveSyncState, ZwlrOutputHeadV1},
    zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
    zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
};
use smithay::reexports::wayland_server::{
    Client, DataInit, DisplayHandle, GlobalDispatch, New, Resource, WEnum, backend::GlobalId,
};
use smithay::utils::{Point, Transform};
use smithay::wayland::GlobalData;

use crate::{GlideState, window::GlideWindow};

pub trait WlrOutputManagerHandler {
    fn wlr_output_manager_state(&mut self) -> &mut WlrOutputManagerState;
    fn apply_output_config(&mut self, config: PendingOutputConfig) -> bool;
}

impl WlrOutputManagerHandler for GlideState {
    fn wlr_output_manager_state(&mut self) -> &mut WlrOutputManagerState {
        &mut self.protocols.wlr_output_manager_state
    }

    fn apply_output_config(&mut self, config: PendingOutputConfig) -> bool {
        let heads = self.output_heads();
        if let Err(rejection) = validate(&config.heads, &heads) {
            tracing::info!(?rejection, "refused an output configuration");
            return false;
        }
        let outputs = self.connected_outputs();
        let mut switched: Vec<(Output, Mode)> = Vec::new();
        for wanted in config.heads.iter().filter(|wanted| wanted.enabled) {
            let Some(output) = outputs.iter().find(|output| output.name() == wanted.output_name) else {
                continue;
            };
            let Some(info) = heads.iter().find(|info| info.name == wanted.output_name) else {
                continue;
            };
            let (Some(mode), Some(previous)) = (wanted.mode.and_then(|mode| info.resolve(mode)), output.current_mode())
            else {
                continue;
            };
            if mode == previous {
                continue;
            }
            if !self.set_output_mode(output, mode) {
                for (output, previous) in switched.iter().rev() {
                    self.set_output_mode(output, *previous);
                }
                return false;
            }
            switched.push((output.clone(), previous));
        }
        arrange(&mut self.space, &outputs, &config.heads);
        self.sync_layout_outputs();
        self.reconcile_space();
        self.queue_redraw();
        true
    }
}

impl GlideState {
    #[must_use]
    pub fn output_heads(&self) -> Vec<HeadInfo> {
        let mut heads: Vec<HeadInfo> =
            self.connected_outputs().iter().map(|output| HeadInfo::of(output, &self.space)).collect();
        heads.sort_by(|a, b| a.name.cmp(&b.name));
        heads
    }
}

fn arrange(space: &mut Space<GlideWindow>, outputs: &[Output], heads: &[PendingHeadConfig]) {
    for head in heads {
        let Some(output) = outputs.iter().find(|output| output.name() == head.output_name) else {
            continue;
        };
        if !head.enabled {
            space.unmap_output(output);
            continue;
        }
        let position = head.position.map_or_else(
            || space.output_geometry(output).map_or_else(|| output.current_location(), |geometry| geometry.loc),
            Point::from,
        );
        let scale = head.scale.map(smithay::output::Scale::Fractional);
        output.change_current_state(None, head.transform, scale, Some(position));
        space.map_output(output, position);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeadInfo {
    pub name: String,
    pub description: String,
    pub physical_size: (i32, i32),
    pub make: String,
    pub model: String,
    pub serial_number: String,
    pub modes: Vec<Mode>,
    pub preferred: Option<Mode>,
    pub current: Option<Mode>,
    pub enabled: bool,
    pub position: (i32, i32),
    pub scale: f64,
    pub transform: Transform,
}

impl HeadInfo {
    #[must_use]
    pub fn of(output: &Output, space: &Space<GlideWindow>) -> Self {
        let properties = output.physical_properties();
        let name = output.name();
        let label: Vec<&str> =
            [properties.make.as_str(), properties.model.as_str()].into_iter().filter(|part| !part.is_empty()).collect();
        let description = if label.is_empty() { name.clone() } else { format!("{} ({name})", label.join(" ")) };
        let geometry = space.output_geometry(output);
        let position = geometry.map_or_else(|| output.current_location(), |geometry| geometry.loc);
        Self {
            name,
            description,
            physical_size: properties.size.into(),
            make: properties.make,
            model: properties.model,
            serial_number: properties.serial_number,
            modes: output.modes(),
            preferred: output.preferred_mode(),
            current: output.current_mode(),
            enabled: geometry.is_some(),
            position: position.into(),
            scale: output.current_scale().fractional_scale(),
            transform: output.current_transform(),
        }
    }

    #[must_use]
    pub fn resolve(&self, wanted: Mode) -> Option<Mode> {
        if wanted.refresh > 0 {
            return self.modes.iter().copied().find(|mode| *mode == wanted);
        }
        self.modes.iter().copied().filter(|mode| mode.size == wanted.size).max_by_key(|mode| mode.refresh)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Modes,
    Enabled,
    Current,
    Position,
    Transform,
    Scale,
}

#[must_use]
pub fn delta(old: Option<&HeadInfo>, new: &HeadInfo) -> Vec<Part> {
    let modes = old.is_none_or(|old| old.modes != new.modes || old.preferred != new.preferred);
    let enabled = old.is_none_or(|old| old.enabled != new.enabled);
    let mut parts = Vec::new();
    if modes {
        parts.push(Part::Modes);
    }
    if enabled {
        parts.push(Part::Enabled);
    }
    if !new.enabled {
        return parts;
    }
    let reveal = enabled || modes;
    if reveal || old.is_some_and(|old| old.current != new.current) {
        parts.push(Part::Current);
    }
    if enabled || old.is_some_and(|old| old.position != new.position) {
        parts.push(Part::Position);
    }
    if enabled || old.is_some_and(|old| old.transform != new.transform) {
        parts.push(Part::Transform);
    }
    if enabled || old.is_some_and(|old| old.scale.to_bits() != new.scale.to_bits()) {
        parts.push(Part::Scale);
    }
    parts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    UnknownHead,
    UnknownMode,
    BadScale,
    AdaptiveSync,
    NothingEnabled,
}

pub fn validate(wanted: &[PendingHeadConfig], heads: &[HeadInfo]) -> Result<(), Rejection> {
    for head in wanted {
        let Some(info) = heads.iter().find(|info| info.name == head.output_name) else {
            return Err(Rejection::UnknownHead);
        };
        if !head.enabled {
            continue;
        }
        if head.mode.is_some_and(|mode| info.resolve(mode).is_none()) {
            return Err(Rejection::UnknownMode);
        }
        if head.scale.is_some_and(|scale| !(0.25..=10.0).contains(&scale)) {
            return Err(Rejection::BadScale);
        }
        if head.adaptive_sync == Some(true) {
            return Err(Rejection::AdaptiveSync);
        }
    }
    let enabled = heads
        .iter()
        .any(|info| wanted.iter().find(|head| head.output_name == info.name).map_or(info.enabled, |head| head.enabled));
    if enabled { Ok(()) } else { Err(Rejection::NothingEnabled) }
}

#[derive(Debug, Clone, Default)]
pub struct PendingHeadConfig {
    pub output_name: String,
    pub enabled: bool,
    pub mode: Option<Mode>,
    pub position: Option<(i32, i32)>,
    pub scale: Option<f64>,
    pub transform: Option<Transform>,
    pub adaptive_sync: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct PendingOutputConfig {
    pub serial: u32,
    pub heads: Vec<PendingHeadConfig>,
    pub used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeData {
    pub output: String,
    pub mode: Mode,
}

#[derive(Debug)]
struct Head {
    resource: ZwlrOutputHeadV1,
    modes: Vec<(Mode, ZwlrOutputModeV1)>,
}

impl Head {
    fn finish(self) {
        for (_, mode) in self.modes {
            mode.finished();
        }
        self.resource.finished();
    }

    fn send(&mut self, dh: &DisplayHandle, info: &HeadInfo, parts: &[Part]) {
        for part in parts {
            match part {
                Part::Modes => {
                    for (_, mode) in self.modes.drain(..) {
                        mode.finished();
                    }
                    let Some(client) = self.resource.client() else {
                        return;
                    };
                    for &mode in &info.modes {
                        let data = ModeData { output: info.name.clone(), mode };
                        let Ok(resource) = client.create_resource::<ZwlrOutputModeV1, _, GlideState>(
                            dh,
                            self.resource.version(),
                            data,
                        ) else {
                            continue;
                        };
                        self.resource.mode(&resource);
                        resource.size(mode.size.w, mode.size.h);
                        resource.refresh(mode.refresh);
                        if info.preferred == Some(mode) {
                            resource.preferred();
                        }
                        self.modes.push((mode, resource));
                    }
                }
                Part::Enabled => self.resource.enabled(i32::from(info.enabled)),
                Part::Current => {
                    if let Some((_, mode)) = self.modes.iter().find(|(mode, _)| Some(*mode) == info.current) {
                        self.resource.current_mode(mode);
                    }
                }
                Part::Position => self.resource.position(info.position.0, info.position.1),
                Part::Transform => self.resource.transform(info.transform.into()),
                Part::Scale => self.resource.scale(info.scale),
            }
        }
    }
}

#[derive(Debug)]
struct Manager {
    resource: ZwlrOutputManagerV1,
    heads: HashMap<String, Head>,
    sent: Vec<HeadInfo>,
}

impl Manager {
    fn update(&mut self, dh: &DisplayHandle, now: &[HeadInfo]) -> bool {
        let mut changed = false;
        for old in &self.sent {
            if now.iter().any(|info| info.name == old.name) {
                continue;
            }
            if let Some(head) = self.heads.remove(&old.name) {
                head.finish();
            }
            changed = true;
        }
        for info in now {
            let old = self.sent.iter().find(|old| old.name == info.name);
            let parts = delta(old, info);
            if parts.is_empty() {
                continue;
            }
            changed = true;
            if old.is_none()
                && let Some(head) = self.announce(dh, info)
            {
                self.heads.insert(info.name.clone(), head);
            }
            if let Some(head) = self.heads.get_mut(&info.name) {
                head.send(dh, info, &parts);
            }
        }
        self.sent = now.to_vec();
        changed
    }

    fn announce(&self, dh: &DisplayHandle, info: &HeadInfo) -> Option<Head> {
        let client = self.resource.client()?;
        let resource = client
            .create_resource::<ZwlrOutputHeadV1, _, GlideState>(dh, self.resource.version(), info.name.clone())
            .ok()?;
        self.resource.head(&resource);
        resource.name(info.name.clone());
        resource.description(info.description.clone());
        resource.physical_size(info.physical_size.0, info.physical_size.1);
        if resource.version() >= 2 {
            resource.make(info.make.clone());
            resource.model(info.model.clone());
            resource.serial_number(info.serial_number.clone());
        }
        if resource.version() >= 4 {
            resource.adaptive_sync(AdaptiveSyncState::Disabled);
        }
        Some(Head { resource, modes: Vec::new() })
    }
}

#[derive(Debug)]
pub struct WlrOutputManagerState {
    pub global: GlobalId,
    managers: Vec<Manager>,
    serial: u32,
    last: Vec<HeadInfo>,
}

impl WlrOutputManagerState {
    pub fn new<D>(dh: &DisplayHandle) -> Self
    where
        D: WlrOutputManagerHandler + GlobalDispatch<ZwlrOutputManagerV1, GlobalData> + 'static,
    {
        let global = dh.create_global::<D, ZwlrOutputManagerV1, _>(4, GlobalData);
        Self { global, managers: Vec::new(), serial: 1, last: Vec::new() }
    }

    #[must_use]
    pub const fn is_current(&self, serial: u32) -> bool {
        serial == self.serial
    }

    pub fn sync_outputs(&mut self, dh: &DisplayHandle, heads: Vec<HeadInfo>) {
        self.managers.retain(|manager| manager.resource.is_alive());
        if heads == self.last {
            return;
        }
        self.serial = self.serial.wrapping_add(1);
        for manager in &mut self.managers {
            if manager.update(dh, &heads) {
                manager.resource.done(self.serial);
            }
        }
        self.last = heads;
    }
}

impl GlobalDispatch<ZwlrOutputManagerV1, GlobalData> for GlideState {
    fn bind(
        state: &mut Self, dh: &DisplayHandle, _client: &Client, resource: New<ZwlrOutputManagerV1>,
        _global_data: &GlobalData, data_init: &mut DataInit<'_, Self>,
    ) {
        let resource = data_init.init(resource, GlobalData);
        let heads = state.output_heads();
        let managers = &mut state.protocols.wlr_output_manager_state;
        managers.sync_outputs(dh, heads);
        let mut manager = Manager { resource, heads: HashMap::new(), sent: Vec::new() };
        manager.update(dh, &managers.last);
        manager.resource.done(managers.serial);
        managers.managers.push(manager);
    }

    fn can_view(client: Client, _global_data: &GlobalData) -> bool {
        crate::state::client_is_unrestricted(&client)
    }
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputManagerV1, GlobalData> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, resource: &ZwlrOutputManagerV1, request: zwlr_output_manager_v1::Request,
        _data: &GlobalData, _dhandle: &DisplayHandle, data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_output_manager_v1::Request::CreateConfiguration { id, serial } => {
                let config = PendingOutputConfig { serial, ..PendingOutputConfig::default() };
                data_init.init(id, Arc::new(Mutex::new(config)));
            }
            zwlr_output_manager_v1::Request::Stop => {
                state.protocols.wlr_output_manager_state.managers.retain(|manager| &manager.resource != resource);
                resource.finished();
            }
            _ => {}
        }
    }
}

type ConfigurationUserData = Arc<Mutex<PendingOutputConfig>>;

fn settle(
    state: &GlideState, resource: &ZwlrOutputConfigurationV1, data: &ConfigurationUserData,
) -> Option<(PendingOutputConfig, Vec<HeadInfo>)> {
    let config = {
        let mut config = data.lock().unwrap();
        if config.used {
            resource.post_error(
                zwlr_output_configuration_v1::Error::AlreadyUsed,
                "this configuration was already applied or tested",
            );
            return None;
        }
        config.used = true;
        config.clone()
    };
    if !state.protocols.wlr_output_manager_state.is_current(config.serial) {
        resource.cancelled();
        return None;
    }
    let heads = state.output_heads();
    if let Some(missing) = heads.iter().find(|info| !config.heads.iter().any(|head| head.output_name == info.name)) {
        resource.post_error(
            zwlr_output_configuration_v1::Error::UnconfiguredHead,
            format!("head {} was neither enabled nor disabled", missing.name),
        );
        return None;
    }
    Some((config, heads))
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputConfigurationV1, ConfigurationUserData> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, resource: &ZwlrOutputConfigurationV1,
        request: zwlr_output_configuration_v1::Request, data: &ConfigurationUserData, _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_output_configuration_v1::Request::EnableHead { id, head } => {
                let head_name = head.data::<String>().cloned().unwrap_or_default();
                let configured = configure(resource, data, &head_name, true);
                data_init.init(id, HeadConfigData { config: Arc::clone(data), head_name, configured });
            }
            zwlr_output_configuration_v1::Request::DisableHead { head } => {
                let head_name = head.data::<String>().cloned().unwrap_or_default();
                configure(resource, data, &head_name, false);
            }
            zwlr_output_configuration_v1::Request::Apply => {
                let Some((config, _)) = settle(state, resource, data) else {
                    return;
                };
                if state.apply_output_config(config) {
                    resource.succeeded();
                } else {
                    resource.failed();
                }
            }
            zwlr_output_configuration_v1::Request::Test => {
                let Some((config, heads)) = settle(state, resource, data) else {
                    return;
                };
                if validate(&config.heads, &heads).is_ok() {
                    resource.succeeded();
                } else {
                    resource.failed();
                }
            }
            _ => {}
        }
    }
}

fn configure(resource: &ZwlrOutputConfigurationV1, data: &ConfigurationUserData, name: &str, enabled: bool) -> bool {
    let mut config = data.lock().unwrap();
    if config.heads.iter().any(|head| head.output_name == name) {
        resource.post_error(
            zwlr_output_configuration_v1::Error::AlreadyConfiguredHead,
            format!("head {name} was already enabled or disabled"),
        );
        return false;
    }
    config.heads.push(PendingHeadConfig { output_name: name.to_owned(), enabled, ..PendingHeadConfig::default() });
    true
}

pub struct HeadConfigData {
    pub config: Arc<Mutex<PendingOutputConfig>>,
    pub head_name: String,
    pub configured: bool,
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputConfigurationHeadV1, HeadConfigData> for GlideState {
    fn request(
        _state: &mut Self, _client: &Client, resource: &ZwlrOutputConfigurationHeadV1,
        request: zwlr_output_configuration_head_v1::Request, data: &HeadConfigData, _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        use zwlr_output_configuration_head_v1::{Error, Request};

        if !data.configured {
            return;
        }
        let mut config = data.config.lock().unwrap();
        let Some(head) = config.heads.iter_mut().find(|head| head.output_name == data.head_name) else {
            return;
        };
        match request {
            Request::SetMode { mode } => match mode.data::<ModeData>() {
                Some(mode) if mode.output == data.head_name => head.mode = Some(mode.mode),
                _ => resource.post_error(Error::InvalidMode, "the mode belongs to another head"),
            },
            Request::SetCustomMode { width, height, refresh } => {
                if width <= 0 || height <= 0 || refresh < 0 {
                    resource.post_error(Error::InvalidCustomMode, "a custom mode needs a positive size");
                } else {
                    head.mode = Some(Mode { size: (width, height).into(), refresh });
                }
            }
            Request::SetPosition { x, y } => head.position = Some((x, y)),
            Request::SetTransform { transform: WEnum::Value(transform) } => head.transform = Some(transform.into()),
            Request::SetTransform { .. } => resource.post_error(Error::InvalidTransform, "unknown transform"),
            Request::SetScale { scale } => {
                if scale.is_finite() && scale > 0.0 {
                    head.scale = Some(scale);
                } else {
                    resource.post_error(Error::InvalidScale, "the scale must be positive");
                }
            }
            Request::SetAdaptiveSync { state } => {
                head.adaptive_sync = Some(matches!(state, WEnum::Value(AdaptiveSyncState::Enabled)));
            }
            _ => {}
        }
    }
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputHeadV1, String> for GlideState {
    fn request(
        _state: &mut Self, _client: &Client, _resource: &ZwlrOutputHeadV1, _request: zwlr_output_head_v1::Request,
        _data: &String, _dhandle: &DisplayHandle, _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputModeV1, ModeData> for GlideState {
    fn request(
        _state: &mut Self, _client: &Client, _resource: &ZwlrOutputModeV1, _request: zwlr_output_mode_v1::Request,
        _data: &ModeData, _dhandle: &DisplayHandle, _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::output::{PhysicalProperties, Subpixel};
    use smithay::reexports::wayland_server::Display;

    fn mode(w: i32, h: i32, refresh: i32) -> Mode {
        Mode { size: (w, h).into(), refresh }
    }

    fn sample_output(name: &str, w: i32, h: i32) -> Output {
        let output = Output::new(
            name.to_string(),
            PhysicalProperties {
                size: (530, 300).into(),
                subpixel: Subpixel::Unknown,
                make: "TestMake".into(),
                model: "TestModel".into(),
                serial_number: "1234".into(),
            },
        );
        let current = mode(w, h, 60_000);
        output.add_mode(current);
        output.add_mode(mode(w, h, 144_000));
        output.add_mode(mode(1280, 720, 60_000));
        output.set_preferred(current);
        output.change_current_state(Some(current), Some(Transform::Normal), None, Some((0, 0).into()));
        output
    }

    fn enable(name: &str) -> PendingHeadConfig {
        PendingHeadConfig { output_name: name.to_string(), enabled: true, ..PendingHeadConfig::default() }
    }

    fn disable(name: &str) -> PendingHeadConfig {
        PendingHeadConfig { output_name: name.to_string(), enabled: false, ..PendingHeadConfig::default() }
    }

    fn info(name: &str) -> HeadInfo {
        let output = sample_output(name, 1920, 1080);
        let mut space = Space::<GlideWindow>::default();
        space.map_output(&output, (0, 0));
        HeadInfo::of(&output, &space)
    }

    #[test]
    fn a_head_describes_its_output_and_every_mode() {
        let head = info("HDMI-A-1");
        assert_eq!(head.description, "TestMake TestModel (HDMI-A-1)");
        assert_eq!(head.modes.len(), 3);
        assert_eq!(head.preferred, Some(mode(1920, 1080, 60_000)));
        assert_eq!(head.current, Some(mode(1920, 1080, 60_000)));
        assert!(head.enabled);
    }

    #[test]
    fn an_unmapped_output_is_a_disabled_head_that_stays_listed() {
        let output = sample_output("DP-1", 1920, 1080);
        let space = Space::<GlideWindow>::default();
        let head = HeadInfo::of(&output, &space);
        assert!(!head.enabled);
        assert_eq!(delta(None, &head), vec![Part::Modes, Part::Enabled]);
    }

    #[test]
    fn a_new_head_sends_everything_and_an_unchanged_one_nothing() {
        let head = info("HDMI-A-1");
        assert_eq!(
            delta(None, &head),
            vec![Part::Modes, Part::Enabled, Part::Current, Part::Position, Part::Transform, Part::Scale]
        );
        assert!(delta(Some(&head), &head).is_empty());
    }

    #[test]
    fn only_what_changed_is_resent() {
        let old = info("HDMI-A-1");
        let mut moved = old.clone();
        moved.position = (1920, 0);
        assert_eq!(delta(Some(&old), &moved), vec![Part::Position]);

        let mut switched = old.clone();
        switched.current = Some(mode(1920, 1080, 144_000));
        assert_eq!(delta(Some(&old), &switched), vec![Part::Current]);

        let mut fewer = old.clone();
        fewer.modes.pop();
        assert_eq!(delta(Some(&old), &fewer), vec![Part::Modes, Part::Current]);
    }

    #[test]
    fn re_enabling_a_head_resends_its_state() {
        let on = info("HDMI-A-1");
        let mut off = on.clone();
        off.enabled = false;
        assert_eq!(delta(Some(&on), &off), vec![Part::Enabled]);
        assert_eq!(
            delta(Some(&off), &on),
            vec![Part::Enabled, Part::Current, Part::Position, Part::Transform, Part::Scale]
        );
    }

    #[test]
    fn only_advertised_modes_are_accepted() {
        let heads = vec![info("HDMI-A-1")];
        let mut wanted = enable("HDMI-A-1");
        wanted.mode = Some(mode(1920, 1080, 144_000));
        assert_eq!(validate(&[wanted.clone()], &heads), Ok(()));
        wanted.mode = Some(mode(2560, 1440, 60_000));
        assert_eq!(validate(&[wanted], &heads), Err(Rejection::UnknownMode));
    }

    #[test]
    fn a_custom_mode_without_refresh_takes_the_fastest_match() {
        let head = info("HDMI-A-1");
        assert_eq!(head.resolve(mode(1920, 1080, 0)), Some(mode(1920, 1080, 144_000)));
        assert_eq!(head.resolve(mode(1920, 1080, 75_000)), None);
    }

    #[test]
    fn bad_scales_unknown_heads_and_adaptive_sync_are_refused() {
        let heads = vec![info("HDMI-A-1")];
        let mut wanted = enable("HDMI-A-1");
        wanted.scale = Some(f64::NAN);
        assert_eq!(validate(&[wanted.clone()], &heads), Err(Rejection::BadScale));
        wanted.scale = Some(0.0);
        assert_eq!(validate(&[wanted.clone()], &heads), Err(Rejection::BadScale));
        wanted.scale = Some(1.5);
        assert_eq!(validate(&[wanted.clone()], &heads), Ok(()));
        wanted.adaptive_sync = Some(true);
        assert_eq!(validate(&[wanted], &heads), Err(Rejection::AdaptiveSync));
        assert_eq!(validate(&[enable("DP-9")], &heads), Err(Rejection::UnknownHead));
    }

    #[test]
    fn the_last_enabled_head_cannot_be_disabled() {
        let heads = vec![info("DP-1"), info("HDMI-A-1")];
        assert_eq!(validate(&[disable("DP-1"), enable("HDMI-A-1")], &heads), Ok(()));
        assert_eq!(validate(&[disable("DP-1"), disable("HDMI-A-1")], &heads), Err(Rejection::NothingEnabled));
    }

    #[test]
    fn arranging_moves_scales_rotates_and_disables() {
        let first = sample_output("DP-1", 1920, 1080);
        let second = sample_output("HDMI-A-1", 1920, 1080);
        let mut space = Space::<GlideWindow>::default();
        space.map_output(&first, (0, 0));
        space.map_output(&second, (1920, 0));
        let outputs = vec![first.clone(), second.clone()];

        let mut wanted = enable("HDMI-A-1");
        wanted.position = Some((0, 1080));
        wanted.scale = Some(2.0);
        wanted.transform = Some(Transform::_90);
        arrange(&mut space, &outputs, &[disable("DP-1"), wanted]);

        assert!(space.output_geometry(&first).is_none());
        assert_eq!(space.output_geometry(&second).unwrap().loc, (0, 1080).into());
        assert_eq!(second.current_scale().fractional_scale(), 2.0);
        assert_eq!(second.current_transform(), Transform::_90);
    }

    #[test]
    fn re_enabling_without_a_position_returns_to_the_old_place() {
        let output = sample_output("DP-1", 1920, 1080);
        let mut space = Space::<GlideWindow>::default();
        output.change_current_state(None, None, None, Some((1920, 0).into()));
        let outputs = vec![output.clone()];
        arrange(&mut space, &outputs, &[enable("DP-1")]);
        assert_eq!(space.output_geometry(&output).unwrap().loc, (1920, 0).into());
    }

    #[test]
    fn the_serial_only_moves_when_the_heads_change() {
        let display = Display::<GlideState>::new().unwrap();
        let dh = display.handle();
        let mut managers = WlrOutputManagerState::new::<GlideState>(&dh);
        let start = managers.serial;

        managers.sync_outputs(&dh, vec![info("DP-1")]);
        assert!(managers.is_current(start.wrapping_add(1)));
        managers.sync_outputs(&dh, vec![info("DP-1")]);
        assert!(managers.is_current(start.wrapping_add(1)));
        managers.sync_outputs(&dh, Vec::new());
        assert!(managers.is_current(start.wrapping_add(2)));
    }
}
