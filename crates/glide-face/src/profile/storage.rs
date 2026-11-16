use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::{DateTime, Local};
use nix::unistd::{Uid, User, geteuid, getuid};
use serde::{Deserialize, Serialize};

use super::ProfileError;
use super::crypto::{
    MASTER_KEY_SIZE, build_aad, decrypt, encrypt, generate_master_key, host_bound_secret, template_hash,
};

pub const SYSTEM_STORE_ROOT: &str = "/var/lib/glide-face";
pub const PROFILE_VERSION: i32 = 3;
pub const DIR_MODE: u32 = 0o750;
pub const ROOT_DIR_MODE: u32 = 0o755;
pub const FILE_MODE: u32 = 0o640;
const ATTEMPTS_KEPT: usize = 200;
const ATTEMPTS_TRIM_BYTES: u64 = 64 * 1024;

static STORAGE_LOCK: RwLock<()> = RwLock::new(());

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub version: i32,
    pub user_id: u32,
    pub username: String,
    pub model_name: String,
    #[serde(default, with = "crate::biometrics::go_time::option")]
    pub created_at: Option<DateTime<Local>>,
    #[serde(default, with = "crate::biometrics::go_time::option")]
    pub updated_at: Option<DateTime<Local>>,
    pub embedding: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub multi_embeddings: Vec<Vec<f32>>,
    pub num_samples: i32,
    pub template_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_weights: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_check_score: Option<f64>,
}

impl Profile {
    #[must_use]
    pub fn is_compatible(&self) -> bool {
        self.model_name == crate::biometrics::RECOGNIZER_ID
            && self.embedding.len() == crate::biometrics::EMBEDDING_DIM
            && self.multi_embeddings.iter().all(|m| m.len() == crate::biometrics::EMBEDDING_DIM)
    }
}

pub const STALE_PROFILE_HINT: &str = "profile was enrolled with a previous recognition model; enroll the face again";

#[derive(Debug, Clone)]
pub struct Store {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub dir: PathBuf,
    test_mode: bool,
}

fn current_user() -> Result<User, ProfileError> {
    User::from_uid(getuid())
        .ok()
        .flatten()
        .ok_or_else(|| ProfileError::Other("failed to determine current user".into()))
}

fn pkexec_user() -> Result<Option<String>, ProfileError> {
    let Some(uid) = std::env::var("PKEXEC_UID").ok().filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let uid = uid.parse::<u32>().map_err(|_| ProfileError::Other(format!("invalid PKEXEC_UID {uid:?}")))?;
    User::from_uid(Uid::from_raw(uid))
        .ok()
        .flatten()
        .map(|user| Some(user.name))
        .ok_or_else(|| ProfileError::Other(format!("unknown invoking uid {uid}")))
}

pub fn resolve_username(explicit: &str) -> Result<String, ProfileError> {
    let cur = current_user()?;
    if !geteuid().is_root() {
        if !explicit.is_empty() && explicit != cur.name {
            return Err(ProfileError::Other(format!("only root may access the profile of user {explicit:?}")));
        }
        return Ok(cur.name);
    }
    if let Some(invoker) = pkexec_user()? {
        if !explicit.is_empty() && explicit != invoker {
            return Err(ProfileError::Other(format!("pkexec may only change the profile of {invoker:?}")));
        }
        return Ok(invoker);
    }
    if !explicit.is_empty() {
        return Ok(explicit.to_string());
    }
    for env in ["PAM_USER", "SUDO_USER"] {
        if let Ok(v) = std::env::var(env)
            && !v.is_empty()
        {
            return Ok(v);
        }
    }
    Err(ProfileError::Other("running as root: specify the target account with -user".into()))
}

impl Store {
    pub fn open(username: &str) -> Result<Self, ProfileError> {
        if username.is_empty() || username == "." || username == ".." || username.contains(['/', '\0']) {
            return Err(ProfileError::Other(format!("invalid username {username:?}")));
        }
        let user = User::from_name(username)
            .map_err(|e| ProfileError::Other(format!("unknown user {username:?}: {e}")))?
            .ok_or_else(|| ProfileError::Other(format!("unknown user {username:?}: user: unknown user {username}")))?;
        let dir = Path::new(SYSTEM_STORE_ROOT).join(&user.name);
        Ok(Self::from_user(&user, dir, false))
    }

    pub fn new_test(base_dir: impl AsRef<Path>) -> Result<Self, ProfileError> {
        let user = current_user()?;
        let dir = base_dir.as_ref().join(&user.name);
        Ok(Self::from_user(&user, dir, true))
    }

    fn from_user(user: &User, dir: PathBuf, test_mode: bool) -> Self {
        Self { username: user.name.clone(), uid: user.uid.as_raw(), gid: user.gid.as_raw(), dir, test_mode }
    }

    #[must_use]
    pub fn profile_path(&self) -> PathBuf {
        self.dir.join("profile.dat")
    }

    #[must_use]
    pub fn key_path(&self) -> PathBuf {
        self.dir.join(".master.key")
    }

    #[must_use]
    pub fn attempts_path(&self) -> PathBuf {
        self.dir.join("attempts.jsonl")
    }

    #[must_use]
    pub fn camera(&self) -> Option<String> {
        let camera = fs::read_to_string(self.dir.join("camera")).ok()?.trim().to_string();
        camera.strip_prefix("/dev/video")?.parse::<u32>().ok().map(|_| camera)
    }

    pub fn save_camera(&self, device: &str) -> std::io::Result<()> {
        glide_config::write_atomic(&self.dir.join("camera"), device.as_bytes(), FILE_MODE)
    }

    #[must_use]
    pub fn can_write(&self) -> bool {
        self.test_mode || geteuid().is_root()
    }

    #[must_use]
    pub fn exists(&self) -> bool {
        let _g = STORAGE_LOCK.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        fs::symlink_metadata(self.profile_path()).is_ok_and(|m| m.file_type().is_file() && m.len() > 0)
    }

    pub fn load(&self) -> Result<Profile, ProfileError> {
        let _g = STORAGE_LOCK.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        let parent = self.dir.parent().unwrap_or(Path::new("/"));
        for dir in [parent, self.dir.as_path()] {
            self.check_secure(dir, true)?;
        }
        let encrypted =
            self.read_secure_file(&self.profile_path()).map_err(|e| e.context("failed to read profile file"))?;
        let raw_key = self.read_secure_file(&self.key_path()).map_err(|e| e.context("failed to read master key"))?;
        let secret = host_bound_secret(&raw_key)?;
        let plaintext = decrypt(&secret, &encrypted, &build_aad(self.uid, &self.username))
            .map_err(|e| e.context("failed to decrypt profile"))?;
        let p: Profile = serde_json::from_slice(&plaintext)
            .map_err(|e| ProfileError::Corrupt(format!("failed to parse profile JSON: {e}")))?;
        if p.user_id != self.uid || p.username != self.username {
            return Err(ProfileError::Insecure(format!(
                "profile belongs to uid={} user={:?}, expected uid={} user={:?}",
                p.user_id, p.username, self.uid, self.username
            )));
        }
        Ok(p)
    }

    pub fn save(&self, p: &mut Profile) -> Result<(), ProfileError> {
        if !self.can_write() {
            return Err(ProfileError::NotRoot);
        }
        let _g = STORAGE_LOCK.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let parent = self.dir.parent().unwrap_or(Path::new("/")).to_path_buf();
        self.ensure_dir(&parent, ROOT_DIR_MODE, 0)?;
        self.ensure_dir(&self.dir, DIR_MODE, self.gid)?;

        let raw_key = self.load_or_create_key()?;
        let secret = host_bound_secret(&raw_key)?;

        let now = Local::now();
        p.updated_at = Some(now);
        if p.created_at.is_none() {
            p.created_at = Some(now);
        }
        p.version = PROFILE_VERSION;
        p.user_id = self.uid;
        p.username.clone_from(&self.username);
        if p.model_name.is_empty() {
            p.model_name = crate::biometrics::RECOGNIZER_ID.into();
        }
        p.template_hash = template_hash(&secret, &p.embedding);

        let plaintext =
            serde_json::to_vec(p).map_err(|e| ProfileError::Other(format!("failed to serialize profile: {e}")))?;
        let encrypted = encrypt(&secret, &plaintext, &build_aad(self.uid, &self.username))
            .map_err(|e| e.context("failed to encrypt profile"))?;
        self.write_file_atomic(&self.profile_path(), &encrypted).map_err(|e| e.context("failed to write profile"))
    }

    pub fn delete(&self) -> Result<(), ProfileError> {
        if !self.can_write() {
            return Err(ProfileError::NotRoot);
        }
        let _g = STORAGE_LOCK.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        match self.check_secure(&self.dir, true) {
            Err(e) if e.is_not_found() => return Ok(()),
            Err(e) => return Err(e),
            Ok(()) => {}
        }
        for path in [self.profile_path(), self.key_path(), self.attempts_path()] {
            if let Err(e) = fs::remove_file(&path)
                && e.kind() != ErrorKind::NotFound
            {
                return Err(ProfileError::Io(format!("failed to delete {}: {e}", path.display()), e.kind()));
            }
        }
        let _ = fs::remove_dir(&self.dir);
        Ok(())
    }

    pub fn append_attempt(&self, line: &str) -> Result<(), ProfileError> {
        if !self.can_write() {
            return Err(ProfileError::NotRoot);
        }
        let _g = STORAGE_LOCK.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        self.check_secure(&self.dir, true)?;
        let path = self.attempts_path();
        let previous = match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_file() && meta.len() > ATTEMPTS_TRIM_BYTES => {
                Some(self.read_secure_file(&path)?)
            }
            Ok(meta) if !meta.file_type().is_file() => {
                return Err(ProfileError::Insecure(format!("{} is not a regular file", path.display())));
            }
            _ => None,
        };
        if let Some(previous) = previous {
            let text = String::from_utf8_lossy(&previous);
            let lines: Vec<&str> = text.lines().collect();
            let kept = lines[lines.len().saturating_sub(ATTEMPTS_KEPT)..].join("\n");
            return self.write_file_atomic(&path, format!("{kept}\n{line}\n").as_bytes());
        }
        let io = |e: std::io::Error| ProfileError::Io(e.to_string(), e.kind());
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(FILE_MODE)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(io)?;
        if !self.test_mode {
            std::os::unix::fs::fchown(&file, Some(0), Some(self.gid)).map_err(io)?;
        }
        file.write_all(format!("{line}\n").as_bytes()).map_err(io)
    }

    #[must_use]
    pub fn read_attempts(&self) -> String {
        self.read_secure_file(&self.attempts_path())
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default()
    }

    fn check_secure(&self, path: &Path, want_dir: bool) -> Result<(), ProfileError> {
        let meta = fs::symlink_metadata(path).map_err(|e| ProfileError::from_io(path, &e))?;
        let ft = meta.file_type();
        let shown = path.display();
        if ft.is_symlink() {
            return Err(ProfileError::Insecure(format!("{shown} is a symlink")));
        }
        if want_dir && !ft.is_dir() {
            return Err(ProfileError::Insecure(format!("{shown} is not a directory")));
        }
        if !want_dir && !ft.is_file() {
            return Err(ProfileError::Insecure(format!("{shown} is not a regular file")));
        }
        let perm = meta.permissions().mode() & 0o777;
        if perm & 0o022 != 0 {
            return Err(ProfileError::Insecure(format!("{shown} is writable by group or others ({perm:04o})")));
        }
        if !want_dir && perm & 0o007 != 0 {
            return Err(ProfileError::Insecure(format!("{shown} is accessible by others ({perm:04o})")));
        }
        if !self.test_mode && meta.uid() != 0 {
            return Err(ProfileError::Insecure(format!("{shown} is not owned by root")));
        }
        Ok(())
    }

    fn read_secure_file(&self, path: &Path) -> Result<Vec<u8>, ProfileError> {
        self.check_secure(path, false)?;
        fs::read(path).map_err(|e| ProfileError::from_io(path, &e))
    }

    fn load_or_create_key(&self) -> Result<Vec<u8>, ProfileError> {
        match self.read_secure_file(&self.key_path()) {
            Ok(key) if key.len() < MASTER_KEY_SIZE => {
                Err(ProfileError::Corrupt("master key is truncated or corrupted".into()))
            }
            Ok(key) => Ok(key),
            Err(e) if e.is_not_found() => {
                let key = generate_master_key();
                self.write_file_atomic(&self.key_path(), &key).map_err(|e| e.context("failed to write master key"))?;
                Ok(key)
            }
            Err(e) => Err(e.context("failed to read master key")),
        }
    }

    fn ensure_dir(&self, path: &Path, mode: u32, gid: u32) -> Result<(), ProfileError> {
        if let Err(e) = fs::create_dir(path)
            && e.kind() != ErrorKind::AlreadyExists
        {
            return Err(ProfileError::Io(format!("failed to create {}: {e}", path.display()), e.kind()));
        }
        let meta = fs::symlink_metadata(path).map_err(|e| ProfileError::from_io(path, &e))?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(ProfileError::Insecure(format!("{} is not a plain directory", path.display())));
        }
        if !self.test_mode {
            std::os::unix::fs::lchown(path, Some(0), Some(gid))
                .map_err(|e| ProfileError::Io(format!("failed to set owner of {}: {e}", path.display()), e.kind()))?;
        }
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| ProfileError::Io(format!("failed to set mode of {}: {e}", path.display()), e.kind()))
    }

    fn write_file_atomic(&self, path: &Path, data: &[u8]) -> Result<(), ProfileError> {
        let tmp = PathBuf::from(format!("{}.tmp", path.display()));
        let _ = fs::remove_file(&tmp);
        let io = |e: std::io::Error| ProfileError::Io(e.to_string(), e.kind());
        let mut f: File = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)
            .map_err(io)?;
        let result = (|| -> std::io::Result<()> {
            f.write_all(data)?;
            f.sync_all()?;
            if !self.test_mode {
                std::os::unix::fs::fchown(&f, Some(0), Some(self.gid))?;
            }
            f.set_permissions(fs::Permissions::from_mode(FILE_MODE))?;
            Ok(())
        })();
        drop(f);
        if let Err(e) = result.and_then(|()| fs::rename(&tmp, path)) {
            let _ = fs::remove_file(&tmp);
            return Err(io(e));
        }
        Ok(())
    }
}
