//! Starting a ticket (SDD §9.1): the steps of the start sequence, each idempotent (SES-02).

pub mod sequence;
pub mod setup;
pub mod worktree;

use std::fmt;

use crate::board::BoardError;
use crate::data_dir::DataDirError;
use crate::git::{GitError, GitStep, RefusedHook};
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
    /// One of the user's hooks refused a git step of the start: `message` says what happened and the way on, `hook`
    /// is what the ticket keeps of it (which hook, the step, its whole output).
    HookRefused {
        hook: Box<RefusedHook>,
        message: String,
    },
    Other(String),
}

impl StartError {
    /// What the ticket keeps of a hook's refusal, when one stopped the start. A refusal no step named (a git command
    /// of the start that isn't one of its own hook steps) is kept as a refusal of the start.
    pub fn refused_hook(&self) -> Option<RefusedHook> {
        match self {
            Self::HookRefused { hook, .. } => Some(hook.as_ref().clone()),
            Self::Git(GitError::HookRefused(refusal)) => Some(RefusedHook {
                step: refusal.step(),
                ..RefusedHook::of(refusal, GitStep::AddWorktree, "the start")
            }),
            _ => None,
        }
    }
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The user's Stop on the card: nothing the start made is kept (a reopened ticket's branch and earlier
            // work always are), so Retry start does the whole start again.
            Self::Git(GitError::Stopped { hook, .. }) => {
                f.write_str("You stopped the start")?;
                if let Some(hook) = hook {
                    write!(f, " while your {hook} hook was running")?;
                }
                f.write_str(". Retry start starts it again.")
            }
            Self::Board(e) => write!(f, "{e}"),
            Self::Git(e) => write!(f, "{e}"),
            Self::Store(e) => write!(f, "{e}"),
            Self::Path(e) => write!(f, "{e}"),
            Self::HookRefused { message, .. } => f.write_str(message),
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
