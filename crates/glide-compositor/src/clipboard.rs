use std::sync::Arc;

pub const PNG_MIME: &str = "image/png";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owned {
    Text(Arc<str>),
    Png(Arc<[u8]>),
}

impl Owned {
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Png(_) => None,
        }
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Png(png) => png,
        }
    }
}

pub fn write(selection: Owned, fd: std::os::fd::OwnedFd) {
    std::thread::spawn(move || {
        use std::io::Write;
        let mut file = std::fs::File::from(fd);
        if let Err(err) = file.write_all(selection.bytes()) {
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
        write(Owned::Png(png.clone()), writer.into());
        let mut received = Vec::new();
        std::io::Read::read_to_end(&mut reader, &mut received).expect("read");
        assert_eq!(received.as_slice(), &*png);
    }
}
