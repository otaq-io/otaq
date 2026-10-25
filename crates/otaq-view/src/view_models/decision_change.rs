//! Change… on a recorded decision (DEC-06; d0084, d0034, d0080; *07 → Change… on SHOP-23*, *Change… on 40.1's
//! decision: add a fix child*). On a ticket in review, "Change this decision" sends it back to its agent with the
//! other option (editable), pending line comments included. Every word in the dialog is decided here.

use otaq_core::board::tickets::Ticket;
use otaq_core::decisions::Decision;
use serde::Serialize;
use ts_rs::TS;

/// "Change this decision": Send back with the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DecisionSendBackView")]
pub struct SendBackChange {
    /// `SHOP-23#1`, what `decision.change` takes.
    pub id: String,
    /// The ticket that goes back to Dev.
    pub key: String,
    pub title: &'static str,
    pub question: String,
    /// "Built: only in the profile form · Other option: everywhere".
    pub summary: String,
    pub label: &'static str,
    /// The first other option, editable.
    pub text: String,
    pub help: &'static str,
    /// "This is a Send back (0034): SHOP-23 goes back to Dev with …".
    pub note: String,
    /// The pending line comments that go with it, for the unsent-text question's wording.
    pub comments: u32,
    pub confirm: &'static str,
    pub cancel: &'static str,
    pub hint: &'static str,
}

/// The dialog Change… opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "DecisionChangeView")]
pub enum ChangeView {
    SendBack(SendBackChange),
}

pub const SEND_BACK_TITLE: &str = "Change this decision";

/// "Built: newest first · Other option: in the order they were added" ("Other options: a · b" for several).
pub fn summary(decision: &Decision) -> String {
    let mut out = format!("Built: {}", decision.built.trim());
    match decision.other_options.as_slice() {
        [] => {}
        [one] => out.push_str(&format!(" \u{b7} Other option: {}", one.trim())),
        many => out.push_str(&format!(
            " \u{b7} Other options: {}",
            many.iter()
                .map(|o| o.trim())
                .collect::<Vec<_>>()
                .join(" \u{b7} ")
        )),
    }
    out
}

/// The dialog for `decision` of `ticket`. `comments` the pending line comments that go with a Send back.
pub fn view(decision: &Decision, ticket: &Ticket, comments: u32) -> ChangeView {
    ChangeView::SendBack(SendBackChange {
        id: decision.id.to_string(),
        key: ticket.key.to_string(),
        title: SEND_BACK_TITLE,
        question: decision.question.clone(),
        summary: summary(decision),
        label: "SEND BACK TO THE AGENT WITH",
        text: decision
            .other_options
            .first()
            .map(|o| o.trim().to_owned())
            .unwrap_or_default(),
        help: "Prefilled with the other option. Edit it or write your own.",
        note: format!(
            "This is a Send back: {} goes back to Dev with this message and any line comments you\u{2019}ve \
             written, and its agent continues in the same session. Its next submit is validated again.",
            ticket.key
        ),
        comments,
        confirm: "Send back to the agent",
        cancel: "Cancel",
        hint: "cancel",
    })
}

/// The Gallery's dialog (`gallery.decision_change`): SHOP-23's Send back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DecisionChangeGallery")]
pub struct Gallery {
    pub send_back: ChangeView,
}

pub fn gallery() -> Gallery {
    let (shop23, _) = fixtures::shop23();
    Gallery {
        send_back: view(
            &crate::view_models::decision_item::fixtures::full_name(),
            &shop23,
            0,
        ),
    }
}

/// SHOP-23 in review.
pub mod fixtures {
    use otaq_core::board::tickets::Ticket;
    use otaq_core::domain::Status;

    use crate::view_models::card_fixtures::ticket;

    pub fn shop23() -> (Ticket, Vec<Ticket>) {
        let mut t = ticket("SHOP-23", "Split user full name", Status::WaitingForYou);
        t.place = otaq_core::domain::Place::stage_of("review");
        (t.clone(), vec![t])
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::view_models::decision_item::fixtures::full_name;

    fn send_back(v: ChangeView) -> SendBackChange {
        let ChangeView::SendBack(s) = v;
        s
    }

    /// *07 → Change… on SHOP-23*.
    #[test]
    fn change_on_a_ticket_is_prefilled_with_the_other_option() {
        let (shop23, tickets) = shop23();
        let _ = tickets;
        let v = send_back(view(&full_name(), &shop23, 2));
        assert_eq!(v.id, "SHOP-23#1");
        assert_eq!(v.key, "SHOP-23");
        assert_eq!(v.title, "Change this decision");
        assert_eq!(
            v.question,
            "Split fullName everywhere, or only in the profile form?"
        );
        assert_eq!(
            v.summary,
            "Built: only in the profile form \u{b7} Other option: everywhere"
        );
        assert_eq!(v.label, "SEND BACK TO THE AGENT WITH");
        assert_eq!(v.text, "everywhere");
        assert_eq!(
            v.help,
            "Prefilled with the other option. Edit it or write your own."
        );
        assert!(
            v.note
                .starts_with("This is a Send back: SHOP-23 goes back to Dev"),
            "{}",
            v.note
        );
        assert_eq!(v.comments, 2);
        assert_eq!(v.confirm, "Send back to the agent");
    }

    #[test]
    fn several_other_options_are_listed() {
        let mut d = full_name();
        d.other_options = vec!["everywhere".into(), "nowhere".into()];
        assert_eq!(
            summary(&d),
            "Built: only in the profile form \u{b7} Other options: everywhere \u{b7} nowhere"
        );
    }

    #[test]
    fn the_gallery_has_the_send_back_dialog() {
        let g = gallery();
        assert_eq!(send_back(g.send_back).key, "SHOP-23");
    }
}
