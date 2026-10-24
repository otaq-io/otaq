//! Shared fixtures for the board tests: a store in a temporary data folder and temporary git repositories.

pub mod freed;
pub mod resources;
pub mod stand_in;

use std::path::{Path, PathBuf};

use otaq_core::board::Board;
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Actor, Kind};
use otaq_core::git::testing::TempRepos;
use otaq_core::store::Store;
use otaq_core::warm::clone::{CloneError, Cloner, FsCloner, probe};

pub struct Fixture {
    pub home: tempfile::TempDir,
    pub repos: TempRepos,
    pub board: Board,
}

impl Fixture {
    pub async fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let data = DataDir::new(home.path().join("home"));
        let store = Store::open_in(&data).await.unwrap();
        Self {
            home,
            repos: TempRepos::new(),
            board: Board::with_path_git(store, data),
        }
    }

    /// A fixture with the `shop-web` project (key SHOP, base `main`).
    pub async fn shop() -> Self {
        let f = Self::new().await;
        f.add_project("shop-web").await;
        f
    }

    pub async fn add_project(&self, name: &str) {
        let repo = self.repos.repo(name, 1);
        self.board
            .add_project(&AddProject {
                path: repo.display().to_string(),
                ..Default::default()
            })
            .await
            .unwrap();
    }

    pub async fn create(&self, title: &str) -> Ticket {
        self.create_with(NewTicketInput {
            title: title.into(),
            ..shop_input()
        })
        .await
    }

    pub async fn epic(&self, title: &str) -> Ticket {
        self.create_with(NewTicketInput {
            title: title.into(),
            kind: Kind::Epic,
            ..shop_input()
        })
        .await
    }

    pub async fn child(&self, epic: &Ticket, title: &str) -> Ticket {
        self.create_with(NewTicketInput {
            title: title.into(),
            epic: Some(epic.key.to_string()),
            ..shop_input()
        })
        .await
    }

    pub async fn create_with(&self, input: NewTicketInput) -> Ticket {
        self.board.create_ticket(&input, &me()).await.unwrap()
    }

    /// Creates tickets until the next one would get `number`.
    pub async fn skip_to(&self, number: u32) {
        loop {
            let t = self.create("filler").await;
            if t.key.number + 1 >= number {
                break;
            }
        }
    }

    pub async fn ticket(&self, key: &str) -> Ticket {
        self.board.ticket(key).await.unwrap()
    }
}

pub fn shop_input() -> NewTicketInput {
    NewTicketInput {
        project: "SHOP".into(),
        ..Default::default()
    }
}

pub fn me() -> Actor {
    Actor::User("me".into())
}

pub fn agent(key: &str) -> Actor {
    Actor::Agent(key.parse().unwrap())
}

impl Fixture {
    /// The data folder the fixture's store lives in.
    pub fn data(&self) -> otaq_core::data_dir::DataDir {
        DataDir::new(self.home.path().join("home"))
    }

    /// The `shop-web` repository.
    pub fn repo(&self) -> std::path::PathBuf {
        self.repos.root().join("shop-web")
    }

    /// Gives a ticket its branch and worktree as the start sequence does; returns the worktree.
    pub async fn start_worktree(&self, ticket: &Ticket) -> std::path::PathBuf {
        otaq_core::start::prepare_worktree(&self.board, &self.data(), &ticket.key.to_string())
            .await
            .unwrap()
            .path
    }

    /// Puts a ticket straight into `status`, as if the board had got it there.
    pub async fn force_status(&self, ticket: &Ticket, status: otaq_core::domain::Status) {
        sqlx::query("UPDATE tickets SET stage_id = ?, state = ? WHERE id = ?")
            .bind(otaq_core::flow::engine::default_stage(status, None))
            .bind(status.as_str())
            .bind(&ticket.id)
            .execute(self.board.store().pool())
            .await
            .unwrap();
    }
}

/// Copies instead of cloning, for a test disk that can't clone (keeping modified times, as a clone does). Never used
/// outside the tests.
#[derive(Debug)]
pub struct CopyCloner;

impl Cloner for CopyCloner {
    fn clone_tree(
        &self,
        from: &Path,
        to: &Path,
        only: Option<&[PathBuf]>,
    ) -> Result<(), CloneError> {
        let io = |path: &Path| {
            let path = path.to_owned();
            move |error| CloneError::Io { path, error }
        };
        let whole = [PathBuf::new()];
        for rel in only.unwrap_or(&whole) {
            let dst = to.join(rel);
            if dst.symlink_metadata().is_ok() {
                return Err(CloneError::Io {
                    path: dst,
                    error: std::io::ErrorKind::AlreadyExists.into(),
                });
            }
            copy(&from.join(rel), &dst).map_err(io(&dst))?;
        }
        Ok(())
    }
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to)?;
        // A clone keeps the modified time; so does this stand-in, as the timestamp trick's tests compare times.
        let modified = std::fs::metadata(from)?.modified()?;
        std::fs::File::options()
            .write(true)
            .open(to)?
            .set_modified(modified)
    }
}

/// The file system's cloning where the test disk under `data` can, else the copying stand-in (with a printed note).
pub fn test_cloner(data: &DataDir) -> Box<dyn Cloner> {
    if probe(data).available {
        Box::new(FsCloner)
    } else {
        println!("note: the test disk can't clone; a copying stand-in clones instead");
        Box::new(CopyCloner)
    }
}

/// The ticket's last comment (CMT-05: a hand-over's note or summary), if it has one.
pub async fn last_comment(
    board: &Board,
    key: &str,
) -> Option<otaq_core::board::ticket_comments::TicketComment> {
    board
        .ticket_comments(key)
        .await
        .expect("the comments")
        .comments
        .pop()
}
