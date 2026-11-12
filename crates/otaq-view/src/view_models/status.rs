//! How a ticket's status (the core's `Status`, SDD §7.2) is drawn: which entry of the *Status Legend* it uses
//! (colour + icon) and its label. Colour is never the only signal: every pill has an icon and a label.

use otaq_core::domain::Status as Core;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::theme::{Palette, Status as Legend};

/// The status a ticket shows: a Delivery stop that kept the ticket where it was (a hook's refusal, a push that didn't
/// land) shows as any Delivery stop, Needs you; otherwise its own status. A focused terminal changes nothing here:
/// being in the agent's terminal holds back only the alert, so a waiting agent reads Needs you on the card like the
/// sheet's pill. A session's permission prompt shown without changing the ticket's status (a delivery skill's, in
/// Review) reads Needs you, as any prompt. A finished ticket's answer session at a prompt doesn't: the ticket keeps its
/// finished status (its card's line says "Needs you · answering SHOP-36").
pub fn shown(ticket: &otaq_core::board::tickets::Ticket) -> Core {
    if ticket.prompt_in_place().is_some() {
        Core::NeedsYou
    } else if ticket.stopped_in_place().is_some() {
        Core::DeliveryStopped
    } else {
        ticket.status
    }
}

/// The legend entry and the label for a status.
pub fn pill(status: Core) -> (Legend, &'static str) {
    let legend = match status {
        Core::Backlog => Legend::Backlog,
        Core::Parked => Legend::Parked,
        Core::Queued | Core::WaitingForSlot | Core::Held => Legend::Queued,
        Core::ParentClosed => Legend::ParentClosed,
        Core::StartInterrupted => Legend::StartInterrupted,
        Core::Working => Legend::AgentWorking,
        Core::WorkingInBackground => Legend::WorkingInBackground,
        Core::AsksYou => Legend::AsksYou,
        Core::NeedsYou => Legend::NeedsYou,
        Core::Idle => Legend::Idle,
        Core::WaitingForLimitReset => Legend::WaitingForLimitReset,
        Core::Interrupted | Core::ResumeFailed => Legend::Interrupted,
        Core::WaitingForYou => Legend::ReadyForReview,
        Core::ToDeliver => Legend::ReadyForReview,
        // Waiting for another ticket to land, as a PR waits for its merge.
        Core::PrOpen | Core::MergedInto => Legend::PrOpen,
        Core::Merged => Legend::Merged,
        Core::ClosedNotMerged => Legend::ClosedNotMerged,
        // *Status Legend → Pills · M12* (d0063, d0064).
        Core::Checking => Legend::Validating,
        Core::MergeConflictFixing => Legend::MergeConflictFixing,
        Core::CheckerNeedsYou => Legend::ValidatorNeedsYou,
        Core::CheckFailed => Legend::ValidationFailed,
        Core::CheckNoResult => Legend::ValidationNoResult,
        Core::DeliveryStopped => Legend::AutoMergeStopped,
    };
    let label = match status {
        // The legend's name for a working agent.
        Core::Working => legend.label(),
        other => other.label(),
    };
    (legend, label)
}

/// A Status Pill as the page draws it (*01 · Components → Primitives → Status Pill*): the legend entry's colour
/// pair, its icon and the label. `PillView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct PillView {
    pub palette: Palette,
    pub icon: IconName,
    pub label: String,
}

impl PillView {
    /// A legend entry with its own label, or a completed one ("Running :4020", "Conflicts with main").
    pub fn legend(legend: Legend, label: Option<&str>) -> Self {
        let (_, palette, icon) = legend.spec();
        Self {
            palette,
            icon,
            label: label.unwrap_or(legend.label()).to_owned(),
        }
    }

    /// A ticket's status, as [`pill`] draws it.
    pub fn of(status: Core) -> Self {
        let (legend, label) = pill(status);
        Self::legend(legend, Some(label))
    }

    /// The pill a ticket shows ([`shown`]): a ticket finished without a merge keeps Merged's colour and icon and
    /// says how it ended, as its card ("Done · nothing to merge").
    pub fn of_ticket(ticket: &otaq_core::board::tickets::Ticket) -> Self {
        let status = shown(ticket);
        let mut pill = Self::of(status);
        if status == Core::Merged && ticket.status == Core::Merged {
            ticket.status_words().clone_into(&mut pill.label);
        }
        pill
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Tier;

    #[test]
    fn every_status_has_an_icon_and_a_label() {
        for status in Core::ALL {
            let (legend, label) = pill(*status);
            assert!(!label.is_empty(), "{status}");
            let tier = legend.spec().0;
            if status.is_needs_you_family() || *status == Core::Interrupted {
                assert_eq!(tier, Tier::NeedsYou, "{status} must stand out");
            }
        }
        assert_eq!(pill(Core::Working), (Legend::AgentWorking, "Working"));
        assert_eq!(pill(Core::Held).1, "Held");
        assert_eq!(pill(Core::ResumeFailed).1, "Resume failed");
        assert_eq!(
            pill(Core::ClosedNotMerged),
            (Legend::ClosedNotMerged, "Closed · not merged")
        );
    }

    #[test]
    fn the_page_gets_the_legend_colours_icon_and_label() {
        assert_eq!(
            PillView::of(Core::Working),
            PillView {
                palette: Palette::Working,
                icon: IconName::Activity,
                label: "Working".into(),
            }
        );
        let running = PillView::legend(Legend::Running, Some("Serving :4020"));
        assert_eq!(
            (running.palette, running.label.as_str()),
            (Palette::Approved, "Serving :4020")
        );
        assert_eq!(
            serde_json::to_value(PillView::of(Core::NeedsYou)).unwrap(),
            serde_json::json!({ "palette": "attn", "icon": "hand", "label": "Needs you" })
        );
    }

    /// The M12 states use their own pills (*Pills · M12*): icon + label, never colour alone.
    #[test]
    fn m12_states_have_their_own_pills() {
        use crate::icons::IconName as I;
        let cases = [
            (
                Core::Checking,
                Legend::Validating,
                "Checking",
                I::ShieldEllipsis,
            ),
            (
                Core::CheckerNeedsYou,
                Legend::ValidatorNeedsYou,
                "Needs you · checker",
                I::Hand,
            ),
            (
                Core::CheckFailed,
                Legend::ValidationFailed,
                "Check failed",
                I::ShieldX,
            ),
            (
                Core::CheckNoResult,
                Legend::ValidationNoResult,
                "Check ended without a result",
                I::ShieldQuestionMark,
            ),
            (
                Core::MergeConflictFixing,
                Legend::MergeConflictFixing,
                "Merge conflict · fixing",
                I::GitCompareArrows,
            ),
            (
                Core::DeliveryStopped,
                Legend::AutoMergeStopped,
                "Delivery stopped",
                I::GitCompareArrows,
            ),
        ];
        for (status, legend, label, icon) in cases {
            assert_eq!(pill(status), (legend, label), "{status}");
            assert_eq!(legend.spec().2, icon, "{status}");
            let calm = matches!(status, Core::Checking | Core::MergeConflictFixing);
            assert_eq!(legend.spec().0 == Tier::Working, calm, "{status}");
        }
    }
}
