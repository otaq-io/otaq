//! Answer options (ATT-12; d0061, d0048, d0016; *10 → Answer · inline, Answer · options, no recommendation*): when an
//! `ask human` question carries 2–4 options, Needs you, the card and the sheet list them numbered, the recommended one
//! marked and **preselected**; **1–4** pick (the last number is **Other…**, a free-text field), **Enter** sends. Without a
//! recommendation nothing is preselected and Send stays disabled until the user picks. The answer is typed as
//! [`otaq_core::board::questions::format_answer`] words it. One model for all three places.
//!
//! Each option's note (ATT-17, d0061), its pros and cons, rides on its row whole ([`OptionRow::note`]): never cut,
//! shown under the label in Needs you and the sheet, in the light hover card on the card.
//!
//! A question that takes several answers (`ask human --multiple`) draws its rows as checkboxes: the recommended ones
//! are ticked when it opens, a number key ticks or unticks its row (Other… too), and Send needs at least one ticked.
//! It sends every ticked option and Other…'s text ([`Choice::Several`]).
//!
//! An agent's question asking to send the ticket back to an earlier stage has its two fixed ways out as the rows:
//! **1 Send back to <stage>** and **2 Answer here…** (the free-text field). Nothing is preselected: the user decides.

use otaq_core::board::questions::{Choice, HumanQuestion};
use serde::Serialize;
use ts_rs::TS;

/// What is picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// One of the agent's options, 1-based.
    Option(u32),
    /// "Something else: type your own answer".
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
}

/// The mark on the recommended option.
pub const RECOMMENDED: &str = "Recommended";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerOptions {
    /// Each option's label with its note (ATT-17), in order.
    options: Vec<(String, Option<String>)>,
    recommended: Vec<u32>,
    /// What is picked, in the order it was picked: at most one unless the question takes several answers.
    pub picks: Vec<Pick>,
    /// The question takes several answers: a number key ticks or unticks its row, Other… among them.
    multiple: bool,
    /// A send-back question: option 1 sends the ticket back, Other… answers here.
    send_back: bool,
}

/// The free-text row of a send-back question.
pub const ANSWER_HERE: &str = "Answer here\u{2026} type your answer";

impl AnswerOptions {
    /// The options of a question, the recommended ones preselected; `None` for a free-text question.
    pub fn of(question: &HumanQuestion) -> Option<Self> {
        if let Some(to) = &question.send_back {
            return Some(Self {
                options: vec![(format!("Send back to {}", to.stage), None)],
                recommended: Vec::new(),
                picks: Vec::new(),
                multiple: false,
                send_back: true,
            });
        }
        if question.options.is_empty() {
            return None;
        }
        // A question that takes one answer preselects its one recommendation, whatever was stored.
        let recommended: Vec<u32> = if question.multiple {
            question.recommended.clone()
        } else {
            question.recommended.first().copied().into_iter().collect()
        };
        Some(Self {
            options: question
                .options
                .iter()
                .enumerate()
                .map(|(i, label)| (label.clone(), question.notes.get(i).cloned().flatten()))
                .collect(),
            picks: recommended.iter().copied().map(Pick::Option).collect(),
            recommended,
            multiple: question.multiple,
            send_back: false,
        })
    }

    /// "Recommended".
    pub fn recommended_label(_question: &HumanQuestion) -> &'static str {
        RECOMMENDED
    }

    /// The question takes several answers.
    pub fn multiple(&self) -> bool {
        self.multiple
    }

    /// Something is picked (the recommended options when the list opens).
    pub fn has_pick(&self) -> bool {
        !self.picks.is_empty()
    }

    /// The number of Other… (one after the last option).
    pub fn other_number(&self) -> u32 {
        self.options.len() as u32 + 1
    }

    fn pick_of(&self, n: u32) -> Option<Pick> {
        if (1..=self.options.len() as u32).contains(&n) {
            Some(Pick::Option(n))
        } else if n == self.other_number() {
            Some(Pick::Other)
        } else {
            None
        }
    }

    /// A number key (1–4): `true` when it picked something. On a question that takes several answers it ticks the
    /// row, or unticks it when it was ticked.
    pub fn press(&mut self, n: u32) -> bool {
        let Some(pick) = self.pick_of(n) else {
            return false;
        };
        if !self.multiple {
            self.picks = vec![pick];
        } else if let Some(at) = self.picks.iter().position(|p| *p == pick) {
            self.picks.remove(at);
        } else {
            self.picks.push(pick);
        }
        true
    }

    /// Picks exactly `numbers` (the page's ticked rows): `false` when one of them isn't a row, or several are given
    /// to a question that takes one answer.
    pub fn set(&mut self, numbers: &[u32]) -> bool {
        if numbers.len() > 1 && !self.multiple {
            return false;
        }
        let picks: Option<Vec<Pick>> = numbers.iter().map(|&n| self.pick_of(n)).collect();
        match picks {
            Some(picks) => {
                self.picks = picks;
                true
            }
            None => false,
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
                    recommended: self.recommended.contains(&n),
                    picked: self.picks.contains(&Pick::Option(n)),
                    other: false,
                    note: note.clone(),
                }
            })
            .collect();
        rows.push(OptionRow {
            number: self.other_number(),
            label: if self.send_back {
                ANSWER_HERE.into()
            } else {
                "Something else: type your own answer".into()
            },
            recommended: false,
            picked: self.picks.contains(&Pick::Other),
            other: true,
            note: None,
        });
        rows
    }

    /// What Enter or Send sends; `None` while nothing is picked, or Other… is picked with an empty field. Several
    /// answers send every ticked option, in the options' order, and Other…'s text.
    pub fn choice(&self, other_text: &str) -> Option<Choice> {
        let text = other_text.trim();
        if self.multiple {
            let mut options: Vec<u32> = self
                .picks
                .iter()
                .filter_map(|p| match p {
                    Pick::Option(n) => Some(*n),
                    Pick::Other => None,
                })
                .collect();
            options.sort_unstable();
            let other = match self.picks.contains(&Pick::Other) {
                true if text.is_empty() => return None,
                true => Some(text.to_owned()),
                false => None,
            };
            if options.is_empty() && other.is_none() {
                return None;
            }
            return Some(Choice::Several { options, other });
        }
        match *self.picks.first()? {
            Pick::Option(_) if self.send_back => Some(Choice::SendBack),
            Pick::Option(n) => Some(Choice::Option(n)),
            Pick::Other => (!text.is_empty()).then(|| Choice::Other(text.to_owned())),
        }
    }

    /// Send is enabled.
    pub fn can_send(&self, other_text: &str) -> bool {
        self.choice(other_text).is_some()
    }
}

/// One answer option (ATT-12): the recommended one is picked when the list opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetAnswerRow")]
pub struct AnswerRow {
    pub number: u32,
    pub label: String,
    pub recommended: bool,
    pub picked: bool,
    pub other: bool,
    /// The option's own note, its pros and cons (ATT-17), under the label, whole.
    pub note: Option<String>,
}

/// The question with its options in the header (*SHOP-21 · Asks you · answer options*); a Project agent's question
/// too, in the strip's popover and above its terminal, where an open question has no rows (a free-text box).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetAnswer")]
pub struct Answer {
    pub question: String,
    /// The question itself: its text's first line, drawn bold.
    pub line: String,
    /// Everything after the first line, the question's context: regular weight, its paragraphs and line breaks kept
    /// and simple Markdown drawn. `None` for a question of one line.
    pub context: Option<String>,
    /// The options and Other…; empty for an open question.
    pub rows: Vec<AnswerRow>,
    /// The user may tick several rows (checkboxes, number keys tick and untick); otherwise one is picked (radios).
    /// Optional on the page: a question that takes one answer may leave it out.
    #[ts(as = "Option<bool>", optional)]
    pub multiple: bool,
    pub recommended_label: &'static str,
    pub other_placeholder: &'static str,
    pub hint: &'static str,
    pub in_terminal: &'static str,
    pub send: &'static str,
}

/// A question's text split into its question line (up to the first line break) and its context (the rest, without
/// the blank lines around it; `None` when nothing is left). The stored question stays as the agent sent it: only the
/// drawing splits it, here, so every place shows the same split.
pub fn split_question(text: &str) -> (String, Option<String>) {
    let text = text.trim_start();
    let (line, rest) = text.split_once('\n').unwrap_or((text, ""));
    let context = rest
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let context = context.trim_end();
    (
        line.trim_end().to_owned(),
        (!context.is_empty()).then(|| context.to_owned()),
    )
}

/// The question line, its context and the words every answer box shares.
fn answer(question: &HumanQuestion, rows: Vec<AnswerRow>, hint: &'static str) -> Answer {
    let (line, context) = split_question(&question.text);
    Answer {
        question: question.id.clone(),
        line,
        context,
        multiple: question.multiple && !rows.is_empty(),
        rows,
        recommended_label: RECOMMENDED,
        other_placeholder: "Type your answer",
        hint,
        in_terminal: "Answer in terminal",
        send: "Send answer",
    }
}

fn rows_of(options: &AnswerOptions) -> Vec<AnswerRow> {
    options
        .rows()
        .into_iter()
        .map(|r| AnswerRow {
            number: r.number,
            label: r.label,
            recommended: r.recommended,
            picked: r.picked,
            other: r.other,
            note: r.note,
        })
        .collect()
}

/// The hint under a list of options: a number picks, or ticks when the question takes several answers.
fn options_hint(options: &AnswerOptions) -> &'static str {
    if options.multiple() {
        "Pick any \u{b7} 1\u{2013}4 tick \u{b7} \u{21b5} sends"
    } else {
        "1\u{2013}4 pick \u{b7} \u{21b5} sends"
    }
}

/// A ticket's question, first on its Overview and answered there: its options, or no rows and a box for the user's
/// own words for an open question; Answer in terminal stays beside it.
pub fn question_bar(question: &HumanQuestion) -> Option<Answer> {
    let (rows, hint) = match AnswerOptions::of(question) {
        Some(options) => (
            rows_of(&options),
            match question.send_back {
                Some(_) => "1\u{2013}2 pick \u{b7} \u{21b5} sends",
                None => options_hint(&options),
            },
        ),
        // An open question's box takes a few lines.
        None => (
            Vec::new(),
            "\u{21b5} sends \u{b7} \u{21e7}\u{21b5} new line",
        ),
    };
    Some(answer(question, rows, hint))
}

/// A Project agent's question (d0119, d0068): its options like a ticket's, or no rows and a free-text box for an
/// open question.
pub fn agent_question(question: &HumanQuestion) -> Answer {
    let (rows, hint) = match AnswerOptions::of(question) {
        Some(options) => (rows_of(&options), options_hint(&options)),
        None => (Vec::new(), "\u{21b5} sends"),
    };
    answer(question, rows, hint)
}

/// What the picked rows send for `question`: an option, several (a question that takes several answers), Other…'s
/// text, or an open question's text; `None` when there is nothing to send.
pub fn choice_for(question: &HumanQuestion, picks: &[u32], other: &str) -> Option<Choice> {
    match AnswerOptions::of(question) {
        Some(mut options) => {
            options.set(picks).then_some(())?;
            options.choice(other)
        }
        None => {
            let text = other.trim();
            (!text.is_empty()).then(|| Choice::Other(text.to_owned()))
        }
    }
}

/// "OPTIONS FROM SHOP-21’S AGENT".
pub fn heading(key: &impl std::fmt::Display) -> String {
    format!("OPTIONS FROM {key}’S AGENT")
}

/// The Gallery's questions with notes (ATT-17; *m26-tickets.md → The example*): NOTE-1's four options, one note
/// about 300 characters long and one option without a note; and a two-option question with notes
/// (CHANGES-2026-10-18-ab §1); and a long question with context (*question-box-4*).
pub fn note_samples() -> [HumanQuestion; 3] {
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
        recommended: vec![1],
        multiple: false,
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
        send_back: None,
    };
    let invoice = HumanQuestion {
        id: "q-notes-invoice".into(),
        text: "Which invoice export should the first version have?".into(),
        options: vec!["CSV only".into(), "CSV and PDF".into()],
        recommended: vec![1],
        multiple: false,
        notes: vec![
            note("One small library, about half a day. No printable copy for customers."),
            note(
                "Printable invoices too, but needs a page layout and a PDF library: about two days.",
            ),
        ],
        asked_at: 0,
        send_back: None,
    };
    [asks, invoice, security_review()]
}

/// A long question with context, the kind Noyan's `flow` skill asks (*question-box-4*): its first line the question,
/// then paragraphs with inline code, a list with bold words, and options with notes.
pub fn security_review() -> HumanQuestion {
    let note = |text: &str| Some(text.to_owned());
    HumanQuestion {
        id: "q-security-review".into(),
        text: "Which agent should run the new Security review stage?\n\
               \n\
               Your project has a security skill in `.claude/skills/security-review`, and it expects the code to \
               build first: it runs `npm audit` and reads the lockfile. Two things to weigh:\n\
               \n\
               - **Verifier** already runs Validate on Codex. Using it again keeps one checking agent, but Codex \
               doesn't load `.claude/skills`, so it would fall back to Otaq's own review skill.\n\
               - **Strong** (Claude Code) loads your skill as it is, but it's the agent that writes the code, so it \
               would review its own work.\n\
               \n\
               A third agent costs a new connection in Settings \u{2192} Agents."
            .into(),
        options: vec![
            "Strong, with your security-review skill".into(),
            "Verifier, with Otaq's review skill".into(),
            "A new agent".into(),
        ],
        recommended: vec![1],
        multiple: false,
        notes: vec![
            note("Uses your skill as written; reviews its own code."),
            note("One checking agent for both checks; your skill isn't used."),
            note("Separate reviewer; you set it up in Settings \u{2192} Agents first."),
        ],
        asked_at: 0,
        send_back: None,
    }
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
            recommended: recommended.into_iter().collect(),
            multiple: false,
            notes: Vec::new(),
            asked_at: 0,
            send_back: None,
        }
    }

    /// An open question (no options) is answered on Overview in the user's own words: no rows, a box, and what is
    /// typed is the answer.
    #[test]
    fn an_open_question_is_answered_in_your_own_words() {
        let q = question(&[], None);
        let bar = question_bar(&q).expect("an open question shows too");
        assert!(bar.rows.is_empty());
        assert_eq!(bar.hint, "\u{21b5} sends \u{b7} \u{21e7}\u{21b5} new line");
        assert_eq!(bar.other_placeholder, "Type your answer");
        assert_eq!(
            choice_for(&q, &[1], " Hello from the README "),
            Some(Choice::Other("Hello from the README".into()))
        );
        assert_eq!(choice_for(&q, &[1], "  "), None);
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
                (4, "Something else: type your own answer", false, false),
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
        assert_eq!(o.picks, [Pick::Other]);
        assert!(!o.can_send("  "));
        assert_eq!(o.choice(" Mailgun "), Some(Choice::Other("Mailgun".into())));
    }

    /// An agent's send-back question offers its two ways out: 1 sends the ticket back, 2 answers here in words.
    /// Nothing is picked first: the user decides.
    #[test]
    fn a_send_back_question_offers_send_back_or_answer_here() {
        let q = HumanQuestion {
            send_back: Some(otaq_core::board::questions::SendBackTo {
                stage_id: "product-spec".into(),
                stage: "Product spec".into(),
            }),
            ..question(&[], None)
        };
        let mut o = AnswerOptions::of(&q).expect("a send-back question has its rows");
        let shown: Vec<(u32, String, bool)> = o
            .rows()
            .into_iter()
            .map(|r| (r.number, r.label, r.other))
            .collect();
        assert_eq!(
            shown,
            [
                (1, "Send back to Product spec".to_owned(), false),
                (2, ANSWER_HERE.to_owned(), true),
            ]
        );
        assert!(o.picks.is_empty());
        assert!(o.press(1));
        assert_eq!(o.choice(""), Some(Choice::SendBack));
        assert!(o.press(2));
        assert_eq!(o.choice(""), None, "answering here needs words");
        assert_eq!(
            o.choice("Refunds over 30 days: store credit"),
            Some(Choice::Other("Refunds over 30 days: store credit".into()))
        );
        let bar = question_bar(&q).unwrap();
        assert_eq!(bar.rows.len(), 2);
        assert_eq!(bar.hint, "1\u{2013}2 pick \u{b7} \u{21b5} sends");
    }

    #[test]
    fn without_a_recommendation_nothing_is_picked_and_send_waits() {
        let q = question(&["Split it everywhere", "Only in the profile form"], None);
        let mut o = AnswerOptions::of(&q).unwrap();
        assert!(o.picks.is_empty());
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
        q.notes = Vec::new();
        assert!(
            AnswerOptions::of(&q)
                .unwrap()
                .rows()
                .iter()
                .all(|r| r.note.is_none())
        );
    }

    /// The first line asks, the rest is context: a question of one line has none; the blank lines between them go,
    /// the context's own paragraphs and line breaks stay.
    #[test]
    fn the_first_line_asks_and_the_rest_is_context() {
        assert_eq!(
            split_question("Should the security review run before or after Validate?"),
            (
                "Should the security review run before or after Validate?".into(),
                None
            )
        );
        assert_eq!(
            split_question(
                "Which one?\r\n\r\nFirst paragraph,\r\nsame paragraph.\r\n\r\n- a\r\n- b\r\n\r\n"
            ),
            (
                "Which one?".into(),
                Some("First paragraph,\nsame paragraph.\n\n- a\n- b".into())
            )
        );
        assert_eq!(
            split_question("Which one?\n\n  \n"),
            ("Which one?".into(), None)
        );
        assert_eq!(
            split_question("\n\nWhich one?  "),
            ("Which one?".into(), None)
        );

        let long = security_review();
        let bar = question_bar(&long).unwrap();
        assert_eq!(
            bar.line,
            "Which agent should run the new Security review stage?"
        );
        let context = bar.context.expect("the long question has context");
        assert!(
            context.starts_with(
                "Your project has a security skill in `.claude/skills/security-review`"
            )
        );
        assert!(context.contains("\n\n- **Verifier** already runs Validate"));
        assert!(context.ends_with("Settings \u{2192} Agents."));
        assert_eq!(
            agent_question(&long).context.as_deref(),
            Some(context.as_str())
        );
        assert_eq!(
            long.text.lines().next(),
            Some(bar.line.as_str()),
            "the stored question is unchanged"
        );
    }

    /// A question that takes several answers: the recommended ones open ticked, number keys tick and untick (Other…
    /// too), Send needs at least one, and the answer names every ticked option and Other…'s text.
    #[test]
    fn several_answers_tick_and_send_every_pick() {
        let q = HumanQuestion {
            recommended: vec![1, 3],
            multiple: true,
            ..question(&["Postmark", "SES", "Resend"], None)
        };
        let mut o = AnswerOptions::of(&q).unwrap();
        let ticked = |o: &AnswerOptions| {
            o.rows()
                .iter()
                .filter(|r| r.picked)
                .map(|r| r.number)
                .collect::<Vec<_>>()
        };
        assert_eq!(ticked(&o), [1, 3], "the recommended ones open ticked");
        assert_eq!(
            o.rows().iter().map(|r| r.recommended).collect::<Vec<_>>(),
            [true, false, true, false]
        );
        assert_eq!(
            o.choice(""),
            Some(Choice::Several {
                options: vec![1, 3],
                other: None
            })
        );
        assert!(o.press(3), "3 unticks Resend");
        assert!(o.press(2) && o.press(4), "2 ticks SES, 4 ticks Other…");
        assert_eq!(ticked(&o), [1, 2, 4]);
        assert!(!o.can_send("  "), "a ticked Other… needs its text");
        let choice = o.choice(" Mailgun for receipts ").unwrap();
        assert_eq!(
            format_answer(&q, &choice),
            "Answer to your question: Postmark (option 1), SES (option 2). Also: Mailgun for receipts"
        );
        for n in [1, 2, 4] {
            o.press(n);
        }
        assert!(!o.can_send("Mailgun"), "nothing ticked: Send waits");

        let bar = question_bar(&q).unwrap();
        assert!(bar.multiple);
        assert_eq!(
            bar.hint,
            "Pick any \u{b7} 1\u{2013}4 tick \u{b7} \u{21b5} sends"
        );
        assert!(agent_question(&q).multiple);
        // The page's ticked rows: several, Other… among them, or only Other….
        assert_eq!(
            choice_for(&q, &[3, 1, 4], "Mailgun"),
            Some(Choice::Several {
                options: vec![1, 3],
                other: Some("Mailgun".into())
            })
        );
        assert_eq!(
            choice_for(&q, &[4], "Mailgun"),
            Some(Choice::Several {
                options: Vec::new(),
                other: Some("Mailgun".into())
            })
        );
        assert_eq!(choice_for(&q, &[], ""), None);
        assert_eq!(choice_for(&q, &[1, 5], ""), None, "5 is no row");
    }

    /// A question that takes one answer (every question stored before several answers) keeps its radios: a number
    /// replaces the pick, and several picks from the page are refused.
    #[test]
    fn one_answer_questions_take_one_pick() {
        let q = question(&["Postmark", "SES", "Resend"], Some(2));
        let mut o = AnswerOptions::of(&q).unwrap();
        assert!(!o.multiple());
        assert!(o.press(1) && o.press(3));
        assert_eq!(o.picks, [Pick::Option(3)]);
        assert!(!question_bar(&q).unwrap().multiple);
        assert_eq!(choice_for(&q, &[2], ""), Some(Choice::Option(2)));
        assert_eq!(choice_for(&q, &[1, 2], ""), None);
    }

    #[test]
    fn free_text_questions_have_no_list() {
        assert!(AnswerOptions::of(&question(&[], None)).is_none());
        assert_eq!(heading(&"SHOP-21"), "OPTIONS FROM SHOP-21’S AGENT");
    }
}
