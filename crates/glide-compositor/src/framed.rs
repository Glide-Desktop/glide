use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct FramedClients {
    pids: HashSet<i32>,
}

impl FramedClients {
    pub fn register(&mut self, pid: i32, alive: impl Fn(i32) -> bool) -> bool {
        self.pids.retain(|known| alive(*known));
        pid > 0 && self.pids.insert(pid)
    }

    #[must_use]
    pub fn contains(&self, pid: i32) -> bool {
        self.pids.contains(&pid)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pids.is_empty()
    }
}

#[must_use]
pub fn process_alive(pid: i32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_registers_once_and_dead_ones_are_forgotten() {
        let mut framed = FramedClients::default();
        assert!(framed.register(10, |_| true));
        assert!(!framed.register(10, |_| true), "a second call changes nothing");
        assert!(framed.register(11, |pid| pid != 10));
        assert!(!framed.contains(10), "a pid whose process is gone may be reused by a stranger");
        assert!(framed.contains(11));
        assert!(!framed.register(0, |_| true));
    }

    #[test]
    fn this_process_is_alive() {
        assert!(process_alive(i32::try_from(std::process::id()).expect("pid fits")));
    }
}
