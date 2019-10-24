use std::fmt;

use super::SessionId;

impl fmt::Display for SessionId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}...", &self.0[..5])
    }
}

impl fmt::Debug for SessionId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}...", &self.0[..5])
    }
}

impl Into<String> for SessionId {
    fn into(self) -> String {
        self.0
    }
}

impl From<String> for SessionId {
    fn from(id: String) -> Self {
        SessionId(id)
    }
}
