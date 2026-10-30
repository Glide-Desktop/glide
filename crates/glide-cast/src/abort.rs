#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Abort {
    Stopped,
    Failed(String),
}

impl Abort {
    #[must_use]
    pub fn into_reason(self) -> Option<String> {
        match self {
            Self::Stopped => None,
            Self::Failed(reason) => Some(reason),
        }
    }
}

impl From<String> for Abort {
    fn from(reason: String) -> Self {
        Self::Failed(reason)
    }
}

impl From<&str> for Abort {
    fn from(reason: &str) -> Self {
        Self::Failed(reason.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_has_no_failure_reason_but_a_failure_keeps_its_own() {
        assert_eq!(Abort::Stopped.into_reason(), None);
        assert_eq!(Abort::from("the link broke").into_reason(), Some("the link broke".to_string()));
        assert_eq!(Abort::from(String::from("no address")), Abort::Failed("no address".into()));
    }
}
