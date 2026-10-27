//! A board from a newer version (UPD-08, d0140 design audit AT; *32 → Item 7*): when the service won't start because
//! a newer build changed the database, the window shows why instead of the board, with **Restore the copy from
//! before 0.2.0** (its one confirmation), **Download 0.2.0**, Try again and the command for a terminal; then the
//! restore's three steps while the service is down. Without a copy there is no Restore: the message says to reinstall.

use otaq_core::naming;
use otaq_core::store::NewerDatabase;
use otaq_core::store::restore::{self, RestoreStep};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;

/// The link's state while the service refuses a newer database: the reason, and the restore when one runs or
/// failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewerDatabaseState {
    pub reason: NewerDatabase,
    pub restore: RestoreRun,
}

impl NewerDatabaseState {
    pub fn new(reason: NewerDatabase) -> Self {
        Self {
            reason,
            restore: RestoreRun::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreRun {
    Idle,
    /// Running; the last step it reported, `None` while the service is being stopped.
    Running(Option<RestoreStep>),
    /// The last restore failed: what went wrong and what's left.
    Failed(String),
}

/// What the main area shows instead of the board (`NewerDatabaseScreen` on the page).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewerDatabaseScreen {
    Message(Box<NewerDatabaseMessage>),
    Restoring(RestoringView),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct NewerDatabaseMessage {
    pub icon: IconName,
    /// "This board was last opened by Otaq 0.2.0"
    pub title: String,
    pub body: String,
    /// "Restore the copy from before 0.2.0", the main button; `None` without a copy.
    pub restore: Option<String>,
    /// "Download 0.2.0" and its release page.
    pub download: String,
    pub download_url: String,
    pub try_again: &'static str,
    /// "or in a terminal:" and the command; `None` without a copy.
    pub terminal: Option<TerminalLine>,
    /// The confirmation Restore opens; `None` without a copy.
    pub confirm: Option<RestoreConfirm>,
    /// Why the last restore failed, and what's left.
    pub failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct TerminalLine {
    pub lead: &'static str,
    pub command: String,
}

/// "Restore the board from before 0.2.0?" with what's lost; Restore · Cancel (Esc cancels).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct RestoreConfirm {
    pub title: String,
    pub what: String,
    pub untouched: String,
    pub restore: &'static str,
    pub cancel: &'static str,
}

/// "Restoring the board…" with its three steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct RestoringView {
    pub icon: IconName,
    pub title: &'static str,
    pub note: &'static str,
    pub steps: Vec<RestoreStepRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct RestoreStepRow {
    pub label: String,
    pub state: StepState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Done,
    Current,
    Waiting,
}

/// The screen for `state`, on this build (`running`, "0.1.0").
pub fn screen(state: &NewerDatabaseState, running: &str) -> NewerDatabaseScreen {
    match &state.restore {
        RestoreRun::Running(last) => {
            NewerDatabaseScreen::Restoring(restoring(state, last.as_ref()))
        }
        RestoreRun::Idle => {
            NewerDatabaseScreen::Message(Box::new(message(&state.reason, running, None)))
        }
        RestoreRun::Failed(why) => NewerDatabaseScreen::Message(Box::new(message(
            &state.reason,
            running,
            Some(why.clone()),
        ))),
    }
}

fn message(reason: &NewerDatabase, running: &str, failure: Option<String>) -> NewerDatabaseMessage {
    let version = &reason.last_opened_by;
    let product = naming::PRODUCT_NAME;
    let copy = reason.copy.as_ref();
    NewerDatabaseMessage {
        icon: IconName::Database,
        title: format!("This board was last opened by {product} {version}"),
        body: match copy {
            Some(_) => format!(
                "You\u{2019}re on {running}, which can\u{2019}t open it. Restore the copy {product} made before \
                 {version} changed it, or reinstall {version}."
            ),
            None => format!(
                "You\u{2019}re on {running}, which can\u{2019}t open it. There\u{2019}s no copy from before \
                 {version}, so reinstall {version} to open it."
            ),
        },
        restore: copy.map(|c| format!("Restore the copy from before {}", c.version)),
        download: format!("Download {version}"),
        download_url: naming::release_page(version),
        try_again: "Try again",
        terminal: copy.map(|_| TerminalLine {
            lead: "or in a terminal:",
            command: format!("{} restore-database", naming::COMMAND_NAME),
        }),
        confirm: copy.map(|c| {
            let (title, [what, untouched]) = restore::question(c);
            RestoreConfirm {
                title,
                what,
                untouched,
                restore: "Restore",
                cancel: "Cancel",
            }
        }),
        failure: failure.map(|why| format!("Couldn\u{2019}t restore the board: {why}")),
    }
}

fn restoring(state: &NewerDatabaseState, last: Option<&RestoreStep>) -> RestoringView {
    // The row being worked on: stopping, putting back, starting.
    let current = match last {
        None => 0,
        Some(RestoreStep::ServiceStopped | RestoreStep::PuttingBackCopy { .. }) => 1,
        Some(RestoreStep::StartingService) => 2,
    };
    let made_at_ms = state.reason.copy.as_ref().map_or(0, |c| c.made_at_ms);
    let labels = [
        if current == 0 {
            "Stopping the background service".to_owned()
        } else {
            RestoreStep::ServiceStopped.label()
        },
        RestoreStep::PuttingBackCopy { made_at_ms }.label(),
        RestoreStep::StartingService.label(),
    ];
    RestoringView {
        icon: IconName::History,
        title: "Restoring the board\u{2026}",
        note: "This takes a few seconds; the board opens when it\u{2019}s done.",
        steps: labels
            .into_iter()
            .enumerate()
            .map(|(i, label)| RestoreStepRow {
                label,
                state: match i.cmp(&current) {
                    std::cmp::Ordering::Less => StepState::Done,
                    std::cmp::Ordering::Equal => StepState::Current,
                    std::cmp::Ordering::Greater => StepState::Waiting,
                },
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::store::versions::DatabaseCopy;

    use super::*;

    fn reason(copy: bool) -> NewerDatabase {
        NewerDatabase {
            last_opened_by: "0.2.0".into(),
            copy: copy.then(|| DatabaseCopy {
                path: "/h/board.db.before-0.2.0".into(),
                version: "0.2.0".into(),
                made_at_ms: 0,
            }),
        }
    }

    fn message_of(state: &NewerDatabaseState) -> NewerDatabaseMessage {
        match screen(state, "0.1.0") {
            NewerDatabaseScreen::Message(m) => *m,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn with_a_copy_the_main_button_restores() {
        let m = message_of(&NewerDatabaseState::new(reason(true)));
        let product = naming::PRODUCT_NAME;
        assert_eq!(
            m.title,
            format!("This board was last opened by {product} 0.2.0")
        );
        assert_eq!(
            m.body,
            format!(
                "You\u{2019}re on 0.1.0, which can\u{2019}t open it. Restore the copy {product} made before 0.2.0 \
                 changed it, or reinstall 0.2.0."
            )
        );
        assert_eq!(
            m.restore.as_deref(),
            Some("Restore the copy from before 0.2.0")
        );
        assert_eq!(m.download, "Download 0.2.0");
        assert!(
            m.download_url.ends_with("/releases/tag/v0.2.0"),
            "{}",
            m.download_url
        );
        assert_eq!(
            m.terminal.as_ref().map(|t| (t.lead, t.command.as_str())),
            Some((
                "or in a terminal:",
                format!("{} restore-database", naming::COMMAND_NAME).as_str()
            ))
        );
        let confirm = m.confirm.unwrap();
        assert_eq!(confirm.title, "Restore the board from before 0.2.0?");
        assert!(
            confirm
                .what
                .contains("Changes made on 0.2.0 since then are lost")
        );
        assert_eq!((confirm.restore, confirm.cancel), ("Restore", "Cancel"));
        assert_eq!(m.failure, None);
    }

    #[test]
    fn without_a_copy_it_says_to_reinstall() {
        let m = message_of(&NewerDatabaseState::new(reason(false)));
        assert_eq!(m.restore, None);
        assert_eq!(m.terminal, None);
        assert_eq!(m.confirm, None);
        assert!(
            m.body.ends_with("so reinstall 0.2.0 to open it."),
            "{}",
            m.body
        );
        assert_eq!(m.download, "Download 0.2.0");
    }

    #[test]
    fn the_steps_follow_the_restore() {
        let rows = |last: Option<RestoreStep>| {
            let state = NewerDatabaseState {
                reason: reason(true),
                restore: RestoreRun::Running(last),
            };
            match screen(&state, "0.1.0") {
                NewerDatabaseScreen::Restoring(view) => {
                    assert_eq!(view.title, "Restoring the board\u{2026}");
                    view.steps
                }
                other => panic!("{other:?}"),
            }
        };
        let states = |last| rows(last).iter().map(|r| r.state).collect::<Vec<_>>();
        use StepState::{Current, Done, Waiting};
        let stopping = rows(None);
        assert_eq!(stopping[0].label, "Stopping the background service");
        assert_eq!(states(None), [Current, Waiting, Waiting]);
        let putting = rows(Some(RestoreStep::PuttingBackCopy { made_at_ms: 0 }));
        assert_eq!(putting[0].label, "Background service stopped");
        assert!(putting[1].label.starts_with("Putting back the copy from "));
        assert_eq!(putting[2].label, "Starting the background service");
        assert_eq!(
            states(Some(RestoreStep::ServiceStopped)),
            [Done, Current, Waiting]
        );
        assert_eq!(
            states(Some(RestoreStep::StartingService)),
            [Done, Done, Current]
        );
    }

    #[test]
    fn a_failed_restore_stays_on_the_message_and_says_why() {
        let state = NewerDatabaseState {
            reason: reason(true),
            restore: RestoreRun::Failed("The copy couldn't be put back: denied".into()),
        };
        let m = message_of(&state);
        assert_eq!(
            m.failure.as_deref(),
            Some("Couldn\u{2019}t restore the board: The copy couldn't be put back: denied")
        );
        assert!(m.restore.is_some(), "Restore can be tried again");
    }
}
