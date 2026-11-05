use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use glide_paint::Painter;
use glide_theme::Color;

use crate::protocol::packet::{
    NetworkPacket, PACKET_TYPE_NOTIFICATION_ACTION, PACKET_TYPE_NOTIFICATION_REPLY, PACKET_TYPE_NOTIFICATION_REQUEST,
};

const ICON_SIDE: i32 = 96;
const SILHOUETTE_SIDE: i32 = 52;

static NEXT_NOTIF_SEQ: AtomicU64 = AtomicU64::new(1);
static DESKTOP_NOTIFY_IDS: Mutex<Option<HashMap<u64, u32>>> = Mutex::new(None);

fn remember_desktop_notify_id(seq: u64, id: u32) {
    let mut guard = DESKTOP_NOTIFY_IDS.lock().unwrap_or_else(PoisonError::into_inner);
    let map = guard.get_or_insert_with(HashMap::new);
    if map.len() > 256 {
        map.clear();
    }
    map.insert(seq, id);
}

fn lookup_desktop_notify_id(seq: u64) -> Option<u32> {
    DESKTOP_NOTIFY_IDS.lock().unwrap_or_else(PoisonError::into_inner).as_ref()?.get(&seq).copied()
}

fn take_desktop_notify_id(seq: u64) -> Option<u32> {
    DESKTOP_NOTIFY_IDS.lock().unwrap_or_else(PoisonError::into_inner).as_mut()?.remove(&seq)
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn icon_cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join("glide-phone-icons")
}

#[must_use]
pub fn is_silhouette_bgra(bgra: &[u8]) -> bool {
    let total_pixels = bgra.len() / 4;
    if total_pixels == 0 {
        return true;
    }
    let mut visible = 0usize;
    let mut opaque = 0usize;
    let mut r_sum = 0u64;
    let mut g_sum = 0u64;
    let mut b_sum = 0u64;

    for px in bgra.chunks_exact(4) {
        let a = px[3];
        if a < 48 {
            continue;
        }
        visible += 1;
        if a >= 200 {
            let a_u16 = u16::from(a);
            let b = ((u16::from(px[0]) * 255) / a_u16).min(255);
            let g = ((u16::from(px[1]) * 255) / a_u16).min(255);
            let r = ((u16::from(px[2]) * 255) / a_u16).min(255);
            opaque += 1;
            r_sum += u64::from(r);
            g_sum += u64::from(g);
            b_sum += u64::from(b);
        }
    }

    if visible == 0 || opaque == 0 {
        return true;
    }

    let avg_r = (r_sum / (opaque as u64)) as i32;
    let avg_g = (g_sum / (opaque as u64)) as i32;
    let avg_b = (b_sum / (opaque as u64)) as i32;

    let mut uniform_tight = 0usize;
    let mut distinct_tone = 0usize;
    for px in bgra.chunks_exact(4) {
        let a = px[3];
        if a < 200 {
            continue;
        }
        let a_u16 = u16::from(a);
        let b = ((u16::from(px[0]) * 255) / a_u16).min(255) as i32;
        let g = ((u16::from(px[1]) * 255) / a_u16).min(255) as i32;
        let r = ((u16::from(px[2]) * 255) / a_u16).min(255) as i32;
        let dr = (r - avg_r).abs();
        let dg = (g - avg_g).abs();
        let db = (b - avg_b).abs();
        if dr <= 10 && dg <= 10 && db <= 10 {
            uniform_tight += 1;
        }
        if dr > 22 || dg > 22 || db > 22 {
            distinct_tone += 1;
        }
    }

    uniform_tight * 100 >= opaque * 95 && distinct_tone * 100 < opaque
}

fn sanitize_hash(hash: &str) -> String {
    hash.chars().filter(char::is_ascii_alphanumeric).take(64).collect()
}

fn resolve_payload_bytes(payload_png: Option<&[u8]>, payload_hash: Option<&str>) -> Option<Vec<u8>> {
    let clean_hash = payload_hash.map(sanitize_hash).filter(|s| !s.is_empty());
    if let Some(bytes) = payload_png.filter(|b| !b.is_empty()) {
        if let Some(hash) = &clean_hash
            && let Ok(dir) = std::fs::create_dir_all(icon_cache_dir()).map(|()| icon_cache_dir())
        {
            let raw_path = dir.join(format!("raw-{hash}.bin"));
            if !raw_path.is_file() {
                let _ = std::fs::write(raw_path, bytes);
            }
        }
        return Some(bytes.to_vec());
    }
    let hash = clean_hash?;
    let raw_path = icon_cache_dir().join(format!("raw-{hash}.bin"));
    std::fs::read(raw_path).ok().filter(|b| !b.is_empty())
}

fn decode_payload_rounded(data: &[u8]) -> Option<Vec<u8>> {
    if let Some(rendered) = glide_paint::render_png_rounded(data, ICON_SIDE, 20.0) {
        return Some(rendered);
    }
    glide_paint::decode_cover(data, ICON_SIDE)
}

#[must_use]
pub fn payload_is_rich_avatar(payload_png: Option<&[u8]>) -> bool {
    let Some(bytes) = payload_png else {
        return false;
    };
    let Some(decoded) = decode_payload_rounded(bytes) else {
        return false;
    };
    !is_silhouette_bgra(&decoded)
}

fn app_icon_candidates(app_name: &str) -> Vec<String> {
    let lower = app_name.trim().to_lowercase();
    let mut out = Vec::new();
    let mapped: &[&str] = match lower.as_str() {
        "telegram" | "telegram x" | "nekogram" | "plus messenger" | "ayugram" | "forkgram" => {
            &["org.telegram.desktop", "telegram", "telegram-desktop", "io.github.tdesktop_x64.TDesktop"]
        }
        "whatsapp" | "whatsapp business" => {
            &["whatsapp", "com.whatsapp", "whatsapp-desktop", "io.github.mimbrero.WhatsAppDesktop", "com.rtosta.zapzap"]
        }
        "signal" => &["org.signal.Signal", "signal-desktop", "signal"],
        "viber" => &["com.viber.Viber", "viber"],
        "discord" => &["com.discordapp.Discord", "discord", "dev.vencord.Vesktop", "vesktop"],
        "instagram" => &["instagram", "com.instagram.Instagram"],
        "youtube" | "youtube revanced" | "revanced" => &["youtube", "com.google.android.youtube", "freetube"],
        "gmail" => &["gmail", "org.gnome.Geary", "thunderbird", "org.mozilla.Thunderbird", "mail-client"],
        "messages" | "google messages" | "повідомлення" | "sms" => {
            &["messages", "org.gnome.Polari", "internet-chat", "chat"]
        }
        "slack" => &["com.slack.Slack", "slack"],
        "spotify" | "youtube music" => &["com.spotify.Client", "spotify"],
        "chrome" | "google chrome" => &["google-chrome", "chromium", "com.google.Chrome"],
        "firefox" => &["firefox", "org.mozilla.firefox"],
        "github" => &["github-desktop", "github"],
        "facebook" | "messenger" => &["facebook", "messenger", "caprine"],
        "phone" | "телефон" | "дзвінки" => &["glide-phone", "phone"],
        _ => &[],
    };
    for &item in mapped {
        out.push(item.to_string());
    }
    if !app_name.trim().is_empty() {
        out.push(app_name.trim().to_string());
        out.push(lower.replace(' ', "-"));
        out.push(lower);
    }
    out
}

fn preferred_icon_themes() -> &'static [String] {
    static THEMES: OnceLock<Vec<String>> = OnceLock::new();
    THEMES.get_or_init(|| {
        fn push_unique(themes: &mut Vec<String>, name: &str) {
            let trimmed = name.trim().trim_matches('"').trim_matches('\'');
            if !trimmed.is_empty() && !themes.iter().any(|existing| existing == trimmed) {
                themes.push(trimmed.to_string());
            }
        }

        let mut themes = Vec::new();
        if let Some(config) = glide_config::config_home() {
            if let Ok(toml_text) = std::fs::read_to_string(config.join("glide/glide.toml")) {
                for line in toml_text.lines() {
                    if let Some((key, val)) = line.split_once('=')
                        && matches!(key.trim(), "icon-theme" | "icon_theme")
                    {
                        push_unique(&mut themes, val);
                    }
                }
            }
            for dir in ["gtk-3.0", "gtk-4.0"] {
                if let Ok(ini) = std::fs::read_to_string(config.join(dir).join("settings.ini")) {
                    for line in ini.lines() {
                        if let Some((key, val)) = line.split_once('=')
                            && key.trim() == "gtk-icon-theme-name"
                        {
                            push_unique(&mut themes, val);
                        }
                    }
                }
            }
        }

        let roots = glide_shell::icon::icon_roots();
        let resolved = glide_theme::icons::resolve(
            themes.first().map(String::as_str),
            glide_theme::Appearance::Dark,
            glide_theme::Accent::Blue,
            &roots,
        );
        push_unique(&mut themes, &resolved);
        for candidate in [
            "Glide-Colloid-Dark",
            "Colloid-Dark",
            "Colloid",
            "Papirus-Dark",
            "Papirus",
            "Tela",
            "WhiteSur",
            "MoreWaita",
            "breeze-dark",
            "breeze",
            "Adwaita",
            "hicolor",
        ] {
            push_unique(&mut themes, candidate);
        }
        themes
    })
}

fn load_themed_app_icon(app_name: &str) -> Option<Vec<u8>> {
    let mut candidates = app_icon_candidates(app_name);
    let lower = app_name.trim().to_lowercase();
    for app in glide_shell::icon::list_applications() {
        if (app.name.to_lowercase() == lower || app.id.to_lowercase() == lower)
            && !app.icon.is_empty()
            && !candidates.contains(&app.icon)
        {
            candidates.insert(0, app.icon);
        }
    }

    for theme in preferred_icon_themes() {
        let mut loader = glide_shell::icon::IconLoader::new(Some(theme.clone()));
        for candidate in &candidates {
            if let Some(bytes) = loader.load_named(candidate, ICON_SIDE)
                && !is_silhouette_bgra(bytes)
            {
                return Some(bytes.to_vec());
            }
            if let Some(bytes) = loader.load(candidate, ICON_SIDE)
                && !is_silhouette_bgra(bytes)
            {
                return Some(bytes.to_vec());
            }
        }
    }
    None
}

fn brand_app_gradient(app_name: &str) -> (Color, Color) {
    let lower = app_name.trim().to_lowercase();
    match lower.as_str() {
        "telegram" | "telegram x" | "nekogram" | "plus messenger" | "ayugram" | "forkgram" => {
            (Color([0.16, 0.67, 0.93, 1.0]), Color([0.12, 0.58, 0.82, 1.0]))
        }
        "whatsapp" | "whatsapp business" => (Color([0.15, 0.83, 0.40, 1.0]), Color([0.07, 0.55, 0.49, 1.0])),
        "viber" => (Color([0.45, 0.38, 0.95, 1.0]), Color([0.35, 0.27, 0.82, 1.0])),
        "signal" => (Color([0.23, 0.46, 0.94, 1.0]), Color([0.17, 0.36, 0.77, 1.0])),
        "discord" => (Color([0.35, 0.40, 0.95, 1.0]), Color([0.28, 0.32, 0.77, 1.0])),
        "instagram" => (Color([0.96, 0.38, 0.25, 1.0]), Color([0.76, 0.21, 0.52, 1.0])),
        "youtube" | "youtube revanced" | "revanced" => (Color([1.0, 0.12, 0.20, 1.0]), Color([0.80, 0.0, 0.16, 1.0])),
        "gmail" => (Color([0.92, 0.26, 0.21, 1.0]), Color([0.77, 0.13, 0.12, 1.0])),
        "spotify" => (Color([0.11, 0.73, 0.33, 1.0]), Color([0.08, 0.53, 0.24, 1.0])),
        "facebook" | "messenger" | "messages" | "google messages" | "повідомлення" | "sms" => {
            (Color([0.10, 0.52, 0.98, 1.0]), Color([0.05, 0.38, 0.82, 1.0]))
        }
        _ => {
            const PALETTE: [(Color, Color); 6] = [
                (Color([0.18, 0.60, 0.96, 1.0]), Color([0.12, 0.44, 0.82, 1.0])),
                (Color([0.16, 0.72, 0.52, 1.0]), Color([0.10, 0.54, 0.38, 1.0])),
                (Color([0.56, 0.38, 0.96, 1.0]), Color([0.42, 0.26, 0.82, 1.0])),
                (Color([0.94, 0.40, 0.30, 1.0]), Color([0.78, 0.26, 0.18, 1.0])),
                (Color([0.90, 0.58, 0.16, 1.0]), Color([0.76, 0.42, 0.10, 1.0])),
                (Color([0.20, 0.68, 0.76, 1.0]), Color([0.12, 0.50, 0.58, 1.0])),
            ];
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            lower.hash(&mut hasher);
            let idx = usize::try_from(hasher.finish() % (PALETTE.len() as u64)).unwrap_or(0);
            PALETTE[idx]
        }
    }
}

fn composite_subimage_over(dst: &mut [u8], dst_side: i32, src: &[u8], src_side: i32, offset_x: i32, offset_y: i32) {
    for sy in 0..src_side {
        let dy = offset_y + sy;
        if dy < 0 || dy >= dst_side {
            continue;
        }
        for sx in 0..src_side {
            let dx = offset_x + sx;
            if dx < 0 || dx >= dst_side {
                continue;
            }
            let s_idx = ((sy * src_side + sx) * 4) as usize;
            let d_idx = ((dy * dst_side + dx) * 4) as usize;
            let sa = u16::from(src[s_idx + 3]);
            if sa == 0 {
                continue;
            }
            let inv = 255 - sa;
            for c in 0..4 {
                let val = u16::from(src[s_idx + c]) + (u16::from(dst[d_idx + c]) * inv + 127) / 255;
                dst[d_idx + c] = val.min(255) as u8;
            }
        }
    }
}

fn render_brand_tile_with_silhouette(app_name: &str, silhouette_png: Option<&[u8]>) -> Vec<u8> {
    let mut buf = vec![0u8; (ICON_SIDE * ICON_SIDE * 4) as usize];
    let (from, to) = brand_app_gradient(app_name);
    glide_paint::fill_rounded_rect_gradient_over(&mut buf, ICON_SIDE, ICON_SIDE, 6, 6, 84, 84, 1, from, to, 22.0);
    glide_paint::stroke_rounded_rect_over(
        &mut buf,
        ICON_SIDE,
        ICON_SIDE,
        6,
        6,
        84,
        84,
        1,
        1.0,
        22.0,
        Color([1.0, 1.0, 1.0, 0.18]),
    );

    if let Some(png_bytes) = silhouette_png
        && let Some(tinted) = glide_paint::render_png_tinted(png_bytes, SILHOUETTE_SIDE, Color([1.0, 1.0, 1.0, 0.96]))
    {
        let offset = (ICON_SIDE - SILHOUETTE_SIDE) / 2;
        composite_subimage_over(&mut buf, ICON_SIDE, &tinted, SILHOUETTE_SIDE, offset, offset);
        return buf;
    }

    Painter::fill_rounded_rect_over(
        &mut buf,
        ICON_SIDE,
        ICON_SIDE,
        24,
        28,
        48,
        34,
        1,
        Color([1.0, 1.0, 1.0, 0.95]),
        10.0,
    );
    Painter::fill_rounded_rect_over(&mut buf, ICON_SIDE, ICON_SIDE, 32, 38, 32, 4, 1, to, 2.0);
    Painter::fill_rounded_rect_over(&mut buf, ICON_SIDE, ICON_SIDE, 32, 48, 22, 4, 1, to, 2.0);
    buf
}

fn render_base_app_icon(app_name: &str, payload_png: Option<&[u8]>) -> (Vec<u8>, bool) {
    if let Some(png_bytes) = payload_png
        && let Some(rendered) = decode_payload_rounded(png_bytes)
        && !is_silhouette_bgra(&rendered)
    {
        return (rendered, true);
    }

    if let Some(themed) = load_themed_app_icon(app_name) {
        return (themed, false);
    }

    (render_brand_tile_with_silhouette(app_name, payload_png), false)
}

fn overlay_phone_badge(buf: &mut [u8]) {
    Painter::fill_rounded_rect_over(
        buf,
        ICON_SIDE,
        ICON_SIDE,
        50,
        50,
        46,
        46,
        1,
        Color([0.06, 0.07, 0.10, 0.96]),
        23.0,
    );
    Painter::fill_rounded_rect_over(buf, ICON_SIDE, ICON_SIDE, 53, 53, 40, 40, 1, Color([0.23, 0.51, 0.96, 1.0]), 20.0);
    glide_paint::stroke_rounded_rect_over(
        buf,
        ICON_SIDE,
        ICON_SIDE,
        66,
        61,
        14,
        24,
        1,
        2.2,
        3.5,
        Color([1.0, 1.0, 1.0, 1.0]),
    );
    Painter::fill_rounded_rect_over(buf, ICON_SIDE, ICON_SIDE, 71, 64, 4, 2, 1, Color([1.0, 1.0, 1.0, 0.92]), 1.0);
    Painter::fill_rounded_rect_over(buf, ICON_SIDE, ICON_SIDE, 72, 79, 2, 2, 1, Color([1.0, 1.0, 1.0, 0.95]), 1.0);
}

#[must_use]
pub fn build_badged_notification_icon(
    app_name: &str, payload_png: Option<&[u8]>, payload_hash: Option<&str>,
) -> Option<String> {
    build_badged_notification_icon_with_kind(app_name, payload_png, payload_hash).map(|(path, _)| path)
}

fn build_badged_notification_icon_with_kind(
    app_name: &str, payload_png: Option<&[u8]>, payload_hash: Option<&str>,
) -> Option<(String, bool)> {
    let dir = icon_cache_dir();
    std::fs::create_dir_all(&dir).ok()?;

    let resolved_bytes = resolve_payload_bytes(payload_png, payload_hash);
    let (mut buf, has_avatar) = render_base_app_icon(app_name, resolved_bytes.as_deref());
    overlay_phone_badge(&mut buf);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    buf.hash(&mut hasher);
    let file_path = dir.join(format!("notif-{:016x}.png", hasher.finish()));

    if !file_path.is_file() {
        let side_u32 = u32::try_from(ICON_SIDE).ok()?;
        let png_bytes = glide_shell::image::Image::from_bgra(side_u32, side_u32, buf)?.png().ok()?;
        std::fs::write(&file_path, png_bytes).ok()?;
    }
    Some((file_path.to_string_lossy().into_owned(), has_avatar))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPayload {
    pub id: String,
    #[serde(rename = "appName", default)]
    pub app_name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub ticker: Option<String>,
    #[serde(rename = "isCancel", default)]
    pub is_cancel: bool,
    #[serde(default)]
    pub dismissable: Option<bool>,
    #[serde(default)]
    pub silent: Option<bool>,
    #[serde(rename = "requestReplyId", default)]
    pub request_reply_id: Option<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(rename = "payloadHash", default)]
    pub payload_hash: Option<String>,
}

impl NotificationPayload {
    pub fn from_packet(packet: &NetworkPacket) -> Result<Self, serde_json::Error> {
        serde_json::from_value(packet.body.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveNotification {
    pub id: String,
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub request_reply_id: Option<String>,
    pub actions: Vec<String>,
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub icon_path: Option<String>,
    #[serde(default)]
    pub has_avatar: bool,
    #[serde(default)]
    pub notified_desktop: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationEvent {
    Added(ActiveNotification),
    Dismissed(String),
}

#[must_use]
pub fn create_reply_packet(reply_id: &str, message: &str) -> NetworkPacket {
    NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION_REPLY,
        json!({
            "requestReplyId": reply_id,
            "message": message,
        }),
    )
}

#[must_use]
pub fn create_action_packet(key: &str, action: &str) -> NetworkPacket {
    NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION_ACTION,
        json!({
            "key": key,
            "action": action,
        }),
    )
}

#[must_use]
pub fn create_request_packet() -> NetworkPacket {
    NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION_REQUEST,
        json!({
            "request": true,
        }),
    )
}

#[must_use]
pub fn create_dismiss_packet(notif_id: &str) -> NetworkPacket {
    NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION_REQUEST,
        json!({
            "cancel": notif_id,
        }),
    )
}

#[must_use]
pub fn extract_auth_code(text: &str) -> Option<String> {
    let sanitized: String = text.chars().filter(|c| !c.is_control() && *c != '\u{2063}').collect();
    for word in sanitized.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if clean.len() >= 4 && clean.len() <= 8 && clean.chars().all(|c| c.is_ascii_digit()) {
            return Some(clean.to_string());
        }
    }
    None
}

fn dispatch_desktop_notification(device_name: &str, active: &ActiveNotification, is_upgrade: bool) {
    let summary = if active.title.is_empty() {
        format!("{device_name}: {}", active.app_name)
    } else {
        format!("{device_name}: {}", active.title)
    };
    let icon_name = active.icon_path.as_deref().unwrap_or("glide-phone");
    let mut notification = glide_sdk::system::Notification::new("glide-phone", summary)
        .origin_device(device_name)
        .body(&active.body)
        .icon(icon_name)
        .urgency(glide_sdk::system::Urgency::Normal);

    if is_upgrade {
        notification = notification.silent(true);
    }

    if active.request_reply_id.is_some() {
        notification = notification.reply("Reply", "Type a reply...");
    }

    for action in &active.actions {
        notification = notification.action(action.as_str(), action.as_str());
    }

    let seq = active.seq;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let mut existing_id = lookup_desktop_notify_id(seq);
            if is_upgrade && existing_id.is_none() {
                for _ in 0..25 {
                    tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                    existing_id = lookup_desktop_notify_id(seq);
                    if existing_id.is_some() {
                        break;
                    }
                }
            }
            if let Some(id) = existing_id {
                notification = notification.replaces(id);
            }
            if let Ok(id) = glide_sdk::system::notify(notification).await
                && id != 0
            {
                remember_desktop_notify_id(seq, id);
            }
        });
    }
}

struct IncomingNotification<'a> {
    id: &'a str,
    app_name: &'a str,
    title: &'a str,
    body: &'a str,
    has_avatar: bool,
    reply_id: Option<&'a str>,
    actions: &'a [String],
    silent: bool,
}

fn is_bare_notification(has_avatar: bool, reply_id: Option<&str>, actions: &[String]) -> bool {
    !has_avatar && reply_id.is_none() && actions.is_empty()
}

fn notifications_are_siblings(existing: &ActiveNotification, incoming: &IncomingNotification<'_>) -> bool {
    if existing.id == incoming.id || existing.app_name != incoming.app_name {
        return false;
    }
    if existing.title == incoming.title && existing.body == incoming.body {
        return true;
    }
    let existing_bare =
        is_bare_notification(existing.has_avatar, existing.request_reply_id.as_deref(), &existing.actions);
    let incoming_bare = is_bare_notification(incoming.has_avatar, incoming.reply_id, incoming.actions);
    if existing_bare == incoming_bare {
        return false;
    }
    let (bare_id, bare_title, bare_body, rich_title, rich_body) = if existing_bare {
        (existing.id.as_str(), existing.title.as_str(), existing.body.as_str(), incoming.title, incoming.body)
    } else {
        (incoming.id, incoming.title, incoming.body, existing.title.as_str(), existing.body.as_str())
    };
    bare_id.contains("|null|")
        || bare_title.is_empty()
        || bare_title.eq_ignore_ascii_case(incoming.app_name)
        || bare_title == rich_title
        || (!rich_body.is_empty() && bare_body.contains(rich_body))
        || (!rich_title.is_empty() && bare_body.contains(rich_title))
}

fn upgrade_sibling_notification<S: BuildHasher>(
    device_name: &str, other_id: &str, incoming: &IncomingNotification<'_>, built_icon: Option<String>,
    active_notifications: &mut HashMap<String, ActiveNotification, S>,
) -> Option<NotificationEvent> {
    let existing = active_notifications.get(other_id)?;
    let existing_bare =
        is_bare_notification(existing.has_avatar, existing.request_reply_id.as_deref(), &existing.actions);
    let incoming_bare = is_bare_notification(incoming.has_avatar, incoming.reply_id, incoming.actions);

    let gains_avatar = incoming.has_avatar && !existing.has_avatar;
    let gains_reply = existing.request_reply_id.is_none() && incoming.reply_id.is_some();
    let gains_actions = existing.actions.is_empty() && !incoming.actions.is_empty();
    let replaces_summary = existing_bare && !incoming_bare;

    if !gains_avatar && !gains_reply && !gains_actions && !replaces_summary {
        let existing_mut = active_notifications.get_mut(other_id)?;
        if !existing_mut.notified_desktop && !incoming.silent {
            existing_mut.notified_desktop = true;
            let upgraded = existing_mut.clone();
            dispatch_desktop_notification(device_name, &upgraded, false);
            return Some(NotificationEvent::Added(upgraded));
        }
        return None;
    }

    let mut upgraded = if replaces_summary {
        let mut entry = active_notifications.remove(other_id)?;
        entry.id = incoming.id.to_string();
        entry
    } else {
        active_notifications.get(other_id)?.clone()
    };

    if gains_avatar {
        upgraded.icon_path = built_icon;
        upgraded.has_avatar = true;
    }
    if replaces_summary {
        if !incoming.title.is_empty() && !incoming.title.eq_ignore_ascii_case(&upgraded.app_name) {
            upgraded.title = incoming.title.to_string();
        }
        if !incoming.body.is_empty() {
            upgraded.body = incoming.body.to_string();
        }
    }
    if gains_reply {
        upgraded.request_reply_id = incoming.reply_id.map(str::to_string);
    }
    if gains_actions {
        upgraded.actions = incoming.actions.to_vec();
    }

    let was_notified = upgraded.notified_desktop;
    let should_notify = was_notified || !incoming.silent;
    if should_notify {
        upgraded.notified_desktop = true;
    }
    let target_key = upgraded.id.clone();
    active_notifications.insert(target_key, upgraded.clone());
    if should_notify {
        dispatch_desktop_notification(device_name, &upgraded, was_notified);
    }
    Some(NotificationEvent::Added(upgraded))
}

fn handle_cancel_packet<S: BuildHasher, H: BuildHasher>(
    clean_id: &str, active_notifications: &mut HashMap<String, ActiveNotification, S>,
    seen_history: &mut HashMap<String, (String, String), H>,
) -> NotificationEvent {
    let removed = active_notifications.remove(clean_id).or_else(|| {
        let sibling_key = active_notifications
            .values()
            .find(|existing| {
                let incoming = IncomingNotification {
                    id: clean_id,
                    app_name: &existing.app_name,
                    title: &existing.title,
                    body: &existing.body,
                    has_avatar: existing.has_avatar,
                    reply_id: existing.request_reply_id.as_deref(),
                    actions: &existing.actions,
                    silent: true,
                };
                notifications_are_siblings(existing, &incoming)
            })
            .map(|e| e.id.clone());
        sibling_key.and_then(|k| active_notifications.remove(&k))
    });
    if let Some(removed) = removed
        && let Some(dbus_id) = take_desktop_notify_id(removed.seq)
        && let Ok(handle) = tokio::runtime::Handle::try_current()
    {
        handle.spawn(async move {
            let _ = glide_sdk::system::withdraw(dbus_id).await;
        });
    }
    seen_history.remove(clean_id);
    NotificationEvent::Dismissed(clean_id.to_string())
}

fn handle_seen_notification<S: BuildHasher>(
    device_name: &str, mut active: ActiveNotification, prev_had_avatar: bool, is_silent: bool,
    active_notifications: &mut HashMap<String, ActiveNotification, S>,
) -> Option<NotificationEvent> {
    let clean_id = active.id.clone();
    let prev_notified = active.notified_desktop;
    if active.has_avatar && !prev_had_avatar {
        let should_notify = prev_notified || !is_silent;
        if should_notify {
            active.notified_desktop = true;
        }
        active_notifications.insert(clean_id, active.clone());
        if should_notify {
            dispatch_desktop_notification(device_name, &active, prev_notified);
        }
        return Some(NotificationEvent::Added(active));
    }
    if !prev_notified && !is_silent {
        active.notified_desktop = true;
        active_notifications.insert(clean_id, active.clone());
        dispatch_desktop_notification(device_name, &active, false);
        return Some(NotificationEvent::Added(active));
    }
    active_notifications.insert(clean_id, active);
    None
}

fn resolve_inherited_avatar<S: BuildHasher>(
    app_name: &str, title: &str, prev: Option<&ActiveNotification>,
    active_notifications: &HashMap<String, ActiveNotification, S>,
) -> Option<String> {
    prev.filter(|existing| existing.has_avatar).and_then(|existing| existing.icon_path.clone()).or_else(|| {
        (!title.is_empty() && !title.eq_ignore_ascii_case(app_name))
            .then(|| {
                active_notifications
                    .values()
                    .find(|existing| {
                        existing.app_name == app_name
                            && existing.title == title
                            && existing.has_avatar
                            && existing.icon_path.is_some()
                    })
                    .and_then(|existing| existing.icon_path.clone())
            })
            .flatten()
    })
}

pub fn handle_notification_packet<S: BuildHasher>(
    device_name: &str, packet: &NetworkPacket, active_notifications: &mut HashMap<String, ActiveNotification, S>,
) -> Result<Option<NotificationEvent>, serde_json::Error> {
    let mut dummy_history = HashMap::new();
    handle_notification_packet_with_history(device_name, packet, active_notifications, &mut dummy_history)
}

pub fn handle_notification_packet_with_history<S: BuildHasher, H: BuildHasher>(
    device_name: &str, packet: &NetworkPacket, active_notifications: &mut HashMap<String, ActiveNotification, S>,
    seen_history: &mut HashMap<String, (String, String), H>,
) -> Result<Option<NotificationEvent>, serde_json::Error> {
    handle_notification_packet_with_payload(device_name, packet, None, active_notifications, seen_history)
}

pub fn handle_notification_packet_with_payload<S: BuildHasher, H: BuildHasher>(
    device_name: &str, packet: &NetworkPacket, payload_png: Option<&[u8]>,
    active_notifications: &mut HashMap<String, ActiveNotification, S>,
    seen_history: &mut HashMap<String, (String, String), H>,
) -> Result<Option<NotificationEvent>, serde_json::Error> {
    let payload = NotificationPayload::from_packet(packet)?;
    let clean_id =
        payload.id.strip_prefix("org.kde.kdeconnect_tp::").map_or_else(|| payload.id.clone(), str::to_string);

    if payload.is_cancel {
        return Ok(Some(handle_cancel_packet(&clean_id, active_notifications, seen_history)));
    }

    let app_name = collapse_whitespace(payload.app_name.as_deref().unwrap_or("Phone"));
    let app_name = if app_name.is_empty() { "Phone".to_string() } else { app_name };
    let title = collapse_whitespace(payload.title.as_deref().unwrap_or_default());
    let body = collapse_whitespace(payload.text.as_deref().or(payload.ticker.as_deref()).unwrap_or_default());
    if title.is_empty() && body.is_empty() {
        return Ok(None);
    }

    let is_silent = payload.silent.unwrap_or(false);
    let (built_icon, incoming_has_avatar) =
        build_badged_notification_icon_with_kind(&app_name, payload_png, payload.payload_hash.as_deref())
            .map_or((None, false), |(path, has_avatar)| (Some(path), has_avatar));

    let incoming = IncomingNotification {
        id: &clean_id,
        app_name: &app_name,
        title: &title,
        body: &body,
        has_avatar: incoming_has_avatar,
        reply_id: payload.request_reply_id.as_deref(),
        actions: &payload.actions,
        silent: is_silent,
    };

    let signature = (title.clone(), body.clone());
    let sibling_id = active_notifications
        .values()
        .find(|existing| notifications_are_siblings(existing, &incoming))
        .map(|existing| existing.id.clone());

    if let Some(other_id) = sibling_id {
        seen_history.insert(clean_id.clone(), signature);
        return Ok(upgrade_sibling_notification(device_name, &other_id, &incoming, built_icon, active_notifications));
    }

    let prev = active_notifications.get(&clean_id);
    let already_in_active = prev.is_some_and(|existing| existing.title == title && existing.body == body);
    let already_in_history = seen_history.get(&clean_id) == Some(&signature);
    let already_seen = already_in_active || already_in_history;
    let prev_had_avatar = prev.is_some_and(|existing| existing.has_avatar);
    let prev_notified = prev.is_some_and(|existing| existing.notified_desktop);

    seen_history.insert(clean_id.clone(), signature);

    let (icon_path, has_avatar) = if incoming_has_avatar {
        (built_icon, true)
    } else if let Some(inherited) = resolve_inherited_avatar(&app_name, &title, prev, active_notifications) {
        (Some(inherited), true)
    } else {
        (built_icon.or_else(|| prev.and_then(|existing| existing.icon_path.clone())), prev_had_avatar)
    };

    let seq = prev.map_or_else(|| NEXT_NOTIF_SEQ.fetch_add(1, Ordering::Relaxed), |existing| existing.seq);
    let request_reply_id =
        payload.request_reply_id.or_else(|| prev.and_then(|existing| existing.request_reply_id.clone()));
    let actions = if payload.actions.is_empty() {
        prev.map(|existing| existing.actions.clone()).unwrap_or_default()
    } else {
        payload.actions
    };

    let mut active = ActiveNotification {
        id: clean_id.clone(),
        app_name,
        title,
        body: body.clone(),
        request_reply_id,
        actions,
        seq,
        icon_path,
        has_avatar,
        notified_desktop: prev_notified,
    };

    if already_seen {
        return Ok(handle_seen_notification(device_name, active, prev_had_avatar, is_silent, active_notifications));
    }

    if !is_silent {
        active.notified_desktop = true;
        if let Some(code) = extract_auth_code(&body) {
            let _ = glide_sdk::system::copy_secret(&code, 60);
        }
        dispatch_desktop_notification(device_name, &active, prev_notified);
    } else if prev_notified {
        dispatch_desktop_notification(device_name, &active, true);
    }

    active_notifications.insert(clean_id, active.clone());
    Ok(Some(NotificationEvent::Added(active)))
}
