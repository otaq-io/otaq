//! Starting a ticket (SDD §9.1): the steps of the start sequence, each idempotent (SES-02).

pub mod sequence;
pub mod setup;
pub mod worktree;

use std::fmt;

use crate::board::BoardError;
use crate::data_dir::DataDirError;
use crate::git::GitError;
use crate::store::StoreError;

pub use sequence::{StartOutcome, Starter, own_agent_session};
pub use setup::{Setup, begin_setup_log, copy_env_files, run_setup};
pub use worktree::{Worktree, prepare_worktree};

/// Why a start step failed. The message is written for people; the start sequence stores it.
#[derive(Debug)]
pub enum StartError {
    Board(BoardError),
    Git(GitError),
    Store(StoreError),
    Path(DataDirError),
    Other(String),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Board(e) => write!(f, "{e}"),
            Self::Git(e) => write!(f, "{e}"),
            Self::Store(e) => write!(f, "{e}"),
            Self::Path(e) => write!(f, "{e}"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StartError {}

impl From<BoardError> for StartError {
    fn from(e: BoardError) -> Self {
        match e {
            BoardError::Git(e) => Self::Git(e),
            e => Self::Board(e),
        }
    }
}

impl From<GitError> for StartError {
    fn from(e: GitError) -> Self {
        Self::Git(e)
    }
}

impl From<StoreError> for StartError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

impl From<DataDirError> for StartError {
    fn from(e: DataDirError) -> Self {
        Self::Path(e)
    }
}
