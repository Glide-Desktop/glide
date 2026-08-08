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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Response {
    Ok,
    Pong,
    Workspaces { list: Vec<WorkspaceInfo> },
    Windows { list: Vec<WindowInfo> },
    Error { message: String },
    Subscribed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    WindowOpened { id: u64 },
    WindowClosed { id: u64 },
    TitleChanged { id: u64, title: String },
    AppIdChanged { id: u64, app_id: String },
    FocusChanged { id: Option<u64> },
    WorkspaceChanged { id: u64 },
    OutputChanged,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_roundtrip() {
        let requests = vec![
            Request::Ping,
            Request::Quit,
            Request::ListWorkspaces,
            Request::ListWindows,
            Request::SwitchWorkspace {
                target: WorkspaceTarget::Index(2),
            },
            Request::MoveWindow {
                window: WindowSelector::Focused,
                target: WorkspaceTarget::Next,
            },
            Request::FocusWindow {
                selector: WindowSelector::Id(42),
            },
            Request::Reload,
            Request::Exec {
                command: "alacritty".to_string(),
            },
            Request::Subscribe,
        ];

        for req in requests {
            let json = serde_json::to_string(&req).expect("serialize request");
            let deserialized: Request = serde_json::from_str(&json).expect("deserialize request");
            assert_eq!(req, deserialized);
        }
    }

    #[test]
    fn test_response_roundtrip() {
        let responses = vec![
            Response::Ok,
            Response::Pong,
            Response::Workspaces {
                list: vec![WorkspaceInfo {
                    id: 1,
                    index: 0,
                    active: true,
                    window_count: 2,
                }],
            },
            Response::Windows {
                list: vec![WindowInfo {
                    id: 42,
                    app_id: Some("alacritty".into()),
                    title: Some("Terminal".into()),
                    workspace_index: 0,
                    focused: true,
                    minimized: false,
                }],
            },
            Response::Error {
                message: "something went wrong".into(),
            },
            Response::Subscribed,
        ];

        for resp in responses {
            let json = serde_json::to_string(&resp).expect("serialize response");
            let deserialized: Response = serde_json::from_str(&json).expect("deserialize response");
            assert_eq!(resp, deserialized);
        }
    }

    #[test]
    fn test_event_roundtrip() {
        let events = vec![
            Event::WindowOpened { id: 1 },
            Event::WindowClosed { id: 2 },
            Event::TitleChanged {
                id: 3,
                title: "Terminal".to_string(),
            },
            Event::AppIdChanged {
                id: 4,
                app_id: "alacritty".to_string(),
            },
            Event::FocusChanged { id: Some(5) },
            Event::FocusChanged { id: None },
            Event::WorkspaceChanged { id: 6 },
            Event::OutputChanged,
        ];

        for event in events {
            let json = serde_json::to_string(&event).expect("serialize event");
            let deserialized: Event = serde_json::from_str(&json).expect("deserialize event");
            assert_eq!(event, deserialized);
        }
    }
}
