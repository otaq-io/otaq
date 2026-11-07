//! One-time hints: a concept is explained when it first matters, never behind a setting. A hint is a short note in a
//! dark popover next to the element it explains, with one button; the button dismisses it for good (kept by the
//! service per data folder, so a restart doesn't bring it back and a fresh data folder shows it again).
//!
//! The ids are a closed set ([`Hint`], kept with the store); each hint's wording lives here. A new user's first run
//! places its hints on the board ([`board`]): at the first permission prompt, the first agent question, the first time
//! a checker sends a ticket back, the first ticket waiting for review, and, after the first approve, "That was your
//! flow" at Edit flow. The box for new work shows its own ([`HintId::NewWork`]) when it first appears. One shows at a
//! time on the board, the most urgent first; each waits for its moment and, once dismissed, never comes back.

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, Status};
use otaq_core::flow::Flow;
use otaq_core::naming::PRODUCT_NAME;
use otaq_core::store::settings::Hint;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;

/// A hint's id as the page names it: the same names as the hints the service stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum HintId {
    /// The design gallery's sample, the one hint with no moment in the app.
    GallerySample,
    /// The first time Edit flow turns the board into the editor.
    FlowColumns,
    /// A new user's first ticket, the card in Backlog: answered with Create & start or Not now.
    FirstTicket,
    /// The first agent question.
    FirstQuestion,
    /// The first permission prompt.
    PermissionPrompt,
    /// The first time a checker sends a ticket back.
    CheckerSentBack,
    /// The first ticket waiting for the user's review.
    FirstReview,
    /// After the first approve, at Edit flow.
    ThatWasYourFlow,
    /// The box for new work, when it first appears.
    NewWork,
}

impl From<HintId> for Hint {
    fn from(id: HintId) -> Self {
        match id {
            HintId::GallerySample => Hint::GallerySample,
            HintId::FlowColumns => Hint::FlowColumns,
            HintId::FirstTicket => Hint::FirstTicket,
            HintId::FirstQuestion => Hint::FirstQuestion,
            HintId::PermissionPrompt => Hint::PermissionPrompt,
            HintId::CheckerSentBack => Hint::CheckerSentBack,
            HintId::FirstReview => Hint::FirstReview,
            HintId::ThatWasYourFlow => Hint::ThatWasYourFlow,
            HintId::NewWork => Hint::NewWork,
        }
    }
}

impl From<Hint> for HintId {
    fn from(hint: Hint) -> Self {
        match hint {
            Hint::GallerySample => HintId::GallerySample,
            Hint::FlowColumns => HintId::FlowColumns,
            Hint::FirstTicket => HintId::FirstTicket,
            Hint::FirstQuestion => HintId::FirstQuestion,
            Hint::PermissionPrompt => HintId::PermissionPrompt,
            Hint::CheckerSentBack => HintId::CheckerSentBack,
            Hint::FirstReview => HintId::FirstReview,
            Hint::ThatWasYourFlow => HintId::ThatWasYourFlow,
            Hint::NewWork => HintId::NewWork,
        }
    }
}

/// What the popover shows: its title (when it has one), its text and its one button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HintView")]
pub struct View {
    pub id: HintId,
    pub title: Option<String>,
    pub text: String,
    pub button: String,
}

/// The button most hints have: it dismisses the hint for good.
pub const DISMISS: &str = "Got it";

/// The first time Edit flow turns the board into the editor.
pub const FLOW_COLUMNS: &str = "This is your flow, as columns. Click a stage to see what it\u{2019}s for, + adds one \
     between two stages, and drag to reorder. Watch a ticket to see what your draft would do. Nothing changes until \
     you save.";

/// The box for new work, when it first appears (after the first approve).
pub const NEW_WORK: &str = "Type a ticket and press \u{21b5} to create it as written. Or Ask Noyan to shape it: Noyan \
     takes a look and writes it up, or files separate tickets when you asked for several things. You start them.";

/// The first ticket waiting for review.
pub const FIRST_REVIEW: &str = "Open the ticket to see what changed and what the checker found. Approve merges it or \
     opens a PR, as your flow delivers; Send back returns it to the agent with your notes.";

/// A hint with a title and Got it.
fn titled(id: HintId, title: &str, text: String) -> View {
    View {
        id,
        title: Some(title.to_owned()),
        text,
        button: DISMISS.to_owned(),
    }
}

/// The first agent question: what the turquoise card and Needs you are, and the key that jumps to the next one.
pub fn first_question(keys: &KeyLabels) -> View {
    let next = keys
        .label(KeyAction::NextNeedsYouItem)
        .map(|k| format!(" {k} jumps to the next one, in any project."))
        .unwrap_or_default();
    titled(
        HintId::FirstQuestion,
        "An agent needs you",
        format!("When an agent asks something, its card turns turquoise and shows up here.{next}"),
    )
}

/// The first permission prompt: what the tool's permission mode means while agents work on their own (it used to be
/// a warning on the Tools step).
pub fn permission_prompt() -> View {
    titled(
        HintId::PermissionPrompt,
        "The agent asks for permission",
        format!(
            "Its tool is set to ask before some actions, so the agent waits for you. With several agents at once, \
             allowlists or auto mode in the tool\u{2019}s own settings let them work on. {PRODUCT_NAME} won\u{2019}t \
             change your settings."
        ),
    )
}

/// The first time a checker sends a ticket back: `check` failed it, so it went back to `stage`.
pub fn checker_sent_back(check: &str, stage: &str) -> View {
    titled(
        HintId::CheckerSentBack,
        "Sent back by the checker",
        format!(
            "{check} tested the change and found a problem, so the ticket went back to {stage} with the checker\u{2019}s \
             notes. You only review work that passes."
        ),
    )
}

/// The first ticket waiting for review: its button opens it.
pub fn first_review(key: &str) -> View {
    View {
        id: HintId::FirstReview,
        title: Some("Your turn: review".to_owned()),
        text: FIRST_REVIEW.to_owned(),
        button: format!("Open {key}"),
    }
}

/// After the first approve: the way `key` went is the project's flow; its button is Edit flow.
pub fn that_was_your_flow(key: &str, flow: &Flow) -> View {
    let way: Vec<&str> = std::iter::once("Backlog")
        .chain(flow.stages.iter().map(|s| s.name.as_str()))
        .chain(std::iter::once("Done"))
        .collect();
    View {
        id: HintId::ThatWasYourFlow,
        title: Some("That was your flow".to_owned()),
        text: format!(
            "{key} went {}. These columns are this project\u{2019}s process: add a check, drop one, or pick other \
             agents with Edit flow.",
            way.join(" \u{2192} ")
        ),
        button: super::board_page::EDIT_FLOW.to_owned(),
    }
}

/// The hint's popover, worded without a moment's facts (the gallery, `hints.view`).
pub fn view(id: HintId, keys: &KeyLabels) -> View {
    let plain = |text: &str| View {
        id,
        title: None,
        text: text.to_owned(),
        button: DISMISS.to_owned(),
    };
    match id {
        HintId::GallerySample => {
            plain("A hint like this shows once, when something first matters, and never again.")
        }
        HintId::FlowColumns => plain(FLOW_COLUMNS),
        HintId::FirstTicket => plain(super::empty_board::TIP),
        HintId::FirstQuestion => first_question(keys),
        HintId::PermissionPrompt => permission_prompt(),
        HintId::CheckerSentBack => checker_sent_back("The checker", "its work stage"),
        HintId::FirstReview => View {
            button: DISMISS.to_owned(),
            ..first_review("")
        },
        HintId::ThatWasYourFlow => View {
            id,
            title: Some("That was your flow".to_owned()),
            text: "The columns your first ticket went through are this project\u{2019}s process: add a check, drop \
                   one, or pick other agents with Edit flow."
                .to_owned(),
            button: super::board_page::EDIT_FLOW.to_owned(),
        },
        HintId::NewWork => titled(HintId::NewWork, "New work starts here", NEW_WORK.to_owned()),
    }
}

/// The hint's popover, or nothing once the user dismissed it (`seen`).
pub fn shown(id: HintId, seen: &[Hint], keys: &KeyLabels) -> Option<View> {
    (!seen.contains(&id.into())).then(|| view(id, keys))
}

/// Where a board hint points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "BoardHintAnchor")]
pub enum Anchor {
    /// A ticket's card.
    Card { key: String },
    /// Edit flow at the end of the columns.
    EditFlow,
}

/// What the hint's button does besides dismissing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "BoardHintAction")]
pub enum Action {
    /// Got it: nothing more.
    Dismiss,
    /// Open <key>: the ticket opens.
    OpenTicket { key: String },
    /// Edit flow: the board turns into the editor.
    EditFlow,
}

/// The one first-run hint the board shows now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardHint")]
pub struct BoardHint {
    pub hint: View,
    pub anchor: Anchor,
    pub action: Action,
}

/// What the board's first-run hints are chosen from.
pub struct BoardFacts<'a> {
    pub tickets: &'a [Ticket],
    pub flow: &'a Flow,
    /// The hints the user dismissed.
    pub seen: &'a [Hint],
    /// A ticket was approved and reached Done.
    pub first_approve: bool,
    pub keys: &'a KeyLabels,
}

/// The first-run hint whose moment is on this board now, the most urgent first (a permission prompt, a question, a
/// checker's fail, a review, then "That was your flow" after the first approve); `None` when no unseen hint has its
/// moment here.
pub fn board(facts: &BoardFacts) -> Option<BoardHint> {
    let unseen = |hint: Hint| !facts.seen.contains(&hint);
    let first = |pred: &dyn Fn(&Ticket) -> bool| facts.tickets.iter().find(|t| pred(t));
    let card = |t: &Ticket| Anchor::Card {
        key: t.key.to_string(),
    };
    if unseen(Hint::PermissionPrompt)
        && let Some(t) = first(&|t| matches!(t.status, Status::NeedsYou | Status::CheckerNeedsYou))
    {
        return Some(BoardHint {
            hint: permission_prompt(),
            anchor: card(t),
            action: Action::Dismiss,
        });
    }
    if unseen(Hint::FirstQuestion)
        && let Some(t) = first(&|t| matches!(t.status, Status::AsksYou | Status::Idle))
    {
        return Some(BoardHint {
            hint: first_question(facts.keys),
            anchor: card(t),
            action: Action::Dismiss,
        });
    }
    if unseen(Hint::CheckerSentBack)
        && let Some(t) = first(&|t| {
            t.flags.validation_round.is_some()
                && t.status.set() == otaq_core::domain::StatusSet::AgentStage
        })
    {
        let check = t
            .flags
            .check
            .clone()
            .unwrap_or_else(|| "The checker".to_owned());
        let stage = t
            .place
            .stage_id()
            .and_then(|id| facts.flow.stage(id))
            .map_or("its work stage", |s| s.name.as_str());
        return Some(BoardHint {
            hint: checker_sent_back(&check, stage),
            anchor: card(t),
            action: Action::Dismiss,
        });
    }
    if unseen(Hint::FirstReview)
        && let Some(t) = first(&|t| t.status == Status::WaitingForYou)
    {
        let key = t.key.to_string();
        return Some(BoardHint {
            hint: first_review(&key),
            anchor: card(t),
            action: Action::OpenTicket { key },
        });
    }
    if facts.first_approve
        && unseen(Hint::ThatWasYourFlow)
        && let Some(t) = facts
            .tickets
            .iter()
            .filter(|t| t.place == Place::Done && t.status == Status::Merged)
            .max_by_key(|t| t.status_since.unwrap_or(t.updated_at))
    {
        return Some(BoardHint {
            hint: that_was_your_flow(&t.key.to_string(), facts.flow),
            anchor: Anchor::EditFlow,
            action: Action::EditFlow,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use otaq_core::domain::{Place, Status, ValidationRound};
    use otaq_core::flow::default::default_flow;

    use super::*;
    use crate::key_labels::fixtures;
    use crate::view_models::card_fixtures::ticket;

    #[test]
    fn every_stored_hint_has_a_page_id_with_the_same_name() {
        for &hint in Hint::ALL {
            let id = HintId::from(hint);
            assert_eq!(Hint::from(id), hint);
            assert_eq!(
                serde_json::to_value(id).unwrap(),
                serde_json::to_value(hint).unwrap()
            );
        }
    }

    #[test]
    fn a_hint_has_one_button_until_dismissed() {
        let keys = fixtures::base();
        let shown = shown(HintId::GallerySample, &[], &keys).unwrap();
        assert_eq!(shown.button, "Got it");
        assert_eq!(shown.text.matches(". ").count(), 0, "one sentence");
        assert!(shown.text.ends_with('.'));
        assert_eq!(
            super::shown(HintId::GallerySample, &[Hint::GallerySample], &keys),
            None
        );
    }

    fn in_stage(key: &str, status: Status, stage: &str) -> Ticket {
        Ticket {
            place: Place::stage_of(stage),
            ..ticket(key, "Tidy the README", status)
        }
    }

    fn hint_on(tickets: &[Ticket], seen: &[Hint], first_approve: bool) -> Option<BoardHint> {
        let flow = default_flow();
        let keys = fixtures::base();
        board(&BoardFacts {
            tickets,
            flow: &flow,
            seen,
            first_approve,
            keys: &keys,
        })
    }

    #[test]
    fn each_first_run_hint_waits_for_its_moment_on_the_board() {
        let working = in_stage("SHOP-1", Status::Working, "dev");
        assert_eq!(
            hint_on(std::slice::from_ref(&working), &[], false),
            None,
            "no moment yet"
        );

        let asks = in_stage("SHOP-1", Status::AsksYou, "dev");
        let hint = hint_on(&[asks], &[], false).unwrap();
        assert_eq!(hint.hint.id, HintId::FirstQuestion);
        assert_eq!(
            hint.anchor,
            Anchor::Card {
                key: "SHOP-1".into()
            }
        );
        assert_eq!(hint.hint.title.as_deref(), Some("An agent needs you"));

        let prompt = in_stage("SHOP-1", Status::NeedsYou, "dev");
        assert_eq!(
            hint_on(&[prompt], &[], false).unwrap().hint.id,
            HintId::PermissionPrompt
        );

        let mut fixing = working.clone();
        fixing.flags.validation_round = Some(ValidationRound { round: 1, of: 3 });
        fixing.flags.check = Some("Validate".into());
        let hint = hint_on(&[fixing], &[], false).unwrap();
        assert_eq!(hint.hint.id, HintId::CheckerSentBack);
        assert!(
            hint.hint.text.starts_with(
                "Validate tested the change and found a problem, so the ticket went back to Dev"
            ),
            "{}",
            hint.hint.text
        );

        let review = in_stage("SHOP-1", Status::WaitingForYou, "review");
        let hint = hint_on(&[review], &[], false).unwrap();
        assert_eq!(hint.hint.id, HintId::FirstReview);
        assert_eq!(hint.hint.button, "Open SHOP-1");
        assert_eq!(
            hint.action,
            Action::OpenTicket {
                key: "SHOP-1".into()
            }
        );
    }

    #[test]
    fn a_dismissed_hint_never_shows_again() {
        let review = in_stage("SHOP-2", Status::WaitingForYou, "review");
        assert_eq!(hint_on(&[review], &[Hint::FirstReview], false), None);
        let asks = in_stage("SHOP-3", Status::AsksYou, "dev");
        assert_eq!(hint_on(&[asks], Hint::FIRST_RUN, true), None);
    }

    #[test]
    fn that_was_your_flow_follows_the_first_approve_at_edit_flow() {
        let done = Ticket {
            place: Place::Done,
            ..ticket("SHOP-1", "Tidy the README", Status::Merged)
        };
        assert_eq!(
            hint_on(std::slice::from_ref(&done), &[], false),
            None,
            "not before the first approve"
        );
        let hint = hint_on(&[done], &[], true).unwrap();
        assert_eq!(hint.anchor, Anchor::EditFlow);
        assert_eq!(hint.action, Action::EditFlow);
        assert_eq!(hint.hint.button, "Edit flow");
        assert!(
            hint.hint.text.starts_with(
                "SHOP-1 went Backlog \u{2192} Dev \u{2192} Validate \u{2192} Review \u{2192} Done."
            ),
            "{}",
            hint.hint.text
        );
    }

    #[test]
    fn the_most_urgent_moment_goes_first() {
        let tickets = [
            in_stage("SHOP-1", Status::WaitingForYou, "review"),
            in_stage("SHOP-2", Status::AsksYou, "dev"),
        ];
        assert_eq!(
            hint_on(&tickets, &[], false).unwrap().hint.id,
            HintId::FirstQuestion
        );
        assert_eq!(
            hint_on(&tickets, &[Hint::FirstQuestion], false)
                .unwrap()
                .hint
                .id,
            HintId::FirstReview
        );
    }
}
