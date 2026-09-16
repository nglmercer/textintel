use std::fmt::{Display, Formatter};

/// Error returned by a pluggable external or local provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    pub provider: String,
    pub message: String,
}

impl ProviderError {
    pub fn new(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            message: message.into(),
        }
    }
}

impl Display for ProviderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} provider failed: {}", self.provider, self.message)
    }
}

impl std::error::Error for ProviderError {}

/// Errors raised by the text-intelligence pipeline.
#[derive(Debug)]
pub enum TextIntelError {
    InputTooLong { length: usize, maximum: usize },
    TooManySegments { maximum: usize },
    InvalidConfiguration(String),
    Provider(ProviderError),
    Storage(String),
    Serialization(String),
}

impl Display for TextIntelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooLong { length, maximum } => {
                write!(
                    f,
                    "input length {length} exceeds max_input_length={maximum}"
                )
            }
            Self::TooManySegments { maximum } => {
                write!(f, "message exceeds max_segments={maximum}")
            }
            Self::InvalidConfiguration(message) => write!(f, "invalid configuration: {message}"),
            Self::Provider(error) => error.fmt(f),
            Self::Storage(message) => write!(f, "storage error: {message}"),
            Self::Serialization(message) => write!(f, "serialization error: {message}"),
        }
    }
}

impl std::error::Error for TextIntelError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ProviderError> for TextIntelError {
    fn from(value: ProviderError) -> Self {
        Self::Provider(value)
    }
}

impl From<serde_json::Error> for TextIntelError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value.to_string())
    }
}
