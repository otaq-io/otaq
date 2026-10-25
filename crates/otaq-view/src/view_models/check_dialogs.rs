//! The dialogs of a human check and an open PR (FLW-14, FLW-16, CMT-13; d0108, d0109; `s03-50`): **Fail** ("Fail Review
//! on SHOP-18?", "What's wrong? (a comment for Dev · Rust developer)", **Fail and send to Dev**) and **Send back with PR
//! comments** ("Send SHOP-26 back with PR comments?", the optional "Comment for Dev (optional)", **Send back**). Both
//! send the ticket to the nearest earlier Agent stage of its flow, whose session resumes; what the user writes is
//! posted as a comment for that stage, in the comment box (a long paste becomes its report, the limit applies), and
//! their requests are `ticket.fail` and `ticket.send_back_pr`.

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

/// "Dev · Rust developer": the stage and its agent, or the stage alone without one.
fn dotted(stage: &Stage) -> String {
    match stage.has_agent() {
        true => format!("{} \u{b7} {}", stage.name, stage.agents_line()),
        false => stage.name.clone(),
    }
}

/// "Dev (Rust developer)".
fn bracketed(stage: &Stage) -> String {
    match stage.has_agent() {
        true => format!("{} ({})", stage.name, stage.agents_line()),
        false => stage.name.clone(),
    }
}

/// "Security", "Validate and Security", "Validate, Security and Lint".
fn joined(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The Fail dialog (`s03-50` → *Fail asks for comments*). `TicketFailView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TicketFailView")]
pub struct FailView {
    /// "Fail Review on SHOP-18?"
    pub title: String,
    /// "What's wrong? (a comment for Dev · Rust developer)", the placeholder and "Posted as a comment for Dev ·
    /// every later stage reads it".
    pub comment: CommentBoxView,
    /// "Then Security runs again before it comes back to you."; `None` with no agent check in between.
    pub footnote: Option<String>,
    /// "Fail and send to Dev"
    pub confirm: String,
    pub cancel: &'static str,
    /// The `esc cancel` hint.
    pub hint: &'static str,
}

/// The comment box's placeholder (the drag-back dialog's words, `s03-50`).
pub const WHAT_SHOULD_CHANGE: &str = "What should change? Every later stage reads it too.";

/// The Fail dialog of `ticket`, waiting in a human check of `flow`. `None` when it isn't in a stage of the flow or no
/// Agent stage comes before it (nothing to send it to).
pub fn fail_view(ticket: &Ticket, flow: &Flow) -> Option<FailView> {
    let here = flow.stage(ticket.place.stage()?)?;
    let to = back_to(flow, &here.id, &[])?;
    let (from, until) = (flow.position(&to.id)?, flow.position(&here.id)?);
    let checks: Vec<&str> = flow.stages[from + 1..until]
        .iter()
        .filter(|s| s.is_agent_check())
        .map(|s| s.name.as_str())
        .collect();
    Some(FailView {
        title: format!("Fail {} on {}?", here.name, ticket.key),
        comment: CommentBoxView::dialog(
            format!("What\u{2019}s wrong? (a comment for {})", dotted(to)),
            posted_for(&to.name),
            true,
        )
        .placeholder(WHAT_SHOULD_CHANGE),
        footnote: (!checks.is_empty()).then(|| {
            let runs = if checks.len() == 1 { "runs" } else { "run" };
            format!(
                "Then {} {runs} again before it comes back to you.",
                joined(&checks)
            )
        }),
        confirm: format!("Fail and send to {}", to.name),
        cancel: "Cancel",
        hint: "cancel",
    })
}

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
    fn fail_names_the_check_the_agent_stage_and_what_runs_again() {
        let view = fail_view(&at("SHOP-18", Status::WaitingForYou, "qa"), &every_kind()).unwrap();
        assert_eq!(view.title, "Fail QA on SHOP-18?");
        assert_eq!(
            view.comment.label.as_deref(),
            Some("What\u{2019}s wrong? (a comment for Dev \u{b7} Rust developer)")
        );
        assert_eq!(
            view.comment.footer.as_deref(),
            Some("Posted as a comment for Dev \u{b7} every later stage reads it")
        );
        assert_eq!(
            view.comment.placeholder,
            "What should change? Every later stage reads it too."
        );
        assert!(view.comment.optional);
        assert_eq!(
            view.comment.submit, None,
            "the dialog's own button sends it"
        );
        assert_eq!(view.confirm, "Fail and send to Dev");
        assert_eq!(
            view.footnote.as_deref(),
            Some("Then Validate and Security run again before it comes back to you.")
        );

        // The default flow: Review fails to Dev, and Validate runs again (`s03-50` draws Security for shop-web).
        let flow = otaq_core::flow::default_flow();
        let view = fail_view(&at("NOTE-1", Status::WaitingForYou, "review"), &flow).unwrap();
        assert_eq!(view.title, "Fail Review on NOTE-1?");
        assert_eq!(
            view.comment.label.as_deref(),
            Some("What\u{2019}s wrong? (a comment for Dev \u{b7} Developer)")
        );
        assert_eq!(
            view.footnote.as_deref(),
            Some("Then Validate runs again before it comes back to you.")
        );
    }

    #[test]
    fn a_check_right_after_its_agent_stage_has_no_footnote() {
        let mut flow = otaq_core::flow::default_flow();
        flow.stages.retain(|s| s.id != "validate");
        let view = fail_view(&at("API-1", Status::WaitingForYou, "review"), &flow).unwrap();
        assert_eq!(view.footnote, None);
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
