pub const DEVICES_PATH: &str = "/proc/bus/input/devices";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Mouse,
    Touchpad,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointerDevice {
    pub name: String,
    pub kind: PointerKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PointerState {
    pub read: bool,
    pub devices: Vec<PointerDevice>,
}

impl PointerState {
    #[must_use]
    pub fn has(&self, kind: PointerKind) -> bool {
        self.devices.iter().any(|device| device.kind == kind)
    }

    #[must_use]
    pub fn names(&self, kind: PointerKind) -> Vec<&str> {
        self.devices.iter().filter(|device| device.kind == kind).map(|device| device.name.as_str()).collect()
    }
}

const TOUCHPAD_WORDS: &[&str] = &["touchpad", "trackpad", "clickpad", "glidepoint", "synaptics"];

#[must_use]
pub fn is_touchpad_name(name: &str) -> bool {
    let name = name.to_lowercase();
    TOUCHPAD_WORDS.iter().any(|word| name.contains(word))
}

fn classify(name: &str, handlers: &str) -> Option<PointerKind> {
    let handles_pointer = handlers.split_whitespace().any(|handler| handler.starts_with("mouse"));
    if !handles_pointer {
        return None;
    }
    if is_touchpad_name(name) {
        Some(PointerKind::Touchpad)
    } else if name.to_lowercase().contains("touchscreen") {
        None
    } else {
        Some(PointerKind::Mouse)
    }
}

#[must_use]
pub fn parse_devices(text: &str) -> Vec<PointerDevice> {
    let mut devices = Vec::new();
    for block in text.split("\n\n") {
        let mut name = None;
        let mut handlers = "";
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("N: Name=") {
                name = Some(rest.trim().trim_matches('"').to_string());
            } else if let Some(rest) = line.strip_prefix("H: Handlers=") {
                handlers = rest;
            }
        }
        let Some(name) = name.filter(|name| !name.is_empty()) else {
            continue;
        };
        if let Some(kind) = classify(&name, handlers)
            && !devices.iter().any(|known: &PointerDevice| known.name == name && known.kind == kind)
        {
            devices.push(PointerDevice { name, kind });
        }
    }
    devices
}

#[must_use]
pub fn read() -> PointerState {
    match std::fs::read_to_string(DEVICES_PATH) {
        Ok(text) => PointerState { read: true, devices: parse_devices(&text) },
        Err(_) => PointerState::default(),
    }
}
