use std::time::{Duration, SystemTime};

use glide_netbird::{self as nb, PeerStatus, Remaining, State};
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Fill};

use super::{
    BundleForm, Busy, Confirm, Msg, Page, PeerFilter, ResourceFilter, Setting, T, Tab, parse_minutes, parse_mtu,
    parse_port, valid_interface, valid_url,
};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::panels::Ctx;
use crate::services::system::format_bytes;
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};
use crate::widgets::{actions, form_row, lines, note, sheet};

const DOCS: &str = "https://docs.netbird.io";
const GITHUB: &str = "https://github.com/netbirdio/netbird";
const FORUM: &str = "https://forum.netbird.io";
const SLACK: &str = "https://docs.netbird.io/slack-url";
const BUGS: &str = "https://docs.netbird.io/help/report-bug-issues";
const TERMS: &str = "https://netbird.io/terms";
const PRIVACY: &str = "https://netbird.io/privacy";
const MAX_EVENTS: usize = 10;

#[must_use]
pub(crate) const fn state_label(state: State) -> T {
    match state {
        State::Connected => T::Connected,
        State::Connecting => T::Connecting,
        State::Idle => T::Disconnected,
        State::NeedsLogin => T::LoginRequired,
        State::LoginFailed => T::LoginFailed,
        State::SessionExpired => T::SessionExpired,
        State::Unavailable => T::ServiceDown,
    }
}

fn remaining_text(c: &Ctx, remaining: Remaining) -> String {
    let count = |key: T, n: u64| c.t(key).replace("{n}", &n.to_string());
    match remaining {
        Remaining::Expired => c.t(T::SessionExpired).to_string(),
        Remaining::LessThanMinute => c.t(T::LessThanMinute).to_string(),
        Remaining::Minutes(n) => count(T::Minutes, n),
        Remaining::Hours(n) => count(T::Hours, n),
        Remaining::Days(n) => count(T::Days, n),
    }
}

fn ago(c: &Ctx, when: Option<SystemTime>) -> String {
    let Some(when) = when else {
        return c.t(T::Never).to_string();
    };
    match SystemTime::now().duration_since(when) {
        Ok(elapsed) if elapsed >= Duration::from_mins(1) => {
            c.t(T::Ago).replace("{time}", &remaining_text(c, nb::remaining(Some(elapsed))))
        }
        _ => c.t(T::JustNow).to_string(),
    }
}

fn address(ip: &str) -> &str {
    ip.split('/').next().unwrap_or(ip)
}

fn count(c: &Ctx, key: T, n: usize, total: usize) -> String {
    c.t(key).replace("{n}", &n.to_string()).replace("{total}", &total.to_string())
}

fn matches(query: &str, fields: &[&str]) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || fields.iter().any(|field| field.to_lowercase().contains(&query))
}

#[must_use]
pub(crate) fn filtered_peers(page: &Page) -> Vec<&nb::Peer> {
    let mut peers: Vec<&nb::Peer> = page
        .snapshot
        .status
        .peers
        .iter()
        .filter(|peer| match page.peer_filter {
            PeerFilter::All => true,
            PeerFilter::Online => peer.status == PeerStatus::Connected,
            PeerFilter::Offline => peer.status != PeerStatus::Connected,
        })
        .filter(|peer| matches(&page.peer_query, &[&peer.fqdn, &peer.ip, &peer.ipv6]))
        .collect();
    peers.sort_by_key(|peer| (peer.status != PeerStatus::Connected, peer.name().to_lowercase()));
    peers
}

#[must_use]
pub(crate) fn filtered_resources(page: &Page) -> Vec<nb::Network> {
    let all = nb::resources(&page.snapshot.networks);
    all.iter()
        .filter(|network| match page.resource_filter {
            ResourceFilter::All => true,
            ResourceFilter::Active => network.selected,
            ResourceFilter::Overlapping => all.iter().any(|other| network.overlaps(other)),
        })
        .filter(|network| {
            let domains = network.domains.join(" ");
            matches(&page.resource_query, &[&network.id, &network.range, &domains])
        })
        .cloned()
        .collect()
}

fn pressable(row: Row<'_, Msg>, message: Option<Msg>) -> Row<'_, Msg> {
    match message {
        Some(message) => row.on_press(message),
        None => row,
    }
}

const fn category_label(category: nb::Category) -> T {
    match category {
        nb::Category::Network => T::Network,
        nb::Category::Dns => T::Dns,
        nb::Category::Authentication => T::CatAuthentication,
        nb::Category::Connectivity => T::CatConnectivity,
        nb::Category::System => T::CatSystem,
    }
}

fn update_notice(status: &nb::Status) -> Option<(String, bool)> {
    status.events.iter().rev().find_map(|event| match nb::classify(event) {
        nb::Notice::UpdateAvailable { version, enforced } => Some((version, enforced)),
        _ => None,
    })
}

impl Page {
    #[must_use]
    pub fn view(&self, c: Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        if !self.installed {
            return widgets::empty_state(t, Icon::Link, c.t(T::NotInstalled), c.t(T::NotInstalledHint));
        }
        if !self.loaded {
            return widgets::empty_state(t, Icon::Link, c.t(K::Loading), "");
        }
        if self.snapshot.status.state == State::Unavailable {
            return widgets::empty_state(t, Icon::Link, c.t(T::ServiceDown), c.t(T::ServiceDownHint));
        }
        let mut page = column![self.hero(&c)].spacing(theme::SECTION_GAP);
        for banner in self.banners(&c) {
            page = page.push(banner);
        }
        page = page.push(self.tabs(&c));
        let content = match self.tab {
            Tab::Overview => self.overview(&c),
            Tab::Peers => self.peers(&c),
            Tab::Resources => self.resources(&c),
            Tab::Settings => self.settings(&c),
            Tab::Troubleshoot => self.troubleshoot(&c),
            Tab::About => self.about(&c),
        };
        page.push(content).into()
    }

    fn hero(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let status = &self.snapshot.status;
        let mut line = c.t(state_label(status.state)).to_string();
        if status.state.is_connected() {
            line = format!(
                "{line} \u{b7} {} \u{b7} {}",
                address(&status.local.ip),
                count(c, T::PeersCount, status.connected_peers(), status.peers.len())
            );
        }
        let trailing: Element<'_, Msg> = match &self.busy {
            Some(Busy::Connect) => widgets::spinner_label(t, c.t(T::Connecting)),
            Some(Busy::Disconnect) => widgets::spinner_label(t, c.t(T::Disconnecting)),
            Some(Busy::SignIn) => widgets::spinner_label(t, c.t(T::SigningIn)),
            _ if status.state.needs_login() => widgets::primary(t, c.t(T::SignIn), Some(Msg::SignIn)),
            _ => widgets::switch(t, status.state.is_up(), |on| if on { Msg::Connect } else { Msg::Disconnect }),
        };
        widgets::section_hero(t, Icon::Link, super::super::Panel::Netbird.color(), c.t(T::Title), line, Some(trailing))
    }

    fn banners(&self, c: &Ctx) -> Vec<Element<'_, Msg>> {
        let t = c.theme;
        let mut banners = Vec::new();
        if !self.snapshot.compatible {
            banners.push(widgets::banner(t, crate::theme::Tone::Warn, Icon::Warning, c.t(T::Outdated), None));
        }
        if let Some(error) = &self.error {
            banners.push(widgets::banner(t, crate::theme::Tone::Danger, Icon::Warning, error.as_str(), None));
        }
        if let Some((summary, command)) = &self.privilege {
            banners.push(widgets::banner(t, crate::theme::Tone::Warn, Icon::Lock, summary.as_str(), None));
            if !command.is_empty() {
                let copy = widgets::secondary(t, c.t(T::Copy), Some(Msg::CopyText(command.clone())));
                banners.push(
                    Group::new(t)
                        .push(Row::new(t, c.t(T::PrivilegeHint)).subtitle(command.clone()).suffix(copy).build())
                        .build(),
                );
            }
        }
        banners
    }

    fn tabs(&self, c: &Ctx) -> Element<'_, Msg> {
        let features = self.snapshot.features;
        let mut options =
            vec![(Tab::Overview, c.t(T::TabOverview).to_string()), (Tab::Peers, c.t(T::TabPeers).to_string())];
        if !features.disable_networks {
            options.push((Tab::Resources, c.t(T::TabResources).to_string()));
        }
        options.extend([
            (Tab::Settings, c.t(T::TabSettings).to_string()),
            (Tab::Troubleshoot, c.t(T::TabTroubleshoot).to_string()),
            (Tab::About, c.t(T::TabAbout).to_string()),
        ]);
        if c.theme.compact {
            let chips = options.into_iter().map(|(tab, label)| {
                widgets::chip(c.theme, label, tab == self.tab, (tab != self.tab).then_some(Msg::Tab(tab)))
            });
            return iced::widget::Row::with_children(chips).spacing(6).wrap().vertical_spacing(6).into();
        }
        container(widgets::segmented(c.theme, options, self.tab, Msg::Tab)).center_x(Fill).into()
    }

    fn overview(&self, c: &Ctx) -> Element<'_, Msg> {
        let mut page = column![self.connection(c)].spacing(theme::SECTION_GAP);
        if self.snapshot.status.state.is_connected() && !self.snapshot.features.disable_networks {
            page = page.push(self.exit_nodes(c));
        }
        if !self.snapshot.features.disable_profiles {
            page = page.push(self.profiles(c));
        }
        page.push(self.events(c)).into()
    }

    fn summary_facts(&self, c: &Ctx) -> Vec<glide_sdk::widget::Fact> {
        let snapshot = &self.snapshot;
        if !snapshot.status.state.is_connected() {
            return Vec::new();
        }
        let mut facts = Vec::new();
        if !snapshot.features.disable_profiles && !snapshot.active.name.is_empty() {
            facts.push(glide_sdk::widget::Fact::new(c.t(T::Profiles), &snapshot.active.name));
        }
        facts.push(glide_sdk::widget::Fact::new(
            c.t(T::TabPeers),
            count(c, T::PeersCount, snapshot.status.connected_peers(), snapshot.status.peers.len()),
        ));
        if !snapshot.features.disable_networks {
            let resources = nb::resources(&snapshot.networks);
            facts.push(glide_sdk::widget::Fact::new(
                c.t(T::TabResources),
                count(
                    c,
                    T::ResourcesCount,
                    resources.iter().filter(|resource| resource.selected).count(),
                    resources.len(),
                ),
            ));
        }
        facts
    }

    fn connection(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let status = &self.snapshot.status;
        let link = |link: &nb::Link| {
            if link.connected {
                c.t(T::Reachable).to_string()
            } else if link.error.is_empty() {
                c.t(T::Unreachable).to_string()
            } else {
                link.error.clone()
            }
        };
        let mut info = vec![
            (c.t(T::Management).to_string(), link(&status.management)),
            (c.t(T::Signal).to_string(), link(&status.signal)),
            (
                c.t(T::Relays).to_string(),
                count(
                    c,
                    T::RelaysLine,
                    status.relays.iter().filter(|relay| relay.available).count(),
                    status.relays.len(),
                ),
            ),
        ];
        let local = &status.local;
        for (key, value) in
            [(T::NetbirdIp, address(&local.ip)), (T::NetbirdIpv6, address(&local.ipv6)), (T::Domain, &local.fqdn)]
        {
            if !value.is_empty() {
                info.push((c.t(key).to_string(), value.to_string()));
            }
        }
        if status.state.is_connected() {
            let kind = c.t(if local.kernel_interface { T::Kernel } else { T::Userspace });
            info.push((
                c.t(T::Interface).to_string(),
                format!("{kind} \u{b7} {} {}", c.t(T::Port), local.wireguard_port),
            ));
            let enabled = status.dns.iter().filter(|group| group.enabled).count();
            info.push((c.t(T::Dns).to_string(), c.t(T::DnsLine).replace("{n}", &enabled.to_string())));
            let ssh = if status.ssh.enabled {
                c.t(T::SshSessions).replace("{n}", &status.ssh.sessions.len().to_string())
            } else {
                c.t(T::Off).to_string()
            };
            info.push((c.t(T::SshServer).to_string(), ssh));
            if status.forwarding_rules > 0 {
                info.push((
                    c.t(T::Forwarding).to_string(),
                    c.t(T::ForwardingLine).replace("{n}", &status.forwarding_rules.to_string()),
                ));
            }
        }
        info.push((c.t(T::DaemonVersion).to_string(), status.daemon_version.clone()));
        let mut group = Group::new(t).title(c.t(T::Connection));
        let summary = self.summary_facts(c);
        if !summary.is_empty() {
            group = group.push(glide_sdk::widget::kv_grid(&t.paint, summary, if t.compact { 1 } else { 3 }));
        }
        if status.management.error.is_empty() && status.signal.error.is_empty() {
            let facts = info.drain(..3).map(|(label, value)| glide_sdk::widget::Fact::new(label, value)).collect();
            group = group.push(glide_sdk::widget::kv_grid(&t.paint, facts, if t.compact { 1 } else { 3 }));
        }
        group = group.push(lines(t, info));
        if let Some(row) = self.session_row(c) {
            group = group.push(row);
        }
        if !self.snapshot.forwarding.is_empty() {
            let rules = self
                .snapshot
                .forwarding
                .iter()
                .map(|rule| {
                    let target = if rule.translated_hostname.is_empty() {
                        &rule.translated_address
                    } else {
                        &rule.translated_hostname
                    };
                    (format!("{} {}", rule.protocol, rule.destination), format!("{target}:{}", rule.translated_port))
                })
                .collect();
            group = group.push(sheet(t, vec![lines(t, rules)]));
        }
        group.build()
    }

    fn session_row(&self, c: &Ctx) -> Option<Element<'_, Msg>> {
        let t = c.theme;
        let status = &self.snapshot.status;
        let deadline = status.session_expires_at.filter(|_| status.state.is_connected())?;
        let left = nb::remaining_until(deadline, SystemTime::now());
        let expired = left == Remaining::Expired;
        let subtitle = if expired {
            c.t(T::SessionExpired).to_string()
        } else {
            c.t(T::SessionLeft).replace("{time}", &remaining_text(c, left))
        };
        let action: Element<'_, Msg> = match &self.busy {
            Some(Busy::Extend) => widgets::spinner_label(t, c.t(T::Renewing)),
            Some(Busy::SignIn) => widgets::spinner_label(t, c.t(T::SigningIn)),
            _ if expired => widgets::primary(t, c.t(T::SignIn), Some(Msg::SignIn)),
            _ => widgets::secondary(t, c.t(T::Renew), Some(Msg::Extend)),
        };
        Some(
            Row::new(t, c.t(T::Session))
                .subtitle(subtitle)
                .prefix(widgets::icon(Icon::Clock, 18.0, t.fg_2()))
                .suffix(action)
                .build(),
        )
    }

    fn exit_nodes(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let exits = nb::exit_nodes(&self.snapshot.networks);
        let mut group = Group::new(t).title(c.t(T::ExitNode)).description(c.t(T::ExitNodeHint));
        if exits.is_empty() {
            return group.push(Row::new(t, c.t(T::ExitNoneAvailable)).dimmed(true).build()).build();
        }
        let idle = self.busy.is_none();
        let none = exits.iter().all(|exit| !exit.selected);
        let row = Row::new(t, c.t(T::ExitNone)).subtitle(c.t(T::ExitNoneHint)).prefix(widgets::radio_mark(t, none));
        group = group.push(pressable(row, (idle && !none).then_some(Msg::ExitNode(None))).build());
        for exit in exits {
            let mut row = Row::new(t, exit.id.clone()).prefix(widgets::radio_mark(t, exit.selected));
            if exit.selected {
                row = row.subtitle(c.t(T::Active));
            }
            group =
                group.push(pressable(row, (idle && !exit.selected).then_some(Msg::ExitNode(Some(exit.id)))).build());
        }
        group.build()
    }

    fn profiles(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Profiles)).description(c.t(T::ProfilesHint));
        for profile in &self.snapshot.profiles {
            group = group.push(self.profile_row(c, profile));
            if self.renaming.as_deref() == Some(profile.id.as_str()) {
                group = group.push(self.rename_sheet(c));
            }
            match &self.confirm {
                Some(confirm @ (Confirm::Remove(id) | Confirm::Deregister(id))) if *id == profile.id => {
                    let (message, button) = match confirm {
                        Confirm::Remove(_) => (T::DeleteConfirm, T::Delete),
                        Confirm::Deregister(_) => (T::DeregisterConfirm, T::Deregister),
                    };
                    group = group.push(sheet(
                        t,
                        vec![
                            note(t, Icon::Warning, c.t(message)),
                            actions(vec![
                                widgets::secondary(t, c.t(T::Cancel), Some(Msg::CancelConfirm)),
                                widgets::danger(t, c.t(button), Some(Msg::Confirmed(confirm.clone()))),
                            ]),
                        ],
                    ));
                }
                _ => {}
            }
        }
        group = group.push(
            Row::new(t, c.t(T::AddProfile))
                .prefix(widgets::icon(Icon::Plus, 18.0, t.acc_text()))
                .suffix(widgets::icon(if self.adding { Icon::ChevronUp } else { Icon::ChevronDown }, 14.0, t.fg_3()))
                .on_press(Msg::ToggleAdd)
                .build(),
        );
        if self.adding {
            group = group.push(self.add_sheet(c));
        }
        group.build()
    }

    fn profile_row<'a>(&'a self, c: &Ctx, profile: &'a nb::Profile) -> Element<'a, Msg> {
        let t = c.theme;
        let idle = self.busy.is_none();
        let subtitle = profile.email.clone().unwrap_or_else(|| c.t(T::NoEmail).to_string());
        let mut suffix = row![].spacing(4).align_y(Alignment::Center);
        suffix = suffix.push(match &self.busy {
            Some(Busy::Switch(id)) if *id == profile.id => widgets::spinner_label(t, c.t(T::Switching)),
            _ if profile.active => widgets::status_pill(t, Tone::Ok, c.t(T::Active)),
            _ => widgets::secondary(t, c.t(T::Switch), idle.then(|| Msg::Switch(profile.id.clone()))),
        });
        suffix = suffix
            .push(widgets::icon_button(t, Icon::Pencil, idle.then(|| Msg::StartRename(profile.id.clone()))))
            .push(widgets::icon_button(
                t,
                Icon::External,
                idle.then(|| Msg::Ask(Confirm::Deregister(profile.id.clone()))),
            ));
        let deletable = idle && !profile.active && !profile.is_default();
        suffix = suffix.push(widgets::icon_button(
            t,
            Icon::Trash,
            deletable.then(|| Msg::Ask(Confirm::Remove(profile.id.clone()))),
        ));
        Row::new(t, profile.name.clone())
            .subtitle(subtitle)
            .prefix(widgets::icon(Icon::User, 18.0, if profile.active { t.acc_text() } else { t.fg_2() }))
            .suffix(suffix)
            .build()
    }

    fn rename_sheet(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let valid = !self.rename_to.trim().is_empty();
        sheet(
            t,
            vec![form_row(
                t,
                c.t(T::ProfileName),
                None,
                row![
                    widgets::field(
                        t,
                        c.t(T::ProfileName),
                        &self.rename_to,
                        Msg::RenameInput,
                        valid.then_some(Msg::Rename),
                        false
                    )
                    .width(Fill),
                    widgets::secondary(t, c.t(T::Cancel), Some(Msg::CancelRename)),
                    widgets::primary(t, c.t(T::Save), valid.then_some(Msg::Rename)),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            )],
        )
    }

    fn add_sheet(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let url_ok = !self.new_self_hosted || valid_url(&self.new_url);
        let ready = !self.new_name.trim().is_empty() && url_ok && self.busy.is_none();
        let mut rows = vec![
            form_row(
                t,
                c.t(T::ProfileName),
                None,
                widgets::field(
                    t,
                    c.t(T::ProfileNamePlaceholder),
                    &self.new_name,
                    Msg::NewName,
                    ready.then_some(Msg::AddProfile),
                    false,
                )
                .width(Fill)
                .into(),
            ),
            form_row(
                t,
                c.t(T::ManagementServer),
                None,
                widgets::segmented(
                    t,
                    vec![(false, c.t(T::Cloud).to_string()), (true, c.t(T::SelfHosted).to_string())],
                    self.new_self_hosted,
                    Msg::NewSelfHosted,
                ),
            ),
        ];
        if self.new_self_hosted {
            rows.push(form_row(
                t,
                c.t(T::ManagementUrl),
                (!url_ok && !self.new_url.is_empty()).then(|| c.t(T::UrlInvalid)),
                widgets::field(
                    t,
                    c.t(T::ManagementUrlPlaceholder),
                    &self.new_url,
                    Msg::NewUrl,
                    ready.then_some(Msg::AddProfile),
                    !url_ok,
                )
                .width(Fill)
                .into(),
            ));
        }
        rows.push(actions(vec![if self.busy == Some(Busy::Profile) {
            widgets::spinner_label(t, c.t(K::Working))
        } else {
            widgets::primary(t, c.t(T::AddProfile), ready.then_some(Msg::AddProfile))
        }]));
        sheet(t, rows)
    }

    fn events(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Events));
        let events: Vec<&nb::Event> = self
            .snapshot
            .status
            .events
            .iter()
            .rev()
            .filter(|event| !event.user_message.is_empty() || !event.message.is_empty())
            .filter(|event| !matches!(nb::classify(event), nb::Notice::Silent) || !event.user_message.is_empty())
            .take(MAX_EVENTS)
            .collect();
        if events.is_empty() {
            return group.push(Row::new(t, c.t(T::NoEvents)).dimmed(true).build()).build();
        }
        for event in events {
            let tone = match event.severity {
                nb::Severity::Info => Tone::Neutral,
                nb::Severity::Warning => Tone::Warn,
                nb::Severity::Error | nb::Severity::Critical => Tone::Danger,
            };
            let message = if event.user_message.is_empty() { &event.message } else { &event.user_message };
            group = group.push(
                Row::new(t, message.clone())
                    .subtitle(ago(c, event.timestamp))
                    .suffix(widgets::pill(t, tone, c.t(category_label(event.category))))
                    .build(),
            );
        }
        group.build()
    }

    fn not_connected(&self, c: &Ctx) -> Option<Element<'_, Msg>> {
        (!self.snapshot.status.state.is_connected())
            .then(|| widgets::list_empty(c.theme, Icon::Link, c.t(T::NotConnected), c.t(T::NotConnectedHint)))
    }

    fn peers(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let status = &self.snapshot.status;
        let mut group = Group::new(t).title(c.t(T::TabPeers)).description(count(
            c,
            T::PeersCount,
            status.connected_peers(),
            status.peers.len(),
        ));
        if let Some(empty) = self.not_connected(c) {
            return group.push(empty).build();
        }
        let filters = widgets::segmented(
            t,
            vec![
                (PeerFilter::All, c.t(T::All).to_string()),
                (PeerFilter::Online, c.t(T::Online).to_string()),
                (PeerFilter::Offline, c.t(T::Offline).to_string()),
            ],
            self.peer_filter,
            Msg::PeerFilter,
        );
        group = group.push(
            container(
                row![
                    widgets::field(t, c.t(T::PeersSearch), &self.peer_query, Msg::PeerQuery, None, false).width(Fill),
                    filters
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([8.0, theme::ROW_PADDING_X]),
        );
        if status.peers.is_empty() {
            return group.push(widgets::list_empty(t, Icon::Users, c.t(T::NoPeers), c.t(T::NoPeersHint))).build();
        }
        let peers = filtered_peers(self);
        if peers.is_empty() {
            return group.push(widgets::list_empty(t, Icon::Search, c.t(T::NoMatches), c.t(T::NoMatchesHint))).build();
        }
        for peer in peers {
            group = group.push(self.peer_row(c, peer));
            if self.expanded_peer.as_deref() == Some(peer.pub_key.as_str()) {
                group = group.push(peer_sheet(c, peer));
            }
        }
        group.build()
    }

    fn peer_row<'a>(&'a self, c: &Ctx, peer: &'a nb::Peer) -> Element<'a, Msg> {
        let t = c.theme;
        let (tone, label) = match peer.status {
            PeerStatus::Connected => (Tone::Ok, T::Connected),
            PeerStatus::Connecting => (Tone::Warn, T::PeerConnecting),
            PeerStatus::Disconnected => (Tone::Neutral, T::PeerDisconnected),
        };
        let mut parts = vec![peer.ip.clone()];
        if peer.status == PeerStatus::Connected {
            parts.push(c.t(if peer.relayed { T::Relayed } else { T::P2p }).to_string());
            if let Some(latency) = peer.latency.filter(|latency| !latency.is_zero()) {
                parts.push(format!("{} ms", latency.as_millis()));
            }
        }
        let subtitle = parts.join(" \u{b7} ");
        let open = self.expanded_peer.as_deref() == Some(peer.pub_key.as_str());
        Row::new(t, peer.name().to_string())
            .subtitle(subtitle)
            .prefix(widgets::status_dot(if peer.status == PeerStatus::Connected { t.ok() } else { t.fg_4() }, 8.0))
            .suffix(
                row![
                    widgets::status_pill(t, tone, c.t(label)),
                    widgets::icon(if open { Icon::ChevronUp } else { Icon::ChevronDown }, 14.0, t.fg_3())
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .on_press(Msg::ExpandPeer(peer.pub_key.clone()))
            .build()
    }

    fn resources(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let all = nb::resources(&self.snapshot.networks);
        let active = all.iter().filter(|network| network.selected).count();
        let mut group =
            Group::new(t).title(c.t(T::TabResources)).description(count(c, T::ResourcesCount, active, all.len()));
        if let Some(empty) = self.not_connected(c) {
            return group.push(empty).build();
        }
        if all.is_empty() {
            return group
                .push(widgets::list_empty(t, Icon::Globe, c.t(T::NoResources), c.t(T::NoResourcesHint)))
                .build();
        }
        let idle = self.busy.is_none();
        let filters = widgets::segmented(
            t,
            vec![
                (ResourceFilter::All, c.t(T::All).to_string()),
                (ResourceFilter::Active, c.t(T::ResourcesActive).to_string()),
                (ResourceFilter::Overlapping, c.t(T::Overlapping).to_string()),
            ],
            self.resource_filter,
            Msg::ResourceFilter,
        );
        group = group.push(
            container(
                row![
                    widgets::field(t, c.t(T::ResourcesSearch), &self.resource_query, Msg::ResourceQuery, None, false)
                        .width(Fill),
                    filters
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([8.0, theme::ROW_PADDING_X]),
        );
        let shown = filtered_resources(self);
        group = group.push(actions(vec![
            widgets::secondary(t, c.t(T::DisableAll), (idle && !shown.is_empty()).then_some(Msg::AllResources(false))),
            widgets::tonal(t, c.t(T::EnableAll), (idle && !shown.is_empty()).then_some(Msg::AllResources(true))),
        ]));
        if shown.is_empty() {
            return group.push(widgets::list_empty(t, Icon::Search, c.t(T::NoMatches), c.t(T::NoMatchesHint))).build();
        }
        for network in shown {
            let open = self.expanded_resource.as_deref() == Some(network.id.as_str());
            let detail = if network.domains.is_empty() { network.range.clone() } else { network.domains.join(", ") };
            let id = network.id.clone();
            let toggle: Element<'_, Msg> = if self.busy == Some(Busy::Resource(network.id.clone())) {
                widgets::spinner_label(t, "")
            } else if idle {
                widgets::switch(t, network.selected, move |on| Msg::Resource(id.clone(), on))
            } else {
                widgets::switch_disabled(t, network.selected)
            };
            group = group.push(
                Row::new(t, network.id.clone())
                    .subtitle(detail)
                    .suffix(
                        row![
                            toggle,
                            widgets::icon(if open { Icon::ChevronUp } else { Icon::ChevronDown }, 14.0, t.fg_3())
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Msg::ExpandResource(network.id.clone()))
                    .build(),
            );
            if open {
                group = group.push(resource_sheet(c, &network));
            }
        }
        group.build()
    }

    fn settings(&self, c: &Ctx) -> Element<'_, Msg> {
        let Some(config) = &self.snapshot.config else {
            return widgets::empty_state(c.theme, Icon::Link, c.t(K::Loading), "");
        };
        let mut page = column![self.general(c, config)].spacing(theme::SECTION_GAP);
        page = page.push(self.switches(
            c,
            config,
            T::Network,
            &[
                Setting::ClientRoutes,
                Setting::ServerRoutes,
                Setting::Dns,
                Setting::Ipv6,
                Setting::Monitor,
                Setting::Lazy,
            ],
        ));
        let mut security = vec![Setting::BlockInbound, Setting::BlockLan, Setting::Quantum];
        if config.rosenpass_enabled {
            security.push(Setting::Permissive);
        }
        page = page.push(self.switches(c, config, T::Security, &security));
        page = page.push(self.ssh(c, config));
        if self.snapshot.features.disable_advanced_view != Some(true) {
            page = page.push(self.advanced(c, config));
        }
        page.into()
    }

    fn setting_row(&self, c: &Ctx, config: &nb::Config, setting: Setting) -> Element<'_, Msg> {
        let t = c.theme;
        let (title, hint) = setting.text();
        let value = setting.value(config);
        let managed = setting.mdm_key().is_some_and(|key| config.managed(key));
        let control: Element<'_, Msg> = if managed || self.busy.is_some() {
            widgets::switch_disabled(t, value)
        } else {
            widgets::switch(t, value, move |on| Msg::Set(setting, on))
        };
        let suffix: Element<'_, Msg> = if managed {
            row![widgets::pill(t, Tone::Neutral, c.t(T::Managed)), control].spacing(8).align_y(Alignment::Center).into()
        } else {
            control
        };
        Row::new(t, c.t(title)).subtitle(c.t(hint)).suffix(suffix).build()
    }

    fn switches(&self, c: &Ctx, config: &nb::Config, title: T, settings: &[Setting]) -> Element<'_, Msg> {
        let mut group = Group::new(c.theme).title(c.t(title));
        for setting in settings {
            group = group.push(self.setting_row(c, config, *setting));
        }
        group.build()
    }

    fn general(&self, c: &Ctx, config: &nb::Config) -> Element<'_, Msg> {
        let t = c.theme;
        let managed = config.managed("managementURL");
        let idle = self.busy.is_none() && !managed;
        let mut group = Group::new(t).title(c.t(T::General)).push(form_row(
            t,
            c.t(T::ManagementServer),
            Some(c.t(T::ManagementHint)),
            widgets::inert(
                t,
                idle,
                widgets::segmented(
                    t,
                    vec![(false, c.t(T::Cloud).to_string()), (true, c.t(T::SelfHosted).to_string())],
                    self.self_hosted,
                    Msg::SelfHosted,
                ),
            ),
        ));
        if self.self_hosted {
            let valid = valid_url(&self.management_url);
            let changed = self.management_url.trim() != config.management_url;
            let save = (idle && valid && changed).then_some(Msg::SaveManagement);
            group = group.push(form_row(
                t,
                c.t(T::ManagementUrl),
                (!valid && !self.management_url.is_empty()).then(|| c.t(T::UrlInvalid)),
                row![
                    widgets::field(
                        t,
                        c.t(T::ManagementUrlPlaceholder),
                        &self.management_url,
                        Msg::ManagementUrl,
                        save.clone(),
                        !valid
                    )
                    .width(Fill),
                    widgets::primary(t, c.t(T::Save), save),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
        if managed {
            group = group.push(note(t, Icon::Lock, c.t(T::Managed)));
        }
        group
            .push(self.setting_row(c, config, Setting::ConnectOnStartup))
            .push(self.setting_row(c, config, Setting::Notifications))
            .build()
    }

    fn ssh(&self, c: &Ctx, config: &nb::Config) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Ssh)).push(self.setting_row(c, config, Setting::SshServer));
        if config.server_ssh_allowed {
            for setting in
                [Setting::SshRoot, Setting::Sftp, Setting::LocalForward, Setting::RemoteForward, Setting::Jwt]
            {
                group = group.push(self.setting_row(c, config, setting));
            }
            if !config.disable_ssh_auth {
                let ttl_ok = self.jwt_ttl.trim().parse::<i32>().is_ok_and(|ttl| ttl >= 0);
                let save =
                    (ttl_ok && self.busy.is_none() && self.jwt_ttl.trim() != config.ssh_jwt_cache_ttl.to_string())
                        .then_some(Msg::SaveJwtTtl);
                group = group.push(form_row(
                    t,
                    c.t(T::JwtTtl),
                    Some(c.t(T::JwtTtlHint)),
                    row![
                        widgets::field(t, "0", &self.jwt_ttl, Msg::JwtTtlInput, save.clone(), !ttl_ok).width(120.0),
                        text(c.t(T::Seconds), TextStyle::CALL).color(t.fg_3()),
                        widgets::secondary(t, c.t(T::Save), save),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                ));
            }
        }
        group.build()
    }

    fn advanced(&self, c: &Ctx, config: &nb::Config) -> Element<'_, Msg> {
        let t = c.theme;
        let name_ok = valid_interface(self.interface.trim());
        let port_ok = parse_port(&self.port).is_some();
        let mtu_ok = parse_mtu(&self.mtu).is_some();
        let port_managed = config.managed("wireguardPort");
        let changed = self.interface.trim() != config.interface_name
            || self.port.trim() != config.wireguard_port.to_string()
            || self.mtu.trim() != config.mtu.to_string();
        let save = (name_ok && port_ok && mtu_ok && changed && self.busy.is_none()).then_some(Msg::SaveAdvanced);
        let field = |placeholder: &str, value: &str, on: fn(String) -> Msg, ok: bool| {
            widgets::field(t, placeholder, value, on, save.clone(), !ok).width(Fill)
        };
        let psk_managed = config.managed("preSharedKey");
        let psk_save = (!self.psk.0.trim().is_empty() && self.busy.is_none() && !psk_managed).then_some(Msg::SavePsk);
        let mut psk = row![
            widgets::secret_field(
                t,
                c.t(T::PskPlaceholder),
                &self.psk.0,
                |text| Msg::PskInput(crate::services::network::Secret(text)),
                psk_save.clone(),
                false,
                (self.psk_shown, Msg::TogglePsk),
            ),
            widgets::secondary(t, c.t(T::Save), psk_save),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if config.pre_shared_key_set && !psk_managed {
            psk = psk.push(widgets::danger(t, c.t(T::PskClear), self.busy.is_none().then_some(Msg::ClearPsk)));
        }
        Group::new(t)
            .title(c.t(T::Advanced))
            .push(form_row(
                t,
                c.t(T::InterfaceName),
                (!name_ok).then(|| c.t(T::InterfaceInvalid)),
                field("wt0", &self.interface, Msg::InterfaceInput, name_ok).into(),
            ))
            .push(form_row(
                t,
                c.t(T::WireguardPort),
                Some(c.t(if port_ok { T::PortHint } else { T::PortInvalid })),
                widgets::inert(t, !port_managed, field("51820", &self.port, Msg::PortInput, port_ok)),
            ))
            .push(form_row(
                t,
                c.t(T::Mtu),
                (!mtu_ok).then(|| c.t(T::MtuInvalid)),
                field("1280", &self.mtu, Msg::MtuInput, mtu_ok).into(),
            ))
            .push(actions(vec![widgets::primary(t, c.t(T::Save), save)]))
            .push(form_row(
                t,
                c.t(T::Psk),
                Some(if config.pre_shared_key_set { c.t(T::PskSet) } else { c.t(T::PskHint) }),
                widgets::inert(t, !psk_managed, psk),
            ))
            .build()
    }

    fn troubleshoot(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let form = self.bundle;
        let working = self.bundling.is_some();
        let set = move |change: fn(&mut BundleForm, bool)| {
            move |on: bool| {
                let mut next = form;
                change(&mut next, on);
                Msg::BundleChanged(next)
            }
        };
        let switch = |value: bool, change: fn(&mut BundleForm, bool)| -> Element<'_, Msg> {
            if working { widgets::switch_disabled(t, value) } else { widgets::switch(t, value, set(change)) }
        };
        let anonymize = widgets::segmented(
            t,
            vec![
                (nb::Anonymize::None, c.t(T::AnonymizeNone).to_string()),
                (nb::Anonymize::Default, c.t(T::AnonymizeDefault).to_string()),
                (nb::Anonymize::Strict, c.t(T::AnonymizeStrict).to_string()),
            ],
            form.anonymize,
            move |anonymize| Msg::BundleChanged(BundleForm { anonymize, ..form }),
        );
        let mut group = Group::new(t)
            .title(c.t(T::Bundle))
            .description(c.t(T::BundleHint))
            .push(form_row(t, c.t(T::Anonymize), Some(c.t(T::AnonymizeHint)), widgets::inert(t, !working, anonymize)))
            .push(
                Row::new(t, c.t(T::SystemInfo))
                    .subtitle(c.t(T::SystemInfoHint))
                    .suffix(switch(form.system_info, |f, on| f.system_info = on))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Upload))
                    .subtitle(c.t(T::UploadHint))
                    .suffix(switch(form.upload, |f, on| f.upload = on))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Trace))
                    .subtitle(c.t(T::TraceHint))
                    .suffix(switch(form.trace, |f, on| f.trace = on))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::CaptureSession))
                    .subtitle(c.t(T::CaptureSessionHint))
                    .suffix(switch(form.capture, |f, on| f.capture = on))
                    .build(),
            );
        if form.capture {
            let minutes_ok = self.duration.trim().is_empty() || parse_minutes(&self.duration).is_some();
            group = group
                .push(form_row(
                    t,
                    c.t(T::Duration),
                    Some(c.t(T::DurationHint)),
                    widgets::field(t, "1", &self.duration, Msg::DurationInput, None, !minutes_ok).width(120.0).into(),
                ))
                .push(
                    Row::new(t, c.t(T::Packets))
                        .subtitle(c.t(T::PacketsHint))
                        .suffix(switch(form.packets, |f, on| f.packets = on))
                        .build(),
                );
        }
        let action = if working {
            row![
                widgets::spinner_label(t, c.t(if self.capturing() { T::Capturing } else { T::Collecting })),
                widgets::secondary(t, c.t(T::Cancel), self.capturing().then_some(Msg::CancelBundle)),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        } else {
            widgets::primary(t, c.t(T::Create), Some(Msg::CreateBundle))
        };
        group = group.push(actions(vec![action]));
        if let Some(result) = &self.bundle_result {
            group = group.push(bundle_sheet(c, result));
        }
        group.build()
    }

    fn about(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let versions = Group::new(t).title(c.t(T::Versions)).push(lines(
            t,
            vec![
                (c.t(T::ServiceVersion).to_string(), self.snapshot.status.daemon_version.clone()),
                (c.t(T::GlideVersion).to_string(), env!("CARGO_PKG_VERSION").to_string()),
            ],
        ));
        let mut update = Group::new(t).title(c.t(T::Update));
        match update_notice(&self.snapshot.status) {
            Some((version, enforced)) if !self.snapshot.features.disable_update_settings => {
                let action = match (&self.busy, enforced) {
                    (Some(Busy::Update), _) => widgets::spinner_label(t, c.t(K::Working)),
                    (_, true) => widgets::primary(t, c.t(T::InstallNow), Some(Msg::InstallUpdate)),
                    (_, false) => widgets::secondary(t, c.t(T::Download), Some(Msg::OpenLink(nb::DOWNLOAD_URL))),
                };
                let mut row = Row::new(t, c.t(T::UpdateAvailable).replace("{version}", &version)).suffix(action);
                if enforced {
                    row = row.subtitle(c.t(T::UpdateEnforced));
                }
                update = update.push(row.build());
            }
            _ => update = update.push(Row::new(t, c.t(T::UpdateNone)).dimmed(true).build()),
        }
        let mut links = Group::new(t).title(c.t(T::Links));
        for (key, url) in [
            (T::Documentation, DOCS),
            (T::GitHub, GITHUB),
            (T::Forum, FORUM),
            (T::Slack, SLACK),
            (T::ReportBug, BUGS),
            (T::Terms, TERMS),
            (T::Privacy, PRIVACY),
        ] {
            links = links.push(
                Row::new(t, c.t(key))
                    .suffix(widgets::icon(Icon::External, 14.0, t.fg_3()))
                    .on_press(Msg::OpenLink(url))
                    .build(),
            );
        }
        column![versions.build(), update.build(), links.build()].spacing(theme::SECTION_GAP).into()
    }
}

fn peer_sheet<'a>(c: &Ctx, peer: &nb::Peer) -> Element<'a, Msg> {
    let t = c.theme;
    let ice = |ice: &nb::Ice| {
        if ice.endpoint.is_empty() { ice.kind.clone() } else { format!("{} \u{b7} {}", ice.kind, ice.endpoint) }
    };
    let mut info = vec![(c.t(T::NetbirdIp).to_string(), peer.ip.clone())];
    for (key, value) in
        [(T::NetbirdIpv6, peer.ipv6.clone()), (T::Domain, peer.fqdn.clone()), (T::PublicKey, peer.pub_key.clone())]
    {
        if !value.is_empty() {
            info.push((c.t(key).to_string(), value));
        }
    }
    if peer.status == PeerStatus::Connected {
        info.push((c.t(T::Connection2).to_string(), c.t(if peer.relayed { T::Relayed } else { T::P2p }).to_string()));
        info.push((c.t(T::LocalIce).to_string(), ice(&peer.local_ice)));
        info.push((c.t(T::RemoteIce).to_string(), ice(&peer.remote_ice)));
        if peer.relayed && !peer.relay_address.is_empty() {
            info.push((c.t(T::Relay).to_string(), peer.relay_address.clone()));
        }
        if let Some(latency) = peer.latency {
            info.push((c.t(T::Latency).to_string(), format!("{} ms", latency.as_millis())));
        }
    }
    info.push((c.t(T::LastHandshake).to_string(), ago(c, peer.last_handshake)));
    info.push((c.t(T::StatusSince).to_string(), ago(c, peer.status_since)));
    info.push((
        c.t(T::Transfer).to_string(),
        c.t(T::TransferLine)
            .replace("{rx}", &format_bytes(peer.bytes_rx, c.lang))
            .replace("{tx}", &format_bytes(peer.bytes_tx, c.lang)),
    ));
    info.push((c.t(T::Rosenpass).to_string(), c.t(if peer.rosenpass { T::On } else { T::Off }).to_string()));
    if !peer.networks.is_empty() {
        info.push((c.t(T::PeerResources).to_string(), peer.networks.join(", ")));
    }
    let copy = widgets::secondary(t, c.t(T::Copy), Some(Msg::CopyText(peer.ip.clone())));
    sheet(t, vec![lines(t, info), actions(vec![copy])])
}

fn resource_sheet<'a>(c: &Ctx, network: &nb::Network) -> Element<'a, Msg> {
    let t = c.theme;
    let mut info = Vec::new();
    if !network.range.is_empty() {
        info.push((c.t(T::Network).to_string(), network.range.clone()));
    }
    for (domain, ips) in &network.resolved_ips {
        info.push((domain.clone(), ips.join(", ")));
    }
    if info.is_empty() {
        info.push((c.t(T::ResolvedIps).to_string(), "\u{2014}".to_string()));
    }
    sheet(
        t,
        vec![
            container(widgets::micro_label(t, c.t(T::ResolvedIps))).padding([4.0, theme::ROW_PADDING_X]).into(),
            lines(t, info),
        ],
    )
}

fn bundle_sheet<'a>(c: &Ctx, result: &nb::BundleResult) -> Element<'a, Msg> {
    let t = c.theme;
    let mut rows = Vec::new();
    if result.uploaded_key.is_empty() {
        rows.push(note(t, Icon::Check, c.t(T::Saved)));
    } else {
        rows.push(note(t, Icon::Check, c.t(T::Uploaded)));
        rows.push(
            Row::new(t, c.t(T::UploadKey))
                .subtitle(result.uploaded_key.clone())
                .suffix(widgets::secondary(t, c.t(T::Copy), Some(Msg::CopyText(result.uploaded_key.clone()))))
                .build(),
        );
    }
    if !result.upload_failure.is_empty() {
        rows.push(note(t, Icon::Warning, &c.t(T::UploadFailed).replace("{reason}", &result.upload_failure)));
    }
    if !result.path.is_empty() {
        rows.push(
            Row::new(t, result.path.clone())
                .suffix(widgets::secondary(t, c.t(T::OpenFolder), Some(Msg::OpenFolder(result.path.clone()))))
                .build(),
        );
    }
    sheet(t, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx {
            theme: crate::theme::ThemeState::new(crate::theme::Appearance::Dark, crate::theme::Accent::default()),
            lang: crate::i18n::Lang::Uk,
            writable: true,
            online: true,
        }
    }

    fn populated() -> Page {
        let mut page = Page { installed: true, loaded: true, ..Page::default() };
        page.snapshot.active = nb::ActiveProfile { id: "default".into(), name: "default".into(), ..Default::default() };
        let now = SystemTime::now();
        page.snapshot.status = nb::Status {
            state: State::Connected,
            daemon_version: "0.80.0".into(),
            session_expires_at: Some(now + Duration::from_hours(1)),
            peers: vec![
                nb::Peer {
                    fqdn: "laptop.netbird.cloud".into(),
                    status: PeerStatus::Connected,
                    relayed: true,
                    latency: Some(Duration::from_millis(12)),
                    last_handshake: Some(now),
                    networks: vec!["10.0.0.0/8".into()],
                    ..nb::Peer::default()
                },
                nb::Peer { ip: "100.64.0.9".into(), ..nb::Peer::default() },
            ],
            events: vec![nb::Event {
                user_message: "DNS changed".into(),
                timestamp: Some(now),
                ..nb::Event::default()
            }],
            ..nb::Status::default()
        };
        page.snapshot.config = Some(nb::Config {
            server_ssh_allowed: true,
            rosenpass_enabled: true,
            pre_shared_key_set: true,
            mdm_managed: vec!["allowServerSSH".into(), "managementURL".into()],
            ..nb::Config::default()
        });
        page.snapshot.profiles = vec![
            nb::Profile {
                id: "default".into(),
                name: "default".into(),
                active: true,
                email: Some("me@example.com".into()),
            },
            nb::Profile { id: "work".into(), name: "Work".into(), active: false, email: None },
        ];
        page.snapshot.networks = vec![
            nb::Network { id: "office".into(), range: "10.0.0.0/8".into(), selected: true, ..nb::Network::default() },
            nb::Network { id: "exit".into(), range: "0.0.0.0/0".into(), selected: true, ..nb::Network::default() },
        ];
        page.snapshot.compatible = true;
        page.bundle_result = Some(nb::BundleResult {
            path: "/tmp/b.zip".into(),
            uploaded_key: "k".into(),
            upload_failure: String::new(),
        });
        page.privilege = Some(("Needs root".into(), "sudo netbird up --allow-server-ssh".into()));
        page
    }

    #[test]
    fn summary_uses_connected_peers_and_excludes_exit_nodes() {
        let mut page = populated();
        let c = ctx();
        assert_eq!(
            page.summary_facts(&c),
            vec![
                glide_sdk::widget::Fact::new(c.t(T::Profiles), "default"),
                glide_sdk::widget::Fact::new(c.t(T::TabPeers), count(&c, T::PeersCount, 1, 2)),
                glide_sdk::widget::Fact::new(c.t(T::TabResources), count(&c, T::ResourcesCount, 1, 1)),
            ]
        );
        page.snapshot.features.disable_profiles = true;
        page.snapshot.features.disable_networks = true;
        assert_eq!(page.summary_facts(&c).len(), 1);
        page.snapshot.status.state = State::Idle;
        assert!(page.summary_facts(&c).is_empty());
    }

    #[test]
    #[ignore = "writes a populated NetBird preview"]
    fn preview_populated_netbird() {
        use iced::advanced::renderer::{self, Headless};
        use iced::advanced::widget::Tree;
        use iced::advanced::{Layout, layout};
        use iced::{Font, Pixels, Rectangle, Renderer, Size, mouse};

        glide_sdk::theme::motion::set_scale(0.0);
        glide_sdk::theme::use_ui_family(None);
        let mut c = ctx();
        c.theme = crate::theme::ThemeState::new(crate::theme::Appearance::Light, crate::theme::Accent::default());
        let mut page = populated();
        page.privilege = None;
        let mut element = page.view(c);
        let mut tree = Tree::new(&element);
        let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
        let size = Size::new(880.0, 2200.0);
        let node = element.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &c.theme.paint.iced_theme(),
            &renderer::Style { text_color: c.theme.fg_1() },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::with_size(size),
        );
        let Renderer::Secondary(renderer) = &mut renderer else { unreachable!() };
        let rgba = renderer.screenshot(Size::new(880, 2200), 1.0, c.theme.win_bg());
        let path = std::env::var_os("GLIDE_NETBIRD_PREVIEW")
            .map_or_else(|| std::path::PathBuf::from("/tmp/opencode/netbird-populated.png"), std::path::PathBuf::from);
        image::save_buffer(path, &rgba, 880, 2200, image::ColorType::Rgba8).expect("preview is saved");
    }

    #[test]
    fn every_section_builds_from_a_populated_snapshot() {
        let mut page = populated();
        page.adding = true;
        page.new_self_hosted = true;
        page.renaming = Some("work".into());
        page.confirm = Some(Confirm::Deregister("default".into()));
        page.expanded_peer = Some(String::new());
        page.expanded_resource = Some("office".into());
        page.bundle.capture = true;
        page.self_hosted = true;
        for tab in [Tab::Overview, Tab::Peers, Tab::Resources, Tab::Settings, Tab::Troubleshoot, Tab::About] {
            page.tab = tab;
            drop(page.view(ctx()));
        }
        for state in [State::Idle, State::NeedsLogin, State::Unavailable] {
            page.snapshot.status.state = state;
            page.tab = Tab::Peers;
            drop(page.view(ctx()));
        }
        drop(Page::default().view(ctx()));
    }

    #[test]
    fn peers_are_found_by_name_or_address_and_listed_online_first() {
        let peer =
            |fqdn: &str, ip: &str, status| nb::Peer { fqdn: fqdn.into(), ip: ip.into(), status, ..nb::Peer::default() };
        let mut page = Page::default();
        page.snapshot.status.peers = vec![
            peer("zeta.netbird.cloud", "100.64.0.9", PeerStatus::Connected),
            peer("alpha.netbird.cloud", "100.64.0.2", PeerStatus::Disconnected),
            peer("beta.netbird.cloud", "100.64.0.3", PeerStatus::Connected),
        ];
        let names = |page: &Page| filtered_peers(page).iter().map(|peer| peer.name().to_string()).collect::<Vec<_>>();
        assert_eq!(names(&page), ["beta", "zeta", "alpha"]);
        page.peer_filter = PeerFilter::Offline;
        assert_eq!(names(&page), ["alpha"]);
        page.peer_filter = PeerFilter::All;
        page.peer_query = "0.9".into();
        assert_eq!(names(&page), ["zeta"]);
    }

    #[test]
    fn resources_filter_out_exit_nodes_and_find_overlaps() {
        let network = |id: &str, range: &str, selected| nb::Network {
            id: id.into(),
            range: range.into(),
            selected,
            ..nb::Network::default()
        };
        let mut page = Page::default();
        page.snapshot.networks = vec![
            network("a", "10.0.0.0/8", true),
            network("b", "10.0.0.0/8", false),
            network("c", "192.168.1.0/24", true),
            network("exit", "0.0.0.0/0", true),
        ];
        let ids = |page: &Page| filtered_resources(page).into_iter().map(|network| network.id).collect::<Vec<_>>();
        assert_eq!(ids(&page), ["a", "b", "c"]);
        page.resource_filter = ResourceFilter::Active;
        assert_eq!(ids(&page), ["a", "c"]);
        page.resource_filter = ResourceFilter::Overlapping;
        assert_eq!(ids(&page), ["a", "b"]);
        page.resource_filter = ResourceFilter::All;
        page.resource_query = "192.168".into();
        assert_eq!(ids(&page), ["c"]);
    }
}
