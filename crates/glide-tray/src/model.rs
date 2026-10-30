use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

pub const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
pub const WATCHER_PATH: &str = "/StatusNotifierWatcher";
pub const WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";
pub const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
pub const HOST_PREFIX: &str = "org.kde.StatusNotifierHost";
pub const DEFAULT_ITEM_PATH: &str = "/StatusNotifierItem";
pub const NO_DBUSMENU: &str = "/NO_DBUSMENU";
pub const MAX_PIXMAP_SIDE: i32 = 512;
pub const MAX_PIXMAP_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PROPERTY_REPLY: usize = MAX_PIXMAP_BYTES + 64 * 1024;
pub const MAX_ALL_REPLY: usize = 3 * MAX_PIXMAP_BYTES + 256 * 1024;

pub type WirePixmaps = Vec<(i32, i32, Vec<u8>)>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    pub bus: String,
    pub path: String,
}

fn plausible_bus(bus: &str) -> bool {
    !bus.is_empty() && !bus.contains(char::is_whitespace) && !bus.contains('/')
}

impl Address {
    #[must_use]
    pub fn parse(service: &str, sender: Option<&str>) -> Option<Self> {
        let service = service.trim();
        if service.is_empty() {
            return None;
        }
        if service.starts_with('/') {
            let bus = sender.filter(|bus| plausible_bus(bus))?;
            return Some(Self { bus: bus.to_string(), path: service.to_string() });
        }
        let (bus, path) = match service.split_once('/') {
            Some((bus, "")) => (bus, DEFAULT_ITEM_PATH.to_string()),
            Some((bus, rest)) => (bus, format!("/{rest}")),
            None => (service, DEFAULT_ITEM_PATH.to_string()),
        };
        plausible_bus(bus).then(|| Self { bus: bus.to_string(), path })
    }

    #[must_use]
    pub fn key(&self) -> String {
        format!("{}{}", self.bus, self.path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    Passive,
    #[default]
    Active,
    NeedsAttention,
}

impl Status {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "Passive" => Self::Passive,
            "NeedsAttention" => Self::NeedsAttention,
            _ => Self::Active,
        }
    }

    #[must_use]
    pub const fn visible(self) -> bool {
        !matches!(self, Self::Passive)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pixmap {
    pub width: i32,
    pub height: i32,
    pub argb: Vec<u8>,
}

impl Pixmap {
    fn is_valid(&self) -> bool {
        let (Ok(w), Ok(h)) = (usize::try_from(self.width), usize::try_from(self.height)) else {
            return false;
        };
        w > 0 && h > 0 && w.checked_mul(h).and_then(|n| n.checked_mul(4)) == Some(self.argb.len())
    }

    fn side(&self) -> i32 {
        self.width.max(self.height)
    }

    fn within_limits(&self) -> bool {
        self.is_valid() && self.side() <= MAX_PIXMAP_SIDE
    }
}

#[must_use]
pub fn admit_pixmaps(raw: WirePixmaps) -> Vec<Pixmap> {
    let mut total = 0usize;
    let mut admitted = Vec::new();
    for (width, height, argb) in raw {
        let pixmap = Pixmap { width, height, argb };
        let next = total.saturating_add(pixmap.argb.len());
        if !pixmap.within_limits() || next > MAX_PIXMAP_BYTES {
            continue;
        }
        total = next;
        admitted.push(pixmap);
    }
    admitted
}

#[must_use]
pub fn pick_pixmap(pixmaps: &[Pixmap], target: i32) -> Option<&Pixmap> {
    let valid = pixmaps.iter().filter(|pixmap| pixmap.is_valid());
    let at_least = valid.clone().filter(|pixmap| pixmap.side() >= target).min_by_key(|pixmap| pixmap.side());
    at_least.or_else(|| valid.max_by_key(|pixmap| pixmap.side()))
}

#[must_use]
pub fn argb_to_premultiplied_bgra(argb: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(argb.len());
    for pixel in argb.chunks_exact(4) {
        let alpha = u16::from(pixel[0]);
        let premultiply = |channel: u8| {
            let value = (u16::from(channel) * alpha + 127) / 255;
            u8::try_from(value).unwrap_or(u8::MAX)
        };
        out.extend_from_slice(&[premultiply(pixel[3]), premultiply(pixel[2]), premultiply(pixel[1]), pixel[0]]);
    }
    out
}

fn overlap(start: f64, end: f64, cell: usize) -> f64 {
    let (lo, hi) = (cell as f64, cell as f64 + 1.0);
    (end.min(hi) - start.max(lo)).max(0.0)
}

#[must_use]
pub fn fit_square(bgra: &[u8], width: i32, height: i32, side: i32) -> Option<Vec<u8>> {
    let w = usize::try_from(width).ok().filter(|w| *w > 0)?;
    let h = usize::try_from(height).ok().filter(|h| *h > 0)?;
    let side_px = usize::try_from(side).ok().filter(|s| *s > 0)?;
    if bgra.len() < w.checked_mul(h)?.checked_mul(4)? {
        return None;
    }
    let scale = (side_px as f64 / w as f64).min(side_px as f64 / h as f64);
    let fit = |len: usize| ((len as f64 * scale).round() as usize).clamp(1, side_px);
    let (out_w, out_h) = (fit(w), fit(h));
    let (off_x, off_y) = ((side_px - out_w) / 2, (side_px - out_h) / 2);
    let mut out = vec![0u8; side_px * side_px * 4];
    let (step_x, step_y) = (w as f64 / out_w as f64, h as f64 / out_h as f64);
    for oy in 0..out_h {
        let (y0, y1) = (oy as f64 * step_y, (oy + 1) as f64 * step_y);
        let rows = (y0.floor() as usize)..(y1.ceil() as usize).min(h);
        for ox in 0..out_w {
            let (x0, x1) = (ox as f64 * step_x, (ox + 1) as f64 * step_x);
            let cols = (x0.floor() as usize)..(x1.ceil() as usize).min(w);
            let mut sum = [0.0_f64; 4];
            let mut total = 0.0;
            for sy in rows.clone() {
                let wy = overlap(y0, y1, sy);
                for sx in cols.clone() {
                    let weight = wy * overlap(x0, x1, sx);
                    if weight <= 0.0 {
                        continue;
                    }
                    let at = (sy * w + sx) * 4;
                    for (channel, acc) in sum.iter_mut().enumerate() {
                        *acc += f64::from(bgra[at + channel]) * weight;
                    }
                    total += weight;
                }
            }
            if total <= 0.0 {
                continue;
            }
            let at = ((oy + off_y) * side_px + ox + off_x) * 4;
            for (channel, acc) in sum.iter().enumerate() {
                {
                    out[at + channel] = (acc / total).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    Some(out)
}

#[must_use]
pub fn pixmap_square(pixmaps: &[Pixmap], side: i32) -> Option<Vec<u8>> {
    let pixmap = pick_pixmap(pixmaps, side)?;
    let bgra = argb_to_premultiplied_bgra(&pixmap.argb);
    fit_square(&bgra, pixmap.width, pixmap.height, side)
}

const GREY_TOLERANCE: u8 = 24;
const TONE_SPREAD: u8 = 56;

#[must_use]
pub fn is_monochrome(bgra: &[u8]) -> bool {
    let mut seen = false;
    let (mut lo, mut hi) = (u8::MAX, u8::MIN);
    for pixel in bgra.chunks_exact(4) {
        let alpha = pixel[3];
        if alpha < 128 {
            continue;
        }
        let straight = |channel: u8| {
            let value = u16::from(channel) * 255 / u16::from(alpha);
            u8::try_from(value.min(255)).unwrap_or(u8::MAX)
        };
        let (b, g, r) = (straight(pixel[0]), straight(pixel[1]), straight(pixel[2]));
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        if max - min > GREY_TOLERANCE {
            return false;
        }
        lo = lo.min(min);
        hi = hi.max(max);
        seen = true;
    }
    seen && hi - lo <= TONE_SPREAD
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemProps {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub icon_name: String,
    pub icon_pixmaps: Vec<Pixmap>,
    pub attention_icon_name: String,
    pub attention_pixmaps: Vec<Pixmap>,
    pub icon_theme_path: String,
    pub tooltip_title: String,
    pub tooltip_body: String,
    pub menu: Option<String>,
    pub item_is_menu: bool,
}

impl ItemProps {
    #[must_use]
    pub fn label(&self) -> &str {
        [&self.tooltip_title, &self.title, &self.id]
            .into_iter()
            .map(|text| text.trim())
            .find(|text| !text.is_empty())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn shown_icon(&self) -> (&str, &[Pixmap]) {
        let attention = self.status == Status::NeedsAttention
            && (!self.attention_icon_name.is_empty() || !self.attention_pixmaps.is_empty());
        if attention {
            (&self.attention_icon_name, &self.attention_pixmaps)
        } else {
            (&self.icon_name, &self.icon_pixmaps)
        }
    }
}

fn text(value: &Value<'_>) -> Option<String> {
    match value {
        Value::Str(text) => Some(text.to_string()),
        Value::ObjectPath(path) => Some(path.to_string()),
        Value::Value(inner) => text(inner),
        _ => None,
    }
}

fn unwrap_variant<'a>(value: &'a Value<'a>) -> &'a Value<'a> {
    match value {
        Value::Value(inner) => inner,
        other => other,
    }
}

fn pixmaps(value: &Value<'_>) -> Vec<Pixmap> {
    let Value::Array(array) = unwrap_variant(value) else {
        return Vec::new();
    };
    let raw = array
        .iter()
        .filter_map(|entry| {
            let Value::Structure(structure) = entry else {
                return None;
            };
            let fields = structure.fields();
            let (Some(Value::I32(width)), Some(Value::I32(height)), Some(Value::Array(data))) =
                (fields.first(), fields.get(1), fields.get(2))
            else {
                return None;
            };
            if data.len() > MAX_PIXMAP_BYTES {
                return None;
            }
            let argb = data
                .iter()
                .map(|byte| match byte {
                    Value::U8(byte) => Some(*byte),
                    _ => None,
                })
                .collect::<Option<Vec<u8>>>()?;
            Some((*width, *height, argb))
        })
        .collect();
    admit_pixmaps(raw)
}

fn tooltip(value: &Value<'_>) -> (String, String) {
    let Value::Structure(structure) = unwrap_variant(value) else {
        return Default::default();
    };
    let field = |index| structure.fields().get(index).and_then(text);
    (field(2).unwrap_or_default(), field(3).unwrap_or_default())
}

#[must_use]
pub fn parse_props<S: std::hash::BuildHasher>(map: &HashMap<String, OwnedValue, S>) -> ItemProps {
    let pixmaps = |key: &str| map.get(key).map(|value| pixmaps(value)).unwrap_or_default();
    let tooltip = map.get("ToolTip").map(|v| tooltip(v)).unwrap_or_default();
    assemble(map, pixmaps("IconPixmap"), pixmaps("AttentionIconPixmap"), tooltip)
}

pub(crate) fn assemble<S: std::hash::BuildHasher>(
    map: &HashMap<String, OwnedValue, S>, icon_pixmaps: Vec<Pixmap>, attention_pixmaps: Vec<Pixmap>,
    (tooltip_title, tooltip_body): (String, String),
) -> ItemProps {
    let text = |key: &str| map.get(key).and_then(|value| text(value));
    let string = |key: &str| text(key).unwrap_or_default();
    ItemProps {
        id: string("Id"),
        title: string("Title"),
        status: Status::parse(&string("Status")),
        icon_name: string("IconName"),
        icon_pixmaps,
        attention_icon_name: string("AttentionIconName"),
        attention_pixmaps,
        icon_theme_path: string("IconThemePath"),
        tooltip_title,
        tooltip_body,
        menu: text("Menu").filter(|path| path.starts_with('/') && path != "/" && path != NO_DBUSMENU),
        item_is_menu: map.get("ItemIsMenu").is_some_and(|value| matches!(&**value, Value::Bool(true))),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    items: Vec<Address>,
    hosts: Vec<String>,
}

impl Registry {
    pub(crate) fn add_item(&mut self, address: Address) -> bool {
        if self.items.contains(&address) {
            return false;
        }
        self.items.push(address);
        true
    }

    pub(crate) fn remove_item(&mut self, address: &Address) -> bool {
        let before = self.items.len();
        self.items.retain(|known| known != address);
        self.items.len() != before
    }

    pub(crate) fn add_host(&mut self, name: &str) -> bool {
        if name.is_empty() || self.hosts.iter().any(|host| host == name) {
            return false;
        }
        self.hosts.push(name.to_string());
        true
    }

    pub(crate) fn has_hosts(&self) -> bool {
        !self.hosts.is_empty()
    }

    pub fn vanish(&mut self, name: &str) -> Vec<Address> {
        self.hosts.retain(|host| host != name);
        let (gone, kept) = std::mem::take(&mut self.items).into_iter().partition(|address| address.bus == name);
        self.items = kept;
        gone
    }

    #[must_use]
    pub fn items(&self) -> &[Address] {
        &self.items
    }

    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.items.iter().map(Address::key).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(bus: &str, path: &str) -> Address {
        Address { bus: bus.into(), path: path.into() }
    }

    #[test]
    fn a_bare_service_name_uses_the_default_item_path() {
        assert_eq!(
            Address::parse("org.kde.StatusNotifierItem-4242-1", Some(":1.9")),
            Some(address("org.kde.StatusNotifierItem-4242-1", DEFAULT_ITEM_PATH))
        );
        assert_eq!(Address::parse(":1.77", None), Some(address(":1.77", DEFAULT_ITEM_PATH)));
    }

    #[test]
    fn a_service_with_a_path_splits_at_the_first_slash() {
        assert_eq!(
            Address::parse(":1.45/org/ayatana/NotificationItem/nm", None),
            Some(address(":1.45", "/org/ayatana/NotificationItem/nm"))
        );
        assert_eq!(Address::parse("org.example.App/", None), Some(address("org.example.App", DEFAULT_ITEM_PATH)));
    }

    #[test]
    fn a_bare_object_path_is_owned_by_the_sender() {
        assert_eq!(
            Address::parse("/org/ayatana/NotificationItem/app", Some(":1.12")),
            Some(address(":1.12", "/org/ayatana/NotificationItem/app"))
        );
        assert_eq!(Address::parse("/StatusNotifierItem", None), None, "a path without a sender names no peer");
    }

    #[test]
    fn garbage_registrations_are_refused() {
        assert_eq!(Address::parse("", Some(":1.2")), None);
        assert_eq!(Address::parse("   ", Some(":1.2")), None);
        assert_eq!(Address::parse("bad name", None), None);
        assert_eq!(Address::parse("/path", Some("")), None);
    }

    #[test]
    fn keys_join_bus_and_path_like_kde_does() {
        assert_eq!(address(":1.45", "/StatusNotifierItem").key(), ":1.45/StatusNotifierItem");
        let parsed = Address::parse(&address(":1.45", "/a/b").key(), None);
        assert_eq!(parsed, Some(address(":1.45", "/a/b")), "keys round-trip");
    }

    #[test]
    fn status_parsing_hides_only_passive_items() {
        assert!(!Status::parse("Passive").visible());
        assert!(Status::parse("Active").visible());
        assert!(Status::parse("NeedsAttention").visible());
        assert_eq!(Status::parse("whatever"), Status::Active);
    }

    #[test]
    fn needs_attention_swaps_in_the_attention_icon_when_there_is_one() {
        let mut props = ItemProps {
            icon_name: "normal".into(),
            attention_icon_name: "alert".into(),
            status: Status::NeedsAttention,
            ..ItemProps::default()
        };
        assert_eq!(props.shown_icon().0, "alert");
        props.status = Status::Active;
        assert_eq!(props.shown_icon().0, "normal");
        props.status = Status::NeedsAttention;
        props.attention_icon_name.clear();
        assert_eq!(props.shown_icon().0, "normal");
    }

    #[test]
    fn the_label_prefers_the_tooltip_then_the_title_then_the_id() {
        let mut props =
            ItemProps { id: "app".into(), title: "App".into(), tooltip_title: "  ".into(), ..ItemProps::default() };
        assert_eq!(props.label(), "App");
        props.tooltip_title = "3 unread".into();
        assert_eq!(props.label(), "3 unread");
        props.tooltip_title.clear();
        props.title.clear();
        assert_eq!(props.label(), "app");
    }

    fn solid(side: i32, argb: [u8; 4]) -> Pixmap {
        let count = usize::try_from(side * side).expect("positive");
        Pixmap { width: side, height: side, argb: argb.repeat(count) }
    }

    #[test]
    fn the_smallest_pixmap_at_least_as_big_as_the_target_wins() {
        let set = vec![solid(16, [255; 4]), solid(64, [255; 4]), solid(32, [255; 4])];
        assert_eq!(pick_pixmap(&set, 24).map(|p| p.width), Some(32));
        assert_eq!(pick_pixmap(&set, 32).map(|p| p.width), Some(32));
        assert_eq!(pick_pixmap(&set, 16).map(|p| p.width), Some(16));
        assert_eq!(
            pick_pixmap(&set, 128).map(|p| p.width),
            Some(64),
            "with nothing big enough the largest is upscaled"
        );
    }

    #[test]
    fn pixmaps_whose_data_does_not_match_their_size_are_skipped() {
        let broken = Pixmap { width: 22, height: 22, argb: vec![0; 10] };
        let set = vec![broken, solid(8, [255; 4])];
        assert_eq!(pick_pixmap(&set, 22).map(|p| p.width), Some(8));
        assert_eq!(pick_pixmap(&[], 22), None);
    }

    #[test]
    fn network_order_argb_becomes_premultiplied_bgra() {
        let argb = [255, 10, 20, 30, 128, 255, 0, 100, 0, 200, 200, 200];
        assert_eq!(argb_to_premultiplied_bgra(&argb), vec![30, 20, 10, 255, 50, 0, 128, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn fitting_keeps_the_aspect_ratio_and_centres_the_image() {
        let (w, h) = (4, 2);
        let bgra = [9u8, 9, 9, 255].repeat(8);
        let out = fit_square(&bgra, w, h, 4).expect("valid input");
        let alpha = |x: usize, y: usize| out[(y * 4 + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0, "the band above the image stays clear");
        assert_eq!(alpha(0, 1), 255);
        assert_eq!(alpha(3, 2), 255);
        assert_eq!(alpha(3, 3), 0, "the band below the image stays clear");
    }

    #[test]
    fn downscaling_averages_the_covered_pixels() {
        let mut bgra = Vec::new();
        for index in 0..4 {
            let v = if index % 2 == 0 { 0 } else { 200 };
            bgra.extend_from_slice(&[v, v, v, 255]);
        }
        let out = fit_square(&bgra, 2, 2, 1).expect("valid input");
        assert_eq!(out, vec![100, 100, 100, 255]);
        assert_eq!(fit_square(&bgra, 2, 2, 0), None);
        assert_eq!(fit_square(&bgra[..8], 2, 2, 1), None);
    }

    #[test]
    fn a_pixmap_becomes_a_square_raster_of_the_requested_side() {
        let set = vec![solid(48, [255, 255, 0, 0])];
        let out = pixmap_square(&set, 16).expect("renders");
        assert_eq!(out.len(), 16 * 16 * 4);
        assert_eq!(&out[..4], &[0, 0, 255, 255], "opaque red in BGRA");
    }

    #[test]
    fn grey_single_tone_art_counts_as_monochrome() {
        let white = [255u8, 255, 255, 255].repeat(4);
        assert!(is_monochrome(&white));
        let mut two_tone = white.clone();
        two_tone[..4].copy_from_slice(&[20, 20, 20, 255]);
        assert!(!is_monochrome(&two_tone), "a light and dark mix is art");
        let mut coloured = white;
        coloured[..4].copy_from_slice(&[0, 0, 255, 255]);
        assert!(!is_monochrome(&coloured));
        assert!(!is_monochrome(&[0u8; 16]), "nothing visible is not a glyph");
    }

    #[test]
    fn properties_are_read_from_the_variant_map() {
        let mut map: HashMap<String, OwnedValue> = HashMap::new();
        let put = |map: &mut HashMap<String, OwnedValue>, key: &str, value: Value<'_>| {
            map.insert(key.into(), value.try_to_owned().expect("owned"));
        };
        put(&mut map, "Id", Value::from("nm-applet"));
        put(&mut map, "Title", Value::from("Network"));
        put(&mut map, "Status", Value::from("NeedsAttention"));
        put(&mut map, "IconName", Value::from("nm-signal-75"));
        put(&mut map, "Menu", Value::from(zbus::zvariant::ObjectPath::try_from("/MenuBar").expect("path")));
        put(&mut map, "ItemIsMenu", Value::from(true));
        put(&mut map, "IconPixmap", Value::from(vec![(2i32, 1i32, vec![255u8, 1, 2, 3, 255, 4, 5, 6])]));
        put(
            &mut map,
            "ToolTip",
            Value::from((
                String::new(),
                Vec::<(i32, i32, Vec<u8>)>::new(),
                String::from("Wi-Fi"),
                String::from("Connected"),
            )),
        );
        let props = parse_props(&map);
        assert_eq!(props.id, "nm-applet");
        assert_eq!(props.status, Status::NeedsAttention);
        assert_eq!(props.menu.as_deref(), Some("/MenuBar"));
        assert!(props.item_is_menu);
        assert_eq!(props.icon_pixmaps.len(), 1);
        assert_eq!(props.icon_pixmaps[0].argb.len(), 8);
        assert_eq!(props.tooltip_title, "Wi-Fi");
        assert_eq!(props.tooltip_body, "Connected");
    }

    #[test]
    fn a_root_menu_path_means_no_menu() {
        let mut map: HashMap<String, OwnedValue> = HashMap::new();
        map.insert(
            "Menu".into(),
            Value::from(zbus::zvariant::ObjectPath::try_from("/").expect("path")).try_to_owned().expect("owned"),
        );
        assert_eq!(parse_props(&map).menu, None);
        assert_eq!(parse_props(&HashMap::new()), ItemProps::default());
    }

    #[test]
    fn the_qt_no_menu_sentinel_means_no_menu() {
        let menu = |path: &str| {
            let mut map: HashMap<String, OwnedValue> = HashMap::new();
            let value = Value::from(zbus::zvariant::ObjectPath::try_from(path).expect("path"));
            map.insert("Menu".into(), value.try_to_owned().expect("owned"));
            parse_props(&map).menu
        };
        assert_eq!(menu(NO_DBUSMENU), None);
        assert_eq!(menu("/NO_DBUSMENU_AT_ALL").as_deref(), Some("/NO_DBUSMENU_AT_ALL"));
        assert_eq!(menu("/MenuBar").as_deref(), Some("/MenuBar"));
    }

    #[test]
    fn pixmaps_wider_than_the_side_limit_are_dropped_even_when_their_data_matches() {
        let wide = (MAX_PIXMAP_SIDE + 1, 1, vec![0u8; usize::try_from(MAX_PIXMAP_SIDE + 1).expect("positive") * 4]);
        let tall = (1, MAX_PIXMAP_SIDE + 1, vec![0u8; usize::try_from(MAX_PIXMAP_SIDE + 1).expect("positive") * 4]);
        let fine = (MAX_PIXMAP_SIDE, 1, vec![0u8; usize::try_from(MAX_PIXMAP_SIDE).expect("positive") * 4]);
        let kept = admit_pixmaps(vec![wide, tall, fine]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].width, MAX_PIXMAP_SIDE);
    }

    #[test]
    fn pixmaps_whose_length_is_not_width_times_height_times_four_are_dropped() {
        let kept = admit_pixmaps(vec![
            (2, 2, vec![0; 15]),
            (2, 2, vec![0; 17]),
            (0, 4, Vec::new()),
            (-2, 2, vec![0; 16]),
            (i32::MAX, i32::MAX, vec![0; 16]),
            (2, 2, vec![0; 16]),
        ]);
        assert_eq!(kept.len(), 1);
        assert_eq!((kept[0].width, kept[0].height), (2, 2));
    }

    #[test]
    fn one_property_cannot_carry_more_than_the_byte_budget_of_pixmaps() {
        let side = MAX_PIXMAP_SIDE;
        let one_mib = || (side, side, vec![0u8; usize::try_from(side * side).expect("positive") * 4]);
        let kept = admit_pixmaps((0..6).map(|_| one_mib()).collect());
        assert_eq!(kept.len(), MAX_PIXMAP_BYTES / (one_mib().2.len()));
        assert!(kept.iter().map(|pixmap| pixmap.argb.len()).sum::<usize>() <= MAX_PIXMAP_BYTES);
    }

    #[test]
    fn the_reply_caps_leave_room_for_the_largest_admitted_properties() {
        const {
            assert!(MAX_PROPERTY_REPLY > MAX_PIXMAP_BYTES);
            assert!(MAX_ALL_REPLY >= 3 * MAX_PIXMAP_BYTES, "icon, attention icon and tooltip icon at their limits fit");
        }
    }

    #[test]
    fn decoded_variant_pixmaps_above_the_limits_never_reach_the_props() {
        let mut map: HashMap<String, OwnedValue> = HashMap::new();
        let side = MAX_PIXMAP_SIDE + 1;
        let data = vec![0u8; usize::try_from(side).expect("positive") * 4];
        let value = Value::from(vec![(side, 1i32, data), (1i32, 1i32, vec![9u8; 4])]);
        map.insert("IconPixmap".into(), value.try_to_owned().expect("owned"));
        let props = parse_props(&map);
        assert_eq!(props.icon_pixmaps.len(), 1);
        assert_eq!(props.icon_pixmaps[0].width, 1);
    }

    #[test]
    fn the_registry_drops_every_item_of_a_vanished_owner() {
        let mut registry = Registry::default();
        assert!(registry.add_item(address(":1.5", "/a")));
        assert!(!registry.add_item(address(":1.5", "/a")), "no duplicates");
        assert!(registry.add_item(address(":1.5", "/b")));
        assert!(registry.add_item(address("org.example.Tray", "/c")));
        assert!(!registry.has_hosts());
        assert!(registry.add_host(":1.5"));
        assert!(registry.has_hosts());
        let gone = registry.vanish(":1.5");
        assert_eq!(gone, vec![address(":1.5", "/a"), address(":1.5", "/b")]);
        assert_eq!(registry.keys(), vec![String::from("org.example.Tray/c")]);
        assert!(!registry.has_hosts());
        assert!(registry.vanish(":1.99").is_empty());
        assert!(registry.remove_item(&address("org.example.Tray", "/c")));
        assert!(registry.items().is_empty());
    }
}
