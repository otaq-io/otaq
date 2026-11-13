//! Warm builds (SDD §9.6, d0082, d0085): a project's kept build output, the stock `<app-data>/warm/<repo-name>/<n>`,
//! that a new worktree is cloned from so its first build only redoes what changed.
//!
//! - [`clone`]: copy-on-write cloning and whether the data folder's disk can clone (WRM-01). A plain copy is never
//!   made.
//! - [`store`]: the stock and its entries, their records, the lock that lets a Clear wait for a copy, claims,
//!   replace (forward only), the trash, Clear and the size (WRM-02, WRM-05…WRM-07).
//! - [`keep`]: keeping a clean worktree's gitignored files at Done (WRM-03).
//! - [`seed`]: seeding a new worktree at start, with the timestamp trick (WRM-04).
//! - [`usage`]: what the last tickets used (decisions 054, 055): the usage ledger, arming and the read probe; a build
//!   handed on, and a copy of the user's checkout, carry only that.

pub mod clone;
pub(crate) mod keep;
pub mod seed;
pub mod store;
pub mod usage;
