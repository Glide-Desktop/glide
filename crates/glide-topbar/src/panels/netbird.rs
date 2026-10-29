use std::time::SystemTime;

use glide_netbird::{self as nb, Remaining, State};

use super::super::{LocalRect, QsRenderContext, Topbar};
use super::{CardSpec, Column, ItemKind, Key, Trailing, card_background, draw_card, row_slot, t};
use crate::raster::Canvas;

const STATUS_ROW: usize = 0;
const SESSION_ROW: usize = 1;
const PROFILE_ROWS: usize = 10;
const EXIT_NONE_ROW: usize = 100;
const EXIT_ROWS: usize = 101;
const PEERS_ROW: usize = 1000;
const RESOURCES_ROW: usize = 1001;
const MAX_PROFILES: usize = EXIT_NONE_ROW - PROFILE_ROWS;
const MAX_EXITS: usize = PEERS_ROW - EXIT_ROWS;

#[derive(Debug)]
pub enum NetbirdUpdate {
    Status(Box<nb::Status>),
    Networks(Vec<nb::Network>),
    Profiles(Vec<nb::Profile>),
    Config(Box<nb::Config>),
    Features(nb::Features),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetbirdPending {
    Connect,
    Disconnect,
    Profile(String),
    Exit(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetbirdRow {
    SignIn,
    Session { expired: bool },
    Profile(String),
    Exit { id: String, select: bool },
    Peers,
    Resources,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NbItem {
    Status,
    Session,
    Profile(usize),
    ExitNone,
    Exit(usize),
    Peers,
    Resources,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Summary {
    state: State,
    ip: String,
    peers: (usize, usize),
    session: Option<SystemTime>,
    management_error: String,
}

#[derive(Debug, Default)]
pub struct NetbirdView {
    pub(crate) installed: bool,
    status: Option<nb::Status>,
    profiles: Vec<nb::Profile>,
    networks: Vec<nb::Network>,
    config: Option<nb::Config>,
    features: nb::Features,
    pending: Option<(NetbirdPending, Option<State>)>,
    session: Option<Remaining>,
}

fn summary(status: &nb::Status) -> Summary {
    Summary {
        state: status.state,
        ip: status.local.ip.clone(),
        peers: (status.connected_peers(), status.peers.len()),
        session: status.session_expires_at,
        management_error: status.management.error.clone(),
    }
}

impl NetbirdView {
    fn state(&self) -> Option<State> {
        self.status.as_ref().map(|status| status.state)
    }

    fn exits(&self) -> Vec<nb::ExitNode> {
        nb::exit_nodes(&self.networks).into_iter().take(MAX_EXITS).collect()
    }

    fn session_left(&self, now: SystemTime) -> Option<Remaining> {
        let status = self.status.as_ref().filter(|status| status.state.is_connected())?;
        status.session_expires_at.map(|deadline| nb::remaining_until(deadline, now))
    }

    fn pending(&self) -> Option<&NetbirdPending> {
        self.pending.as_ref().map(|(pending, _)| pending)
    }

    fn apply(&mut self, update: NetbirdUpdate, now: SystemTime) -> bool {
        match update {
            NetbirdUpdate::Status(status) => {
                let before = self.status.as_ref().map(summary);
                let after = summary(&status);
                let settled = matches!(
                    &self.pending,
                    Some((NetbirdPending::Connect | NetbirdPending::Disconnect, from)) if *from != Some(status.state)
                );
                if settled {
                    self.pending = None;
                }
                self.status = Some(*status);
                self.session = self.session_left(now);
                settled || before.as_ref() != Some(&after)
            }
            NetbirdUpdate::Networks(networks) => {
                if matches!(self.pending, Some((NetbirdPending::Exit(_), _))) {
                    self.pending = None;
                } else if self.networks == networks {
                    return false;
                }
                self.networks = networks;
                true
            }
            NetbirdUpdate::Profiles(profiles) => {
                if matches!(self.pending, Some((NetbirdPending::Profile(_), _))) {
                    self.pending = None;
                } else if self.profiles == profiles {
                    return false;
                }
                self.profiles = profiles;
                true
            }
            NetbirdUpdate::Config(config) => {
                self.config = Some(*config);
                false
            }
            NetbirdUpdate::Features(features) => {
                let changed = self.features != features;
                self.features = features;
                changed
            }
        }
    }

    pub(crate) fn tile(&self) -> (bool, &'static str) {
        match (self.pending(), self.state()) {
            (Some(NetbirdPending::Connect), _) => (false, t(Key::Connecting)),
            (Some(NetbirdPending::Disconnect), _) => (true, t(Key::QsDisconnecting)),
            (_, state) => (state.is_some_and(State::is_connected), state_short(state)),
        }
    }

    pub(crate) fn switched_on(&self) -> bool {
        self.state().is_some_and(State::is_up)
    }

    fn active_profile(&self) -> Option<&nb::Profile> {
        self.profiles.iter().find(|profile| profile.active)
    }
}

fn state_short(state: Option<State>) -> &'static str {
    let key = match state {
        Some(State::Connected) => Key::Connected,
        Some(State::Connecting) => Key::Connecting,
        Some(State::NeedsLogin) => Key::NbLoginRequired,
        Some(State::LoginFailed) => Key::NbStateLoginFailed,
        Some(State::SessionExpired) => Key::NbSessionExpired,
        Some(State::Unavailable) => Key::NbNotRunning,
        Some(State::Idle) | None => Key::QsOff,
    };
    t(key)
}

const fn state_title(state: State) -> Key {
    match state {
        State::Connected => Key::Connected,
        State::Connecting => Key::Connecting,
        State::Idle => Key::NbDisconnected,
        State::NeedsLogin => Key::NbLoginRequired,
        State::LoginFailed => Key::NbStateLoginFailed,
        State::SessionExpired => Key::NbSessionExpired,
        State::Unavailable => Key::NbServiceDown,
    }
}

#[must_use]
pub fn row(view: &NetbirdView, index: u16) -> Option<NetbirdRow> {
    let index = usize::from(index);
    let state = view.state()?;
    match index {
        STATUS_ROW => state.needs_login().then_some(NetbirdRow::SignIn),
        SESSION_ROW => view.session.map(|left| NetbirdRow::Session { expired: left == Remaining::Expired }),
        PEERS_ROW => Some(NetbirdRow::Peers),
        RESOURCES_ROW => Some(NetbirdRow::Resources),
        EXIT_NONE_ROW => {
            let selected = view.exits().into_iter().find(|exit| exit.selected)?;
            Some(NetbirdRow::Exit { id: selected.id, select: false })
        }
        _ if (EXIT_ROWS..PEERS_ROW).contains(&index) => {
            let exit = view.exits().into_iter().nth(index - EXIT_ROWS)?;
            Some(NetbirdRow::Exit { select: !exit.selected, id: exit.id })
        }
        _ if (PROFILE_ROWS..EXIT_NONE_ROW).contains(&index) => {
            let profile = view.profiles.get(index - PROFILE_ROWS)?;
            (!profile.active).then(|| NetbirdRow::Profile(profile.id.clone()))
        }
        _ => None,
    }
}

pub(super) fn items(column: &mut Column<'_>, view: &NetbirdView) {
    let Some(status) = view.status.as_ref() else {
        return column.message(Key::NbLoading);
    };
    let sign_in = status.state.needs_login().then_some(STATUS_ROW).and_then(row_slot);
    column.card(ItemKind::Netbird(NbItem::Status), sign_in);
    if status.state == State::Unavailable {
        return;
    }
    if view.session.is_some() {
        column.card(ItemKind::Netbird(NbItem::Session), row_slot(SESSION_ROW));
    }
    column.end_group();
    if !view.features.disable_profiles && !view.profiles.is_empty() && column.section_over_card(Key::NbProfiles) {
        for index in 0..view.profiles.len().min(MAX_PROFILES) {
            if !column.card(ItemKind::Netbird(NbItem::Profile(index)), row_slot(PROFILE_ROWS + index)) {
                return;
            }
        }
        column.end_group();
    }
    if !status.state.is_connected() {
        return;
    }
    let exits = view.exits();
    if !view.features.disable_networks && !exits.is_empty() && column.section_over_card(Key::NbExitNode) {
        if !column.card(ItemKind::Netbird(NbItem::ExitNone), row_slot(EXIT_NONE_ROW)) {
            return;
        }
        for index in 0..exits.len() {
            if !column.card(ItemKind::Netbird(NbItem::Exit(index)), row_slot(EXIT_ROWS + index)) {
                return;
            }
        }
        column.end_group();
    }
    if column.section_over_card(Key::NbNetwork) {
        column.card(ItemKind::Netbird(NbItem::Peers), row_slot(PEERS_ROW));
        if !view.features.disable_networks {
            column.card(ItemKind::Netbird(NbItem::Resources), row_slot(RESOURCES_ROW));
        }
    }
}

const VPN: (&str, super::super::GlyphPainter) = ("network-vpn", crate::paint::draw_lock_glyph_over);

fn status_subtitle(view: &NetbirdView, status: &nb::Status) -> String {
    match view.pending() {
        Some(NetbirdPending::Connect) => return t(Key::Connecting).to_string(),
        Some(NetbirdPending::Disconnect) => return t(Key::QsDisconnecting).to_string(),
        _ => {}
    }
    match status.state {
        State::Connected => t(Key::NbPeersLine)
            .replace("{ip}", status.local.ip.split('/').next().unwrap_or_default())
            .replace("{n}", &status.connected_peers().to_string())
            .replace("{total}", &status.peers.len().to_string()),
        State::NeedsLogin | State::LoginFailed | State::SessionExpired => t(Key::NbSignInHint).to_string(),
        State::Unavailable => t(Key::NbServiceDownHint).to_string(),
        State::Connecting => t(Key::NbConnectingHint).to_string(),
        State::Idle => {
            if !status.management.error.is_empty() {
                return status.management.error.clone();
            }
            view.active_profile().map_or_else(
                || t(Key::NbOffHint).to_string(),
                |profile| match &profile.email {
                    Some(email) => format!("{} \u{b7} {email}", profile.name),
                    None => profile.name.clone(),
                },
            )
        }
    }
}

fn session_title(left: Remaining) -> String {
    if left == Remaining::Expired {
        return t(Key::NbSessionExpired).to_string();
    }
    t(Key::NbExpiresIn).replace("{time}", &remaining_text(left))
}

struct Card {
    icon: &'static str,
    title: String,
    subtitle: String,
    danger: bool,
    active: bool,
    action: Option<&'static str>,
    trailing: Trailing,
}

impl Card {
    const fn new(icon: &'static str, title: String, subtitle: String) -> Self {
        Self { icon, title, subtitle, danger: false, active: false, action: None, trailing: Trailing::Nothing }
    }
}

const fn ticked(on: bool) -> Trailing {
    if on { Trailing::Tick } else { Trailing::Nothing }
}

fn count_line(key: Key, n: usize, total: usize) -> String {
    t(key).replace("{n}", &n.to_string()).replace("{total}", &total.to_string())
}

fn status_card(view: &NetbirdView, status: &nb::Status) -> Card {
    Card {
        active: status.state.is_connected(),
        action: status.state.needs_login().then(|| t(Key::NbSignIn)),
        ..Card::new(VPN.0, t(state_title(status.state)).to_string(), status_subtitle(view, status))
    }
}

fn session_card(left: Remaining) -> Card {
    let expired = left == Remaining::Expired;
    Card {
        danger: matches!(left, Remaining::Expired | Remaining::LessThanMinute | Remaining::Minutes(..=10)),
        action: Some(t(if expired { Key::NbSignIn } else { Key::NbRenew })),
        ..Card::new("alarm", session_title(left), t(Key::NbRenewHint).to_string())
    }
}

fn profile_card(view: &NetbirdView, profile: &nb::Profile) -> Card {
    let switching = view.pending() == Some(&NetbirdPending::Profile(profile.id.clone()));
    let subtitle = if switching {
        t(Key::NbSwitching).to_string()
    } else {
        profile.email.clone().unwrap_or_else(|| t(Key::NbNoEmail).to_string())
    };
    Card {
        active: profile.active,
        trailing: ticked(profile.active),
        ..Card::new("avatar-default", profile.name.clone(), subtitle)
    }
}

fn exit_card(view: &NetbirdView, exit: nb::ExitNode) -> Card {
    let switching = view.pending() == Some(&NetbirdPending::Exit(exit.id.clone()));
    let subtitle = t(if switching {
        Key::NbSwitching
    } else if exit.selected {
        Key::NbActive
    } else {
        Key::NbInactive
    });
    Card { active: exit.selected, trailing: ticked(exit.selected), ..Card::new(VPN.0, exit.id, subtitle.to_string()) }
}

fn card_for(item: NbItem, view: &NetbirdView, status: &nb::Status) -> Option<Card> {
    Some(match item {
        NbItem::Status => status_card(view, status),
        NbItem::Session => session_card(view.session?),
        NbItem::Profile(index) => profile_card(view, view.profiles.get(index)?),
        NbItem::ExitNone => Card {
            trailing: ticked(view.exits().iter().all(|exit| !exit.selected)),
            ..Card::new("network-wired", t(Key::NbExitNone).to_string(), t(Key::NbExitNoneHint).to_string())
        },
        NbItem::Exit(index) => exit_card(view, view.exits().into_iter().nth(index)?),
        NbItem::Peers => Card {
            trailing: Trailing::Chevron,
            ..Card::new(
                "system-users",
                t(Key::NbPeers).to_string(),
                count_line(Key::NbPeersCount, status.connected_peers(), status.peers.len()),
            )
        },
        NbItem::Resources => {
            let resources = nb::resources(&view.networks);
            let active = resources.iter().filter(|network| network.selected).count();
            Card {
                trailing: Trailing::Chevron,
                ..Card::new(
                    "network-server",
                    t(Key::NbResources).to_string(),
                    count_line(Key::NbResourcesCount, active, resources.len()),
                )
            }
        }
    })
}

pub(super) fn draw(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, item: NbItem, view: &NetbirdView) {
    let Some(card) = view.status.as_ref().and_then(|status| card_for(item, view, status)) else {
        return;
    };
    card_background(c, ctx.theme, rect, card.active);
    let spec = CardSpec {
        icon: card.icon,
        glyph: VPN.1,
        title: &card.title,
        subtitle: (card.subtitle, card.danger.then_some(ctx.theme.danger)),
        active: card.active,
        action: card.action,
        trailing: card.trailing,
    };
    draw_card(c, ctx, rect, &spec);
}

impl Topbar {
    pub fn set_netbird_installed(&mut self, installed: bool) {
        if self.qs_panel.netbird.installed != installed {
            self.qs_panel.netbird.installed = installed;
            self.qs_panel.bump();
            self.touch();
        }
    }

    #[must_use]
    pub fn netbird_state(&self) -> Option<State> {
        self.qs_panel.netbird.state()
    }

    #[must_use]
    pub fn netbird_profile_name(&self, id: &str) -> Option<String> {
        self.qs_panel.netbird.profiles.iter().find(|profile| profile.id == id).map(|profile| profile.name.clone())
    }

    pub fn begin_netbird(&mut self, pending: NetbirdPending) {
        let state = self.netbird_state();
        self.qs_panel.netbird.pending = Some((pending, state));
        self.qs_panel.bump();
        self.touch();
    }

    pub fn fail_netbird(&mut self) {
        if self.qs_panel.netbird.pending.take().is_some() {
            self.qs_panel.bump();
            self.touch();
        }
    }

    pub fn apply_netbird(&mut self, update: NetbirdUpdate) -> bool {
        if !self.qs_panel.netbird.apply(update, SystemTime::now()) {
            return false;
        }
        self.qs_panel.bump();
        self.touch();
        true
    }

    pub fn tick_netbird(&mut self, now: SystemTime) -> bool {
        let view = &mut self.qs_panel.netbird;
        let left = view.session_left(now);
        if left == view.session {
            return false;
        }
        view.session = left;
        self.qs_panel.bump();
        self.touch();
        true
    }
}

#[must_use]
pub fn remaining_text(remaining: Remaining) -> String {
    let count = |key: Key, n: u64| t(key).replace("{n}", &n.to_string());
    match remaining {
        Remaining::Expired => t(Key::NbSessionExpired).to_string(),
        Remaining::LessThanMinute => t(Key::NbLessThanMinute).to_string(),
        Remaining::Minutes(n) => count(Key::NbMinutes, n),
        Remaining::Hours(n) => count(Key::NbHours, n),
        Remaining::Days(n) => count(Key::NbDays, n),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn status(state: State) -> nb::Status {
        nb::Status { state, ..nb::Status::default() }
    }

    #[test]
    fn remaining_session_time_fills_in_the_count_of_the_largest_unit() {
        for (remaining, count) in
            [(Remaining::Hours(2), "2"), (Remaining::Minutes(15), "15"), (Remaining::Days(3), "3")]
        {
            let text = remaining_text(remaining);
            assert!(text.contains(count) && !text.contains('{'), "{text}");
        }
        assert_eq!(remaining_text(Remaining::LessThanMinute), t(Key::NbLessThanMinute));
    }

    fn view_with(state: State) -> NetbirdView {
        let mut view = NetbirdView { installed: true, ..NetbirdView::default() };
        view.apply(NetbirdUpdate::Status(Box::new(status(state))), SystemTime::now());
        view
    }

    fn network(id: &str, range: &str, selected: bool) -> nb::Network {
        nb::Network { id: id.into(), range: range.into(), selected, ..nb::Network::default() }
    }

    #[test]
    fn a_pending_connect_settles_when_the_daemon_reports_another_state() {
        let mut view = view_with(State::Idle);
        view.pending = Some((NetbirdPending::Connect, Some(State::Idle)));
        assert_eq!(view.tile().1, t(Key::Connecting));
        assert!(!view.apply(NetbirdUpdate::Status(Box::new(status(State::Idle))), SystemTime::now()));
        assert!(view.pending.is_some(), "the same state is not an answer");
        assert!(view.apply(NetbirdUpdate::Status(Box::new(status(State::Connecting))), SystemTime::now()));
        assert!(view.pending.is_none());
    }

    #[test]
    fn only_visible_changes_repaint_the_panel() {
        let mut view = view_with(State::Connected);
        let mut busy = status(State::Connected);
        busy.events.push(nb::Event::default());
        assert!(!view.apply(NetbirdUpdate::Status(Box::new(busy)), SystemTime::now()), "events are not drawn");
        let mut peer = status(State::Connected);
        peer.peers.push(nb::Peer::default());
        assert!(view.apply(NetbirdUpdate::Status(Box::new(peer)), SystemTime::now()));
        assert!(!view.apply(NetbirdUpdate::Config(Box::default()), SystemTime::now()));
    }

    #[test]
    fn rows_resolve_to_the_action_they_offer() {
        let mut view = view_with(State::Connected);
        view.apply(
            NetbirdUpdate::Networks(vec![
                network("office", "10.0.0.0/8", true),
                network("zurich", "0.0.0.0/0", false),
                network("berlin", "0.0.0.0/0", true),
            ]),
            SystemTime::now(),
        );
        view.apply(
            NetbirdUpdate::Profiles(vec![
                nb::Profile { id: "default".into(), name: "default".into(), active: true, email: None },
                nb::Profile { id: "work".into(), name: "Work".into(), active: false, email: None },
            ]),
            SystemTime::now(),
        );
        let row = |index: usize| row(&view, u16::try_from(index).expect("index"));
        assert_eq!(row(STATUS_ROW), None, "connected needs no sign-in");
        assert_eq!(row(PROFILE_ROWS), None, "the active profile is already active");
        assert_eq!(row(PROFILE_ROWS + 1), Some(NetbirdRow::Profile("work".into())));
        assert_eq!(row(EXIT_NONE_ROW), Some(NetbirdRow::Exit { id: "berlin".into(), select: false }));
        assert_eq!(row(EXIT_ROWS), Some(NetbirdRow::Exit { id: "berlin".into(), select: false }));
        assert_eq!(row(EXIT_ROWS + 1), Some(NetbirdRow::Exit { id: "zurich".into(), select: true }));
        assert_eq!(row(EXIT_ROWS + 2), None);
        assert_eq!(row(PEERS_ROW), Some(NetbirdRow::Peers));
        let login = view_with(State::NeedsLogin);
        assert_eq!(super::row(&login, 0), Some(NetbirdRow::SignIn));
    }

    #[test]
    fn the_session_row_appears_only_while_connected_and_counts_down() {
        let now = SystemTime::now();
        let mut connected = status(State::Connected);
        connected.session_expires_at = Some(now + Duration::from_hours(2));
        let mut view = NetbirdView::default();
        view.apply(NetbirdUpdate::Status(Box::new(connected.clone())), now);
        assert_eq!(view.session, Some(Remaining::Hours(2)));
        assert_eq!(row(&view, 1), Some(NetbirdRow::Session { expired: false }));
        assert_eq!(view.session_left(now + Duration::from_hours(3)), Some(Remaining::Expired));
        let idle = nb::Status { state: State::Idle, ..connected };
        view.apply(NetbirdUpdate::Status(Box::new(idle)), now);
        assert_eq!(view.session, None);
    }
}
