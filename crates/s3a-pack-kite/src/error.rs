/// Pack error: a message string and a stable code (§5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KiteError {
    message: String,
    code: String,
}

impl KiteError {
    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn code(&self) -> &str {
        &self.code
    }
}
