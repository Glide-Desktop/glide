use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use glide_ipc::{Event, Request, Response, WindowSelector, WorkspaceTarget};

#[derive(Debug, PartialEq, Eq)]
pub struct CliArgs {
    pub json_output: bool,
    pub subcommand_args: Vec<String>,
}

pub fn parse_cli_args(args: &[String]) -> anyhow::Result<CliArgs> {
    if args.len() < 2 {
        anyhow::bail!("No command specified");
    }

    let mut json_output = false;
    let mut idx = 1;

    while idx < args.len() {
        let arg = &args[idx];
        if arg == "--json" {
            json_output = true;
            idx += 1;
        } else {
            break;
        }
    }

    if idx >= args.len() {
        anyhow::bail!("No subcommand specified");
    }

    let mut subcommand_args: Vec<String> = args[idx..].to_vec();

    if subcommand_args.first().map(String::as_str) != Some("exec") {
        if let Some(pos) = subcommand_args.iter().position(|a| a == "--json") {
            json_output = true;
            subcommand_args.remove(pos);
        }
    }

    Ok(CliArgs {
        json_output,
        subcommand_args,
    })
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cli = match parse_cli_args(&args) {
        Ok(c) => c,
        Err(_) => {
            print_usage();
            std::process::exit(1);
        }
    };

    let request = parse_args(&cli.subcommand_args)?;
    let socket_path = find_socket_path()?;

    if matches!(request, Request::Subscribe) {
        return run_subscribe(&socket_path, cli.json_output);
    }

    let response = send_request(&socket_path, &request)?;

    if cli.json_output {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        print_response(&response);
    }

    Ok(())
}

fn run_subscribe(socket_path: &PathBuf, json_output: bool) -> anyhow::Result<()> {
    let stream = UnixStream::connect(socket_path)?;
    let mut json = serde_json::to_string(&Request::Subscribe)?;
    json.push('\n');

    let mut reader = BufReader::new(stream);
    reader.get_mut().write_all(json.as_bytes())?;
    reader.get_mut().flush()?;

    let mut line = String::new();
    reader.read_line(&mut line)?;
    let response: Response = serde_json::from_str(&line)?;
    if !matches!(response, Response::Subscribed) {
        anyhow::bail!("subscribe request was not accepted: {response:?}");
    }

    if !json_output {
        eprintln!("Subscribed. Waiting for events (Ctrl+C to stop)...");
    }

    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            if !json_output {
                eprintln!("Connection closed by the compositor.");
            }
            return Ok(());
        }

        let event: Event = match serde_json::from_str(line.trim()) {
            Ok(event) => event,
            Err(err) => {
                eprintln!("warning: could not parse an event line: {err}");
                continue;
            }
        };

        if json_output {
            println!("{}", serde_json::to_string(&event)?);
        } else {
            println!("{}", format_event(&event));
        }
    }
}

fn format_event(event: &Event) -> String {
    match event {
        Event::WindowOpened { id } => format!("window opened: {id}"),
        Event::WindowClosed { id } => format!("window closed: {id}"),
        Event::TitleChanged { id, title } => format!("window {id} title changed: {title:?}"),
        Event::AppIdChanged { id, app_id } => format!("window {id} app-id changed: {app_id:?}"),
        Event::FocusChanged { id: Some(id) } => format!("focus changed: window {id}"),
        Event::FocusChanged { id: None } => "focus changed: none".to_string(),
        Event::WorkspaceChanged { id } => format!("workspace changed: {id}"),
        Event::OutputChanged => "output changed".to_string(),
    }
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
    subscribe                  Stream window/focus/workspace events until Ctrl+C

OPTIONS:
    --json                     Output response as raw JSON
"#
    );
}

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
        "subscribe" | "sub" => Ok(Request::Subscribe),
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
        Response::Subscribed => println!("Subscribed"),
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
        assert_eq!(
            parse_args(&["subscribe".into()]).unwrap(),
            Request::Subscribe
        );
        assert_eq!(parse_args(&["sub".into()]).unwrap(), Request::Subscribe);
    }

    #[test]
    fn format_event_covers_every_variant_without_panicking() {
        let events = [
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
            assert!(!format_event(&event).is_empty());
        }
    }

    #[test]
    fn test_parse_cli_args_flags() {
        let args = vec![
            "glidectl".into(),
            "--json".into(),
            "workspace".into(),
            "list".into(),
        ];
        let cli = parse_cli_args(&args).unwrap();
        assert!(cli.json_output);
        assert_eq!(cli.subcommand_args, vec!["workspace", "list"]);

        let args2 = vec![
            "glidectl".into(),
            "workspace".into(),
            "list".into(),
            "--json".into(),
        ];
        let cli2 = parse_cli_args(&args2).unwrap();
        assert!(cli2.json_output);
        assert_eq!(cli2.subcommand_args, vec!["workspace", "list"]);

        let args3 = vec![
            "glidectl".into(),
            "exec".into(),
            "my_script".into(),
            "--json".into(),
        ];
        let cli3 = parse_cli_args(&args3).unwrap();
        assert!(!cli3.json_output);
        assert_eq!(cli3.subcommand_args, vec!["exec", "my_script", "--json"]);
        let req = parse_args(&cli3.subcommand_args).unwrap();
        assert_eq!(
            req,
            Request::Exec {
                command: "my_script --json".into()
            }
        );

        let args4 = vec![
            "glidectl".into(),
            "--json".into(),
            "exec".into(),
            "my_script".into(),
            "--json".into(),
        ];
        let cli4 = parse_cli_args(&args4).unwrap();
        assert!(cli4.json_output);
        assert_eq!(cli4.subcommand_args, vec!["exec", "my_script", "--json"]);
        let req4 = parse_args(&cli4.subcommand_args).unwrap();
        assert_eq!(
            req4,
            Request::Exec {
                command: "my_script --json".into()
            }
        );
    }

    #[test]
    fn test_adversarial_cli_edge_cases() {
        assert!(parse_cli_args(&[]).is_err());
        assert!(parse_cli_args(&["glidectl".into()]).is_err());
        assert!(parse_cli_args(&["glidectl".into(), "--json".into()]).is_err());

        let args_ws0 = vec![
            "glidectl".into(),
            "workspace".into(),
            "switch".into(),
            "0".into(),
        ];
        let cli_ws0 = parse_cli_args(&args_ws0).unwrap();
        assert!(parse_args(&cli_ws0.subcommand_args).is_err());

        let args_prev = vec![
            "glidectl".into(),
            "workspace".into(),
            "switch".into(),
            "prev".into(),
        ];
        let cli_prev = parse_cli_args(&args_prev).unwrap();
        assert_eq!(
            parse_args(&cli_prev.subcommand_args).unwrap(),
            Request::SwitchWorkspace {
                target: WorkspaceTarget::Previous
            }
        );

        let args_next = vec![
            "glidectl".into(),
            "workspace".into(),
            "switch".into(),
            "next".into(),
        ];
        let cli_next = parse_cli_args(&args_next).unwrap();
        assert_eq!(
            parse_args(&cli_next.subcommand_args).unwrap(),
            Request::SwitchWorkspace {
                target: WorkspaceTarget::Next
            }
        );

        let args_foc = vec![
            "glidectl".into(),
            "window".into(),
            "focus".into(),
            "focused".into(),
        ];
        let cli_foc = parse_cli_args(&args_foc).unwrap();
        assert_eq!(
            parse_args(&cli_foc.subcommand_args).unwrap(),
            Request::FocusWindow {
                selector: WindowSelector::Focused
            }
        );

        let args_id = vec![
            "glidectl".into(),
            "--json".into(),
            "window".into(),
            "focus".into(),
            "42".into(),
        ];
        let cli_id = parse_cli_args(&args_id).unwrap();
        assert!(cli_id.json_output);
        assert_eq!(
            parse_args(&cli_id.subcommand_args).unwrap(),
            Request::FocusWindow {
                selector: WindowSelector::Id(42)
            }
        );

        let args_mv = vec![
            "glidectl".into(),
            "window".into(),
            "move".into(),
            "3".into(),
            "--json".into(),
        ];
        let cli_mv = parse_cli_args(&args_mv).unwrap();
        assert!(cli_mv.json_output);
        assert_eq!(
            parse_args(&cli_mv.subcommand_args).unwrap(),
            Request::MoveWindow {
                window: WindowSelector::Focused,
                target: WorkspaceTarget::Index(2),
            }
        );

        let args_unk = vec!["glidectl".into(), "unknown_cmd".into()];
        let cli_unk = parse_cli_args(&args_unk).unwrap();
        assert!(parse_args(&cli_unk.subcommand_args).is_err());
    }
}
