use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Debug)]
pub enum ResourceError {
    Io { path: PathBuf, message: String },
    Parse { path: PathBuf, message: String },
    Validation { path: PathBuf, message: String },
}

impl Display for ResourceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(f, "resource I/O error at {}: {message}", path.display())
            }
            Self::Parse { path, message } => {
                write!(f, "resource parse error at {}: {message}", path.display())
            }
            Self::Validation { path, message } => {
                write!(f, "invalid resource at {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ResourceError {}
