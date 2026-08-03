//! `glide-ipc`: IPC DTO types and protocol helpers for Glide Wayland compositor and glidectl CLI.

use serde::{Deserialize, Serialize};

/// Target for switching or moving workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum WorkspaceTarget {
    Index(usize),
    Next,
    Previous,
}

/// Selector for choosing a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "kebab-case")]
pub enum WindowSelector {
    Focused,
    Id(u64),
}

/// Request sent from client (glidectl) to compositor.
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
}

/// Information about a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub index: usize,
    pub active: bool,
    pub window_count: usize,
}

/// Information about a window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub workspace_index: usize,
    pub focused: bool,
}

/// Response returned from compositor to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Response {
    Ok,
    Pong,
    Workspaces { list: Vec<WorkspaceInfo> },
    Windows { list: Vec<WindowInfo> },
    Error { message: String },
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
                }],
            },
            Response::Error {
                message: "something went wrong".into(),
            },
        ];

        for resp in responses {
            let json = serde_json::to_string(&resp).expect("serialize response");
            let deserialized: Response = serde_json::from_str(&json).expect("deserialize response");
            assert_eq!(resp, deserialized);
        }
    }
}
