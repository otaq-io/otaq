//! What a hand-off typed into a live session is (d0118): the card says "Sent back · waiting for the agent to pick it
//! up" until the agent's tool reports activity ([`crate::session::hand_offs`]).

use serde::{Deserialize, Serialize};

/// The kind of hand-off Otaq typed into a running agent session: the card's words for it while the agent hasn't
/// picked it up ([`crate::domain::Flags::hand_off`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandOffKind {
    /// Send back, a decision's Change…, Send back with PR comments, a drag back, Apply now: `address-review`.
    SendBack,
    /// A merge conflict at the Delivery: `restack`.
    MergeConflict,
    /// The Merge check failed on the merged result (d0120): `restack` with its comment.
    MergeCheck,
    /// Open PR into the live session that delivers.
    OpenPr,
    /// Push to PR into the live session that delivers.
    Push,
}

impl HandOffKind {
    /// Every kind, in the order above.
    pub const ALL: [Self; 5] = [
        Self::SendBack,
        Self::MergeConflict,
        Self::MergeCheck,
        Self::OpenPr,
        Self::Push,
    ];

    /// The hand-off a returning skill carries ([`crate::review::ADDRESS_REVIEW_SKILL`],
    /// [`crate::review::RESTACK_SKILL`]); `None` for any other skill.
    pub fn of_skill(skill: &str) -> Option<Self> {
        match skill {
            crate::review::ADDRESS_REVIEW_SKILL => Some(Self::SendBack),
            crate::review::RESTACK_SKILL => Some(Self::MergeConflict),
            _ => None,
        }
    }
}
