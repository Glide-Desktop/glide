use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

use glide_ipc::{Event, Request, Response, WindowSelector, WorkspaceTarget};

const IO_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
pub struct CliArgs {
    pub json_output: bool,
    pub subcommand_args: Vec<String>,
}

pub fn parse_cli_args(args: &[String]) -> anyhow::Result<CliArgs> {
    if args.len() < 2 {
        anyhow::bail!("No command specified");
    }

    let flags = args[1..].iter().take_while(|arg| *arg == "--json").count();
    let mut json_output = flags > 0;
    let mut subcommand_args = args[1 + flags..].to_vec();
    if subcommand_args.is_empty() {
        anyhow::bail!("No subcommand specified");
    }

    if subcommand_args.first().map(String::as_str) != Some("exec")
        && let Some(pos) = subcommand_args.iter().position(|a| a == "--json")
    {
        json_output = true;
        subcommand_args.remove(pos);
    }

    Ok(CliArgs { json_output, subcommand_args })
}

fn main() -> anyhow::Result<()> {
    let Some(args) = std::env::args_os().map(|arg| arg.into_string().ok()).collect::<Option<Vec<String>>>() else {
        eprintln!("glidectl: arguments must be valid UTF-8");
        std::process::exit(1);
    };
    let cli = match parse_cli_args(&args) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("{USAGE}");
            std::process::exit(1);
        }
    };

    if cli.subcommand_args[0] == "tokens" {
        println!("{}", tokens(&cli.subcommand_args[1..])?);
        return Ok(());
    }

    let request = parse_args(&cli.subcommand_args)?;
    let socket_path = find_socket_path()?;

    if matches!(request, Request::Subscribe) {
        return run_subscribe(&socket_path, cli.json_output);
    }

    let (_, response) = exchange(&socket_path, &request)?;

    if cli.json_output {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        print_response(&response);
    }

    if is_failure(&response) {
        std::process::exit(1);
    }
    Ok(())
}

pub fn tokens(args: &[String]) -> anyhow::Result<String> {
    let mut appearance = glide_theme::Appearance::default();
    let mut accent = glide_theme::Accent::default();
    for arg in args {
        if let Some(value) = glide_theme::Appearance::from_key(arg) {
            appearance = value;
        } else if let Some(value) = glide_theme::Accent::from_key(arg) {
            accent = value;
        } else {
            anyhow::bail!("unknown appearance or accent {arg:?}");
        }
    }
    Ok(glide_theme::data::json(&glide_theme::Tokens::new(appearance, accent)))
}

#[must_use]
pub fn is_failure(response: &Response) -> bool {
    matches!(response, Response::Error { .. } | Response::Unknown)
}

fn connect(socket_path: &PathBuf) -> anyhow::Result<UnixStream> {
    let stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    Ok(stream)
}

fn exchange(socket_path: &PathBuf, request: &Request) -> anyhow::Result<(BufReader<UnixStream>, Response)> {
    let mut json = serde_json::to_string(request)?;
    json.push('\n');

    let mut reader = BufReader::new(connect(socket_path)?);
    reader.get_mut().write_all(json.as_bytes())?;
    reader.get_mut().flush()?;

    let mut line = String::new();
    std::io::Read::take(&mut reader, glide_ipc::client::MAX_RESPONSE_BYTES).read_line(&mut line)?;
    Ok((reader, serde_json::from_str(&line)?))
}

fn run_subscribe(socket_path: &PathBuf, json_output: bool) -> anyhow::Result<()> {
    let (mut reader, response) = exchange(socket_path, &Request::Subscribe)?;
    if !matches!(response, Response::Subscribed) {
        anyhow::bail!("subscribe request was not accepted: {response:?}");
    }
    reader.get_ref().set_read_timeout(None)?;

    if !json_output {
        eprintln!("Subscribed. Waiting for events (Ctrl+C to stop)...");
    }

    let mut line = String::new();
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
        Event::LayoutChanged { index, name } => format!("layout changed: {index} ({name})"),
        Event::Gesture { sample } => format!(
            "gesture: {:?} {:?} {} fingers ({:.0}, {:.0}) {:?}",
            sample.kind, sample.phase, sample.fingers, sample.dx, sample.dy, sample.action
        ),
    }
}

const USAGE: &str = r#"glidectl — CLI controller for Glide Wayland compositor

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
    lock                       Lock the screen
    inhibitors                 List what keeps the screen from going dark
    action <name>              Run a named shell action, as a keybind would (e.g. screen-mirroring)
    exec <command...>          Execute shell command
    subscribe                  Stream window/focus/workspace events until Ctrl+C
    dpms <on|off>              Turn the outputs on or blank them
    layout [get|next]          Read or cycle the active keyboard layout
    outputs                    List connected outputs with mode, scale and position
    night-light <on|off|status>  Turn night light on or off, or show its state
    tokens [light|dark] [ACCENT]  Print the design tokens as JSON (no session needed)

OPTIONS:
    --json                     Output response as raw JSON
"#;

fn find_socket_path() -> anyhow::Result<PathBuf> {
    glide_ipc::client::socket_path().filter(|path| path.exists()).ok_or_else(|| {
        anyhow::anyhow!(
            "Glide IPC socket not found. Is Glide running, and are GLIDE_IPC_SOCK \
             or WAYLAND_DISPLAY + XDG_RUNTIME_DIR set?"
        )
    })
}

#[must_use]
pub fn shell_quote(word: &str) -> String {
    let safe = !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c));
    if safe { word.to_string() } else { format!("'{}'", word.replace('\'', r"'\''")) }
}

#[must_use]
pub fn exec_command(words: &[String]) -> String {
    match words {
        [single] => single.clone(),
        many => many.iter().map(|word| shell_quote(word)).collect::<Vec<_>>().join(" "),
    }
}

fn parse_args(args: &[String]) -> anyhow::Result<Request> {
    match args[0].as_str() {
        "ping" => Ok(Request::Ping),
        "quit" => Ok(Request::Quit),
        "reload" => Ok(Request::Reload),
        "lock" => Ok(Request::Lock),
        "inhibitors" => Ok(Request::IdleInhibitors),
        "action" => match args.get(1) {
            Some(name) => Ok(Request::Action { name: name.clone() }),
            None => anyhow::bail!("Usage: glidectl action <name>"),
        },
        "outputs" | "output" => Ok(Request::ListOutputs),
        "night-light" | "nightlight" => match args.get(1).map(String::as_str) {
            Some("on") => Ok(Request::NightLight { on: Some(true) }),
            Some("off") => Ok(Request::NightLight { on: Some(false) }),
            Some("status") | None => Ok(Request::NightLight { on: None }),
            _ => anyhow::bail!("Usage: glidectl night-light <on|off|status>"),
        },
        "subscribe" | "sub" => Ok(Request::Subscribe),
        "dpms" => match args.get(1).map(String::as_str) {
            Some("on") => Ok(Request::SetDpms { on: true }),
            Some("off") => Ok(Request::SetDpms { on: false }),
            _ => anyhow::bail!("Usage: glidectl dpms <on|off>"),
        },
        "layout" => match args.get(1).map(String::as_str) {
            Some("next") | Some("cycle") => Ok(Request::CycleLayout),
            Some("get") | None => Ok(Request::GetLayout),
            _ => anyhow::bail!("Usage: glidectl layout <get|next>"),
        },
        "exec" => {
            required(args, 1, "exec <command>")?;
            Ok(Request::Exec { command: exec_command(&args[1..]) })
        }
        "workspace" | "ws" => match required(args, 1, "workspace <list|switch>")? {
            "list" | "ls" => Ok(Request::ListWorkspaces),
            "switch" => Ok(Request::SwitchWorkspace {
                target: parse_workspace_target(required(args, 2, "workspace switch <N|next|prev>")?)?,
            }),
            other => anyhow::bail!("Unknown workspace subcommand: {other}"),
        },
        "window" | "win" => match required(args, 1, "window <list|focus|move>")? {
            "list" | "ls" => Ok(Request::ListWindows),
            "focus" => Ok(Request::FocusWindow {
                selector: match required(args, 2, "window focus <ID|focused>")? {
                    id if id.eq_ignore_ascii_case("focused") => WindowSelector::Focused,
                    id => WindowSelector::Id(id.parse()?),
                },
            }),
            "move" => Ok(Request::MoveWindow {
                window: WindowSelector::Focused,
                target: parse_workspace_target(required(args, 2, "window move <N|next|prev>")?)?,
            }),
            other => anyhow::bail!("Unknown window subcommand: {other}"),
        },
        other => anyhow::bail!("Unknown command: {other}"),
    }
}

fn required<'a>(args: &'a [String], index: usize, usage: &str) -> anyhow::Result<&'a str> {
    args.get(index).map(String::as_str).ok_or_else(|| anyhow::anyhow!("Usage: glidectl {usage}"))
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

fn print_response(response: &Response) {
    match response {
        Response::Ok => println!("OK"),
        Response::Pong => println!("PONG"),
        Response::Error { message } => eprintln!("Error: {message}"),
        Response::Subscribed => println!("Subscribed"),
        Response::Unknown => {
            eprintln!("Error: the compositor sent a response this glidectl does not understand")
        }
        Response::Layout { index, name } => println!("{index} {name}"),
        Response::NightLight { on } => println!("{}", if *on { "on" } else { "off" }),
        Response::ActivationToken { token } => println!("{token}"),
        Response::Screenshot { path } => println!("{path}"),
        Response::Color { red, green, blue } => println!("{red:.4} {green:.4} {blue:.4}"),
        Response::QrCodes { codes } => {
            for code in codes {
                println!("{}", code.expose());
            }
        }
        Response::IdleInhibitors { apps, caffeine } => {
            for app in apps {
                println!("{app}");
            }
            if *caffeine {
                println!("caffeine");
            }
        }
        Response::Outputs { list } => {
            println!("{:<12} {:<28} {:<22} {:<7} {:<12}", "NAME", "MODEL", "MODE", "SCALE", "POSITION");
            println!("{}", "-".repeat(84));
            for o in list {
                let model = format!("{} {}", o.make, o.model);
                let mode = format!("{}x{}@{:.2}", o.width, o.height, f64::from(o.refresh_mhz) / 1000.0);
                println!("{:<12} {:<28} {:<22} {:<7} {},{}", o.name, model.trim(), mode, o.scale, o.x, o.y);
            }
        }
        Response::Workspaces { list } => {
            println!("{:<5} {:<10} {:<8} {:<10}", "INDEX", "ID", "ACTIVE", "WINDOWS");
            println!("{}", "-".repeat(35));
            for ws in list {
                println!("{:<5} {:<10} {:<8} {:<10}", ws.index + 1, ws.id, star(ws.active), ws.window_count);
            }
        }
        Response::Windows { list } => {
            println!(
                "{:<6} {:<15} {:<10} {:<25} {:<8} {:<9}",
                "ID", "APP_ID", "WS_INDEX", "TITLE", "FOCUSED", "MINIMIZED"
            );
            println!("{}", "-".repeat(78));
            for w in list {
                println!(
                    "{:<6} {:<15} {:<10} {:<25} {:<8} {:<9}",
                    w.id,
                    w.app_id.as_deref().unwrap_or("<unknown>"),
                    w.workspace_index + 1,
                    w.title.as_deref().unwrap_or("<untitled>"),
                    star(w.focused),
                    star(w.minimized)
                );
            }
        }
    }
}

fn star(on: bool) -> &'static str {
    if on { "*" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(ToString::to_string).collect()
    }

    fn parse(args: &[&str]) -> Request {
        parse_args(&words(args)).unwrap()
    }

    fn cli(args: &[&str]) -> CliArgs {
        parse_cli_args(&words(args)).unwrap()
    }

    fn exec(command: &str) -> Request {
        Request::Exec { command: command.into() }
    }

    #[test]
    fn test_parse_args() {
        assert_eq!(parse(&["ping"]), Request::Ping);
        assert_eq!(parse(&["quit"]), Request::Quit);
        assert_eq!(parse(&["workspace", "list"]), Request::ListWorkspaces);
        assert_eq!(
            parse(&["workspace", "switch", "2"]),
            Request::SwitchWorkspace { target: WorkspaceTarget::Index(1) }
        );
        assert_eq!(parse(&["exec", "alacritty"]), exec("alacritty"));
        assert_eq!(parse(&["exec", "notify-send", "a b"]), exec("notify-send 'a b'"));
        assert_eq!(parse(&["exec", "echo hi | wc -c"]), exec("echo hi | wc -c"));
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(parse(&["subscribe"]), Request::Subscribe);
        assert_eq!(parse(&["sub"]), Request::Subscribe);
        assert_eq!(parse(&["lock"]), Request::Lock);
        assert_eq!(parse(&["inhibitors"]), Request::IdleInhibitors);
        assert_eq!(parse(&["action", "screen-mirroring"]), Request::Action { name: "screen-mirroring".into() });
    }

    #[test]
    fn test_parse_cli_args_flags() {
        let first = cli(&["glidectl", "--json", "workspace", "list"]);
        assert!(first.json_output);
        assert_eq!(first.subcommand_args, vec!["workspace", "list"]);

        let trailing = cli(&["glidectl", "workspace", "list", "--json"]);
        assert!(trailing.json_output);
        assert_eq!(trailing.subcommand_args, vec!["workspace", "list"]);

        let in_exec = cli(&["glidectl", "exec", "my_script", "--json"]);
        assert!(!in_exec.json_output);
        assert_eq!(in_exec.subcommand_args, vec!["exec", "my_script", "--json"]);
        assert_eq!(parse_args(&in_exec.subcommand_args).unwrap(), exec("my_script --json"));

        let both = cli(&["glidectl", "--json", "exec", "my_script", "--json"]);
        assert!(both.json_output);
        assert_eq!(both.subcommand_args, vec!["exec", "my_script", "--json"]);
        assert_eq!(parse_args(&both.subcommand_args).unwrap(), exec("my_script --json"));
    }

    #[test]
    fn test_adversarial_cli_edge_cases() {
        assert!(parse_cli_args(&[]).is_err());
        assert!(parse_cli_args(&words(&["glidectl"])).is_err());
        assert!(parse_cli_args(&words(&["glidectl", "--json"])).is_err());

        let ws0 = cli(&["glidectl", "workspace", "switch", "0"]);
        assert!(parse_args(&ws0.subcommand_args).is_err());

        for (word, target) in [("prev", WorkspaceTarget::Previous), ("next", WorkspaceTarget::Next)] {
            let switch = cli(&["glidectl", "workspace", "switch", word]);
            assert_eq!(parse_args(&switch.subcommand_args).unwrap(), Request::SwitchWorkspace { target });
        }

        let focused = cli(&["glidectl", "window", "focus", "focused"]);
        assert_eq!(
            parse_args(&focused.subcommand_args).unwrap(),
            Request::FocusWindow { selector: WindowSelector::Focused }
        );

        let by_id = cli(&["glidectl", "--json", "window", "focus", "42"]);
        assert!(by_id.json_output);
        assert_eq!(
            parse_args(&by_id.subcommand_args).unwrap(),
            Request::FocusWindow { selector: WindowSelector::Id(42) }
        );

        let moved = cli(&["glidectl", "window", "move", "3", "--json"]);
        assert!(moved.json_output);
        assert_eq!(
            parse_args(&moved.subcommand_args).unwrap(),
            Request::MoveWindow { window: WindowSelector::Focused, target: WorkspaceTarget::Index(2) }
        );

        let unknown = cli(&["glidectl", "unknown_cmd"]);
        assert!(parse_args(&unknown.subcommand_args).is_err());
    }

    #[test]
    fn tokens_take_an_appearance_and_an_accent_in_any_order() {
        let sheet = tokens(&words(&["teal", "light"])).unwrap();
        assert!(sheet.contains("\"appearance\":\"light\"") && sheet.contains("\"accent\":\"teal\""));
        assert!(tokens(&words(&["mauve"])).is_err());
    }

    #[test]
    fn usage_lists_night_light_with_the_arguments_it_parses() {
        assert!(USAGE.contains("\n    night-light <on|off|status>  "));
        assert_eq!(parse(&["night-light", "on"]), Request::NightLight { on: Some(true) });
        assert_eq!(parse(&["night-light", "off"]), Request::NightLight { on: Some(false) });
        assert_eq!(parse(&["night-light", "status"]), Request::NightLight { on: None });
    }
}
