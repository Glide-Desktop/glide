use std::sync::Arc;

pub const PNG_MIME: &str = "image/png";
pub const PASSWORD_HINT: &str = "x-kde-passwordManagerHint";

#[derive(Clone, PartialEq, Eq)]
pub enum Owned {
    Text(Arc<str>),
    Secret(Arc<str>),
    Png(Arc<[u8]>),
}

impl std::fmt::Debug for Owned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(text) => f.debug_tuple("Text").field(text).finish(),
            Self::Secret(text) => write!(f, "Secret({} bytes)", text.len()),
            Self::Png(png) => write!(f, "Png({} bytes)", png.len()),
        }
    }
}

impl Owned {
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) | Self::Secret(text) => Some(text),
            Self::Png(_) => None,
        }
    }

    #[must_use]
    pub fn bytes(&self, mime: &str) -> &[u8] {
        match self {
            Self::Secret(_) if mime == PASSWORD_HINT => b"secret",
            Self::Text(text) | Self::Secret(text) => text.as_bytes(),
            Self::Png(png) => png,
        }
    }
}

pub fn write(selection: Owned, mime: String, fd: std::os::fd::OwnedFd) {
    std::thread::spawn(move || {
        use std::io::Write;
        let mut file = std::fs::File::from(fd);
        if let Err(err) = file.write_all(selection.bytes(&mime)) {
            tracing::debug!(?err, "a clipboard reader went away mid-transfer");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_text_selection_can_be_pasted_as_text() {
        let text = Owned::Text(Arc::from("hi"));
        let png = Owned::Png(Arc::from(&b"\x89PNG"[..]));
        assert_eq!(text.text(), Some("hi"));
        assert_eq!(png.text(), None);
    }

    #[test]
    fn a_reader_receives_the_exact_bytes() {
        let (mut reader, writer) = std::io::pipe().expect("pipe");
        let png: Arc<[u8]> = Arc::from(&b"\x89PNG\r\n\x1a\n"[..]);
        write(Owned::Png(png.clone()), PNG_MIME.into(), writer.into());
        let mut received = Vec::new();
        std::io::Read::read_to_end(&mut reader, &mut received).expect("read");
        assert_eq!(received.as_slice(), &*png);
    }

    #[test]
    fn a_secret_tells_clipboard_managers_to_skip_it_and_never_prints() {
        let secret = Owned::Secret(Arc::from("hunter2"));
        assert_eq!(secret.bytes("text/plain;charset=utf-8"), b"hunter2");
        assert_eq!(secret.bytes(PASSWORD_HINT), b"secret");
        assert_eq!(secret.text(), Some("hunter2"));
        assert!(!format!("{secret:?}").contains("hunter2"));
    }
}
