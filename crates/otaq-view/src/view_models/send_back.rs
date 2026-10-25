//! The Send back dialog (REV-04, CMT-13; d0034, d0048, d0058, d0109, *07 → Send back · review & confirm* `s07-20` /
//! `s07-21`): what the agent will receive (every pending line comment), the optional comment for Dev in the comment box
//! (a long paste becomes its report), which tickets keep waiting, and the request it sends. Sending with no line
//! comments and no comment is allowed: the comment is optional.

use otaq_core::agents::facts::TicketAgents;
use otaq_core::board::comments::{ReviewComment, Side};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Status, TicketKey};
use otaq_core::flow::engine::fail_to;
use otaq_core::flow::{Flow, Stage};
use otaq_core::protocol::{FailParams, Request};

use crate::view_models::check_dialogs::posted_for;
use crate::view_models::ticket_comments::CommentBoxView;

/// Whether Send back is offered: a ticket (never an epic) in Ready for Review.
pub fn available(ticket: &Ticket) -> bool {
    !ticket.is_epic() && ticket.status == Status::WaitingForYou
}

/// What the dialog lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendBackPlan {
    /// "Send back to Product spec" (d0093).
    pub title: String,
    pub intro: String,
    /// "api/checkout/route.ts · L9" and the comment.
    pub comments: Vec<(String, String)>,
    /// Tickets that keep waiting until this one is merged: key, "Checkout page · Queued".
    pub waiting: Vec<(TicketKey, String)>,
}

/// The dialog of `ticket` going back to the Agent stage `to` ("Product spec", [`back_stage`]).
pub fn plan(
    ticket: &Ticket,
    pending: &[ReviewComment],
    tickets: &[Ticket],
    to: &str,
) -> SendBackPlan {
    let comments = pending
        .iter()
        .filter(|c| c.is_pending())
        .map(|c| {
            let side = if c.side == Side::Old {
                " (removed line)"
            } else {
                ""
            };
            (
                format!("{} \u{b7} L{}{side}", c.file, c.line),
                c.body.clone(),
            )
        })
        .collect();
    let mut waiting: Vec<(TicketKey, String)> = tickets
        .iter()
        .filter(|t| t.depends_on.contains(&ticket.key) && t.status != Status::Merged)
        .map(|t| {
            (
                t.key.clone(),
                format!("{} \u{b7} {}", t.title, t.status.label()),
            )
        })
        .collect();
    waiting.sort();
    SendBackPlan {
        title: title(to),
        intro: format!(
            "{} goes straight back to its agent in {to} (Working) and continues in the same session.",
            ticket.key
        ),
        comments,
        waiting,
    }
}

/// The dialog's title and its confirm (d0093): "Send back to Product spec".
pub fn title(to: &str) -> String {
    format!("Send back to {to}")
}

/// The human check's button (d0093, d0131): "Send back · to Product spec…", "Send back (2) · to Dev…" with the
/// line comments it would send (REV-04).
pub fn button_label(to: &str, pending: usize) -> String {
    match pending {
        0 => format!("Send back \u{b7} to {to}\u{2026}"),
        n => format!("Send back ({n}) \u{b7} to {to}\u{2026}"),
    }
}

/// The Agent stage a Send back returns `ticket` to, as the flow engine says (d0131): its human check's fails-go-to
/// stage, or, when the ticket skips that stage (`skips`, [`skips`]), the nearest Agent stage before the check that it
/// runs.
pub fn back_stage<'f>(ticket: &Ticket, flow: &'f Flow, skips: &[String]) -> Option<&'f Stage> {
    let here = flow.stage(ticket.place.stage()?)?;
    fail_to(flow, &here.id, skips)
}

/// The stages the ticket skips by its choices (AGM-08), from its path in `agents` (`ticket.agents`).
pub fn skips(agents: Option<&TicketAgents>) -> Vec<String> {
    agents.map_or_else(Vec::new, |a| {
        a.path
            .iter()
            .filter(|s| s.skipped)
            .map(|s| s.stage_id.clone())
            .collect()
    })
}

/// "Then Security runs again before it comes back to you.": the agent checks between `to` and the human check the
/// ticket waits in, which the card walks again; `None` with none in between.
pub fn runs_again(ticket: &Ticket, flow: &Flow, to: &Stage, skips: &[String]) -> Option<String> {
    let here = flow.stage(ticket.place.stage()?)?;
    let (from, until) = (flow.position(&to.id)?, flow.position(&here.id)?);
    let checks: Vec<&str> = flow
        .stages
        .get(from + 1..until)?
        .iter()
        .filter(|s| s.is_agent_check() && !skips.contains(&s.id))
        .map(|s| s.name.as_str())
        .collect();
    let (runs, names) = match checks.as_slice() {
        [] => return None,
        [one] => ("runs", (*one).to_owned()),
        [rest @ .., last] => ("run", format!("{} and {last}", rest.join(", "))),
    };
    Some(format!(
        "Then {names} {runs} again before it comes back to you."
    ))
}

/// The comment box of a Send back to `stage` (`s07-20`): "Comment for Dev (optional)", "Posted as a comment for Dev ·
/// every later stage reads it".
pub fn comment_box(stage: &str) -> CommentBoxView {
    CommentBoxView::dialog(
        format!("Comment for {stage} (optional)"),
        posted_for(stage),
        true,
    )
    .placeholder("Anything that isn\u{2019}t tied to a line")
}

/// "These keep waiting until SHOP-18 is merged".
pub fn waiting_title(key: &TicketKey) -> String {
    format!("These keep waiting until {key} is merged")
}

/// The request; `clear_first` is the unsent-text prompt's "Clear it and send"; `report` the text pasted into the
/// comment box as its report (CMT-13; empty: none).
pub fn request(key: &TicketKey, note: &str, report: Option<&str>, clear_first: bool) -> Request {
    Request::TicketFail(FailParams {
        key: key.to_string(),
        note: note.trim().to_owned(),
        clear_first,
        report: pasted_report(report),
    })
}

/// The text pasted into a dialog's note as its report (CMT-13); none when empty.
pub fn pasted_report(text: Option<&str>) -> Option<otaq_core::board::reports::ReportSource> {
    text.filter(|t| !t.trim().is_empty())
        .map(|t| otaq_core::board::reports::ReportSource::Text(t.to_owned()))
}

/// What the unsent-text prompt says Send back would type.
pub fn sending(comments: usize) -> String {
    match comments {
        0 => "Your review (the review feedback skill)".to_owned(),
        1 => "Your review: 1 comment (the review feedback skill)".to_owned(),
        n => format!("Your review: {n} comments (the review feedback skill)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use crate::view_models::flow_columns::every_kind;

    fn comment(file: &str, line: u32, side: Side, body: &str) -> ReviewComment {
        ReviewComment {
            id: body.into(),
            round: 1,
            file: file.into(),
            line,
            side,
            body: body.into(),
            created_at: 0,
            sent_at: None,
        }
    }

    #[test]
    fn the_dialog_lists_comments_and_waiting_tickets() {
        let shop18 = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        let mut shop20 = ticket("SHOP-20", "Checkout page", Status::Queued);
        shop20.depends_on = vec![shop18.key.clone()];
        let mut shop21 = ticket("SHOP-21", "Order confirmation email", Status::Queued);
        shop21.depends_on = vec![shop18.key.clone()];
        let other = ticket("SHOP-22", "Other", Status::Queued);
        let p = plan(
            &shop18,
            &[
                comment(
                    "api/checkout/route.ts",
                    9,
                    Side::New,
                    "Return { code, message }",
                ),
                comment("api/checkout/route.ts", 4, Side::Old, "Why removed?"),
            ],
            &[shop21.clone(), shop20.clone(), other],
            "Dev",
        );
        assert_eq!(p.title, "Send back to Dev");
        assert_eq!(
            p.intro,
            "SHOP-18 goes straight back to its agent in Dev (Working) and continues in the same session."
        );
        assert_eq!(
            p.comments,
            [
                (
                    "api/checkout/route.ts \u{b7} L9".to_owned(),
                    "Return { code, message }".to_owned()
                ),
                (
                    "api/checkout/route.ts \u{b7} L4 (removed line)".to_owned(),
                    "Why removed?".to_owned()
                ),
            ]
        );
        assert_eq!(
            p.waiting,
            [
                (
                    shop20.key.clone(),
                    "Checkout page \u{b7} Ready for Dev".to_owned()
                ),
                (
                    shop21.key.clone(),
                    "Order confirmation email \u{b7} Ready for Dev".to_owned()
                ),
            ]
        );
        assert_eq!(
            waiting_title(&shop18.key),
            "These keep waiting until SHOP-18 is merged"
        );
    }

    /// `s07-20`: the comment for the stage it goes back to, posted for every later stage.
    #[test]
    fn the_comment_box_names_the_stage_it_goes_back_to() {
        let flow = otaq_core::flow::default_flow();
        let mut t = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        t.place = otaq_core::domain::Place::of(Status::WaitingForYou, Some("review"), None);
        let to = back_stage(&t, &flow, &[]).unwrap();
        assert_eq!(to.name, "Dev");
        let b = comment_box(&to.name);
        assert_eq!(b.label.as_deref(), Some("Comment for Dev (optional)"));
        assert_eq!(
            b.footer.as_deref(),
            Some("Posted as a comment for Dev \u{b7} every later stage reads it")
        );
        assert!(b.optional);
        assert_eq!(b.submit, None);
        assert_eq!(b.footer_icon, crate::icons::IconName::MessageSquare);
        assert_eq!(b.limits.chars, 2_000);
    }

    fn waiting_in(stage: &str) -> Ticket {
        let mut t = ticket("SHOP-40", "Order history page", Status::WaitingForYou);
        t.place = otaq_core::domain::Place::of(Status::WaitingForYou, Some(stage), None);
        t
    }

    /// d0093, d0131: the button names the stage the engine sends the ticket to: the nearest Agent stage (the default
    /// flow's Review → Dev), the check's own `fail_to` (QA → Grill), and the nearest one the ticket runs when it skips
    /// that stage (→ Dev).
    #[test]
    fn the_button_names_where_the_fails_go() {
        let default = otaq_core::flow::default_flow();
        let review = waiting_in("review");
        let to = back_stage(&review, &default, &[]).unwrap();
        assert_eq!(button_label(&to.name, 0), "Send back \u{b7} to Dev\u{2026}");
        assert_eq!(
            button_label(&to.name, 2),
            "Send back (2) \u{b7} to Dev\u{2026}"
        );

        let flow = every_kind();
        let qa = waiting_in("qa");
        let to = back_stage(&qa, &flow, &[]).unwrap();
        assert_eq!(
            button_label(&to.name, 0),
            "Send back \u{b7} to Grill\u{2026}",
            "not the nearest (Dev)"
        );
        let skipping = vec!["grill".to_owned()];
        let to = back_stage(&qa, &flow, &skipping).unwrap();
        assert_eq!(button_label(&to.name, 0), "Send back \u{b7} to Dev\u{2026}");
        assert_eq!(
            back_stage(&ticket("SHOP-1", "t", Status::WaitingForYou), &flow, &[]),
            None,
            "no stage"
        );
    }

    /// The agent checks the card walks again on its way back to the human check, without the ones it skips.
    #[test]
    fn the_checks_that_run_again() {
        let flow = every_kind();
        let qa = waiting_in("qa");
        let grill = flow.stage("grill").unwrap();
        assert_eq!(
            runs_again(&qa, &flow, grill, &[]).as_deref(),
            Some("Then Validate and Security run again before it comes back to you.")
        );
        let dev = flow.stage("dev").unwrap();
        assert_eq!(
            runs_again(&qa, &flow, dev, &["security".to_owned()]).as_deref(),
            Some("Then Validate runs again before it comes back to you.")
        );
        let default = otaq_core::flow::default_flow();
        let review = waiting_in("review");
        let dev = back_stage(&review, &default, &[]).unwrap();
        assert_eq!(
            runs_again(&review, &default, dev, &[]).as_deref(),
            Some("Then Validate runs again before it comes back to you.")
        );
    }

    /// The skips come from the ticket's path (AGM-08).
    #[test]
    fn the_skips_are_the_path_s() {
        use otaq_core::agents::facts::{PathKind, PathStage};
        let stage = |id: &str, skipped| PathStage {
            stage_id: id.into(),
            stage: id.into(),
            kind: PathKind::Agent,
            shown_in: None,
            rounds: None,
            delivery: None,
            agent: None,
            skipped,
        };
        let agents = TicketAgents {
            key: None,
            kind: otaq_core::domain::Kind::Ticket,
            status: None,
            agents: Vec::new(),
            stages: Vec::new(),
            lead: None,
            sessions: Vec::new(),
            tokens: Vec::new(),
            holds: Vec::new(),
            place: None,
            path: vec![stage("grill", true), stage("dev", false)],
            runs: Vec::new(),
            choices: Vec::new(),
        };
        assert_eq!(skips(Some(&agents)), ["grill"]);
        assert!(skips(None).is_empty());
    }

    #[test]
    fn only_tickets_in_review_and_never_epics() {
        assert!(available(&ticket("SHOP-18", "t", Status::WaitingForYou)));
        assert!(!available(&ticket("SHOP-18", "t", Status::Working)));
        let mut epic = ticket("SHOP-40", "Wishlist", Status::WaitingForYou);
        epic.kind = otaq_core::domain::Kind::Epic;
        assert!(!available(&epic));
    }

    #[test]
    fn an_empty_send_back_is_allowed() {
        let key: TicketKey = "SHOP-18".parse().unwrap();
        assert_eq!(
            request(&key, "  ", Some(""), false),
            Request::TicketFail(FailParams {
                key: "SHOP-18".into(),
                note: String::new(),
                clear_first: false,
                report: None,
            })
        );
        assert!(matches!(
            request(&key, "x", Some("log"), true),
            Request::TicketFail(FailParams {
                report: Some(otaq_core::board::reports::ReportSource::Text(_)),
                ..
            })
        ));
        assert!(matches!(
            request(&key, "x", None, true),
            Request::TicketFail(FailParams {
                clear_first: true,
                ..
            })
        ));
        assert_eq!(sending(0), "Your review (the review feedback skill)");
        assert_eq!(
            sending(2),
            "Your review: 2 comments (the review feedback skill)"
        );
    }
}
