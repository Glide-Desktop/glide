use std::fmt::Write as _;

#[allow(clippy::struct_excessive_bools)] // One field per command-line switch
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub camera: String,
    pub user: String,
    pub verify: bool,
    pub enroll: bool,
    pub status: bool,
    pub delete_profile: bool,
    pub cameras: bool,
    pub attempts: bool,
    pub pam_status: bool,
    pub pam_enable: String,
    pub pam_disable: String,
    pub image: String,
    pub out: String,
    pub report: String,
    pub source: String,
    pub json: bool,
    pub stream: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Options),
    Help,
    Error(String),
}

struct FlagDef {
    name: &'static str,
    is_bool: bool,
    usage: &'static str,
}

const FLAGS: &[FlagDef] = &[
    FlagDef { name: "attempts", is_bool: true, usage: "Print recent face unlock attempts as JSON" },
    FlagDef {
        name: "camera",
        is_bool: false,
        usage: "V4L2 camera device (default: the configured or first trusted one)",
    },
    FlagDef { name: "cameras", is_bool: true, usage: "List capture devices and whether they are trusted, as JSON" },
    FlagDef { name: "delete-profile", is_bool: true, usage: "Delete the enrolled face (root)" },
    FlagDef { name: "enroll", is_bool: true, usage: "Enroll the face with the camera (root)" },
    FlagDef { name: "image", is_bool: false, usage: "Evaluate a still image" },
    FlagDef { name: "json", is_bool: true, usage: "Print only structured JSON" },
    FlagDef { name: "out", is_bool: false, usage: "Save the annotated image here (with -image)" },
    FlagDef {
        name: "pam-disable",
        is_bool: false,
        usage: "Remove face unlock from a PAM service: polkit, sudo or login (root)",
    },
    FlagDef {
        name: "pam-enable",
        is_bool: false,
        usage: "Add face unlock to a PAM service: polkit, sudo or login (root)",
    },
    FlagDef { name: "pam-status", is_bool: true, usage: "Print which PAM services use face unlock, as JSON" },
    FlagDef { name: "report", is_bool: false, usage: "Save the diagnostic report here" },
    FlagDef { name: "source", is_bool: false, usage: "Where the check runs: lock, polkit, sudo, login or test" },
    FlagDef { name: "status", is_bool: true, usage: "Print the enrolled face status as JSON" },
    FlagDef { name: "stream", is_bool: true, usage: "Print progress as JSON lines (with -enroll or -verify)" },
    FlagDef {
        name: "user",
        is_bool: false,
        usage: "Target account (root only; pkexec always means the invoking user)",
    },
    FlagDef { name: "verify", is_bool: true, usage: "Check the face; exit 0 when recognised" },
];

#[must_use]
pub fn usage() -> String {
    let mut s = String::from("FaceKey: local face unlock for Glide\n\nUsage:\n");
    s.push_str("  facekey -status\n  facekey -verify [-stream] [-source lock]\n  pkexec facekey -enroll -stream\n");
    s.push_str("  pkexec facekey -delete-profile\n  facekey -cameras\n  pkexec facekey -pam-enable sudo\n");
    s.push_str("\nOptions:\n");
    for f in FLAGS {
        let kind = if f.is_bool { "" } else { " string" };
        let _ = writeln!(s, "  -{}{kind}\n    \t{}", f.name, f.usage);
    }
    s
}

fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "1" | "t" | "T" | "true" | "TRUE" | "True" => Some(true),
        "0" | "f" | "F" | "false" | "FALSE" | "False" => Some(false),
        _ => None,
    }
}

fn set(o: &mut Options, name: &str, value: String) {
    let on = value == "true";
    match name {
        "attempts" => o.attempts = on,
        "camera" => o.camera = value,
        "cameras" => o.cameras = on,
        "delete-profile" => o.delete_profile = on,
        "enroll" => o.enroll = on,
        "image" => o.image = value,
        "json" => o.json = on,
        "out" => o.out = value,
        "pam-disable" => o.pam_disable = value,
        "pam-enable" => o.pam_enable = value,
        "pam-status" => o.pam_status = on,
        "report" => o.report = value,
        "source" => o.source = value,
        "status" => o.status = on,
        "stream" => o.stream = on,
        "user" => o.user = value,
        "verify" => o.verify = on,
        _ => {}
    }
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Parsed {
    let mut o = Options::default();
    let mut it = args.into_iter().peekable();
    while let Some(arg) = it.next() {
        if arg == "--" || !arg.starts_with('-') || arg == "-" {
            break;
        }
        let body = arg.trim_start_matches('-');
        if arg.starts_with("---") || body.is_empty() {
            return Parsed::Error(format!("bad flag syntax: {arg}"));
        }
        let (name, inline) = match body.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (body, None),
        };
        if name == "h" || name == "help" {
            return Parsed::Help;
        }
        let Some(def) = FLAGS.iter().find(|f| f.name == name) else {
            return Parsed::Error(format!("flag provided but not defined: -{name}"));
        };
        let value = if def.is_bool {
            match inline {
                Some(v) => match parse_bool(&v) {
                    Some(b) => b.to_string(),
                    None => return Parsed::Error(format!("invalid boolean value {v:?} for -{name}: parse error")),
                },
                None => "true".to_string(),
            }
        } else {
            match inline.or_else(|| it.next()) {
                Some(v) => v,
                None => return Parsed::Error(format!("flag needs an argument: -{name}")),
            }
        };
        set(&mut o, name, value);
    }
    Parsed::Run(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Parsed {
        parse(args.iter().map(ToString::to_string))
    }

    #[test]
    fn go_style_flags() {
        let Parsed::Run(o) = p(&["-verify", "-stream", "-camera", "/dev/video2", "--user=alice", "-source=lock"])
        else {
            panic!()
        };
        assert!(o.verify && o.stream && !o.json);
        assert_eq!(o.camera, "/dev/video2");
        assert_eq!(o.user, "alice");
        assert_eq!(o.source, "lock");
        let Parsed::Run(o) = p(&["-pam-enable", "sudo"]) else { panic!() };
        assert_eq!(o.pam_enable, "sudo");
        let Parsed::Run(o) = p(&["-verify=false"]) else { panic!() };
        assert!(!o.verify);
        assert_eq!(p(&["-help"]), Parsed::Help);
        assert!(matches!(p(&["-nope"]), Parsed::Error(_)));
        assert!(matches!(p(&["-image"]), Parsed::Error(_)));
    }
}
