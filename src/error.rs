#![allow(dead_code)] // error variants exercised across all phases

use std::io;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("io error: {0}")]
    IoBare(#[from] io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("keychain error: {0}")]
    Keychain(String),

    #[error("profile not found: {0}")]
    ProfileNotFound(String),

    #[error("profile already exists: {0}")]
    ProfileExists(String),

    #[error("no active profile")]
    NoActiveProfile,

    #[error("no previous profile recorded")]
    NoPreviousProfile,

    #[error("config error: {0}")]
    Config(String),

    #[error("subprocess `{cmd}` failed: {message}")]
    Subprocess { cmd: String, message: String },

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("operation refused: {0}")]
    Refused(String),

    #[error("no master profile designated")]
    NoMasterProfile,

    #[error("`{0}` is the master profile; run `cs master --unset` first")]
    MasterProfileLocked(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn io_at(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}
