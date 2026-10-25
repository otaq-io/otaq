//! Validation reports in History and Review (VAL-12, UI-04, REV-02, CMT-14; d0063, d0064, d0109; *05 → Row · validation
//! → History · validation rounds*, *07 → Row · validation report & Auto-merge*): one History entry per validation round
//! (outcome, duration, tokens; its verdict is the checker's comment, a row of its own with its report, so the round
//! opens nothing), and on the Files tab the Review entry ("Validated · round 2") with the
//! final report next to the agent's summary: what was checked, the findings with their file and line, the notes with
//! their linked keys, and **Create ticket** on each "Not covered by any ticket" note (the user's click; the validator
//! never creates tickets). A ticket that failed every round and was moved on by the user shows the last findings,
//! in red, where the report usually sits.
//!
//! The report is the validator's text in the validate skill's format: `Checked: a; b; c`, `Note: …` lines, and one
//! finding per other line (`src/cart.ts:42: the total ignores the discount`).

use otaq_core::board::validation::{ValidationRecord, not_covered, note_of};
use otaq_core::domain::validation::ValidationTrigger;
use otaq_core::domain::{ReviewEntry, ReviewEntryFlag, TicketKey, ValidationOutcome};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::theme::Palette;
use crate::view_models::card::tokens_label;
use crate::view_models::clock;

/// A piece of a report line: plain text, or a ticket key the page links (`SHOP-21`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReportSegment")]
pub struct Segment {
    pub text: String,
    pub key: bool,
}

/// A finding: what is wrong, and where ("components/AddressAutocomplete.tsx:88").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReportFinding")]
pub struct Finding {
    pub text: Vec<Segment>,
    pub place: Option<String>,
}

/// A note: never a failure. `create` holds the note as the service takes it for Create ticket, on a "Not covered by
/// any ticket" note only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReportNote")]
pub struct Note {
    pub text: Vec<Segment>,
    pub create: Option<String>,
}

/// A round's report as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ValidationReportView")]
pub struct Report {
    /// "Validation report", or "Did not pass · last report, round 3 of 3".
    pub title: String,
    /// "round 2 · 3 min · 11k".
    pub meta: String,
    /// Did not pass: drawn in red, findings first, no headings.
    pub failed: bool,
    /// A passed report's own lines that are neither checks nor notes ("Pass: every item in the description is
    /// done"): its verdict, drawn with a check, never as a finding.
    pub summary: Vec<Vec<Segment>>,
    /// Only on a failed report: a passed one has none (its other lines are its summary).
    pub findings: Vec<Finding>,
    /// "CHECKED" (none on a failed report).
    pub checked_heading: Option<&'static str>,
    pub checked: Vec<Vec<Segment>>,
    /// "NOTES" (none on a failed report).
    pub notes_heading: Option<&'static str>,
    pub notes: Vec<Note>,
    pub create: &'static str,
    pub create_icon: IconName,
}

/// A report line without its bullet ("- ", "* ").
fn bare(line: &str) -> &str {
    line.trim().trim_start_matches(['-', '*']).trim_start()
}

/// "src/cart.ts:42: the total ignores the discount" → the text and "src/cart.ts:42".
fn finding(line: &str) -> Finding {
    let place = line.split_once(": ").filter(|(place, _)| {
        !place.contains(char::is_whitespace)
            && place.rsplit_once(':').is_some_and(|(path, n)| {
                !path.is_empty() && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())
            })
    });
    match place {
        Some((place, text)) => Finding {
            text: segments(text.trim()),
            place: Some(place.to_owned()),
        },
        None => Finding {
            text: segments(line),
            place: None,
        },
    }
}

/// The text with its ticket keys apart, so the page can link them ("covered by SHOP-21").
pub fn segments(text: &str) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut push = |text: &str, key: bool| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if !last.key && !key => last.text.push_str(text),
            _ => out.push(Segment {
                text: text.to_owned(),
                key,
            }),
        }
    };
    let word = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '.';
    let mut rest = text;
    while let Some(start) = rest.find(word) {
        push(&rest[..start], false);
        let tail = &rest[start..];
        let end = tail.find(|c| !word(c)).unwrap_or(tail.len());
        // "SHOP-21." at the end of a sentence: the dot isn't part of the key.
        let candidate = tail[..end].trim_end_matches('.');
        if candidate.parse::<TicketKey>().is_ok() {
            push(candidate, true);
            push(&tail[candidate.len()..end], false);
        } else {
            push(&tail[..end], false);
        }
        rest = &tail[end..];
    }
    push(rest, false);
    out
}

/// "round 2 · 3 min · 11k" (no duration while it runs, no tokens when unknown).
fn meta(r: &ValidationRecord) -> String {
    let mut parts = vec![format!("round {}", r.round)];
    if let Some(ms) = r.duration_ms {
        parts.push(clock::duration(ms));
    }
    if let Some(t) = &r.tokens {
        parts.push(tokens_label(t.shown()));
    }
    parts.join(" \u{b7} ")
}

/// The report of a round, `failed` when it reads as "did not pass". A passed round has no findings: the lines that
/// aren't `Checked:` or `Note:` are its summary.
pub fn report(r: &ValidationRecord, title: String, failed: bool) -> Report {
    let mut summary = Vec::new();
    let mut findings = Vec::new();
    let mut checked = Vec::new();
    for line in r.report.as_deref().unwrap_or_default().lines() {
        let line = bare(line);
        if line.is_empty() || note_of(line).is_some() {
            continue;
        }
        let head = line.get(..8).filter(|h| h.eq_ignore_ascii_case("checked:"));
        match head {
            Some(_) => checked.extend(
                line[8..]
                    .split(';')
                    .map(|c| c.trim().trim_end_matches('.').trim())
                    .filter(|c| !c.is_empty())
                    .map(segments),
            ),
            None if failed => findings.push(finding(line)),
            None => summary.push(segments(line)),
        }
    }
    Report {
        title,
        meta: meta(r),
        failed,
        summary,
        findings,
        checked_heading: (!failed).then_some("CHECKED"),
        checked,
        notes_heading: (!failed).then_some("NOTES"),
        notes: r
            .notes
            .iter()
            .map(|n| Note {
                text: segments(n),
                create: not_covered(n).map(|_| n.clone()),
            })
            .collect(),
        create: "Create ticket",
        create_icon: IconName::Plus,
    }
}

fn plural(n: usize, one: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {one}s")
    }
}

/// "2nd".
fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// One History entry per validation round (*05 → History · validation rounds*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundRow {
    /// "Round 1 · failed", "Validation cancelled · not counted".
    pub title: String,
    /// "agent submitted · 3 findings", "the code changed · round 2 starts over".
    pub detail: Option<String>,
    pub at: i64,
    /// "4 min", "running".
    pub duration: String,
    /// "12k", or "—".
    pub tokens: String,
    pub icon: IconName,
    pub palette: Palette,
}

/// The outcome's icon and colours, as in the frame's OUTCOMES legend.
fn outcome_look(outcome: Option<ValidationOutcome>) -> (IconName, Palette) {
    use ValidationOutcome as O;
    match outcome {
        Some(O::Pass) => (IconName::ShieldCheck, Palette::Review),
        Some(O::Fail) => (IconName::ShieldX, Palette::Attn),
        Some(O::Cancelled) => (IconName::Ban, Palette::Queued),
        Some(O::NoResult) => (IconName::ShieldQuestionMark, Palette::Attn),
        Some(O::Discarded) => (IconName::Trash2, Palette::Queued),
        Some(O::Interrupted) => (IconName::ZapOff, Palette::Error),
        Some(O::Skipped) => (IconName::SkipForward, Palette::Queued),
        Some(O::Stopped) => (IconName::Square, Palette::Queued),
        None => (IconName::ShieldEllipsis, Palette::Working),
    }
}

/// The History entry of one round; `rounds` is the project's Validation rounds (the last failed round isn't sent to
/// the agent: the card turns turquoise).
pub fn round_row(r: &ValidationRecord, rounds: u32) -> RoundRow {
    use ValidationOutcome as O;
    let trigger = match r.trigger {
        ValidationTrigger::Agent => "agent submitted",
        ValidationTrigger::User => "you submitted",
    };
    // Why a round that doesn't count ended (the service's words), else the legend's.
    let why = |fallback: &str| {
        r.report
            .as_deref()
            .and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()))
            .unwrap_or(fallback)
            .to_owned()
    };
    let last = r.round >= rounds;
    let (title, detail) = match r.outcome {
        Some(O::Pass) => (
            format!("Round {} \u{b7} passed", r.round),
            Some(match r.notes.len() {
                0 => trigger.to_owned(),
                n => format!("{trigger} \u{b7} {}", plural(n, "note")),
            }),
        ),
        Some(O::Fail) if last => (
            format!(
                "Round {} \u{b7} failed \u{b7} not sent to the agent",
                r.round
            ),
            Some("the ticket comes to you".to_owned()),
        ),
        Some(O::Fail) => (
            format!("Round {} \u{b7} failed", r.round),
            Some(trigger.to_owned()),
        ),
        Some(O::Cancelled) => (
            "Validation cancelled \u{b7} not counted".to_owned(),
            Some(why("code or description changed")),
        ),
        Some(O::NoResult) => (
            "Validation ended without a result \u{b7} not counted".to_owned(),
            Some(why("validator ended without pass/fail")),
        ),
        Some(O::Discarded) => (
            "Validation discarded \u{b7} not counted".to_owned(),
            Some(why("validator changed tracked files; result thrown away")),
        ),
        Some(O::Interrupted) => (
            "Validation interrupted \u{b7} not counted".to_owned(),
            Some(why("restart; Resume starts a fresh validation")),
        ),
        Some(O::Skipped) => ("Validation skipped by you".to_owned(), None),
        Some(O::Stopped) => ("Validation stopped by you".to_owned(), None),
        None => (
            format!("Round {} \u{b7} validating\u{2026}", r.round),
            Some(trigger.to_owned()),
        ),
    };
    let (icon, palette) = outcome_look(r.outcome);
    RoundRow {
        title,
        detail,
        at: r.started_at,
        duration: match r.duration_ms {
            Some(ms) => clock::duration(ms),
            None if r.outcome.is_none() => "running".to_owned(),
            None => "\u{2014}".to_owned(),
        },
        tokens: r
            .tokens
            .as_ref()
            .map_or_else(|| "\u{2014}".to_owned(), |t| tokens_label(t.shown())),
        icon,
        palette,
    }
}

/// "Validation total · 4 entries" and "41k" under the rounds; none without rounds.
pub fn total(records: &[ValidationRecord]) -> Option<(String, String)> {
    if records.is_empty() {
        return None;
    }
    let tokens: u64 = records
        .iter()
        .filter_map(|r| r.tokens.as_ref())
        .map(|t| t.shown())
        .sum();
    Some((
        match records.len() {
            1 => "Validation total \u{b7} 1 entry".to_owned(),
            n => format!("Validation total \u{b7} {n} entries"),
        },
        tokens_label(tokens),
    ))
}

/// How the ticket reached Review, at the top of the Files tab: "Validated · round 2" with "passed on the 2nd of 3
/// rounds · round 1 findings in History".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReviewEntryView")]
pub struct Entry {
    pub label: String,
    pub icon: IconName,
    pub palette: Palette,
    pub text: Option<String>,
}

pub fn entry(flag: ReviewEntryFlag, rounds: u32) -> Entry {
    let label = flag.to_string();
    match flag.entry {
        ReviewEntry::Validated => Entry {
            label,
            icon: IconName::ShieldCheck,
            palette: Palette::Review,
            text: Some(match flag.round {
                0 | 1 => format!("passed on the 1st of {rounds} rounds"),
                2 => format!(
                    "passed on the 2nd of {rounds} rounds \u{b7} round 1 findings in History"
                ),
                n => format!(
                    "passed on the {} of {rounds} rounds \u{b7} rounds 1\u{2013}{} findings in History",
                    ordinal(n),
                    n - 1
                ),
            }),
        },
        ReviewEntry::FailedMoved => Entry {
            label,
            icon: IconName::ShieldX,
            palette: Palette::Error,
            text: None,
        },
        ReviewEntry::Skipped => Entry {
            label,
            icon: IconName::SkipForward,
            palette: Palette::Queued,
            text: None,
        },
        ReviewEntry::SubmittedByUser => Entry {
            label,
            icon: IconName::User,
            palette: Palette::Queued,
            text: None,
        },
    }
}

/// The Files tab's Review part: the entry, the final report (or the last findings of a ticket that didn't pass), and
/// "Moved by you at 11:12 · earlier rounds in History" under a failed one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DiffReviewView")]
pub struct Review {
    pub entry: Entry,
    pub report: Option<Report>,
    pub foot: Option<String>,
}

/// From the ticket's Review entry, its rounds (newest first), the project's Validation rounds and when the user
/// moved it on (for a failed one); `None` when it has no Review entry.
pub fn review(
    flag: Option<ReviewEntryFlag>,
    records: &[ValidationRecord],
    rounds: u32,
    moved_at: Option<i64>,
) -> Option<Review> {
    let flag = flag?;
    let newest = |outcome: ValidationOutcome| records.iter().find(|r| r.outcome == Some(outcome));
    let (report, foot) = match flag.entry {
        ReviewEntry::Validated => (
            newest(ValidationOutcome::Pass)
                .map(|r| report(r, "Validation report".to_owned(), false)),
            None,
        ),
        ReviewEntry::FailedMoved => (
            newest(ValidationOutcome::Fail).map(|r| {
                report(
                    r,
                    format!(
                        "Did not pass \u{b7} last report, round {} of {}",
                        r.round,
                        rounds.max(r.round)
                    ),
                    true,
                )
            }),
            Some(match moved_at {
                Some(at) => format!(
                    "Moved by you at {} \u{b7} earlier rounds in History",
                    clock::local_hhmm(at)
                ),
                None => "Moved by you \u{b7} earlier rounds in History".to_owned(),
            }),
        ),
        ReviewEntry::Skipped | ReviewEntry::SubmittedByUser => (None, None),
    };
    Some(Review {
        entry: entry(flag, rounds),
        report,
        foot,
    })
}

/// The toast after Create ticket (*11 → Success toasts*): "Created SHOP-41 from a validation note" with
/// "“Not covered by any ticket: refunds endpoint” → Backlog, prefilled from the note".
pub fn created_toast(created: &TicketKey, note: &str) -> (String, String) {
    (
        format!("Created {created} from a validation note"),
        format!(
            "\u{201c}{}\u{201d} \u{2192} Backlog, prefilled from the note",
            note.trim()
        ),
    )
}

#[cfg(test)]
mod tests {
    use otaq_core::usage::Tokens;

    use super::*;

    fn record(round: u32, outcome: Option<ValidationOutcome>, report: &str) -> ValidationRecord {
        ValidationRecord {
            agent: None,
            round,
            trigger: ValidationTrigger::Agent,
            outcome,
            started_at: 0,
            ended_at: outcome.map(|_| 180_000),
            duration_ms: outcome.map(|_| 180_000),
            session: None,
            report: (!report.is_empty()).then(|| report.to_owned()),
            notes: report
                .lines()
                .filter_map(|l| note_of(l).map(str::to_owned))
                .collect(),
            tokens: Some(Tokens {
                input: 10_000,
                cached: 0,
                output: 1_000,
                reasoning: None,
            }),
        }
    }

    fn text(segments: &[Segment]) -> String {
        segments.iter().map(|s| s.text.as_str()).collect()
    }

    const PASSED: &str = "Checked: pnpm test passed (38 tests) · pnpm lint passed; Does what the description \
                          asks: POST /api/checkout, 422 on empty carts; Scope matches the description.\n\
                          Note: Order confirmation email: covered by SHOP-21\n\
                          - Note: Not covered by any ticket: refunds endpoint";

    /// *07 → SHOP-18 · Diff · Validated · round 2*: what was checked, the notes with the linked key, and Create
    /// ticket on the uncovered gap only.
    #[test]
    fn a_passed_report_has_its_checks_and_notes() {
        let r = report(
            &record(2, Some(ValidationOutcome::Pass), PASSED),
            "Validation report".into(),
            false,
        );
        assert_eq!(r.meta, "round 2 · 3 min · 11k");
        assert!(r.findings.is_empty());
        assert!(r.summary.is_empty());
        assert_eq!(r.checked_heading, Some("CHECKED"));
        let checked: Vec<_> = r.checked.iter().map(|c| text(c)).collect();
        assert_eq!(
            checked,
            [
                "pnpm test passed (38 tests) · pnpm lint passed",
                "Does what the description asks: POST /api/checkout, 422 on empty carts",
                "Scope matches the description"
            ]
        );
        assert_eq!(r.notes.len(), 2);
        assert_eq!(
            r.notes[0].text,
            [
                Segment {
                    text: "Order confirmation email: covered by ".into(),
                    key: false
                },
                Segment {
                    text: "SHOP-21".into(),
                    key: true
                }
            ]
        );
        assert_eq!(r.notes[0].create, None);
        assert_eq!(
            r.notes[1].create.as_deref(),
            Some("Not covered by any ticket: refunds endpoint"),
            "the note exactly as the service knows it"
        );
        assert_eq!(r.create, "Create ticket");
    }

    /// A passed report's verdict line is its summary, with a check, never a red finding (dogfood #17).
    #[test]
    fn a_passed_reports_other_lines_are_its_summary() {
        let r = report(
            &record(
                1,
                Some(ValidationOutcome::Pass),
                "Pass: every item in the description is done, see SHOP-21\n\
                 Checked: pnpm test passed",
            ),
            "Validation report".into(),
            false,
        );
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        let summary: Vec<_> = r.summary.iter().map(|s| text(s)).collect();
        assert_eq!(
            summary,
            ["Pass: every item in the description is done, see SHOP-21"]
        );
        assert!(r.summary[0].iter().any(|s| s.key && s.text == "SHOP-21"));
        assert_eq!(text(&r.checked[0]), "pnpm test passed");
    }

    /// *07 → Review · Failed 3 times · moved by you*: the findings with their place first, then the checks, no
    /// headings, the foot with the time it was moved.
    #[test]
    fn a_ticket_that_did_not_pass_shows_the_last_findings() {
        let failed = record(
            3,
            Some(ValidationOutcome::Fail),
            "components/AddressAutocomplete.tsx:88: The description asks for keyboard navigation; arrow keys do \
             nothing\nChecked: pnpm test passed · pnpm lint passed",
        );
        let older = record(2, Some(ValidationOutcome::Fail), "a.ts:1: old");
        let flag = ReviewEntryFlag {
            entry: ReviewEntry::FailedMoved,
            round: 3,
        };
        let v = review(Some(flag), &[failed, older], 3, None).unwrap();
        assert_eq!(v.entry.label, "Failed 3 times · moved by you");
        assert_eq!(v.entry.palette, Palette::Error);
        let r = v.report.unwrap();
        assert_eq!(r.title, "Did not pass · last report, round 3 of 3");
        assert!(r.failed);
        assert!(r.summary.is_empty());
        assert_eq!(r.checked_heading, None);
        assert_eq!(
            r.findings[0].place.as_deref(),
            Some("components/AddressAutocomplete.tsx:88")
        );
        assert_eq!(
            text(&r.findings[0].text),
            "The description asks for keyboard navigation; arrow keys do nothing"
        );
        assert_eq!(text(&r.checked[0]), "pnpm test passed · pnpm lint passed");
        assert_eq!(
            v.foot.as_deref(),
            Some("Moved by you · earlier rounds in History")
        );
    }

    #[test]
    fn the_review_entry_says_how_it_got_here() {
        let flag = |entry, round| ReviewEntryFlag { entry, round };
        let passed = record(2, Some(ValidationOutcome::Pass), PASSED);
        let v = review(
            Some(flag(ReviewEntry::Validated, 2)),
            std::slice::from_ref(&passed),
            3,
            None,
        )
        .unwrap();
        assert_eq!(v.entry.label, "Validated · round 2");
        assert_eq!(
            v.entry.text.as_deref(),
            Some("passed on the 2nd of 3 rounds · round 1 findings in History")
        );
        assert_eq!(v.report.unwrap().title, "Validation report");
        assert_eq!(v.foot, None);
        assert_eq!(
            entry(flag(ReviewEntry::Validated, 1), 3).text.as_deref(),
            Some("passed on the 1st of 3 rounds")
        );
        assert_eq!(
            entry(flag(ReviewEntry::Validated, 3), 5).text.as_deref(),
            Some("passed on the 3rd of 5 rounds · rounds 1–2 findings in History")
        );
        let skipped = review(
            Some(flag(ReviewEntry::SubmittedByUser, 0)),
            &[passed],
            3,
            None,
        )
        .unwrap();
        assert_eq!(skipped.entry.label, "Submitted by you");
        assert_eq!(skipped.report, None);
        assert_eq!(review(None, &[], 3, None), None);
    }

    /// *05 → History · validation rounds (SHOP-32)*: each round with its outcome and detail; a cancelled round isn't
    /// counted; the last failed round isn't sent to the agent; the total under them. No round opens a report any more:
    /// the verdict is the checker's comment, with its report (CMT-14).
    #[test]
    fn history_lists_every_round() {
        let fail = |round, findings: &str| record(round, Some(ValidationOutcome::Fail), findings);
        let first = round_row(&fail(1, "a.ts:1: x\nb.ts:2: y\nc.ts:3: z"), 3);
        assert_eq!(first.title, "Round 1 · failed");
        assert_eq!(first.detail.as_deref(), Some("agent submitted"));
        assert_eq!(first.duration, "3 min");
        assert_eq!(first.tokens, "11k");
        assert_eq!(first.icon, IconName::ShieldX);
        let cancelled = round_row(
            &record(
                2,
                Some(ValidationOutcome::Cancelled),
                "the code changed · round 2 starts over",
            ),
            3,
        );
        assert_eq!(cancelled.title, "Validation cancelled · not counted");
        assert_eq!(
            cancelled.detail.as_deref(),
            Some("the code changed · round 2 starts over")
        );
        let last = round_row(&fail(3, "x.ts:9: still broken"), 3);
        assert_eq!(last.title, "Round 3 · failed · not sent to the agent");
        assert_eq!(last.detail.as_deref(), Some("the ticket comes to you"));
        let passed = round_row(&record(2, Some(ValidationOutcome::Pass), PASSED), 3);
        assert_eq!(passed.title, "Round 2 · passed");
        assert_eq!(passed.detail.as_deref(), Some("agent submitted · 2 notes"));
        let running = round_row(&record(1, None, ""), 3);
        assert_eq!(running.duration, "running");
        let (label, tokens) = total(&[fail(1, ""), fail(2, ""), fail(3, ""), fail(4, "")]).unwrap();
        assert_eq!(label, "Validation total · 4 entries");
        assert_eq!(tokens, "44k");
        assert_eq!(total(&[]), None);
    }

    #[test]
    fn keys_are_found_in_text() {
        let s = segments("covered by SHOP-40.2. See BILL-3, not SHOP- or foo-1.");
        let keys: Vec<_> = s
            .iter()
            .filter(|s| s.key)
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(keys, ["SHOP-40.2", "BILL-3"]);
        assert_eq!(
            text(&s),
            "covered by SHOP-40.2. See BILL-3, not SHOP- or foo-1."
        );
    }

    #[test]
    fn the_toast_after_create_ticket() {
        let (title, text) = created_toast(
            &"SHOP-41".parse().unwrap(),
            "Not covered by any ticket: refunds endpoint",
        );
        assert_eq!(title, "Created SHOP-41 from a validation note");
        assert_eq!(
            text,
            "“Not covered by any ticket: refunds endpoint” → Backlog, prefilled from the note"
        );
    }
}
