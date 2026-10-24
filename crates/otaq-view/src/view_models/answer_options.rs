//! Answer options (ATT-12; d0061, d0048, d0016; *10 → Answer · inline, Answer · options, no recommendation*): when an
//! `ask human` question carries 2–4 options, Needs you, the card and the sheet list them numbered, the recommended one
//! marked and **preselected**; **1–4** pick (the last number is **Other…**, a free-text field), **Enter** sends. Without a
//! recommendation nothing is preselected and Send stays disabled until the user picks. The answer is typed as
//! [`otaq_core::board::questions::format_answer`] words it. One model for all three places.
//!
//! Each option's note (ATT-17, d0061), its pros and cons, rides on its row whole ([`OptionRow::note`]): never cut,
//! shown under the label in Needs you and the sheet, in the light hover card on the card.
//!
//! A question an epic's lead passed on (LEAD-06, d0083; *10 → Row · Epic lead items (0083)*): the lead's option is the
//! recommended one, marked "Recommended by the lead" with the lead's reason under it, after the option's own note.

use otaq_core::board::questions::{Choice, HumanQuestion};

/// What is picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// One of the agent's options, 1-based.
    Option(u32),
    /// "Other… type your own answer".
    Other,
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionRow {
    /// The number to press (1–4; Other… is the last).
    pub number: u32,
    pub label: String,
    pub recommended: bool,
    pub picked: bool,
    pub other: bool,
    /// The option's own note, its pros and cons (ATT-17): "Opens in any editor; one file per note, no attachments.".
    /// Whole, never cut. `None` for Other… and an option without one.
    pub note: Option<String>,
    /// The recommended row of a passed-on question: the lead's reason ("PAY-123 only mentions signed-in users."),
    /// shown after [`Self::note`].
    pub lead_reason: Option<String>,
}

/// The mark on the recommended option.
pub const RECOMMENDED: &str = "Recommended";
/// The mark when the epic's lead passed the question on (LEAD-06).
pub const RECOMMENDED_BY_LEAD: &str = "Recommended by the lead";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerOptions {
    /// Each option's label with its note (ATT-17), in order.
    options: Vec<(String, Option<String>)>,
    recommended: Option<u32>,
    /// The lead's reason for the recommended option, when the lead passed the question on.
    lead_reason: Option<String>,
    pub pick: Option<Pick>,
}

impl AnswerOptions {
    /// The options of a question, the recommended one preselected; `None` for a free-text question.
    pub fn of(question: &HumanQuestion) -> Option<Self> {
        if question.options.is_empty() {
            return None;
        }
        Some(Self {
            options: question
                .options
                .iter()
                .enumerate()
                .map(|(i, label)| (label.clone(), question.notes.get(i).cloned().flatten()))
                .collect(),
            recommended: question.recommended,
            lead_reason: question
                .via_lead
                .as_ref()
                .map(|v| v.reason.trim().to_owned())
                .filter(|r| !r.is_empty()),
            pick: question.recommended.map(Pick::Option),
        })
    }

    /// "Recommended", or "Recommended by the lead" on a passed-on question.
    pub fn recommended_label(question: &HumanQuestion) -> &'static str {
        if question.via_lead.is_some() {
            RECOMMENDED_BY_LEAD
        } else {
            RECOMMENDED
        }
    }

    /// The number of Other… (one after the last option).
    pub fn other_number(&self) -> u32 {
        self.options.len() as u32 + 1
    }

    /// A number key (1–4): `true` when it picked something.
    pub fn press(&mut self, n: u32) -> bool {
        if (1..=self.options.len() as u32).contains(&n) {
            self.pick = Some(Pick::Option(n));
            true
        } else if n == self.other_number() {
            self.pick = Some(Pick::Other);
            true
        } else {
            false
        }
    }

    pub fn rows(&self) -> Vec<OptionRow> {
        let mut rows: Vec<OptionRow> = self
            .options
            .iter()
            .enumerate()
            .map(|(i, (label, note))| {
                let n = i as u32 + 1;
                OptionRow {
                    number: n,
                    label: label.clone(),
                    recommended: self.recommended == Some(n),
                    picked: self.pick == Some(Pick::Option(n)),
                    other: false,
                    note: note.clone(),
                    lead_reason: self
                        .lead_reason
                        .clone()
                        .filter(|_| self.recommended == Some(n)),
                }
            })
            .collect();
        rows.push(OptionRow {
            number: self.other_number(),
            label: "Other… type your own answer".into(),
            recommended: false,
            picked: self.pick == Some(Pick::Other),
            other: true,
            note: None,
            lead_reason: None,
        });
        rows
    }

    /// What Enter or Send sends; `None` while nothing is picked, or Other… is picked with an empty field.
    pub fn choice(&self, other_text: &str) -> Option<Choice> {
        match self.pick? {
            Pick::Option(n) => Some(Choice::Option(n)),
            Pick::Other => {
                let text = other_text.trim();
                (!text.is_empty()).then(|| Choice::Other(text.to_owned()))
            }
        }
    }

    /// Send is enabled.
    pub fn can_send(&self, other_text: &str) -> bool {
        self.choice(other_text).is_some()
    }
}

/// "OPTIONS FROM SHOP-21’S AGENT".
pub fn heading(key: &impl std::fmt::Display) -> String {
    format!("OPTIONS FROM {key}’S AGENT")
}

/// The Gallery's questions with notes (ATT-17; *m26-tickets.md → The example*): NOTE-1's four options, one note
/// about 300 characters long and one option without a note; and a question an epic's lead passed on, with notes and
/// the lead's reason (CHANGES-2026-10-18-ab §1).
pub fn note_samples() -> [HumanQuestion; 2] {
    use otaq_core::board::questions::ViaLead;
    let note = |text: &str| Some(text.to_owned());
    let asks = HumanQuestion {
        id: "q-notes".into(),
        text: "Question 1 of 2: Which export format?".into(),
        options: vec![
            "Markdown files".into(),
            "One ZIP".into(),
            "Plain text".into(),
            "JSON".into(),
        ],
        recommended: Some(1),
        notes: vec![
            note("Opens in any editor; one file per note, no attachments."),
            note("One download with everything, but needs a ZIP library and an import later."),
            None,
            note(
                "Keeps every field (tags, dates, pinned state) so a later import can rebuild the notes exactly, \
                 and other tools can read it. Not readable for most people without a viewer, the files are larger, \
                 and the format becomes something we have to keep stable once users rely on it for backups.",
            ),
        ],
        asked_at: 0,
        via_lead: None,
    };
    let passed = HumanQuestion {
        id: "q-notes-lead".into(),
        text: "Which invoice export should the first version have?".into(),
        options: vec!["CSV only".into(), "CSV and PDF".into()],
        recommended: Some(1),
        notes: vec![
            note("One small library, about half a day. No printable copy for customers."),
            note(
                "Printable invoices too, but needs a page layout and a PDF library: about two days.",
            ),
        ],
        asked_at: 0,
        via_lead: Some(ViaLead {
            lead: "Sharing lead".into(),
            reason: "NOTE-10's description asks for a spreadsheet export only.".into(),
        }),
    };
    [asks, passed]
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::board::questions::format_answer;

    fn question(options: &[&str], recommended: Option<u32>) -> HumanQuestion {
        HumanQuestion {
            id: "q".into(),
            text: "Which email provider should the order confirmation use?".into(),
            options: options.iter().map(|o| (*o).to_owned()).collect(),
            recommended,
            notes: Vec::new(),
            asked_at: 0,
            via_lead: None,
        }
    }

    #[test]
    fn the_recommended_option_is_preselected_and_enter_sends_it() {
        let q = question(&["Postmark", "SES", "Resend"], Some(1));
        let o = AnswerOptions::of(&q).unwrap();
        let rows = o.rows();
        let shown: Vec<(u32, &str, bool, bool)> = rows
            .iter()
            .map(|r| (r.number, r.label.as_str(), r.recommended, r.picked))
            .collect();
        assert_eq!(
            shown,
            [
                (1, "Postmark", true, true),
                (2, "SES", false, false),
                (3, "Resend", false, false),
                (4, "Other… type your own answer", false, false),
            ]
        );
        let choice = o.choice("").unwrap();
        assert_eq!(
            format_answer(&q, &choice),
            "Answer to your question: Postmark (option 1)."
        );
    }

    #[test]
    fn number_keys_pick_and_other_needs_text() {
        let q = question(&["Postmark", "SES", "Resend"], Some(1));
        let mut o = AnswerOptions::of(&q).unwrap();
        assert!(o.press(3));
        assert_eq!(o.choice(""), Some(Choice::Option(3)));
        assert!(!o.press(5), "only 1–4");
        assert!(o.press(4));
        assert_eq!(o.pick, Some(Pick::Other));
        assert!(!o.can_send("  "));
        assert_eq!(o.choice(" Mailgun "), Some(Choice::Other("Mailgun".into())));
    }

    #[test]
    fn without_a_recommendation_nothing_is_picked_and_send_waits() {
        let q = question(&["Split it everywhere", "Only in the profile form"], None);
        let mut o = AnswerOptions::of(&q).unwrap();
        assert_eq!(o.pick, None);
        assert!(!o.can_send(""));
        assert!(o.rows().iter().all(|r| !r.recommended && !r.picked));
        assert_eq!(
            o.rows().last().unwrap().number,
            3,
            "Other… is 3 with two options"
        );
        assert!(o.press(2));
        assert!(o.can_send(""));
    }

    #[test]
    fn number_keys_reach_the_list_only_when_the_terminal_has_no_focus() {
        use crate::keymap::{KeyPress, KeyRoute, Platform, bindings, route};
        let one = KeyPress {
            key: "1".into(),
            code: "Digit1".into(),
            modifiers: Default::default(),
        };
        let map = bindings(Platform::Mac);
        // SES-10: with the terminal focused, "1" is typed into Claude Code; with the list focused, it picks.
        assert_eq!(route(Platform::Mac, true, &one, &map), KeyRoute::Terminal);
        assert_eq!(route(Platform::Mac, false, &one, &map), KeyRoute::App);
    }

    /// *10 → Row · Epic lead items (0083)*: the lead's option is preselected, "Recommended by the lead" with its reason.
    #[test]
    fn a_passed_on_question_shows_the_leads_recommendation_and_reason() {
        let mut q = question(
            &["Logged-in users only", "Guests too, kept in the browser"],
            Some(1),
        );
        assert_eq!(AnswerOptions::recommended_label(&q), "Recommended");
        q.via_lead = Some(otaq_core::board::questions::ViaLead {
            lead: "Wishlist lead".into(),
            reason: "PAY-123 only mentions signed-in users.".into(),
        });
        assert_eq!(
            AnswerOptions::recommended_label(&q),
            "Recommended by the lead"
        );
        let rows = AnswerOptions::of(&q).unwrap().rows();
        assert!(rows[0].picked && rows[0].recommended);
        assert_eq!(
            rows[0].lead_reason.as_deref(),
            Some("PAY-123 only mentions signed-in users.")
        );
        assert_eq!(rows[1].lead_reason, None);
        assert_eq!(
            (rows[2].number, rows[2].label.as_str()),
            (3, "Other… type your own answer")
        );
    }

    /// ATT-17: each row carries its option's note whole; Other… and an option without one have none.
    #[test]
    fn each_row_carries_its_options_note() {
        let mut q = question(&["Markdown files", "One ZIP", "PDF"], Some(1));
        let long = "z".repeat(600);
        q.notes = vec![
            Some("Opens in any editor; one file per note, no attachments.".into()),
            None,
            Some(long.clone()),
        ];
        let rows = AnswerOptions::of(&q).unwrap().rows();
        let notes: Vec<Option<&str>> = rows.iter().map(|r| r.note.as_deref()).collect();
        assert_eq!(
            notes,
            [
                Some("Opens in any editor; one file per note, no attachments."),
                None,
                Some(long.as_str()),
                None
            ]
        );
        assert!(
            rows.iter().all(|r| r.lead_reason.is_none()),
            "the lead's reason stays apart"
        );
        q.notes = Vec::new();
        assert!(
            AnswerOptions::of(&q)
                .unwrap()
                .rows()
                .iter()
                .all(|r| r.note.is_none())
        );
    }

    /// ATT-17 on a passed-on question (d0083): the option's note and the lead's reason are two fields on the
    /// recommended row, the note first; the other rows keep their own notes.
    #[test]
    fn a_passed_on_question_keeps_the_note_and_the_leads_reason_apart() {
        let mut q = question(&["Markdown files", "One ZIP"], Some(2));
        q.notes = vec![
            Some("Opens in any editor; one file per note, no attachments.".into()),
            Some(
                "One download with everything, but needs a ZIP library and an import later.".into(),
            ),
        ];
        q.via_lead = Some(otaq_core::board::questions::ViaLead {
            lead: "Sharing lead".into(),
            reason: "  NOTE-12 imports a ZIP already.  ".into(),
        });
        let rows = AnswerOptions::of(&q).unwrap().rows();
        let shown: Vec<(Option<&str>, Option<&str>)> = rows
            .iter()
            .map(|r| (r.note.as_deref(), r.lead_reason.as_deref()))
            .collect();
        assert_eq!(
            shown,
            [
                (
                    Some("Opens in any editor; one file per note, no attachments."),
                    None
                ),
                (
                    Some(
                        "One download with everything, but needs a ZIP library and an import later."
                    ),
                    Some("NOTE-12 imports a ZIP already.")
                ),
                (None, None),
            ]
        );
    }

    #[test]
    fn free_text_questions_have_no_list() {
        assert!(AnswerOptions::of(&question(&[], None)).is_none());
        assert_eq!(heading(&"SHOP-21"), "OPTIONS FROM SHOP-21’S AGENT");
    }
}
