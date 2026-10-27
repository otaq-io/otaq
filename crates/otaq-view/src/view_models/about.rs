//! Settings → About (d0071, d0076; *15 · Settings → Row · About & open-source licenses*): the mark, the name and the
//! line under it, the version with the commit it was built from and **Copy version info**, Otaq's own license with
//! **View license**, the **Open-source licenses** and **Theme credits** rows, and the trademark line. The version,
//! the commit, the OS and the license text come from the desktop app; this only words them.
//!
//! Under the version row: the Updates rows (0140, page 32 items 1–2), then **Use otaq in your own terminal**
//! ([`path_row`], page 32 item 5) with **Add to my shell's PATH**.

use serde::Serialize;
use ts_rs::TS;

use otaq_core::naming::{COMMAND_NAME, PRODUCT_NAME};

use crate::view_models::theme_credits;

/// The line under the name (0071).
pub const TAGLINE: &str = "Run your coding agents from one board";

/// What the build and the machine say about this copy of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    /// `0.1.0`.
    pub version: String,
    /// The short commit it was built from; `unknown` outside a git checkout.
    pub commit: String,
    /// `macOS 26.0`, `Windows 11 (26100)`, `Ubuntu 26.04`.
    pub os: String,
    /// `arm64` or `x64`, as people call them (see [`arch_name`]).
    pub arch: String,
    /// A release: built by the release workflow with `OTAQ_RELEASE=1` (d0140). Every other build is built from source.
    pub release: bool,
}

/// *Settings · About*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AboutView")]
pub struct View {
    pub product: String,
    pub tagline: String,
    /// "Version 0.1.0 · build", followed by the commit chip.
    pub version: String,
    pub commit: String,
    pub version_description: String,
    /// "Built from source · bed9c10. …" for a build that isn't a release (d0140, page 32 item 1); `None` for a release.
    pub built_from_source: Option<String>,
    pub copy_label: String,
    pub copied_label: String,
    /// What Copy version info puts on the clipboard: "Otaq 0.1.0 (bed9c10) · macOS 26.0 · arm64".
    pub version_info: String,
    /// "© 2026 The Otaq Authors · FSL-1.1-MIT".
    pub copyright: String,
    pub license_description: String,
    pub view_license_label: String,
    pub licenses_label: String,
    pub licenses_description: String,
    pub view_licenses_label: String,
    pub credits_label: String,
    pub credits_description: String,
    pub view_credits_label: String,
    pub trademark: String,
}

/// *View license · dialog*: Otaq's own license text (FSL-1.1-MIT).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OwnLicenseDialog")]
pub struct LicenseView {
    pub title: String,
    pub description: String,
    /// The text in paragraphs, each with its hard line breaks joined.
    pub paragraphs: Vec<String>,
    /// What Copy license puts on the clipboard: the file as it is.
    pub text: String,
    pub copy_label: String,
    pub copied_label: String,
    pub done_label: String,
}

/// Where **Add to my shell's PATH** writes (page 32 item 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathTarget {
    /// The login shell's files as the row names them: `~/.zshrc`, or `~/.bashrc` and `~/.bash_profile`.
    Files(Vec<String>),
    /// The user's own PATH on Windows, for this account only.
    Windows,
    /// A login shell the button doesn't write for (tcsh, nushell…): only the folder, to add by hand.
    OtherShell,
}

/// The row's state, from the shell side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathState {
    Offer,
    Added,
    AlreadyThere,
    /// The file (or Windows' PATH) couldn't be written: `line` to paste.
    Failed {
        file: Option<String>,
        read_only: bool,
        why: String,
        line: String,
    },
}

/// What the row is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathInputs {
    pub target: PathTarget,
    pub state: PathState,
    /// The folder as the row names it: `~/.otaq/bin`.
    pub folder: String,
}

/// The row's look: the offer (a description and the button), or a result line with its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AboutPathTone")]
pub enum PathTone {
    Offer,
    Done,
    Failed,
}

/// *Use otaq in your own terminal* (page 32 item 5): under the Updates rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AboutPathRow")]
pub struct PathRow {
    pub label: String,
    pub tone: PathTone,
    /// The offer's description, under the label.
    pub description: Option<String>,
    /// **Add to my shell's PATH**, while it is offered.
    pub button: Option<String>,
    /// The result line, after its icon: "Added · open a new terminal to use otaq".
    pub result: Option<String>,
    /// Under the result line.
    pub detail: Option<String>,
    /// The line to paste, with **Copy**.
    pub paste: Option<String>,
    pub copy_label: String,
    pub copied_label: String,
}

/// "~/.zshrc", "~/.bashrc and ~/.bash_profile".
fn and_files(files: &[String]) -> String {
    match files {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The row for `inputs` (page 32 item 5's words).
pub fn path_row(inputs: &PathInputs) -> PathRow {
    let cmd = COMMAND_NAME;
    let folder = &inputs.folder;
    let mut row = PathRow {
        label: format!("Use {cmd} in your own terminal"),
        tone: PathTone::Done,
        description: None,
        button: None,
        result: None,
        detail: None,
        paste: None,
        copy_label: "Copy".into(),
        copied_label: "Copied".into(),
    };
    match (&inputs.state, &inputs.target) {
        (PathState::Offer, target) => {
            row.tone = PathTone::Offer;
            let example = format!("{cmd} attach SHOP-12");
            row.description = Some(match target {
                PathTarget::Files(files) => format!(
                    "To run commands like {example} in your own terminal, {PRODUCT_NAME} can add its folder to {}. Agents don’t need it.",
                    and_files(files)
                ),
                PathTarget::Windows => format!(
                    "To run commands like {example} in PowerShell or Command Prompt, {PRODUCT_NAME} can add its folder to your PATH, for this account only. Agents don’t need it."
                ),
                PathTarget::OtherShell => format!(
                    "To run commands like {example} in your own terminal, add {folder} to your shell’s PATH. Agents don’t need it."
                ),
            });
            match target {
                PathTarget::OtherShell => row.paste = Some(folder.clone()),
                _ => row.button = Some("Add to my shell’s PATH".into()),
            }
        }
        (PathState::Added, target) => {
            row.result = Some(format!("Added · open a new terminal to use {cmd}"));
            row.detail = Some(match target {
                PathTarget::Files(files) => format!(
                    "One line marked “{}” is at the end of {}. Uninstalling {PRODUCT_NAME} removes it.",
                    added_by(),
                    and_files(files)
                ),
                _ => format!(
                    "{folder} is at the end of your PATH, for this account only. Uninstalling {PRODUCT_NAME} removes it."
                ),
            });
        }
        (PathState::AlreadyThere, _) => {
            row.result = Some(format!("{cmd} works in your terminal"));
            row.detail = Some(format!("{folder} is already on your PATH."));
        }
        (
            PathState::Failed {
                file,
                read_only,
                why,
                line,
            },
            _,
        ) => {
            row.tone = PathTone::Failed;
            let reason = if *read_only {
                "The file is read-only.".to_owned()
            } else {
                format!("{}.", capitalized(why))
            };
            match file {
                Some(file) => {
                    row.result = Some(format!("{PRODUCT_NAME} couldn’t write to {file}"));
                    row.detail = Some(format!(
                        "{reason} Paste this line at its end, then open a new terminal:"
                    ));
                }
                None => {
                    row.result = Some(format!("{PRODUCT_NAME} couldn’t change your PATH"));
                    row.detail = Some(format!(
                        "{}. Add this folder to your PATH in Environment Variables, then open a new terminal:",
                        capitalized(why)
                    ));
                }
            }
            row.paste = Some(line.clone());
        }
    }
    row
}

/// The shell line's mark, as the row quotes it: "# added by Otaq".
fn added_by() -> String {
    format!("# added by {PRODUCT_NAME}")
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Page 32 item 1's line for a build that isn't a release: it never checks for updates and no release replaces it.
pub fn built_from_source(commit: &str) -> String {
    format!(
        "Built from source · {commit}. This build never checks for updates; a release never replaces it. Build it again to update."
    )
}

/// `std::env::consts::ARCH` as people call it: `aarch64` → `arm64`, `x86_64` → `x64`.
pub fn arch_name(arch: &str) -> &str {
    match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    }
}

/// The license file's copyright holder line, as the About row shows it: "© 2026 The Otaq Authors · FSL-1.1-MIT".
pub fn copyright(license: &str) -> String {
    let holder = license
        .lines()
        .find_map(|l| l.trim().strip_prefix("Copyright "))
        .map(|h| h.strip_prefix("(c) ").unwrap_or(h))
        .unwrap_or(PRODUCT_NAME);
    format!("© {holder} · FSL-1.1-MIT")
}

/// The About section for this build. `rust_crates` and `npm_packages` are the notices file's counts.
pub fn view(build: &Build, license: &str, rust_crates: usize, npm_packages: usize) -> View {
    let name = PRODUCT_NAME;
    View {
        product: name.into(),
        tagline: TAGLINE.into(),
        version: format!("Version {} · build", build.version),
        commit: build.commit.clone(),
        version_description:
            "For bug reports: copies the version, the build commit, your OS and its architecture."
                .into(),
        built_from_source: (!build.release).then(|| built_from_source(&build.commit)),
        copy_label: "Copy version info".into(),
        copied_label: "Copied".into(),
        version_info: format!(
            "{name} {} ({}) · {} · {}",
            build.version, build.commit, build.os, build.arch
        ),
        copyright: copyright(license),
        license_description: format!(
            "{name} itself is free to use under the Functional Source License (FSL-1.1-MIT). Each version becomes MIT two years after its release."
        ),
        view_license_label: "View license".into(),
        licenses_label: "Open-source licenses".into(),
        licenses_description: format!(
            "{name} ships with {rust_crates} open-source Rust crates, {npm_packages} npm packages, fonts, icons and syntax definitions."
        ),
        view_licenses_label: "View".into(),
        credits_label: "Theme credits".into(),
        credits_description: theme_credits::ROW_DESCRIPTION.into(),
        view_credits_label: "View credits".into(),
        trademark: format!(
            "Claude and Claude Code are trademarks of Anthropic. {name} is an independent project and isn’t affiliated with or endorsed by Anthropic."
        ),
    }
}

/// *View license · dialog* for the license file `text`. Its Markdown heading marks are dropped.
pub fn license(text: &str) -> LicenseView {
    let paragraphs = text
        .split("\n\n")
        .map(|p| {
            p.trim_start_matches('#')
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|p| !p.is_empty())
        .collect();
    LicenseView {
        title: "Functional Source License".into(),
        description: format!(
            "{PRODUCT_NAME}’s own license. It applies to the app and its source code."
        ),
        paragraphs,
        text: text.to_owned(),
        copy_label: "Copy license".into(),
        copied_label: "Copied".into(),
        done_label: "Done".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LICENSE: &str = "# Functional Source License, Version 1.1, MIT Future License\n\n## Notice\n\nCopyright 2026 The Example Authors\n\nSubject to your compliance with this License Grant,\nwe hereby grant you\n";

    fn build() -> Build {
        Build {
            version: "0.1.0".into(),
            commit: "bed9c10".into(),
            os: "macOS 26.0".into(),
            arch: arch_name("aarch64").into(),
            release: true,
        }
    }

    #[test]
    fn the_rows_as_the_frame_words_them() {
        let v = view(&build(), LICENSE, 364, 9);
        assert_eq!(v.product, PRODUCT_NAME);
        assert_eq!(v.version, "Version 0.1.0 · build");
        assert_eq!(v.commit, "bed9c10");
        assert_eq!(v.copyright, "© 2026 The Example Authors · FSL-1.1-MIT");
        assert_eq!(
            v.licenses_description,
            format!(
                "{PRODUCT_NAME} ships with 364 open-source Rust crates, 9 npm packages, fonts, icons and syntax definitions."
            )
        );
        assert!(v.trademark.starts_with(&format!(
            "Claude and Claude Code are trademarks of Anthropic. {PRODUCT_NAME} is"
        )));
    }

    #[test]
    fn a_build_from_source_says_so() {
        assert_eq!(view(&build(), LICENSE, 1, 1).built_from_source, None);
        let source = Build {
            release: false,
            ..build()
        };
        assert_eq!(
            view(&source, LICENSE, 1, 1).built_from_source.as_deref(),
            Some(
                "Built from source · bed9c10. This build never checks for updates; a release never replaces it. Build it again to update."
            )
        );
    }

    #[test]
    fn copy_version_info_names_version_commit_os_and_arch() {
        assert_eq!(
            view(&build(), LICENSE, 1, 1).version_info,
            format!("{PRODUCT_NAME} 0.1.0 (bed9c10) · macOS 26.0 · arm64")
        );
        assert_eq!(arch_name("x86_64"), "x64");
        assert_eq!(arch_name("riscv64"), "riscv64");
    }

    fn inputs(target: PathTarget, state: PathState) -> PathInputs {
        PathInputs {
            target,
            state,
            folder: "~/.d/bin".into(),
        }
    }

    fn zshrc() -> PathTarget {
        PathTarget::Files(vec!["~/.zshrc".into()])
    }

    #[test]
    fn the_path_row_as_page_32_words_it() {
        let cmd = COMMAND_NAME;
        let offer = path_row(&inputs(zshrc(), PathState::Offer));
        assert_eq!(offer.label, format!("Use {cmd} in your own terminal"));
        assert_eq!(offer.tone, PathTone::Offer);
        assert_eq!(
            offer.description.unwrap(),
            format!(
                "To run commands like {cmd} attach SHOP-12 in your own terminal, {PRODUCT_NAME} can add its folder to ~/.zshrc. Agents don’t need it."
            )
        );
        assert_eq!(offer.button.as_deref(), Some("Add to my shell’s PATH"));

        let added = path_row(&inputs(zshrc(), PathState::Added));
        assert_eq!(added.tone, PathTone::Done);
        assert_eq!(added.button, None);
        assert_eq!(
            added.result.unwrap(),
            format!("Added · open a new terminal to use {cmd}")
        );
        assert_eq!(
            added.detail.unwrap(),
            format!(
                "One line marked “# added by {PRODUCT_NAME}” is at the end of ~/.zshrc. Uninstalling {PRODUCT_NAME} removes it."
            )
        );

        let line = "export PATH=\"$HOME/.d/bin:$PATH\"".to_owned();
        let failed = path_row(&inputs(
            zshrc(),
            PathState::Failed {
                file: Some("~/.zshrc".into()),
                read_only: true,
                why: "permission denied".into(),
                line: line.clone(),
            },
        ));
        assert_eq!(failed.tone, PathTone::Failed);
        assert_eq!(
            failed.result.unwrap(),
            format!("{PRODUCT_NAME} couldn’t write to ~/.zshrc")
        );
        assert_eq!(
            failed.detail.unwrap(),
            "The file is read-only. Paste this line at its end, then open a new terminal:"
        );
        assert_eq!(failed.paste, Some(line));
        assert_eq!(failed.copy_label, "Copy");

        let already = path_row(&inputs(zshrc(), PathState::AlreadyThere));
        assert_eq!(
            already.result.unwrap(),
            format!("{cmd} works in your terminal")
        );
        assert_eq!(already.detail.unwrap(), "~/.d/bin is already on your PATH.");
        assert_eq!(already.button, None);
    }

    #[test]
    fn the_path_row_for_bash_windows_and_other_shells() {
        let bash = PathTarget::Files(vec!["~/.bashrc".into(), "~/.bash_profile".into()]);
        assert!(
            path_row(&inputs(bash, PathState::Offer))
                .description
                .unwrap()
                .contains("add its folder to ~/.bashrc and ~/.bash_profile.")
        );
        let windows = path_row(&inputs(PathTarget::Windows, PathState::Offer));
        assert_eq!(
            windows.description.unwrap(),
            format!(
                "To run commands like {COMMAND_NAME} attach SHOP-12 in PowerShell or Command Prompt, {PRODUCT_NAME} can add its folder to your PATH, for this account only. Agents don’t need it."
            )
        );
        assert_eq!(windows.button.as_deref(), Some("Add to my shell’s PATH"));
        let other = path_row(&inputs(PathTarget::OtherShell, PathState::Offer));
        assert_eq!(other.button, None);
        assert_eq!(other.paste.as_deref(), Some("~/.d/bin"));
        let refused = path_row(&inputs(
            PathTarget::Windows,
            PathState::Failed {
                file: None,
                read_only: false,
                why: "access is denied".into(),
                line: "C:\\d\\bin".into(),
            },
        ));
        assert!(refused.detail.unwrap().starts_with("Access is denied. "));
    }

    #[test]
    fn the_license_in_paragraphs_and_whole_for_the_clipboard() {
        let l = license(LICENSE);
        assert_eq!(l.paragraphs.len(), 4);
        assert_eq!(
            l.paragraphs[0],
            "Functional Source License, Version 1.1, MIT Future License"
        );
        assert_eq!(l.paragraphs[1], "Notice");
        assert_eq!(l.paragraphs[2], "Copyright 2026 The Example Authors");
        assert_eq!(
            l.paragraphs[3],
            "Subject to your compliance with this License Grant, we hereby grant you"
        );
        assert_eq!(l.text, LICENSE);
    }
}
