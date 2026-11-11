use serde::{Deserialize, Serialize};

/// What a command rejects with, in JS `{ code, message }`. `code` is the
/// library's refusal code (`consent_required`, `bad_token`, `bad_config`,
/// `bad_rects`, …) or the plugin's own: `declined`, `no_native_dialog`,
/// `busy`, `no_session`, `library_missing`, `library_unusable`,
/// `create_failed` (or the server's reason), `bad_logo`, `bad_color`,
/// `bad_options`, `pause_refused`, `wrong_thread`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

impl Error {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Error {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
