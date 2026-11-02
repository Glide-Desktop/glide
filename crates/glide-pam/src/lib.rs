#![allow(clippy::missing_safety_doc)] // The entry points are called by libpam with a live handle, the contract of pam_sm_authenticate(3) and its siblings

use std::ffi::{CStr, c_char, c_int, c_void};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use glide_login_key::{KEY_FILE, REKEY_FILE, WrappingKey};
use zeroize::Zeroizing;

#[repr(C)]
pub struct PamHandle {
    _private: [u8; 0],
}

const PAM_IGNORE: c_int = 25;
const PAM_AUTHTOK: c_int = 6;
const PAM_OLDAUTHTOK: c_int = 7;
const PAM_PRELIM_CHECK: c_int = 0x4000;
const DATA: &CStr = c"glide-keychain-wrapping-key";

type Cleanup = unsafe extern "C" fn(*mut PamHandle, *mut c_void, c_int);

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_get_item(pamh: *const PamHandle, item_type: c_int, item: *mut *const c_void) -> c_int;
    fn pam_get_user(pamh: *mut PamHandle, user: *mut *const c_char, prompt: *const c_char) -> c_int;
    fn pam_set_data(pamh: *mut PamHandle, name: *const c_char, data: *mut c_void, cleanup: Option<Cleanup>) -> c_int;
    fn pam_get_data(pamh: *const PamHandle, name: *const c_char, data: *mut *const c_void) -> c_int;
    fn pam_getenv(pamh: *mut PamHandle, name: *const c_char) -> *const c_char;
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Account {
    name: String,
    uid: u32,
    gid: u32,
}

fn account_named(name: &str) -> Option<Account> {
    let wanted = std::ffi::CString::new(name).ok()?;
    // SAFETY: passwd is a C struct of integers and pointers, for which all zero bytes are a valid value.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buffer = vec![0 as c_char; 16 * 1024];
    let mut found = std::ptr::null_mut();
    // SAFETY: wanted, entry, buffer and found outlive the call, and buffer.len() is the length handed over.
    let status =
        unsafe { libc::getpwnam_r(wanted.as_ptr(), &raw mut entry, buffer.as_mut_ptr(), buffer.len(), &raw mut found) };
    (status == 0 && !found.is_null()).then(|| Account { name: name.to_string(), uid: entry.pw_uid, gid: entry.pw_gid })
}

fn user(pamh: *mut PamHandle) -> Option<Account> {
    let mut name: *const c_char = std::ptr::null();
    // SAFETY: pamh is the live handle libpam passed to the entry point, and name outlives the call.
    if unsafe { pam_get_user(pamh, &raw mut name, std::ptr::null()) } != 0 || name.is_null() {
        return None;
    }
    // SAFETY: on success libpam points name at a NUL-terminated string it owns for the life of the handle.
    let name = unsafe { CStr::from_ptr(name) }.to_str().ok()?;
    account_named(name)
}

fn item(pamh: *mut PamHandle, kind: c_int) -> Option<Zeroizing<Vec<u8>>> {
    let mut value: *const c_void = std::ptr::null();
    // SAFETY: pamh is the live handle libpam passed to the entry point, and value outlives the call.
    if unsafe { pam_get_item(pamh, kind, &raw mut value) } != 0 || value.is_null() {
        return None;
    }
    // SAFETY: PAM_AUTHTOK and PAM_OLDAUTHTOK items are NUL-terminated strings owned by libpam.
    let text = unsafe { CStr::from_ptr(value.cast::<c_char>()) };
    Some(Zeroizing::new(text.to_bytes().to_vec())).filter(|bytes| !bytes.is_empty())
}

fn wrapping(pamh: *mut PamHandle, kind: c_int, account: &Account) -> Option<WrappingKey> {
    let password = item(pamh, kind)?;
    glide_login_key::wrapping_key(&password, &account.name, account.uid)
}

unsafe extern "C" fn forget(_: *mut PamHandle, data: *mut c_void, _: c_int) {
    if !data.is_null() {
        // SAFETY: data is the Box handed to pam_set_data in remember, and libpam calls this cleanup exactly once.
        drop(unsafe { Box::from_raw(data.cast::<WrappingKey>()) });
    }
}

fn remember(pamh: *mut PamHandle, key: WrappingKey) {
    let data = Box::into_raw(Box::new(key)).cast::<c_void>();
    // SAFETY: pamh is the live handle libpam passed to the entry point; libpam owns data from here on and frees it through forget.
    if unsafe { pam_set_data(pamh, DATA.as_ptr(), data, Some(forget)) } != 0 {
        // SAFETY: libpam refused the data, so it is still ours to free.
        unsafe { forget(pamh, data, 0) };
    }
}

fn remembered(pamh: *mut PamHandle) -> Option<WrappingKey> {
    let mut data: *const c_void = std::ptr::null();
    // SAFETY: pamh is the live handle libpam passed to the entry point, and data outlives the call.
    if unsafe { pam_get_data(pamh, DATA.as_ptr(), &raw mut data) } != 0 || data.is_null() {
        return None;
    }
    // SAFETY: data is the WrappingKey stored by remember, alive until libpam runs forget.
    Some(unsafe { &*data.cast::<WrappingKey>() }.clone())
}

fn runtime_dir(pamh: *mut PamHandle, account: &Account) -> PathBuf {
    // SAFETY: pamh is the live handle libpam passed to the entry point, and the name is NUL-terminated.
    let value = unsafe { pam_getenv(pamh, c"XDG_RUNTIME_DIR".as_ptr()) };
    if !value.is_null() {
        // SAFETY: pam_getenv returns a NUL-terminated string owned by libpam.
        let dir = unsafe { CStr::from_ptr(value) };
        if let Ok(dir) = dir.to_str()
            && !dir.is_empty()
        {
            return PathBuf::from(dir);
        }
    }
    PathBuf::from(format!("/run/user/{}", account.uid))
}

struct Identity {
    uid: u32,
    gid: u32,
}

impl Identity {
    fn assume(account: &Account) -> Option<Self> {
        // SAFETY: setfsgid and setfsuid only change the file-system identity of this thread; u32::MAX asks without changing.
        let (gid, uid) = unsafe {
            let gid = libc::setfsgid(account.gid) as u32;
            let uid = libc::setfsuid(account.uid) as u32;
            (gid, uid)
        };
        let saved = Self { uid, gid };
        // SAFETY: as above; an invalid id changes nothing and returns the current one.
        let current = unsafe { (libc::setfsuid(u32::MAX) as u32, libc::setfsgid(u32::MAX) as u32) };
        (current == (account.uid, account.gid)).then_some(saved)
    }
}

impl Drop for Identity {
    fn drop(&mut self) {
        // SAFETY: this puts back the file-system identity assume replaced, on the same thread.
        unsafe {
            libc::setfsuid(self.uid);
            libc::setfsgid(self.gid);
        }
    }
}

fn write_as(account: &Account, dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let refused = || std::io::Error::new(std::io::ErrorKind::PermissionDenied, "not the account's runtime folder");
    let _identity = Identity::assume(account).ok_or_else(refused)?;
    let folder = std::fs::symlink_metadata(dir)?;
    if !folder.is_dir() || folder.uid() != account.uid {
        return Err(refused());
    }
    let path = dir.join(name);
    let temp = dir.join(format!(".{name}.{}", std::process::id()));
    match std::fs::remove_file(&temp) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err),
        _ => {}
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&temp)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all()).and_then(|()| std::fs::rename(&temp, &path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pam_sm_authenticate(
    pamh: *mut PamHandle, _flags: c_int, _argc: c_int, _argv: *const *const c_char,
) -> c_int {
    if let Some(account) = user(pamh)
        && let Some(key) = wrapping(pamh, PAM_AUTHTOK, &account)
    {
        remember(pamh, key);
    }
    PAM_IGNORE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pam_sm_setcred(
    _pamh: *mut PamHandle, _flags: c_int, _argc: c_int, _argv: *const *const c_char,
) -> c_int {
    PAM_IGNORE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pam_sm_open_session(
    pamh: *mut PamHandle, _flags: c_int, _argc: c_int, _argv: *const *const c_char,
) -> c_int {
    if let (Some(account), Some(key)) = (user(pamh), remembered(pamh)) {
        let _ = write_as(&account, &runtime_dir(pamh, &account), KEY_FILE, key.as_ref());
    }
    PAM_IGNORE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pam_sm_close_session(
    _pamh: *mut PamHandle, _flags: c_int, _argc: c_int, _argv: *const *const c_char,
) -> c_int {
    PAM_IGNORE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pam_sm_chauthtok(
    pamh: *mut PamHandle, flags: c_int, _argc: c_int, _argv: *const *const c_char,
) -> c_int {
    if flags & PAM_PRELIM_CHECK != 0 {
        return PAM_IGNORE;
    }
    let Some(account) = user(pamh) else {
        return PAM_IGNORE;
    };
    if let (Some(old), Some(new)) = (wrapping(pamh, PAM_OLDAUTHTOK, &account), wrapping(pamh, PAM_AUTHTOK, &account)) {
        let bytes = glide_login_key::rekey_bytes(&old, &new);
        let _ = write_as(&account, &runtime_dir(pamh, &account), REKEY_FILE, &bytes);
    }
    PAM_IGNORE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Account {
        // SAFETY: getuid and getgid have no preconditions and cannot fail.
        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
        Account { name: String::new(), uid, gid }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-pam-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_key_is_written_private_and_replaces_an_older_one() {
        let dir = temp("write");
        write_as(&me(), &dir, KEY_FILE, b"old").unwrap();
        write_as(&me(), &dir, KEY_FILE, b"new").unwrap();
        let path = dir.join(KEY_FILE);
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_planted_link_is_replaced_and_its_target_left_alone() {
        let dir = temp("link");
        let target = dir.join("target");
        std::fs::write(&target, b"keep").unwrap();
        std::os::unix::fs::symlink(&target, dir.join(KEY_FILE)).unwrap();
        write_as(&me(), &dir, KEY_FILE, b"key").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
        assert!(!std::fs::symlink_metadata(dir.join(KEY_FILE)).unwrap().file_type().is_symlink());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_folder_of_another_account_is_refused() {
        let dir = temp("owner");
        let stranger = Account { uid: me().uid + 1, ..me() };
        assert!(write_as(&stranger, &dir, KEY_FILE, b"key").is_err());
        assert!(!dir.join(KEY_FILE).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_running_account_is_found_by_name() {
        let (name, uid) = {
            let me = me();
            // SAFETY: getpwuid returns static storage that is read here before any other passwd lookup.
            let entry = unsafe { libc::getpwuid(me.uid) };
            assert!(!entry.is_null());
            // SAFETY: pw_name of a non-null entry is a NUL-terminated string.
            let name = unsafe { CStr::from_ptr((*entry).pw_name) }.to_string_lossy().into_owned();
            (name, me.uid)
        };
        assert_eq!(account_named(&name).map(|account| account.uid), Some(uid));
        assert_eq!(account_named("no-such-user-glide"), None);
    }
}
