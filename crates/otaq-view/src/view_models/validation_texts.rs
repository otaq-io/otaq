//! The sheet's and the editor's validation texts (VAL-06, VAL-07, VAL-08, VAL-14, AMG-03, AMG-04; d0063, d0064;
//! *05 · Ticket side panel → Action bar · every state, Row · validation (0063)*, *08 → Editing while it
//! validates*): the header's status label with its round, the token split, the Auto-merge stop message, the
//! Answer the agent box and the editor's warning.

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::validation::VALIDATING;
use otaq_core::domain::{AutoMergeStop, Status};
use otaq_core::usage::TicketUsage;

use crate::view_models::card::tokens_label;

/// The header pill's label when it says more than the status: "Validating… · round 1 of 3", "Validation failed 3
/// times".
pub fn pill_label(ticket: &Ticket, custom_flow: bool) -> Option<String> {
    use crate::view_models::card;
    let check = ticket.flags.check.as_deref();
    // A check of several agents (d0130, `s29-04`): "Validate · 2 check agents"; the round is in the sheet's block.
    if let Some(label) = crate::view_models::check_agents::header_label(ticket) {
        return Some(label);
    }
    match ticket.status {
        // Any agent check but the default flow's Validate reads "Checking" (FLW-13).
        Status::Checking => {
            let word = match check {
                Some(c) if c != card::VALIDATE => card::CHECKING,
                _ => VALIDATING,
            };
            ticket
                .flags
                .validating_round
                .map(|r| format!("{word} \u{b7} round {} of {}", r.round, r.of))
        }
        Status::CheckFailed => Some(card::check_failed_times(
            check,
            ticket.validation_round.max(1),
        )),
        // A human check in another flow than the default (FLW-14).
        Status::WaitingForYou if custom_flow => Some(card::YOUR_CHECK.to_owned()),
        _ => None,
    }
}

/// "agent 61k · validation 23k (2 rounds)" once a validator has run (VAL-14). The round being checked now counts
/// too, so a first round shows "(1 round)" while it runs.
pub fn token_split(ticket: &Ticket, usage: &TicketUsage) -> Option<String> {
    let validation = usage.validation?;
    let agent = usage.agent()?;
    let running = matches!(ticket.status, Status::Checking | Status::CheckerNeedsYou);
    let rounds = (usage.rounds + u32::from(running)).max(1);
    Some(format!(
        "agent {} \u{b7} validation {} ({rounds} {})",
        tokens_label(agent.shown()),
        tokens_label(validation.shown()),
        if rounds == 1 { "round" } else { "rounds" }
    ))
}

/// Why Auto-merge stopped, under the action bar (*Auto-merge stop · messages*): "Keeps conflicting with
/// otaq/SHOP-40 (3 tries in a row).", or the Local merge's own message.
pub fn stop_message(ticket: &Ticket) -> Option<String> {
    if ticket.status != Status::DeliveryStopped {
        return None;
    }
    if let (Some(AutoMergeStop::KeepsConflicting), Some(target)) =
        (ticket.auto_merge_stop, &ticket.flags.conflicts_with)
    {
        return Some(format!(
            "Keeps conflicting with {target} ({} tries in a row).",
            ticket.auto_merge_conflicts.max(1)
        ));
    }
    let text = ticket.signal.as_ref()?.text.as_deref()?.replace('`', "");
    Some(if text.ends_with('.') {
        text
    } else {
        format!("{text}.")
    })
}

/// The texts of the Answer the agent box (*SHOP-32 · Validation failed 3 times*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerAgentText {
    pub heading: &'static str,
    pub placeholder: &'static str,
    pub footer: String,
    pub cancel: &'static str,
    pub send: &'static str,
}

/// `rounds` is the project's Validation rounds: the counter restarts at 0 with that many new rounds.
pub fn answer_agent_text(ticket: &Ticket, rounds: u32) -> Option<AnswerAgentText> {
    (ticket.status == Status::CheckFailed).then(|| AnswerAgentText {
        heading: "Answer the agent",
        placeholder: "What the agent should do about the findings",
        footer: format!(
            "Delivered to {}\u{2019}s agent together with the round-{} findings. The round counter restarts at 0 ({} \
             new {}).",
            ticket.key,
            ticket.validation_round.max(1),
            rounds,
            if rounds == 1 { "round" } else { "rounds" }
        ),
        cancel: "Cancel",
        send: "Send to agent",
    })
}

/// The editor's warning while it validates (VAL-08): "SHOP-29’s validation will be cancelled and its agent told
/// what changed."
pub fn editor_warning(ticket: &Ticket) -> Option<String> {
    matches!(ticket.status, Status::Checking | Status::CheckerNeedsYou).then(|| {
        format!(
            "{}\u{2019}s validation will be cancelled and its agent told what changed.",
            ticket.key
        )
    })
}

/// Next to the description once it was changed, while it validates.
pub const UNSAVED_CANCELS: &str = "unsaved \u{b7} saving cancels the validation";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::ValidationRound;
    use otaq_core::usage::Tokens;

    fn tokens(n: u64) -> Option<Tokens> {
        Some(Tokens {
            input: n,
            ..Tokens::default()
        })
    }

    fn validating() -> Ticket {
        let mut t = ticket("SHOP-29", "Coupon field", Status::Checking);
        t.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
        t
    }

    #[test]
    fn the_header_names_the_round() {
        assert_eq!(
            pill_label(&validating(), false).as_deref(),
            Some("Validating\u{2026} \u{b7} round 1 of 3")
        );
        let mut failed = ticket("SHOP-32", "Address autocomplete", Status::CheckFailed);
        failed.validation_round = 3;
        assert_eq!(
            pill_label(&failed, false).as_deref(),
            Some("Validation failed 3 times")
        );
        assert_eq!(
            pill_label(&ticket("SHOP-1", "t", Status::Working), false),
            None
        );
    }

    #[test]
    fn the_token_split_counts_the_running_round() {
        let usage = TicketUsage {
            id: "x".into(),
            tokens: tokens(67_000),
            validation: tokens(9_000),
            rounds: 0,
            helpers: None,
        };
        assert_eq!(
            token_split(&validating(), &usage).as_deref(),
            Some("agent 58k \u{b7} validation 9k (1 round)")
        );
        let failed = ticket("SHOP-32", "t", Status::CheckFailed);
        let usage = TicketUsage {
            id: "x".into(),
            tokens: tokens(244_000),
            validation: tokens(41_000),
            rounds: 3,
            helpers: None,
        };
        assert_eq!(
            token_split(&failed, &usage).as_deref(),
            Some("agent 203k \u{b7} validation 41k (3 rounds)")
        );
        let none = TicketUsage {
            id: "x".into(),
            tokens: tokens(10),
            ..TicketUsage::default()
        };
        assert_eq!(token_split(&failed, &none), None, "no validator ran");
    }

    #[test]
    fn the_stop_messages_match_the_frame() {
        let mut t = ticket("SHOP-42", "Wishlist page", Status::DeliveryStopped);
        t.auto_merge_stop = Some(AutoMergeStop::KeepsConflicting);
        t.auto_merge_conflicts = 3;
        let target = otaq_core::naming::branch("SHOP-40");
        t.flags.conflicts_with = Some(target.clone());
        assert_eq!(
            stop_message(&t),
            Some(format!(
                "Keeps conflicting with {target} (3 tries in a row)."
            ))
        );
        t.auto_merge_stop = Some(AutoMergeStop::BaseDiverged);
        t.flags.conflicts_with = None;
        t.signal = Some(otaq_core::board::tickets::AgentSignal {
            text: Some(
                "`develop` and `origin/develop` have diverged. Push or pull, then retry".into(),
            ),
            ..Default::default()
        });
        assert_eq!(
            stop_message(&t).as_deref(),
            Some("develop and origin/develop have diverged. Push or pull, then retry.")
        );
        assert_eq!(stop_message(&validating()), None);
    }

    #[test]
    fn answer_the_agent_and_the_editor_warning() {
        let mut failed = ticket("SHOP-32", "Address autocomplete", Status::CheckFailed);
        failed.validation_round = 3;
        let text = answer_agent_text(&failed, 3).unwrap();
        assert_eq!(
            text.footer,
            "Delivered to SHOP-32\u{2019}s agent together with the round-3 findings. The round counter restarts \
             at 0 (3 new rounds)."
        );
        assert_eq!(text.send, "Send to agent");
        assert!(answer_agent_text(&validating(), 3).is_none());
        assert_eq!(
            editor_warning(&validating()).as_deref(),
            Some("SHOP-29\u{2019}s validation will be cancelled and its agent told what changed.")
        );
        assert_eq!(editor_warning(&failed), None);
    }
}
