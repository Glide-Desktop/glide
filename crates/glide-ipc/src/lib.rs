use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareSourceKind {
    Screen,
    Window,
    MultiWindow,
    FilteredDesktop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareStreamInfo {
    pub stream_id: u64,
    pub node_id: u32,
    pub source_kind: ShareSourceKind,
    pub width: u32,
    pub height: u32,
    pub window_count: usize,
    #[serde(default)]
    pub app_id: Option<String>,
}

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
    SwitchWorkspace {
        target: WorkspaceTarget,
    },
    MoveWindow {
        window: WindowSelector,
        target: WorkspaceTarget,
    },
    FocusWindow {
        selector: WindowSelector,
    },
    Reload,
    Exec {
        command: String,
    },
    Subscribe,
    SetDpms {
        on: bool,
    },
    CycleLayout,
    GetLayout,
    ListOutputs,
    NightLight {
        on: Option<bool>,
    },
    Lock,
    PracticeGestures,
    ActivationToken {
        app_id: Option<String>,
    },
    Screenshot,
    ScreenshotArea,
    PickColor,
    ScanQr,
    Action {
        name: String,
    },
    IdleInhibitors,
    LoginPasswordChanged {
        password: keychain::Hidden,
    },
    ScreenRecord,
    StopRecording,
    RecordingStatus,
    ScreenShareStart {
        #[serde(default)]
        app_id: Option<String>,
        #[serde(default)]
        multiple: bool,
        #[serde(default)]
        cursor: Option<bool>,
    },
    ScreenShareStop {
        #[serde(default)]
        stream_id: Option<u64>,
    },
    ScreenShareStatus,
    ScreenShareUpdateWindows {
        stream_id: u64,
        window_indices: Vec<usize>,
    },
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
    Recording {
        active: bool,
        #[serde(default)]
        elapsed_secs: Option<u64>,
        #[serde(default)]
        path: Option<String>,
    },
    ScreenShareStarted(ShareStreamInfo),
    ScreenShareStatus {
        streams: Vec<ShareStreamInfo>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    WindowOpened {
        id: u64,
    },
    WindowClosed {
        id: u64,
    },
    TitleChanged {
        id: u64,
        title: String,
    },
    AppIdChanged {
        id: u64,
        app_id: String,
    },
    FocusChanged {
        id: Option<u64>,
    },
    WorkspaceChanged {
        id: u64,
    },
    OutputChanged,
    LayoutChanged {
        index: usize,
        name: String,
    },
    Gesture {
        sample: GestureSample,
    },
    RecordingStateChanged {
        active: bool,
        #[serde(default)]
        path: Option<String>,
    },
    ScreenShareStateChanged {
        stream_id: u64,
        active: bool,
        #[serde(default)]
        source_kind: Option<ShareSourceKind>,
        #[serde(default)]
        app_id: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_share_ipc_messages_round_trip() {
        let requests = [
            Request::ScreenShareStart { app_id: Some("org.example".into()), multiple: true, cursor: Some(true) },
            Request::ScreenShareStart { app_id: None, multiple: false, cursor: None },
            Request::ScreenShareStop { stream_id: Some(42) },
            Request::ScreenShareStop { stream_id: None },
            Request::ScreenShareStatus,
            Request::ScreenShareUpdateWindows { stream_id: 7, window_indices: vec![0, 2] },
        ];
        for req in requests {
            let json = serde_json::to_string(&req).unwrap();
            let parsed: Request = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, req);
        }

        let stream_info = ShareStreamInfo {
            stream_id: 10,
            node_id: 42,
            source_kind: ShareSourceKind::MultiWindow,
            width: 1920,
            height: 1080,
            window_count: 2,
            app_id: Some("org.example".into()),
        };

        let started_resp = Response::ScreenShareStarted(stream_info.clone());
        let json = serde_json::to_string(&started_resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, started_resp);

        let status_resp = Response::ScreenShareStatus { streams: vec![stream_info] };
        let json = serde_json::to_string(&status_resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status_resp);

        let event = Event::ScreenShareStateChanged {
            stream_id: 10,
            active: true,
            source_kind: Some(ShareSourceKind::FilteredDesktop),
            app_id: Some("org.example".into()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);

        let stopped_event =
            Event::ScreenShareStateChanged { stream_id: 10, active: false, source_kind: None, app_id: None };
        let json = serde_json::to_string(&stopped_event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, stopped_event);
    }

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

    #[test]
    fn recording_ipc_messages_round_trip() {
        for request in [Request::ScreenRecord, Request::StopRecording, Request::RecordingStatus] {
            let json = serde_json::to_string(&request).unwrap();
            let parsed: Request = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, request);
        }

        let active_resp = Response::Recording { active: true, elapsed_secs: Some(12), path: None };
        let json = serde_json::to_string(&active_resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, active_resp);

        let stopped_resp = Response::Recording {
            active: false,
            elapsed_secs: None,
            path: Some("/home/user/Videos/glide-recording-1.mp4".into()),
        };
        let json = serde_json::to_string(&stopped_resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, stopped_resp);

        let active_event = Event::RecordingStateChanged { active: true, path: None };
        let json = serde_json::to_string(&active_event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, active_event);

        let stopped_event = Event::RecordingStateChanged {
            active: false,
            path: Some("/home/user/Videos/glide-recording-1.mp4".into()),
        };
        let json = serde_json::to_string(&stopped_event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, stopped_event);
    }
}
