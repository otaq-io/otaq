//! Otaq's screen logic as plain Rust, without Tauri (d0075, SDD §5.5): the view models with the theme data,
//! the keyboard map, the icon names and the diff highlighting they read. The desktop app draws what these say; the
//! tests here run without a display (SDD §23.1).

pub mod file_paths;
pub mod highlight;
pub mod icons;
pub mod key_labels;
pub mod keymap;
pub mod keymap_edit;
pub mod machine;
pub mod shell;
pub mod terminal;
pub mod theme;
pub mod view_models;
