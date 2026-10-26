use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path, time::Duration};

pub const NOTICE_ID: u32 = u32::MAX - 6;
pub const OPEN_STORAGE: &str = "open-storage";
const INTERVAL: Duration = Duration::from_mins(5);
const GIB: u64 = 1024 * 1024 * 1024;
const LOW_SHARE: u64 = 5;
const LOW_BYTES: u64 = 2 * GIB;
const CLEAR_SHARE: u64 = 10;
const CLEAR_BYTES: u64 = 4 * GIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Low {
    pub free: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub free: u64,
    pub total: u64,
}

impl Usage {
    #[must_use]
    pub const fn is_low(self) -> bool {
        self.total > 0 && self.free * 100 < self.total * LOW_SHARE && self.free < LOW_BYTES
    }

    #[must_use]
    pub const fn is_clear(self) -> bool {
        self.total == 0 || self.free * 100 >= self.total * CLEAR_SHARE || self.free >= CLEAR_BYTES
    }
}

#[derive(Debug, Default)]
pub struct Watch {
    warned: bool,
}

impl Watch {
    pub fn update(&mut self, usage: Usage) -> bool {
        if usage.is_low() && !self.warned {
            self.warned = true;
            return true;
        }
        if usage.is_clear() {
            self.warned = false;
        }
        false
    }
}

fn usage(path: &Path) -> Option<Usage> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a NUL-terminated C string and `stats` points at writable
    // storage the size of a `statvfs`; the result is read only on success.
    let stats = unsafe {
        if libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) != 0 {
            return None;
        }
        stats.assume_init()
    };
    let block = stats.f_frsize;
    Some(Usage { free: stats.f_bavail.saturating_mul(block), total: stats.f_blocks.saturating_mul(block) })
}

pub fn start(sink: impl Fn(Low) + Send + 'static) {
    let Some(home) = glide_config::home() else {
        return;
    };
    let spawned = std::thread::Builder::new().name("glide-disk-space".into()).spawn(move || {
        let mut watch = Watch::default();
        loop {
            if let Some(usage) = usage(&home)
                && watch.update(usage)
            {
                sink(Low { free: usage.free });
            }
            std::thread::sleep(INTERVAL);
        }
    });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the disk space watch");
    }
}

#[must_use]
pub fn human(bytes: u64) -> String {
    let mib = bytes / (1024 * 1024);
    if mib >= 1024 { format!("{:.1} GB", bytes as f64 / GIB as f64) } else { format!("{mib} MB") }
}

impl crate::GlideState {
    pub fn handle_low_disk(&mut self, low: Low) {
        use crate::i18n::{Key, t};
        tracing::warn!(free = low.free, "the home disk is almost full");
        if !self.notifications.policy().system.disk_space {
            return;
        }
        let notice = glide_notify::Notification {
            id: NOTICE_ID,
            app_name: "Glide".to_string(),
            app_icon: "drive-harddisk".to_string(),
            summary: t(Key::DiskLowSummary).to_string(),
            body: t(Key::DiskLowBody).replace("{code}", &human(low.free)),
            actions: vec![glide_notify::Action { key: OPEN_STORAGE.into(), label: t(Key::DiskOpenStorage).into() }],
            urgency: glide_notify::Urgency::Critical,
            ..glide_notify::Notification::default()
        };
        self.notifications.push(&mut self.dock.icons, notice, true);
        self.refresh_topbar();
        self.queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOTAL: u64 = 500 * GIB;

    #[test]
    fn a_disk_is_low_only_when_both_the_share_and_the_bytes_are_small() {
        assert!(Usage { free: GIB, total: TOTAL }.is_low());
        assert!(!Usage { free: 3 * GIB, total: 1000 * GIB }.is_low(), "3 GB is still room");
        assert!(!Usage { free: GIB, total: 10 * GIB }.is_low(), "10 % of a small disk is fine");
        assert!(!Usage { free: 0, total: 0 }.is_low(), "pseudo file systems report nothing");
    }

    #[test]
    fn the_warning_is_given_once_until_space_is_really_freed() {
        let mut watch = Watch::default();
        let low = Usage { free: GIB, total: TOTAL };
        assert!(watch.update(low));
        assert!(!watch.update(low));
        assert!(!watch.update(Usage { free: 3 * GIB, total: TOTAL }), "a little relief does not re-arm");
        assert!(!watch.update(low));
        assert!(!watch.update(Usage { free: 5 * GIB, total: TOTAL }));
        assert!(watch.update(low), "after real relief it warns again");
    }

    #[test]
    fn the_home_file_system_can_be_measured() {
        let measured = usage(Path::new("/")).expect("statvfs works on /");
        assert!(measured.total > 0 && measured.free <= measured.total);
        assert_eq!(usage(Path::new("/glide/no/such/dir")), None);
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human(512 * 1024 * 1024), "512 MB");
        assert_eq!(human(GIB + GIB / 2), "1.5 GB");
    }
}
