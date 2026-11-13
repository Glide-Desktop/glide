use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

const LIMIT: usize = 1 << 16;
const KEPT: [&[u8]; 2] = [b"user.", b"system.posix_acl_"];

fn c_path(path: &Path) -> Option<CString> {
    CString::new(path.as_os_str().as_bytes()).ok()
}

fn names(path: &CStr) -> Vec<CString> {
    let mut buffer = vec![0_u8; 1024];
    loop {
        // SAFETY: `path` is NUL-terminated and `buffer` is writable for `buffer.len()` bytes.
        let read = unsafe { libc::llistxattr(path.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if read >= 0 {
            buffer.truncate(read.unsigned_abs());
            return buffer
                .split(|byte| *byte == 0)
                .filter(|name| KEPT.iter().any(|prefix| name.starts_with(prefix)))
                .filter_map(|name| CString::new(name).ok())
                .collect();
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::ERANGE) || buffer.len() >= LIMIT {
            return Vec::new();
        }
        buffer.resize(buffer.len() * 4, 0);
    }
}

fn value(path: &CStr, name: &CStr) -> Option<Vec<u8>> {
    let mut buffer = vec![0_u8; 1024];
    loop {
        // SAFETY: both strings are NUL-terminated and `buffer` is writable for `buffer.len()` bytes.
        let read = unsafe { libc::lgetxattr(path.as_ptr(), name.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if read >= 0 {
            buffer.truncate(read.unsigned_abs());
            return Some(buffer);
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::ERANGE) || buffer.len() >= LIMIT {
            return None;
        }
        buffer.resize(buffer.len() * 4, 0);
    }
}

pub(crate) fn copy(source: &Path, dest: &Path) {
    let (Some(from), Some(to)) = (c_path(source), c_path(dest)) else {
        return;
    };
    for name in names(&from) {
        if let Some(bytes) = value(&from, &name) {
            // SAFETY: the strings are NUL-terminated and `bytes` is readable for its full length.
            unsafe {
                libc::lsetxattr(to.as_ptr(), name.as_ptr(), bytes.as_ptr().cast(), bytes.len(), 0);
            }
        }
    }
}
