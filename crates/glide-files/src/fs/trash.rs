use std::ffi::{OsStr, OsString};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use super::entry::LocalTime;
use super::operations::{move_path, occupied, remove_recursive, unique_path};
use super::places::data_home;
use super::uri::{decode_path, encode_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trash {
    root: PathBuf,
}

impl Trash {
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn user() -> Self {
        let data = data_home().unwrap_or_else(std::env::temp_dir);
        Self::new(data.join("Trash"))
    }

    #[must_use]
    pub fn files(&self) -> PathBuf {
        self.root.join("files")
    }

    fn info_dir(&self) -> PathBuf {
        self.root.join("info")
    }

    fn info_file(&self, name: &OsStr) -> PathBuf {
        let mut file = OsString::from(name);
        file.push(".trashinfo");
        self.info_dir().join(file)
    }

    #[must_use]
    pub fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.root) || path.ancestors().any(is_volume_root)
    }

    #[must_use]
    pub(crate) fn is_files_dir(&self, path: &Path) -> bool {
        path == self.files()
            || (path.file_name().is_some_and(|name| name == "files") && path.parent().is_some_and(is_volume_root))
    }

    #[must_use]
    pub fn is_top_level(&self, path: &Path) -> bool {
        path.parent() == Some(self.files().as_path()) || Self::owner(path).is_some_and(|trash| trash.top().is_some())
    }

    #[must_use]
    pub fn on_volumes(&self, mounts: &[PathBuf]) -> Vec<Self> {
        let uid = uid();
        let mut found: Vec<Self> = Vec::new();
        for top in mounts {
            let shared = top.join(".Trash");
            let roots = [(top.join(format!(".Trash-{uid}")), true), (shared.join(uid.to_string()), sticky(&shared))];
            for (root, trusted) in roots {
                let owned = std::fs::symlink_metadata(&root).is_ok_and(|meta| meta.is_dir() && meta.uid() == uid);
                let trash = Self::new(root);
                if trusted && owned && trash != *self && confined(top, &trash.files()) && !found.contains(&trash) {
                    found.push(trash);
                }
            }
        }
        found
    }

    #[must_use]
    pub fn count(&self) -> usize {
        std::fs::read_dir(self.files()).map_or(0, |entries| entries.flatten().count())
    }

    fn for_target(&self, parent: &Path) -> io::Result<(Self, Option<PathBuf>)> {
        let home = self
            .root
            .ancestors()
            .find_map(device)
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no trash volume"))?;
        let target = device(parent).ok_or_else(|| io::Error::from(ErrorKind::NotFound))?;
        if home == target {
            return Ok((self.clone(), None));
        }
        let top = mount_top(parent, target);
        let uid = uid();
        let shared = top.join(".Trash");
        let candidate = shared.join(uid.to_string());
        let root = if sticky(&shared) && own_dir(&candidate, uid).is_ok() {
            candidate
        } else {
            let own = top.join(format!(".Trash-{uid}"));
            own_dir(&own, uid)?;
            own
        };
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        Ok((Self::new(root), Some(top)))
    }

    pub fn put(&self, target: &Path) -> io::Result<PathBuf> {
        let original = absolute_unresolved(target)?;
        let name = original.file_name().ok_or_else(|| io::Error::new(ErrorKind::InvalidFilename, "no file name"))?;
        let parent = original.parent().unwrap_or(Path::new("/"));
        let (trash, top) = self.for_target(parent)?;
        if top.is_some() {
            plain_or_new_dir(&trash.files())?;
            plain_or_new_dir(&trash.info_dir())?;
        } else {
            std::fs::create_dir_all(trash.files())?;
            std::fs::create_dir_all(trash.info_dir())?;
        }
        let deleted = LocalTime::of(SystemTime::now()).map(LocalTime::iso);
        let recorded = top
            .as_deref()
            .and_then(|top| original.strip_prefix(top).ok())
            .map_or_else(|| encode_path(&original), encode_path);
        let mut attempts = 0;
        let (dest, info, mut file) = loop {
            let dest = unique_path(&trash.files(), name, true, |candidate| {
                occupied(candidate) || candidate.file_name().is_some_and(|slot| occupied(&trash.info_file(slot)))
            });
            let info = trash.info_file(dest.file_name().unwrap_or(name));
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&info) {
                Ok(file) => break (dest, info, file),
                Err(err) if err.kind() == ErrorKind::AlreadyExists && attempts < 16 => attempts += 1,
                Err(err) => return Err(err),
            }
        };
        if let Err(err) = write!(
            file,
            "[Trash Info]\nPath={recorded}\nDeletionDate={}\n",
            deleted.unwrap_or_else(|| String::from("1970-01-01T00:00:00"))
        ) {
            let _ = std::fs::remove_file(&info);
            return Err(err);
        }
        if let Err(err) = move_path(target, &dest) {
            let _ = std::fs::remove_file(&info);
            return Err(err);
        }
        Ok(dest)
    }

    fn owner(trashed: &Path) -> Option<Self> {
        let files = trashed.parent()?;
        let root = files.parent()?;
        (files.file_name()? == "files").then(|| Self::new(root.to_path_buf()))
    }

    fn top(&self) -> Option<&Path> {
        let name = self.root.file_name()?.to_string_lossy();
        let uid = uid().to_string();
        if name.strip_prefix(".Trash-") == Some(uid.as_str()) {
            return self.root.parent();
        }
        let shared = self.root.parent()?;
        (name == uid.as_str() && shared.file_name()? == ".Trash").then(|| shared.parent()).flatten()
    }

    #[must_use]
    pub fn original_path(&self, trashed: &Path) -> Option<PathBuf> {
        if !self.is_top_level(trashed) {
            return None;
        }
        let trash = Self::owner(trashed).unwrap_or_else(|| self.clone());
        let info = std::fs::read_to_string(trash.info_file(trashed.file_name()?)).ok()?;
        let recorded = info.lines().find_map(|line| line.strip_prefix("Path=")).map(|path| decode_path(path.trim()))?;
        if recorded.components().any(|part| part == Component::ParentDir) {
            return None;
        }
        match trash.top() {
            Some(top) if recorded.components().all(|part| matches!(part, Component::Normal(_))) => {
                Some(top.join(recorded))
            }
            Some(top) if recorded.is_absolute() && recorded.starts_with(top) => Some(recorded),
            None if recorded.is_absolute() => Some(recorded),
            Some(_) | None => None,
        }
    }

    pub fn restore(&self, trashed: &Path) -> io::Result<PathBuf> {
        let original =
            self.original_path(trashed).ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no trash info"))?;
        let (Some(parent), Some(name)) = (original.parent(), original.file_name()) else {
            return Err(io::Error::new(ErrorKind::InvalidData, "corrupt trash info"));
        };
        if let Some(top) = Self::owner(trashed).unwrap_or_else(|| self.clone()).top()
            && !confined_so_far(top, parent)
        {
            return Err(io::Error::new(ErrorKind::InvalidData, "the original folder leads off the volume"));
        }
        std::fs::create_dir_all(parent)?;
        let is_dir = std::fs::symlink_metadata(trashed)?.is_dir();
        let dest = unique_path(parent, name, !is_dir, occupied);
        move_path(trashed, &dest)?;
        let trash = Self::owner(trashed).unwrap_or_else(|| self.clone());
        if let Some(slot) = trashed.file_name() {
            let _ = std::fs::remove_file(trash.info_file(slot));
        }
        Ok(dest)
    }

    pub fn forget(&self, trashed: &Path) {
        let trash =
            Self::owner(trashed).filter(|owner| owner == self || owner.top().is_some()).unwrap_or_else(|| self.clone());
        if trash.is_top_level(trashed)
            && let Some(slot) = trashed.file_name()
        {
            let _ = std::fs::remove_file(trash.info_file(slot));
        }
    }

    pub fn empty(&self) -> io::Result<()> {
        let mut failure = None;
        for dir in [self.files(), self.info_dir()] {
            if let Some(top) = self.top()
                && dir.exists()
                && !confined(top, &dir)
            {
                return Err(io::Error::new(ErrorKind::InvalidData, "the trash folder is a link"));
            }
        }
        let listed = |dir: PathBuf, failure: &mut Option<io::Error>| -> Vec<PathBuf> {
            match std::fs::read_dir(dir) {
                Ok(entries) => entries.flatten().map(|entry| entry.path()).collect(),
                Err(err) if err.kind() == ErrorKind::NotFound => Vec::new(),
                Err(err) => {
                    failure.get_or_insert(err);
                    Vec::new()
                }
            }
        };
        for item in listed(self.files(), &mut failure) {
            if let Err(err) = remove_recursive(&item) {
                failure.get_or_insert(err);
            }
        }
        for info in listed(self.info_dir(), &mut failure) {
            let kept = info.file_stem().is_some_and(|slot| occupied(&self.files().join(slot)));
            if !kept && let Err(err) = remove_recursive(&info) {
                failure.get_or_insert(err);
            }
        }
        failure.map_or(Ok(()), Err)
    }
}

fn sticky(shared: &Path) -> bool {
    std::fs::symlink_metadata(shared).is_ok_and(|meta| meta.is_dir() && meta.permissions().mode() & 0o1000 != 0)
}

fn plain_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

fn confined(top: &Path, path: &Path) -> bool {
    let Ok(rest) = path.strip_prefix(top) else { return false };
    let mut at = top.to_path_buf();
    rest.components().all(|part| {
        at.push(part);
        plain_dir(&at)
    })
}

fn confined_so_far(top: &Path, path: &Path) -> bool {
    let Ok(rest) = path.strip_prefix(top) else { return false };
    let mut at = top.to_path_buf();
    for part in rest.components() {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return false,
            Err(_) => return true,
        }
    }
    true
}

fn plain_or_new_dir(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(io::Error::new(ErrorKind::InvalidData, "the trash folder is not a plain folder")),
        Err(err) if err.kind() == ErrorKind::NotFound => std::fs::DirBuilder::new().mode(0o700).create(path),
        Err(err) => Err(err),
    }
}

fn own_dir(path: &Path, uid: u32) -> io::Result<()> {
    plain_or_new_dir(path)?;
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() && meta.uid() == uid {
        Ok(())
    } else {
        Err(io::Error::new(ErrorKind::PermissionDenied, "the trash folder belongs to someone else"))
    }
}

fn uid() -> u32 {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

fn is_volume_root(dir: &Path) -> bool {
    let uid = uid().to_string();
    let Some(name) = dir.file_name().map(|name| name.to_string_lossy()) else {
        return false;
    };
    name.strip_prefix(".Trash-") == Some(uid.as_str())
        || (name == uid.as_str() && dir.parent().and_then(Path::file_name).is_some_and(|parent| parent == ".Trash"))
}

fn device(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|meta| meta.dev())
}

fn mount_top(path: &Path, dev: u64) -> PathBuf {
    let mut top = path.to_path_buf();
    for ancestor in path.ancestors().skip(1) {
        if device(ancestor) != Some(dev) {
            break;
        }
        top = ancestor.to_path_buf();
    }
    top
}

fn absolute_unresolved(target: &Path) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(target)?;
    Ok(match (absolute.parent(), absolute.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf()).join(name),
        _ => absolute,
    })
}
