use std::ffi::OsString;
use std::io::{self, ErrorKind, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Program,
    Script,
    Launcher,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runnable {
    pub kind: Kind,
    pub allowed: bool,
    interpreter: Vec<String>,
}

impl Runnable {
    #[must_use]
    pub fn needs_permission(&self) -> bool {
        !self.allowed && self.kind != Kind::Script
    }

    #[must_use]
    pub const fn runs_in_terminal(&self) -> bool {
        !matches!(self.kind, Kind::Launcher)
    }
}

const ELF: &[u8] = b"\x7fELF";
const ELF_ORDER: usize = 5;
const ELF_BIG_ENDIAN: u8 = 2;
const ELF_KIND: usize = 16;
const ELF_EXECUTABLE: u16 = 2;
const ELF_SHARED: u16 = 3;
const HEAD: usize = 256;
const INTERPRETERS: [(&str, &str); 10] = [
    ("sh", "sh"),
    ("bash", "bash"),
    ("zsh", "zsh"),
    ("fish", "fish"),
    ("py", "python3"),
    ("pl", "perl"),
    ("rb", "ruby"),
    ("lua", "lua"),
    ("js", "node"),
    ("php", "php"),
];

#[must_use]
pub fn shebang(head: &[u8]) -> Option<Vec<String>> {
    let line = head.strip_prefix(b"#!")?.split(|byte| *byte == b'\n').next()?;
    let words: Vec<String> = String::from_utf8_lossy(line).split_whitespace().map(str::to_string).collect();
    (!words.is_empty()).then_some(words)
}

fn elf_program(name: &str, head: &[u8]) -> bool {
    let (Some(order), Some(kind)) = (head.get(ELF_ORDER), head.get(ELF_KIND..ELF_KIND + 2)) else {
        return false;
    };
    let kind = if *order == ELF_BIG_ENDIAN {
        u16::from_be_bytes([kind[0], kind[1]])
    } else {
        u16::from_le_bytes([kind[0], kind[1]])
    };
    let library = name.contains(".so.") || Path::new(name).extension().is_some_and(|extension| extension == "so");
    kind == ELF_EXECUTABLE || (kind == ELF_SHARED && !library)
}

#[must_use]
pub fn classify(name: &str, head: &[u8], mode: u32) -> Option<Runnable> {
    let allowed = mode & 0o111 != 0;
    let extension = Path::new(name).extension().map(|ext| ext.to_string_lossy().to_lowercase());
    if head.starts_with(ELF) {
        return elf_program(name, head).then_some(Runnable { kind: Kind::Program, allowed, interpreter: Vec::new() });
    }
    if extension.as_deref() == Some("desktop") {
        return Some(Runnable { kind: Kind::Launcher, allowed, interpreter: Vec::new() });
    }
    if let Some(interpreter) = shebang(head) {
        return Some(Runnable { kind: Kind::Script, allowed, interpreter });
    }
    let by_extension = INTERPRETERS.iter().find(|(known, _)| extension.as_deref() == Some(*known))?;
    Some(Runnable { kind: Kind::Script, allowed: false, interpreter: vec![by_extension.1.to_string()] })
}

#[must_use]
pub fn probe(path: &Path) -> Option<Runnable> {
    let meta = std::fs::metadata(path).ok().filter(std::fs::Metadata::is_file)?;
    let mut head = Vec::with_capacity(HEAD);
    std::fs::File::open(path).ok()?.take(HEAD as u64).read_to_end(&mut head).ok()?;
    classify(&path.file_name()?.to_string_lossy(), &head, meta.permissions().mode())
}

pub fn allow(path: &Path) -> io::Result<()> {
    let mode = std::fs::metadata(path)?.permissions().mode();
    let readable = (mode & 0o444) >> 2;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode((mode | readable | 0o100) & 0o7777))
}

#[must_use]
pub fn argv(path: &Path, runnable: &Runnable) -> Vec<OsString> {
    let direct = runnable.allowed || runnable.kind == Kind::Program;
    let mut words: Vec<OsString> =
        if direct { Vec::new() } else { runnable.interpreter.iter().map(OsString::from).collect() };
    words.push(path.into());
    words
}

fn folder(path: &Path) -> PathBuf {
    path.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
}

fn detached(words: &[OsString], dir: &Path) -> io::Result<()> {
    let (program, args) = words.split_first().ok_or_else(|| io::Error::from(ErrorKind::InvalidInput))?;
    let mut child = Command::new(program)
        .args(args)
        .envs(glide_sdk::system::launch_env())
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

const HOLD: &str = "note=$1; shift; \"$@\"; code=$?; printf '\\n%s %s\\n' \"$note\" \"$code\"; read -r _";

#[must_use]
pub fn held(words: &[OsString], note: &str) -> Vec<OsString> {
    ["sh", "-c", HOLD, "sh", note].into_iter().map(OsString::from).chain(words.iter().cloned()).collect()
}

fn in_terminal(words: &[OsString], dir: &Path, note: &str) -> io::Result<()> {
    let words = held(words, note);
    let terminal = glide_config::terminal::current();
    let plain: Option<Vec<String>> = words.iter().map(|word| word.to_str().map(str::to_string)).collect();
    if glide_app::integration::is_glide_terminal(&terminal)
        && let Some(plain) = &plain
        && let Some(token) = glide_sdk::system::launch_token()
        && glide_app::integration::open_terminal_tab(dir, plain, &token).is_ok()
    {
        return Ok(());
    }
    let command: Vec<OsString> = terminal
        .split_whitespace()
        .map(OsString::from)
        .chain(glide_config::terminal::exec_args(&terminal).iter().map(OsString::from))
        .chain(words)
        .collect();
    detached(&command, dir)
}

pub fn run(path: &Path, runnable: &Runnable, terminal: bool, note: &str) -> io::Result<()> {
    let dir = folder(path);
    if runnable.kind == Kind::Launcher {
        return super::actions::launch_desktop_file(path, &dir);
    }
    let words = argv(path, runnable);
    if terminal { in_terminal(&words, &dir, note) } else { detached(&words, &dir) }
}
