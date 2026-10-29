#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    Close,
    Field,
    Reveal,
    Submit,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chip {
    Done,
    Current,
    Next,
    Later,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Hint {
    Keys { skip: Option<String> },
    Wrong { master: bool, caps: bool },
    CapsLock,
    Checking,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct View {
    pub app: Option<String>,
    pub chips: Vec<(String, Chip)>,
    pub body: String,
    pub user: String,
    pub master: bool,
    pub bullets: usize,
    pub shown: Option<String>,
    pub hint: Hint,
    pub ready: bool,
    pub checking: bool,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View")
            .field("chips", &self.chips)
            .field("hint", &self.hint)
            .field("checking", &self.checking)
            .finish_non_exhaustive()
    }
}
