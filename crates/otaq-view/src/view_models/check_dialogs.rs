//! The dialog of an open PR (FLW-16, CMT-13; d0108, d0109; `s03-50`): **Send back with PR comments** ("Send SHOP-26
//! back with PR comments?", the optional "Comment for Dev (optional)", **Send back**). It sends the ticket to the
//! nearest earlier Agent stage of its flow, whose session resumes; what the user writes is posted as a comment for that
//! stage, in the comment box (a long paste becomes its report, the limit applies); its request is
//! `ticket.send_back_pr`. A human check's Send back is `send_back.rs`'s dialog (d0093).

use otaq_core::board::tickets::Ticket;
use otaq_core::flow::engine::back_to;
use otaq_core::flow::{Flow, Stage};
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::ticket_comments::CommentBoxView;

/// The line under a dialog's comment box: "Posted as a comment for Dev · every later stage reads it" (CMT-13).
pub fn posted_for(stage: &str) -> String {
    format!("Posted as a comment for {stage} \u{b7} every later stage reads it")
}

/// "Dev (Rust developer)".
fn bracketed(stage: &Stage) -> String {
    match stage.has_agent() {
        true => format!("{} ({})", stage.name, stage.agents_line()),
        false => stage.name.clone(),
    }
}

/// The comment box's placeholder (the drag-back dialog's words, `s03-50`).
pub const WHAT_SHOULD_CHANGE: &str = "What should change? Every later stage reads it too.";

/// The Send back with PR comments dialog (`s03-50` → *Sending an open PR back*). `TicketSendBackPrView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TicketSendBackPrView")]
pub struct SendBackPrView {
    /// "Send SHOP-26 back with PR comments?"
    pub title: String,
    /// "The comments on PR #481 go to Dev (Rust developer) with the ticket."
    pub text: String,
    /// "Comment for Dev (optional)", "If you write one, it's posted as a comment for Dev".
    pub comment: CommentBoxView,
    pub confirm: &'static str,
    pub cancel: &'static str,
    pub hint: &'static str,
}

/// The dialog of `ticket`, whose PR is open in `flow`'s Delivery stage. `None` without a PR or an Agent stage before
/// the Delivery.
pub fn send_back_pr_view(ticket: &Ticket, flow: &Flow) -> Option<SendBackPrView> {
    let n = ticket.flags.pr_number?;
    let delivery = flow.stages.iter().find(|s| s.is_delivery())?;
    let to = back_to(flow, &delivery.id, &[])?;
    Some(SendBackPrView {
        title: format!("Send {} back with PR comments?", ticket.key),
        text: format!(
            "The comments on PR #{n} go to {} with the ticket.",
            bracketed(to)
        ),
        comment: CommentBoxView::dialog(
            format!("Comment for {} (optional)", to.name),
            format!(
                "If you write one, it\u{2019}s posted as a comment for {}",
                to.name
            ),
            true,
        )
        .placeholder("Anything to add to the PR\u{2019}s comments?"),
        confirm: "Send back",
        cancel: "Cancel",
        hint: "cancel",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::{Place, Status};

    use crate::view_models::flow_columns::every_kind;

    fn at(key: &str, status: Status, stage: &str) -> Ticket {
        let mut t = ticket(key, "t", status);
        t.place = Place::of(status, Some(stage), None);
        t
    }

    #[test]
    fn send_back_with_pr_comments_names_the_pr_and_the_agent_stage() {
        let mut t = at("SHOP-26", Status::PrOpen, "ship");
        t.flags.pr_number = Some(481);
        let view = send_back_pr_view(&t, &every_kind()).unwrap();
        assert_eq!(view.title, "Send SHOP-26 back with PR comments?");
        assert_eq!(
            view.text,
            "The comments on PR #481 go to Dev (Rust developer) with the ticket."
        );
        assert_eq!(
            view.comment.label.as_deref(),
            Some("Comment for Dev (optional)")
        );
        assert_eq!(
            view.comment.placeholder,
            "Anything to add to the PR\u{2019}s comments?"
        );
        assert_eq!(
            view.comment.footer.as_deref(),
            Some("If you write one, it\u{2019}s posted as a comment for Dev")
        );
        assert!(view.comment.optional);
        assert_eq!(view.confirm, "Send back");
        t.flags.pr_number = None;
        assert_eq!(
            send_back_pr_view(&t, &every_kind()),
            None,
            "no PR, no dialog"
        );
    }
}
