//! How a ticket in Done ended (`tickets.outcome`): merged, closed, or finished without a merge.

use super::stored_enum;

stored_enum! {
    /// How a ticket in Done ended, the stored `tickets.outcome`. Its status says whether it counts as merged:
    /// [`Outcome::Merged`] and [`Outcome::DoneWithoutMerge`] are [`super::Status::Merged`], so tickets waiting on
    /// them start; [`Outcome::Closed`] is Closed · not merged.
    pub enum Outcome ("outcome") {
        /// Local merge, an automatic Delivery or Mark merged: a merge commit landed (or the PR was merged).
        Merged = "merged",
        /// Close ticket, Mark closed, or an agent's `stage done`: abandoned, never counts as merged.
        Closed = "closed",
        /// The user finished the ticket from the Approve menu without a merge ("Done, nothing to merge"): no merge
        /// commit, the branch is kept, and it counts as merged. The card reads "Done · nothing to merge" when the
        /// branch had nothing beyond its base (`nothing_to_merge` is set), else "Done · not merged".
        DoneWithoutMerge = "done_without_merge",
    }
}
