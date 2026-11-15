//! The comment box's writing helpers (tuq 111, plan comments-as-markdown piece 7): the box stays a text box holding
//! Markdown, with GitHub's helpers around it. Two tabs over it, **Write** and **Preview** (the comment drawn as the
//! thread will draw it), and a toolbar over the text whose buttons add Markdown marks: bold, italic, code, link, the
//! three lists and quote. Bold, italic, code and link have a shortcut (⌘B, ⌘I, ⌘E, ⌘K; Ctrl on Windows and Linux),
//! which the tooltip names; while the box has the focus those keys are the box's, not the app's.

use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::keymap::Platform;

/// What a toolbar button does to the text: the page's `markdownEdits.ts` makes the edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "CommentFormat")]
pub enum Format {
    Bold,
    Italic,
    Code,
    Link,
    BulletList,
    NumberedList,
    TaskList,
    Quote,
}

impl Format {
    /// The buttons in the order they are drawn.
    pub const ALL: [Format; 8] = [
        Format::Bold,
        Format::Italic,
        Format::Code,
        Format::Link,
        Format::BulletList,
        Format::NumberedList,
        Format::TaskList,
        Format::Quote,
    ];

    fn label(self) -> &'static str {
        match self {
            Format::Bold => "Bold",
            Format::Italic => "Italic",
            Format::Code => "Code",
            Format::Link => "Link",
            Format::BulletList => "Bulleted list",
            Format::NumberedList => "Numbered list",
            Format::TaskList => "Task list",
            Format::Quote => "Quote",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Format::Bold => IconName::Bold,
            Format::Italic => IconName::Italic,
            Format::Code => IconName::Code,
            Format::Link => IconName::Link,
            Format::BulletList => IconName::List,
            Format::NumberedList => IconName::ListOrdered,
            Format::TaskList => IconName::ListTodo,
            Format::Quote => IconName::TextQuote,
        }
    }

    /// The letter of its shortcut, with ⌘ (Ctrl on Windows and Linux); the lists and quote have none.
    pub fn letter(self) -> Option<char> {
        match self {
            Format::Bold => Some('B'),
            Format::Italic => Some('I'),
            Format::Code => Some('E'),
            Format::Link => Some('K'),
            _ => None,
        }
    }
}

/// One toolbar button: its tooltip's name, its icon and its shortcut as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentFormatButton")]
pub struct FormatButton {
    pub format: Format,
    /// The tooltip and the screen reader's name: "Bold".
    pub label: String,
    pub icon: IconName,
    /// After the name in the tooltip: "⌘B" on a Mac, "Ctrl+B" on Windows and Linux; none for the lists and quote.
    pub keys: Option<String>,
}

/// The tabs and the toolbar over the comment box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentWritingView")]
pub struct WritingView {
    /// The tab list's name, for screen readers: "Comment".
    pub tabs: String,
    /// "Write": the text box.
    pub write: String,
    /// "Preview": the comment as the thread will draw it.
    pub preview: String,
    /// What Preview shows for an empty box: "Nothing to preview".
    pub nothing_to_preview: String,
    /// The toolbar's name, for screen readers: "Formatting".
    pub toolbar: String,
    pub buttons: Vec<FormatButton>,
}

impl WritingView {
    pub fn new(platform: Platform) -> Self {
        Self {
            tabs: "Comment".to_owned(),
            write: "Write".to_owned(),
            preview: "Preview".to_owned(),
            nothing_to_preview: "Nothing to preview".to_owned(),
            toolbar: "Formatting".to_owned(),
            buttons: Format::ALL
                .iter()
                .map(|&format| FormatButton {
                    format,
                    label: format.label().to_owned(),
                    icon: format.icon(),
                    keys: format.letter().map(|letter| match platform {
                        Platform::Mac => format!("\u{2318}{letter}"),
                        _ => format!("Ctrl+{letter}"),
                    }),
                })
                .collect(),
        }
    }

    /// The words on this build's platform.
    pub fn current() -> Self {
        Self::new(Platform::current())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toolbar_has_eight_buttons_and_four_shortcuts() {
        let view = WritingView::new(Platform::Mac);
        assert_eq!(
            (view.write.as_str(), view.preview.as_str()),
            ("Write", "Preview")
        );
        let names: Vec<(&str, Option<&str>)> = view
            .buttons
            .iter()
            .map(|b| (b.label.as_str(), b.keys.as_deref()))
            .collect();
        assert_eq!(
            names,
            [
                ("Bold", Some("⌘B")),
                ("Italic", Some("⌘I")),
                ("Code", Some("⌘E")),
                ("Link", Some("⌘K")),
                ("Bulleted list", None),
                ("Numbered list", None),
                ("Task list", None),
                ("Quote", None),
            ]
        );
    }

    #[test]
    fn windows_and_linux_print_ctrl() {
        let view = WritingView::new(Platform::WinLinux);
        let keys: Vec<Option<&str>> = view.buttons.iter().map(|b| b.keys.as_deref()).collect();
        assert_eq!(
            keys,
            [
                Some("Ctrl+B"),
                Some("Ctrl+I"),
                Some("Ctrl+E"),
                Some("Ctrl+K"),
                None,
                None,
                None,
                None
            ]
        );
    }
}
