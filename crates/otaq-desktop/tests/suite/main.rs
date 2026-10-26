//! The integration tests of `otaq-desktop` as one test program (decision 0056, SDD §23.2): one module per file, so the
//! crate is linked once rather than once per file. Run them with `cargo nextest run -p otaq-desktop`; one file with
//! `cargo nextest run -p otaq-desktop -E 'test(/^<file>::/)'`.

mod notify_check;
mod one_copy;
mod one_door;
