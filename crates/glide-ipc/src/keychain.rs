use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct Hidden(String);

impl Hidden {
    #[must_use]
    pub const fn new(text: String) -> Self {
        Self(text)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Hidden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hidden({} bytes)", self.0.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Request {
    Status,
    Create { password: Hidden },
    Unlock { password: Hidden },
    Lock,
    Candidates { program: Option<String>, app_id: Option<String> },
    Secret { id: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: u64,
    pub label: String,
    pub username: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Response {
    Status { exists: bool, unlocked: bool },
    Done,
    WrongPassword,
    Locked,
    Candidates { list: Vec<Candidate> },
    Secret { secret: Hidden },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub id: u64,
    pub body: T,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_round_trips_through_one_json_line() {
        let request = Envelope { id: 7, body: Request::Unlock { password: Hidden::new("pa\"ss\nword".into()) } };
        let line = serde_json::to_string(&request).unwrap();
        assert!(!line.contains('\n'), "the channel is line based");
        assert_eq!(serde_json::from_str::<Envelope<Request>>(&line).unwrap(), request);
    }

    #[test]
    fn debug_hides_passwords_and_secrets() {
        let shown = format!(
            "{:?} {:?}",
            Request::Create { password: Hidden::new("hunter2".into()) },
            Response::Secret { secret: Hidden::new("hunter2".into()) }
        );
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}
