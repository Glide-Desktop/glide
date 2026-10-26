use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum WorkspaceTarget {
    Index(usize),
    Next,
    Previous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "kebab-case")]
pub enum WindowSelector {
    Focused,
    Id(u64),
}

pub mod client;
pub mod keychain;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Request {
    Ping,
    Quit,
    ListWorkspaces,
    ListWindows,
    SwitchWorkspace { target: WorkspaceTarget },
    MoveWindow { window: WindowSelector, target: WorkspaceTarget },
    FocusWindow { selector: WindowSelector },
    Reload,
    Exec { command: String },
    Subscribe,
    SetDpms { on: bool },
    CycleLayout,
    GetLayout,
    ListOutputs,
    NightLight { on: Option<bool> },
    Lock,
    PracticeGestures,
    ActivationToken { app_id: Option<String> },
    Screenshot,
    ScreenshotArea,
    PickColor,
    ScanQr,
    Action { name: String },
    IdleInhibitors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GestureKind {
    Swipe,
    Pinch,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GesturePhase {
    Begin,
    Update,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GestureAction {
    #[default]
    None,
    NextWorkspace,
    PreviousWorkspace,
    OpenOverview,
    OpenLauncher,
    SwitchApp,
    MaximizeWindow,
    MinimizeWindow,
    ShowDesktop,
    App,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GestureSample {
    pub kind: GestureKind,
    pub phase: GesturePhase,
    pub fingers: u32,
    pub dx: f64,
    pub dy: f64,
    pub scale: f64,
    pub progress: f64,
    pub action: GestureAction,
    pub committed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModeInfo {
    pub width: i32,
    pub height: i32,
    pub refresh_mhz: i32,
    #[serde(default)]
    pub preferred: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OutputInfo {
    pub name: String,
    pub make: String,
    pub model: String,
    pub width: i32,
    pub height: i32,
    pub refresh_mhz: i32,
    pub scale: f64,
    pub x: i32,
    pub y: i32,
    #[serde(default)]
    pub modes: Vec<ModeInfo>,
    #[serde(default)]
    pub physical_mm: (i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub index: usize,
    pub active: bool,
    pub window_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub workspace_index: usize,
    pub focused: bool,
    pub minimized: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Response {
    Ok,
    Pong,
    Workspaces {
        list: Vec<WorkspaceInfo>,
    },
    Windows {
        list: Vec<WindowInfo>,
    },
    Error {
        message: String,
    },
    Subscribed,
    Layout {
        index: usize,
        name: String,
    },
    Outputs {
        list: Vec<OutputInfo>,
    },
    NightLight {
        on: bool,
    },
    ActivationToken {
        token: String,
    },
    Screenshot {
        path: String,
    },
    Color {
        red: f64,
        green: f64,
        blue: f64,
    },
    QrCodes {
        codes: Vec<keychain::Hidden>,
    },
    IdleInhibitors {
        apps: Vec<String>,
        caffeine: bool,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    WindowOpened { id: u64 },
    WindowClosed { id: u64 },
    TitleChanged { id: u64, title: String },
    AppIdChanged { id: u64, app_id: String },
    FocusChanged { id: Option<u64> },
    WorkspaceChanged { id: u64 },
    OutputChanged,
    LayoutChanged { index: usize, name: String },
    Gesture { sample: GestureSample },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_response_from_a_newer_compositor_parses_as_unknown() {
        let bare: Response = serde_json::from_str(r#"{"type":"from-the-future"}"#).expect("unit form parses");
        assert_eq!(bare, Response::Unknown);
        let with_fields: Response =
            serde_json::from_str(r#"{"type":"from-the-future","list":[1,2],"on":true}"#).expect("fields are ignored");
        assert_eq!(with_fields, Response::Unknown);
    }

    #[test]
    fn an_output_from_an_older_compositor_still_parses() {
        let json = r#"{"name":"eDP-1","make":"BOE","model":"0x0bca","width":2256,"height":1504,"refresh_mhz":60000,"scale":1.5,"x":0,"y":0}"#;
        let output: OutputInfo = serde_json::from_str(json).expect("old output info parses");
        assert!(output.modes.is_empty());
        assert_eq!(output.physical_mm, (0, 0));
    }
}
