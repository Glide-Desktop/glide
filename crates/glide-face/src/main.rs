use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glide_config::facekey::{Attempt, Event, PamService, Reason, Source, attempt_line, recent_attempts};
use glide_face::annotate::annotate;
use glide_face::biometrics::{Pipeline, RgbaImage, go_time};
use glide_face::camera::{self, Camera, CameraError, v4l2};
use glide_face::cli::{self, Options, Parsed};
use glide_face::pam;
use glide_face::profile::{STALE_PROFILE_HINT, Store, resolve_username};
use glide_face::protocol::{self, PREVIEW_SIDE, emit};
use glide_face::session::{EnrollmentState, VerificationSession, build_report, to_pretty_json};

const ENROLL_TIMEOUT: Duration = Duration::from_mins(3);
const PREVIEW_EVERY: Duration = Duration::from_millis(80);
const CAPTURE_WIDTH: usize = 640;
const CAPTURE_HEIGHT: usize = 480;

macro_rules! fatal {
    ($($t:tt)*) => {{
        eprintln!($($t)*);
        return ExitCode::from(1);
    }};
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match cli::parse(args.clone()) {
        Parsed::Run(o) if std::env::var_os("PKEXEC_UID").is_some_and(|uid| !uid.is_empty()) => {
            if let Err(message) = cli::admitted_under_pkexec(&args, &o) {
                eprintln!("{message}");
                return ExitCode::from(2);
            }
            o
        }
        Parsed::Run(o) => o,
        Parsed::Help => {
            print!("{}", cli::usage());
            return ExitCode::SUCCESS;
        }
        Parsed::Error(msg) => {
            eprintln!("{msg}");
            eprint!("{}", cli::usage());
            return ExitCode::from(2);
        }
    };
    install_exit_on_signal();

    if opts.cameras {
        println!("{}", serde_json::to_string_pretty(&v4l2::list()).unwrap_or_default());
        return ExitCode::SUCCESS;
    }
    if opts.pam_status {
        let status = pam::status(Path::new(pam::ETC_DIR), Path::new(pam::VENDOR_DIR));
        println!("{}", serde_json::to_string_pretty(&status).unwrap_or_default());
        return ExitCode::SUCCESS;
    }
    if !opts.pam_enable.is_empty() || !opts.pam_disable.is_empty() {
        return pam_mode(&opts);
    }

    let store = match resolve_username(&opts.user).and_then(|u| Store::open(&u)) {
        Ok(s) => s,
        Err(e) => fatal!("[FATAL] {e}"),
    };
    if opts.status {
        return status_mode(&store);
    }
    if opts.attempts {
        return attempts_mode(&store);
    }
    if opts.delete_profile {
        return match store.delete() {
            Ok(()) => {
                println!("[OK] Face deleted.");
                ExitCode::SUCCESS
            }
            Err(e) => fatal!("[ERROR] Failed to delete the face: {e}"),
        };
    }
    if opts.enroll && !store.can_write() {
        if opts.stream {
            emit(&Event::Failed { reason: Reason::Error, message: "enrollment needs root".into() });
        }
        fatal!("[ERROR] Enrollment requires root privileges. Run: pkexec facekey -enroll");
    }
    let source = Source::parse(&opts.source);
    if opts.verify && source == Some(Source::Sudo) && remote_session() {
        return finish_verify(&store, &opts, source, false, Reason::Remote);
    }
    if !opts.image.is_empty() || opts.verify || opts.enroll {
        let pipeline = match Pipeline::new() {
            Ok(p) => Arc::new(p),
            Err(e) => {
                if opts.stream {
                    emit(&Event::Failed { reason: Reason::Error, message: e.to_string() });
                }
                fatal!("[FATAL] Failed to initialize the face pipeline: {e}");
            }
        };
        if !opts.image.is_empty() {
            return image_mode(&pipeline, &store, &opts);
        }
        if opts.verify {
            return verification(&pipeline, &store, &opts, source);
        }
        return enrollment(&pipeline, store, &opts);
    }
    print!("{}", cli::usage());
    ExitCode::SUCCESS
}

fn install_exit_on_signal() {
    extern "C" fn on_signal(_: libc::c_int) {
        // SAFETY: `_exit` is async-signal-safe and ends the process without unwinding.
        unsafe { libc::_exit(1) };
    }
    let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    // SAFETY: the handler only calls `_exit`, which is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
}

fn remote_session() -> bool {
    ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"].iter().any(|name| std::env::var_os(name).is_some())
        || std::env::var("PAM_RHOST").is_ok_and(|host| !host.is_empty() && host != "localhost")
}

fn pam_mode(opts: &Options) -> ExitCode {
    let (name, on) = if opts.pam_enable.is_empty() { (&opts.pam_disable, false) } else { (&opts.pam_enable, true) };
    let Some(service) = PamService::parse(name) else { fatal!("[ERROR] Unknown PAM service {name:?}") };
    if !nix::unistd::geteuid().is_root() {
        fatal!("[ERROR] Changing PAM services requires root. Run: pkexec facekey -pam-enable {name}");
    }
    let binary = match trusted_binary() {
        Ok(path) => path,
        Err(e) => fatal!("[ERROR] {e}"),
    };
    match pam::apply(service, on, &binary, Path::new(pam::ETC_DIR), Path::new(pam::VENDOR_DIR)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fatal!("[ERROR] Failed to update /etc/pam.d/{}: {e}", service.file()),
    }
}

fn trusted_binary() -> Result<PathBuf, String> {
    use std::os::unix::fs::MetadataExt;
    let exe = std::env::current_exe().and_then(std::fs::canonicalize).map_err(|e| format!("own path: {e}"))?;
    let meta = std::fs::metadata(&exe).map_err(|e| format!("{}: {e}", exe.display()))?;
    if !exe.starts_with("/usr") || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
        return Err(format!(
            "{} must be installed under /usr, owned by root and not writable by others",
            exe.display()
        ));
    }
    Ok(exe)
}

fn status_mode(store: &Store) -> ExitCode {
    let exists = store.exists();
    let mut info = serde_json::Map::new();
    info.insert("enrolled".into(), exists.into());
    info.insert("user".into(), store.username.clone().into());
    info.insert("path".into(), store.profile_path().to_string_lossy().into_owned().into());
    if !glide_face::biometrics::models::installed() {
        info.insert("missing".into(), "models".into());
    } else if glide_face::biometrics::ort_init::library().is_none() {
        info.insert("missing".into(), "onnxruntime".into());
    }
    if exists {
        match store.load() {
            Ok(p) => {
                info.insert("modelName".into(), p.model_name.clone().into());
                info.insert("compatible".into(), p.is_compatible().into());
                if !p.is_compatible() {
                    info.insert("error".into(), STALE_PROFILE_HINT.into());
                }
                info.insert("templateHash".into(), p.template_hash.into());
                info.insert("createdAt".into(), p.created_at.as_ref().map(go_time::format).into());
                info.insert("updatedAt".into(), p.updated_at.as_ref().map(go_time::format).into());
                info.insert("numSamples".into(), p.num_samples.into());
            }
            Err(e) => {
                info.insert("error".into(), e.to_string().into());
            }
        }
    }
    println!("{}", to_pretty_json(&info).unwrap_or_default());
    ExitCode::SUCCESS
}

fn now() -> i64 {
    chrono::Local::now().timestamp()
}

fn attempts_mode(store: &Store) -> ExitCode {
    let mut texts = vec![store.read_attempts()];
    if !nix::unistd::geteuid().is_root()
        && let Some(path) = glide_config::facekey::user_attempts_path()
    {
        texts.push(std::fs::read_to_string(path).unwrap_or_default());
    }
    println!("{}", serde_json::to_string_pretty(&recent_attempts(&texts, now(), 20)).unwrap_or_default());
    ExitCode::SUCCESS
}

fn record(store: &Store, source: Option<Source>, success: bool, reason: Reason) {
    let Some(source) = source.filter(|source| *source != Source::Test) else { return };
    let line = attempt_line(&Attempt { at: now(), source, success, reason });
    if store.can_write() {
        let _ = store.append_attempt(&line);
        return;
    }
    let Some(path) = glide_config::facekey::user_attempts_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let previous = std::fs::read_to_string(&path).unwrap_or_default();
    let kept: Vec<&str> = previous.lines().rev().take(199).collect::<Vec<_>>().into_iter().rev().collect();
    let mut text = kept.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&line);
    text.push('\n');
    let _ = glide_config::write_private(&path, text.as_bytes());
}

fn finish_verify(store: &Store, opts: &Options, source: Option<Source>, success: bool, reason: Reason) -> ExitCode {
    record(store, source, success, reason);
    if opts.stream {
        emit(&Event::Finished { success, reason });
    }
    if success { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

fn configured_camera(store: &Store) -> Option<String> {
    let home = nix::unistd::User::from_name(&store.username).ok().flatten()?.dir;
    let text = std::fs::read_to_string(home.join(".config/glide/glide.toml")).ok()?;
    let value: toml::Value = text.parse().ok()?;
    let camera = value.get("facekey")?.get("camera")?.as_str()?.to_string();
    let index = camera.strip_prefix("/dev/video")?;
    index.parse::<u32>().ok().map(|_| camera)
}

fn pick_camera(store: &Store, opts: &Options) -> String {
    if !opts.camera.is_empty() {
        return opts.camera.clone();
    }
    configured_camera(store)
        .or_else(|| v4l2::list().into_iter().find(|camera| camera.trusted).map(|camera| camera.path))
        .unwrap_or_else(|| "/dev/video0".into())
}

fn start_camera(device: &str, opts: &Options) -> Result<Box<dyn Camera>, CameraError> {
    let mut cam = camera::open(device, CAPTURE_WIDTH, CAPTURE_HEIGHT)?;
    cam.start()?;
    if opts.stream {
        emit(&Event::Started { camera: cam.device_name() });
    }
    Ok(cam)
}

fn image_mode(pipe: &Pipeline, store: &Store, opts: &Options) -> ExitCode {
    let img = match RgbaImage::open(&opts.image) {
        Ok(i) => Arc::new(i),
        Err(e) => fatal!("Failed to decode image: {e}"),
    };
    let (mut reference, mut multi, mut hash) = (Vec::new(), Vec::new(), String::new());
    if store.exists()
        && let Some(p) = store.load().ok().filter(glide_face::profile::Profile::is_compatible)
    {
        reference = p.embedding;
        multi = p.multi_embeddings;
        hash = p.template_hash;
    }
    let eval = match pipe.evaluate_with_multi(&img, &reference, &multi) {
        Ok(e) => e,
        Err(e) => fatal!("Evaluation failed: {e}"),
    };
    let rep = build_report("cli-image-eval", eval.accepted, &eval.reason_code, 1, 1, Some(&eval), &hash);
    if !opts.report.is_empty()
        && let Err(e) = rep.save_to_file(&opts.report)
    {
        eprintln!("[WARN] Failed to save report: {e}");
    }
    if !opts.out.is_empty()
        && let Some(face) = eval.face.as_ref().filter(|_| eval.face_found)
    {
        let path = Path::new(&opts.out);
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = annotate(&img, face).save_png(path) {
            eprintln!("[WARN] Failed to save annotated image: {e}");
        }
    }
    println!("{}", rep.to_json().unwrap_or_default());
    if eval.accepted { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

fn verification(pipe: &Pipeline, store: &Store, opts: &Options, source: Option<Source>) -> ExitCode {
    if !store.exists() {
        return finish_verify(store, opts, None, false, Reason::NotEnrolled);
    }
    let profile = match store.load() {
        Ok(p) if p.is_compatible() => p,
        Ok(_) => return finish_verify(store, opts, None, false, Reason::StaleProfile),
        Err(e) => {
            eprintln!("Failed to load the face: {e}");
            return finish_verify(store, opts, None, false, Reason::Error);
        }
    };
    let mut cam = match start_camera(&pick_camera(store, opts), opts) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Camera: {e}");
            return finish_verify(store, opts, source, false, protocol::camera_reason(&e));
        }
    };
    let mut sess = VerificationSession::new(profile);
    let mut last_preview = None;
    while !sess.is_done {
        match cam.wait_frame(Duration::from_millis(100)) {
            Ok(frame) => {
                if let Err(e) = sess.process_frame(&frame.image, pipe) {
                    eprintln!("[WARN] Frame error: {e}");
                }
                if opts.stream && source == Some(Source::Test) {
                    stream_preview(&frame.image, &mut last_preview);
                }
                if opts.stream {
                    let face = sess.last_evaluation.as_ref().is_some_and(|eval| eval.face_found);
                    emit(&Event::Verify {
                        consensus: u8::try_from(sess.consensus_frames.max(0)).unwrap_or(u8::MAX),
                        required: u8::try_from(sess.required_consensus.max(0)).unwrap_or(u8::MAX),
                        face,
                    });
                }
            }
            Err(CameraError::Timeout) => {}
            Err(e) => {
                let _ = cam.stop();
                eprintln!("Camera: {e}");
                return finish_verify(store, opts, source, false, protocol::camera_reason(&e));
            }
        }
        sess.check_timeout();
    }
    let _ = cam.stop();
    if let Some(rep) = &sess.report {
        if !opts.report.is_empty() {
            let _ = rep.save_to_file(&opts.report);
        }
        if opts.json && !opts.stream {
            println!("{}", rep.to_json().unwrap_or_default());
        }
    }
    let reason = if sess.is_success { Reason::Passed } else { protocol::reason_from_code(&sess.reason_code) };
    finish_verify(store, opts, source, sess.is_success, reason)
}

fn enrollment(pipe: &Pipeline, store: Store, opts: &Options) -> ExitCode {
    let device = pick_camera(&store, opts);
    let mut cam = match start_camera(&device, opts) {
        Ok(c) => c,
        Err(e) => {
            if opts.stream {
                emit(&Event::Failed { reason: protocol::camera_reason(&e), message: e.to_string() });
            }
            fatal!("Failed to start the camera: {e}");
        }
    };
    pipe.reset_tracker();
    let mut sess = EnrollmentState::new(Some(store));
    let started = Instant::now();
    let mut last_preview: Option<Instant> = None;
    while !sess.is_done {
        if started.elapsed() > ENROLL_TIMEOUT {
            sess.is_done = true;
            sess.error_message = "Enrollment timed out".into();
            break;
        }
        match cam.wait_frame(Duration::from_millis(100)) {
            Ok(frame) => {
                if let Err(e) = sess.process_frame(&frame.image, pipe) {
                    eprintln!("[WARN] Frame error: {e}");
                }
                if opts.stream {
                    stream_enrollment(&sess, &frame.image, &mut last_preview);
                }
            }
            Err(CameraError::Timeout) => {}
            Err(e) => {
                let _ = cam.stop();
                if opts.stream {
                    emit(&Event::Failed { reason: protocol::camera_reason(&e), message: e.to_string() });
                }
                fatal!("Camera error: {e}");
            }
        }
    }
    let _ = cam.stop();
    let reason = if sess.is_success { Reason::Passed } else { protocol::enrollment_reason(&sess.error_message) };
    if opts.stream {
        emit(&Event::Finished { success: sess.is_success, reason });
    }
    if sess.is_success {
        println!("[SUCCESS] Face enrolled and encrypted.");
        ExitCode::SUCCESS
    } else {
        println!("[FAILED] {}", sess.error_message);
        ExitCode::from(1)
    }
}

fn stream_enrollment(sess: &EnrollmentState, image: &RgbaImage, last_preview: &mut Option<Instant>) {
    let eval = sess.last_evaluation.as_ref();
    let hint = eval
        .filter(|eval| eval.face_found && !eval.quality.passed)
        .map(|eval| protocol::quality_reason(&eval.quality.reject_cause));
    emit(&Event::Enroll {
        step: u8::try_from(sess.current_step.index()).unwrap_or(u8::MAX),
        hold: u8::try_from(sess.hold_frames.max(0)).unwrap_or(u8::MAX),
        required: u8::try_from(sess.required_hold.max(0)).unwrap_or(u8::MAX),
        face: eval.is_some_and(|eval| eval.face_found),
        hint,
    });
    stream_preview(image, last_preview);
}

fn stream_preview(image: &RgbaImage, last_preview: &mut Option<Instant>) {
    if last_preview.is_none_or(|at| at.elapsed() >= PREVIEW_EVERY)
        && let Some(frame) = protocol::preview(image, PREVIEW_SIDE)
    {
        emit(&frame);
        *last_preview = Some(Instant::now());
    }
}
