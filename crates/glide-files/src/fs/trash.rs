use std::ffi::{OsStr, OsString};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
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
        path.starts_with(&self.root)
    }

    #[must_use]
    pub fn is_top_level(&self, path: &Path) -> bool {
        path.parent() == Some(self.files().as_path()) || Self::owner(path).is_some_and(|trash| trash.top().is_some())
    }

    #[must_use]
    pub fn on_volumes(&self, mounts: &[PathBuf]) -> Vec<Self> {
        // SAFETY: `getuid` has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let mut found: Vec<Self> = Vec::new();
        for top in mounts {
            for root in [top.join(format!(".Trash-{uid}")), top.join(".Trash").join(uid.to_string())] {
                let trash = Self::new(root);
                if trash != *self && trash.files().is_dir() && !found.contains(&trash) {
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
        // SAFETY: `getuid` has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let shared = top.join(".Trash");
        let root = match std::fs::symlink_metadata(&shared) {
            Ok(meta) if meta.is_dir() && meta.permissions().mode() & 0o1000 != 0 => shared.join(uid.to_string()),
            _ => top.join(format!(".Trash-{uid}")),
        };
        std::fs::create_dir_all(&root)?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        Ok((Self::new(root), Some(top)))
    }

    pub fn put(&self, target: &Path) -> io::Result<PathBuf> {
        let original = absolute_unresolved(target)?;
        let name = original.file_name().ok_or_else(|| io::Error::new(ErrorKind::InvalidFilename, "no file name"))?;
        let parent = original.parent().unwrap_or(Path::new("/"));
        let (trash, top) = self.for_target(parent)?;
        std::fs::create_dir_all(trash.files())?;
        std::fs::create_dir_all(trash.info_dir())?;
        let dest = unique_path(&trash.files(), name, true, |candidate| {
            occupied(candidate) || candidate.file_name().is_some_and(|slot| occupied(&trash.info_file(slot)))
        });
        let info = trash.info_file(dest.file_name().unwrap_or(name));
        let deleted = LocalTime::of(SystemTime::now()).map(LocalTime::iso);
        let recorded = top
            .as_deref()
            .and_then(|top| original.strip_prefix(top).ok())
            .map_or_else(|| encode_path(&original), encode_path);
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&info)?;
        write!(file, "[Trash Info]\nPath={recorded}\nDeletionDate={}\n", deleted.unwrap_or_default())?;
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
        if name.starts_with(".Trash-") {
            return self.root.parent();
        }
        let shared = self.root.parent()?;
        (shared.file_name()? == ".Trash").then(|| shared.parent()).flatten()
    }

    #[must_use]
    pub fn original_path(&self, trashed: &Path) -> Option<PathBuf> {
        let trash = Self::owner(trashed).unwrap_or_else(|| self.clone());
        let info = std::fs::read_to_string(trash.info_file(trashed.file_name()?)).ok()?;
        let recorded = info.lines().find_map(|line| line.strip_prefix("Path=")).map(|path| decode_path(path.trim()))?;
        Some(match trash.top() {
            Some(top) if recorded.is_relative() => top.join(recorded),
            _ => recorded,
        })
    }

    pub fn restore(&self, trashed: &Path) -> io::Result<PathBuf> {
        let original =
            self.original_path(trashed).ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no trash info"))?;
        let (Some(parent), Some(name)) = (original.parent(), original.file_name()) else {
            return Err(io::Error::new(ErrorKind::InvalidData, "corrupt trash info"));
        };
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
        for dir in [self.files(), self.info_dir()] {
            match std::fs::read_dir(&dir) {
                Ok(entries) => {
                    for entry in entries {
                        remove_recursive(&entry?.path())?;
                    }
                }
                Err(err) if err.kind() == ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }
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
