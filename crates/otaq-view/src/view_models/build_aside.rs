//! The line that says a ticket's build was set aside (d0125; STG-11, d0082): on entering a human check or Delivery
//! a ticket's build output leaves its worktree to save disk (`ticket_env/free.rs`), and a Run or a new Shell brings it
//! back first (`ticket_env/fetch.rs`). The Overview, and the Run and Shell of such a ticket, say so: "The build was set
//! aside to save disk · Run or Shell brings it back"; while it comes back, "Bringing the build back…". Only a build
//! that was actually freed is mentioned; when it is in the worktree nothing shows.

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::BuildState;
use otaq_core::ticket_env::free::freed_in;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;

pub const SET_ASIDE: &str =
    "The build was set aside to save disk \u{b7} Run or Shell brings it back";
pub const BRINGING_BACK: &str = "Bringing the build back\u{2026}";

/// The line, with its icon; `bringing` while the build comes back (the page shows its spinner).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BuildAsideLine")]
pub struct BuildAsideLine {
    pub text: String,
    pub icon: IconName,
    pub bringing: bool,
}

/// The line for `ticket` now: set aside while it waits in a human check or Delivery with its build freed, bringing
/// back while a Send back, Run or Shell fetches it; none otherwise (an epic has no build of its own).
pub fn line(ticket: &Ticket) -> Option<BuildAsideLine> {
    if ticket.is_epic() {
        return None;
    }
    let work = ticket.work.as_ref()?;
    work.worktree.as_ref()?;
    match work.build_state {
        BuildState::Freed if freed_in(ticket.status) => Some(BuildAsideLine {
            text: SET_ASIDE.to_owned(),
            icon: IconName::Archive,
            bringing: false,
        }),
        BuildState::Fetching => Some(BuildAsideLine {
            text: BRINGING_BACK.to_owned(),
            icon: IconName::LoaderCircle,
            bringing: true,
        }),
        BuildState::Freed | BuildState::Present => None,
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::tickets::Work;
    use otaq_core::domain::{Kind, Status};

    use super::*;
    use crate::view_models::card_fixtures::ticket;

    fn with_build(status: Status, state: BuildState) -> Ticket {
        let mut t = ticket("SHOP-22", "Offline habits", status);
        t.work = Some(Work {
            worktree: Some("/tmp/wt/shop-web/SHOP-22".into()),
            build_state: state,
            ..Default::default()
        });
        t
    }

    /// d0125: the line only when the build was actually freed; "Bringing the build back…" while it comes back.
    #[test]
    fn the_line_only_when_the_build_was_set_aside() {
        for status in [Status::WaitingForYou, Status::ToDeliver, Status::PrOpen] {
            let l = line(&with_build(status, BuildState::Freed)).expect("freed at Review");
            assert_eq!(
                l.text,
                "The build was set aside to save disk \u{b7} Run or Shell brings it back"
            );
            assert!(!l.bringing);
            assert_eq!(
                line(&with_build(status, BuildState::Present)),
                None,
                "in the worktree"
            );
        }
        let back = line(&with_build(Status::WaitingForYou, BuildState::Fetching)).unwrap();
        assert_eq!(
            (back.text.as_str(), back.bringing),
            ("Bringing the build back\u{2026}", true)
        );
        // Sent back: the agent's build comes back too.
        assert!(line(&with_build(Status::Working, BuildState::Fetching)).is_some());
        // Freed but no longer in Review (merged, closed): Run and Shell no longer bring it back.
        assert_eq!(line(&with_build(Status::Merged, BuildState::Freed)), None);
        // No worktree, or an epic: nothing to say.
        assert_eq!(line(&ticket("SHOP-1", "t", Status::WaitingForYou)), None);
        let mut epic = with_build(Status::WaitingForYou, BuildState::Freed);
        epic.kind = Kind::Epic;
        assert_eq!(line(&epic), None);
    }
}
