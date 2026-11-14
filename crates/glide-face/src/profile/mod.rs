pub mod crypto;
pub mod storage;

pub use storage::{Profile, STALE_PROFILE_HINT, SYSTEM_STORE_ROOT, Store, resolve_username};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ProfileError {
    #[error("modifying the biometric profile requires root privileges")]
    NotRoot,
    #[error("insecure profile store: {0}")]
    Insecure(String),
    #[error("{0}")]
    Io(String, std::io::ErrorKind),
    #[error("{0}")]
    Crypto(String),
    #[error("{0}")]
    Corrupt(String),
    #[error("{0}")]
    Other(String),
    #[error("{0}: {1}")]
    Context(String, Box<ProfileError>),
}

impl ProfileError {
    pub(crate) fn from_io(path: &std::path::Path, e: &std::io::Error) -> Self {
        ProfileError::Io(format!("{}: {e}", path.display()), e.kind())
    }

    #[must_use]
    pub fn context(self, what: &str) -> Self {
        ProfileError::Context(what.to_string(), Box::new(self))
    }

    #[must_use]
    pub fn root(&self) -> &ProfileError {
        match self {
            ProfileError::Context(_, inner) => inner.root(),
            other => other,
        }
    }

    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(self.root(), ProfileError::Io(_, std::io::ErrorKind::NotFound))
    }

    #[must_use]
    pub fn is_insecure(&self) -> bool {
        matches!(self.root(), ProfileError::Insecure(_))
    }

    #[must_use]
    pub fn is_not_root(&self) -> bool {
        matches!(self.root(), ProfileError::NotRoot)
    }
}
