//! `glidectl`: CLI client for controlling Glide Wayland compositor over Unix IPC socket.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use glide_ipc::{Request, Response, WindowSelector, WorkspaceTarget};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let json_output = args.iter().any(|arg| arg == "--json");
    let filtered_args: Vec<String> = args
        .into_iter()
        .skip(1)
        .filter(|arg| arg != "--json")
        .collect();

    if filtered_args.is_empty() {
        print_usage();
        std::process::exit(1);
    }

    let request = parse_args(&filtered_args)?;
    let socket_path = find_socket_path()?;
    let response = send_request(&socket_path, &request)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        print_response(&response);
    }

    Ok(())
}

fn print_usage() {
    eprintln!(
        r#"glidectl — CLI controller for Glide Wayland compositor

USAGE:
    glidectl [OPTIONS] <COMMAND>

COMMANDS:
    ping                       Check connection to compositor
    quit                       Stop compositor
    workspace list             List all workspaces
    workspace switch <N|next|prev>  Switch to workspace (N is 1-indexed, e.g. 1..9)
    window list                List all open windows
    window focus <ID|focused>  Focus window by ID or focused
    window move <N|next|prev>  Move focused window to workspace
    reload                     Reload configuration from glide.toml
    exec <command...>          Execute shell command

OPTIONS:
    --json                     Output response as raw JSON
"#
    );
}

/// Locates the compositor's control socket.
///
/// Only `$XDG_RUNTIME_DIR` is searched. There is deliberately no `/tmp`
/// fallback to match the compositor: a predictable socket path in a
/// world-writable directory would let another local user impersonate Glide
/// and receive our `exec` requests.
fn find_socket_path() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("GLIDE_IPC_SOCK")
        && !path.is_empty()
        && std::path::Path::new(&path).exists()
    {
        return Ok(PathBuf::from(path));
    }

    if let (Ok(wayland_display), Ok(runtime_dir)) = (
        std::env::var("WAYLAND_DISPLAY"),
        std::env::var("XDG_RUNTIME_DIR"),
    ) {
        let path = PathBuf::from(runtime_dir).join(format!("glide-{wayland_display}.sock"));
        if path.exists() {
            return Ok(path);
        }
    }

    anyhow::bail!(
        "Glide IPC socket not found. Is Glide running, and are GLIDE_IPC_SOCK \
         or WAYLAND_DISPLAY + XDG_RUNTIME_DIR set?"
    )
}

fn send_request(socket_path: &PathBuf, request: &Request) -> anyhow::Result<Response> {
    let stream = UnixStream::connect(socket_path)?;
    let mut json = serde_json::to_string(request)?;
    json.push('\n');

    let mut reader = BufReader::new(stream);
    reader.get_mut().write_all(json.as_bytes())?;
    reader.get_mut().flush()?;

    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: Response = serde_json::from_str(&response_line)?;
    Ok(response)
}

fn parse_args(args: &[String]) -> anyhow::Result<Request> {
    match args[0].as_str() {
        "ping" => Ok(Request::Ping),
        "quit" => Ok(Request::Quit),
        "reload" => Ok(Request::Reload),
        "exec" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: glidectl exec <command>");
            }
            Ok(Request::Exec {
                command: args[1..].join(" "),
            })
        }
        "workspace" | "ws" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: glidectl workspace <list|switch>");
            }
            match args[1].as_str() {
                "list" | "ls" => Ok(Request::ListWorkspaces),
                "switch" => {
                    if args.len() < 3 {
                        anyhow::bail!("Usage: glidectl workspace switch <N|next|prev>");
                    }
                    let target = parse_workspace_target(&args[2])?;
                    Ok(Request::SwitchWorkspace { target })
                }
                _ => anyhow::bail!("Unknown workspace subcommand: {}", args[1]),
            }
        }
        "window" | "win" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: glidectl window <list|focus|move>");
            }
            match args[1].as_str() {
                "list" | "ls" => Ok(Request::ListWindows),
                "focus" => {
                    if args.len() < 3 {
                        anyhow::bail!("Usage: glidectl window focus <ID|focused>");
                    }
                    let selector = parse_window_selector(&args[2])?;
                    Ok(Request::FocusWindow { selector })
                }
                "move" => {
                    if args.len() < 3 {
                        anyhow::bail!("Usage: glidectl window move <N|next|prev>");
                    }
                    let target = parse_workspace_target(&args[2])?;
                    Ok(Request::MoveWindow {
                        window: WindowSelector::Focused,
                        target,
                    })
                }
                _ => anyhow::bail!("Unknown window subcommand: {}", args[1]),
            }
        }
        _ => anyhow::bail!("Unknown command: {}", args[0]),
    }
}

fn parse_workspace_target(s: &str) -> anyhow::Result<WorkspaceTarget> {
    match s.to_lowercase().as_str() {
        "next" => Ok(WorkspaceTarget::Next),
        "prev" | "previous" => Ok(WorkspaceTarget::Previous),
        other => {
            let n: usize = other.parse()?;
            if n == 0 {
                anyhow::bail!("Workspace index is 1-indexed (1..9)");
            }
            Ok(WorkspaceTarget::Index(n - 1))
        }
    }
}

fn parse_window_selector(s: &str) -> anyhow::Result<WindowSelector> {
    if s.eq_ignore_ascii_case("focused") {
        Ok(WindowSelector::Focused)
    } else {
        let id: u64 = s.parse()?;
        Ok(WindowSelector::Id(id))
    }
}

fn print_response(response: &Response) {
    match response {
        Response::Ok => println!("OK"),
        Response::Pong => println!("PONG"),
        Response::Error { message } => eprintln!("Error: {message}"),
        Response::Workspaces { list } => {
            println!(
                "{:<5} {:<10} {:<8} {:<10}",
                "INDEX", "ID", "ACTIVE", "WINDOWS"
            );
            println!("{}", "-".repeat(35));
            for ws in list {
                let active_str = if ws.active { "*" } else { "" };
                println!(
                    "{:<5} {:<10} {:<8} {:<10}",
                    ws.index + 1,
                    ws.id,
                    active_str,
                    ws.window_count
                );
            }
        }
        Response::Windows { list } => {
            println!(
                "{:<6} {:<15} {:<10} {:<25} {:<8} {:<9}",
                "ID", "APP_ID", "WS_INDEX", "TITLE", "FOCUSED", "MINIMIZED"
            );
            println!("{}", "-".repeat(78));
            for w in list {
                let app_id = w.app_id.as_deref().unwrap_or("<unknown>");
                let title = w.title.as_deref().unwrap_or("<untitled>");
                let focused_str = if w.focused { "*" } else { "" };
                // There is no taskbar yet, so this listing plus `glidectl
                // window focus <id>` is currently the only way to restore a
                // minimized window.
                let minimized_str = if w.minimized { "*" } else { "" };
                println!(
                    "{:<6} {:<15} {:<10} {:<25} {:<8} {:<9}",
                    w.id,
                    app_id,
                    w.workspace_index + 1,
                    title,
                    focused_str,
                    minimized_str
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args() {
        assert_eq!(parse_args(&["ping".into()]).unwrap(), Request::Ping);
        assert_eq!(parse_args(&["quit".into()]).unwrap(), Request::Quit);
        assert_eq!(
            parse_args(&["workspace".into(), "list".into()]).unwrap(),
            Request::ListWorkspaces
        );
        assert_eq!(
            parse_args(&["workspace".into(), "switch".into(), "2".into()]).unwrap(),
            Request::SwitchWorkspace {
                target: WorkspaceTarget::Index(1)
            }
        );
        assert_eq!(
            parse_args(&["exec".into(), "alacritty".into()]).unwrap(),
            Request::Exec {
                command: "alacritty".into()
            }
        );
    }
}
