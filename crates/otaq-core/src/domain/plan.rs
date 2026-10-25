//! A ticket's Plan section (d0133): Markdown next to the description, written by the planning stage (Milestone plan)
//! or Noyan, which the user reads and edits like the description. It holds one **Pieces** list in a set shape the
//! skills write (audit answer 1):
//!
//! ```markdown
//! ## Pieces
//!
//! 1. Wishlist API — waits for nothing
//! 2. Wishlist button — waits for 1
//!    The heart on each product card.
//! 3. Wishlist page — waits for 1
//! 4. Share link — waits for 2, 3
//! ```
//!
//! The line naming the list is "Pieces" at any heading level or in bold; items are numbered ("1." or "1)") or
//! bulleted (numbered in order), "waits for" follows an em dash, an en dash or a plain dash. [`pieces`] reads it
//! **leniently**: a plan that doesn't parse has no pieces and no error, a broken item is skipped. Piece numbers are
//! display names within their ticket ("40.3" is piece 3 of SHOP-40, [`piece_name`]), never ticket keys (audit
//! answer 2). Statuses (working, merged) are Otaq's own records, never written into the text.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{Actor, TicketKey};

/// One piece of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Piece {
    /// Its number within the plan (display name "40.3" with [`piece_name`]).
    pub number: u32,
    /// "Wishlist page".
    pub title: String,
    /// The pieces it waits for, by number; empty for "waits for nothing" or no "waits for".
    pub waits_for: Vec<u32>,
    /// The lines under the item until the next one, indentation removed (may be empty).
    pub text: String,
}

/// The pieces of `plan`'s Pieces list, in the order written. No list, or nothing that reads as an item: none.
pub fn pieces(plan: &str) -> Vec<Piece> {
    let mut lines = plan.lines();
    if !lines.by_ref().any(is_pieces_label) {
        return Vec::new();
    }
    let mut out: Vec<Piece> = Vec::new();
    // Whether the last piece takes the text lines that follow: not before the first item, nor after a broken one.
    let mut open = false;
    let mut item_indent = None;
    for line in lines {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if ends_list(trimmed) {
            break;
        }
        let top = item_indent.is_none_or(|i| indent <= i);
        if top && let Some((number, rest)) = item(trimmed) {
            item_indent.get_or_insert(indent);
            let number = number.unwrap_or_else(|| out.last().map_or(1, |p| p.number + 1));
            open = false;
            if let Some(piece) = parse_item(number, rest)
                && !out.iter().any(|p| p.number == piece.number)
            {
                out.push(piece);
                open = true;
            }
            continue;
        }
        if open
            && let Some(piece) = out.last_mut()
            && (!piece.text.is_empty() || !trimmed.is_empty())
        {
            piece.text.push_str(trimmed);
            piece.text.push('\n');
        }
    }
    for piece in &mut out {
        piece.text = piece.text.trim_end().to_owned();
    }
    out
}

/// Piece `number` of `plan`, if its Pieces list has it (`otaq show SHOP-40 --piece 3`).
pub fn piece(plan: &str, number: u32) -> Option<Piece> {
    pieces(plan).into_iter().find(|p| p.number == number)
}

/// "40.3": piece 3 of SHOP-40 as the UI and the change note name it.
pub fn piece_name(key: &TicketKey, number: u32) -> String {
    format!("{}.{number}", key.number)
}

/// The pieces in the order they can run: those waiting for nothing first, then each piece once everything it waits
/// for came earlier; `[[1], [2, 3], [4]]`. A wait on a number the plan doesn't have is ignored; pieces waiting on
/// each other (a cycle) come last, together.
pub fn waves(pieces: &[Piece]) -> Vec<Vec<u32>> {
    let known: Vec<u32> = pieces.iter().map(|p| p.number).collect();
    let mut placed: Vec<u32> = Vec::new();
    let mut out: Vec<Vec<u32>> = Vec::new();
    while placed.len() < pieces.len() {
        let wave: Vec<u32> = pieces
            .iter()
            .filter(|p| !placed.contains(&p.number))
            .filter(|p| {
                p.waits_for
                    .iter()
                    .all(|w| placed.contains(w) || !known.contains(w))
            })
            .map(|p| p.number)
            .collect();
        let wave = if wave.is_empty() {
            pieces
                .iter()
                .map(|p| p.number)
                .filter(|n| !placed.contains(n))
                .collect()
        } else {
            wave
        };
        placed.extend(&wave);
        out.push(wave);
    }
    out
}

/// "Pieces: 4 (1 → 2, 3 → 4)": the count and the order [`waves`] gives (`prime`); `None` with no pieces.
pub fn order_line(pieces: &[Piece]) -> Option<String> {
    if pieces.is_empty() {
        return None;
    }
    let order = waves(pieces)
        .iter()
        .map(|wave| {
            wave.iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect::<Vec<_>>()
        .join(" \u{2192} ");
    Some(format!("Pieces: {} ({order})", pieces.len()))
}

/// Who wrote the Plan last (`plan_by`): the user, Noyan, or a stage's agent by its stage's name ("Milestone plan").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlanBy {
    /// A person, by OS user name.
    User(String),
    /// A Project agent (Noyan), by its id.
    Noyan(String),
    /// A ticket's agent, by the name of the stage it wrote from.
    Stage(String),
}

const STAGE_PREFIX: &str = "stage:";

impl PlanBy {
    /// The writer when `actor` saves the Plan; `stage` is the name of the stage a ticket's agent writes from.
    pub fn of(actor: &Actor, stage: Option<&str>) -> Self {
        match actor {
            Actor::User(name) => Self::User(name.clone()),
            Actor::ProjectAgent(id) => Self::Noyan(id.clone()),
            Actor::Agent(key) => Self::Stage(stage.map_or_else(|| key.to_string(), str::to_owned)),
        }
    }

    /// "you", "Noyan", "Milestone plan": as "Written by …" names it.
    pub fn label(&self) -> &str {
        match self {
            Self::User(_) => "you",
            Self::Noyan(_) => "Noyan",
            Self::Stage(name) => name,
        }
    }
}

/// Stored like an [`Actor`] (`me`, `project-agent:<id>`), a stage's agent as `stage:<stage name>`.
impl fmt::Display for PlanBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User(name) => Actor::User(name.clone()).fmt(f),
            Self::Noyan(id) => Actor::ProjectAgent(id.clone()).fmt(f),
            Self::Stage(name) => write!(f, "{STAGE_PREFIX}{name}"),
        }
    }
}

impl FromStr for PlanBy {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(name) = s.strip_prefix(STAGE_PREFIX) {
            return Ok(Self::Stage(name.to_owned()));
        }
        Ok(match s.parse::<Actor>()? {
            Actor::User(name) => Self::User(name),
            Actor::ProjectAgent(id) => Self::Noyan(id),
            Actor::Agent(key) => Self::Stage(key.to_string()),
        })
    }
}

impl Serialize for PlanBy {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PlanBy {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        Ok(s.parse().expect("infallible"))
    }
}

/// "Pieces", "## Pieces", "**Pieces:**": the line naming the list.
fn is_pieces_label(line: &str) -> bool {
    let label = line.trim().trim_start_matches('#').trim();
    let label = label.trim_matches(|c| c == '*' || c == '_').trim();
    let label = label.strip_suffix(':').unwrap_or(label);
    let label = label.trim_matches(|c| c == '*' || c == '_').trim();
    label.eq_ignore_ascii_case("pieces")
}

/// A heading, a bold line of its own or a rule: the list is over.
fn ends_list(line: &str) -> bool {
    line.starts_with('#')
        || line == "---"
        || line == "***"
        || (line.len() > 4 && line.starts_with("**") && line.trim_end_matches(':').ends_with("**"))
}

/// An item line: its explicit number (`None` for a bullet) and what follows the marker.
fn item(line: &str) -> Option<(Option<u32>, &str)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            // "- 2. Wishlist button": a bullet that carries a number.
            return Some(match numbered(rest.trim_start()) {
                Some((n, rest)) => (Some(n), rest),
                None => (None, rest),
            });
        }
    }
    numbered(line).map(|(n, rest)| (Some(n), rest))
}

/// "3. Wishlist page" or "3) Wishlist page" → (3, "Wishlist page").
fn numbered(line: &str) -> Option<(u32, &str)> {
    let digits = line.find(|c: char| !c.is_ascii_digit())?;
    let n = line[..digits].parse().ok()?;
    let rest = line[digits..]
        .strip_prefix(". ")
        .or_else(|| line[digits..].strip_prefix(") "))?;
    Some((n, rest))
}

/// "Share link — waits for 2, 3" → the piece; `None` for an item with no title.
fn parse_item(number: u32, rest: &str) -> Option<Piece> {
    let (title, waits) = split_waits(rest);
    let title = title
        .trim()
        .trim_matches(|c| c == '*' || c == '_')
        .trim()
        .trim_end_matches([':', '.', ','])
        .trim();
    if title.is_empty() {
        return None;
    }
    let waits_for = waits.map(|w| wait_numbers(w, number)).unwrap_or_default();
    Some(Piece {
        number,
        title: title.to_owned(),
        waits_for,
        text: String::new(),
    })
}

/// Splits "Share link — waits for 2, 3" at its dash: ("Share link", Some("2, 3")).
fn split_waits(rest: &str) -> (&str, Option<&str>) {
    let lower = rest.to_ascii_lowercase();
    let Some(at) = lower.find("waits for") else {
        return (rest, None);
    };
    let before = rest[..at].trim_end();
    let title = before
        .strip_suffix('\u{2014}')
        .or_else(|| before.strip_suffix('\u{2013}'))
        .or_else(|| before.strip_suffix("--"))
        .or_else(|| before.strip_suffix('-'))
        .or_else(|| before.strip_suffix(','))
        .or_else(|| before.strip_suffix('('))
        .unwrap_or(before);
    (title, Some(&rest[at + "waits for".len()..]))
}

/// "2, 3", "1 and 2", "40.1", "nothing" → the piece numbers (never `own`).
fn wait_numbers(text: &str, own: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for token in text.split(|c: char| c == ',' || c.is_whitespace()) {
        let token = token.trim_matches(|c: char| !c.is_ascii_digit());
        // "40.3" names piece 3.
        let token = token.rsplit('.').next().unwrap_or(token);
        if let Ok(n) = token.parse::<u32>()
            && n != own
            && !out.contains(&n)
        {
            out.push(n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(pieces: &[Piece]) -> Vec<u32> {
        pieces.iter().map(|p| p.number).collect()
    }

    const FIXED: &str = "We split the wishlist so the API lands first.\n\n## Pieces\n\n\
        1. Wishlist API — waits for nothing\n\
        2. Wishlist button — waits for 1\n   The heart on each product card.\n   Toggles without a reload.\n\
        3. Wishlist page — waits for 1\n\
        4. Share link — waits for 2, 3\n\n## Risks\n\n1. Not a piece\n";

    #[test]
    fn the_fixed_shape() {
        let p = pieces(FIXED);
        assert_eq!(numbers(&p), [1, 2, 3, 4]);
        assert_eq!(p[0].title, "Wishlist API");
        assert!(p[0].waits_for.is_empty(), "waits for nothing");
        assert_eq!(p[1].waits_for, [1]);
        assert_eq!(
            p[1].text,
            "The heart on each product card.\nToggles without a reload."
        );
        assert_eq!(p[3].title, "Share link");
        assert_eq!(p[3].waits_for, [2, 3]);
        assert_eq!(p[3].text, "", "the next heading ends the list");
        assert_eq!(
            order_line(&p).as_deref(),
            Some("Pieces: 4 (1 \u{2192} 2, 3 \u{2192} 4)")
        );
    }

    #[test]
    fn a_bulleted_shape_with_a_bold_label_and_plain_dashes() {
        let plan = "**Pieces:**\n\n- Wishlist API\n- Wishlist button - waits for 1\n  * a sub-point stays text\n\
                    - **Share link** – Waits for 40.1 and 2\n";
        let p = pieces(plan);
        assert_eq!(numbers(&p), [1, 2, 3]);
        assert_eq!(p[1].title, "Wishlist button");
        assert_eq!(p[1].text, "* a sub-point stays text");
        assert_eq!(p[2].title, "Share link");
        assert_eq!(p[2].waits_for, [1, 2]);
        assert_eq!(
            order_line(&p).as_deref(),
            Some("Pieces: 3 (1 \u{2192} 2 \u{2192} 3)")
        );
    }

    #[test]
    fn no_list_no_pieces() {
        assert!(pieces("").is_empty());
        assert!(pieces("Just a plan.\n\n1. A numbered list\n2. Under no Pieces line").is_empty());
        assert!(pieces("## Pieces\n\nNothing listed yet.").is_empty());
        assert_eq!(order_line(&[]), None);
    }

    #[test]
    fn a_broken_item_is_skipped_and_the_rest_parsed() {
        let plan = "### Pieces\n1. Wishlist API\n2. — waits for 1\n2. Wishlist button — waits for 1\n\
                    2. Duplicate number\n3) Wishlist page — waits for 9, 3, 1\n";
        let p = pieces(plan);
        assert_eq!(numbers(&p), [1, 2, 3]);
        assert_eq!(p[1].title, "Wishlist button");
        assert_eq!(p[2].title, "Wishlist page");
        assert_eq!(p[2].waits_for, [9, 1], "its own number dropped");
        // 9 isn't in the plan: ignored for the order.
        assert_eq!(waves(&p), [vec![1], vec![2, 3]]);
    }

    #[test]
    fn a_cycle_comes_last() {
        let p = pieces("## Pieces\n1. A\n2. B — waits for 3\n3. C — waits for 2\n");
        assert_eq!(waves(&p), [vec![1], vec![2, 3]]);
    }

    #[test]
    fn one_piece_and_names() {
        let key: TicketKey = "SHOP-40".parse().unwrap();
        assert_eq!(piece_name(&key, 3), "40.3");
        assert_eq!(piece(FIXED, 3).unwrap().title, "Wishlist page");
        assert_eq!(piece(FIXED, 5), None);
    }

    #[test]
    fn plan_by_round_trips() {
        for by in [
            PlanBy::User("me".into()),
            PlanBy::Noyan("pa1".into()),
            PlanBy::Stage("Milestone plan".into()),
        ] {
            assert_eq!(by.to_string().parse::<PlanBy>().unwrap(), by);
        }
        assert_eq!(PlanBy::User("me".into()).label(), "you");
        assert_eq!(PlanBy::Noyan("pa1".into()).label(), "Noyan");
        assert_eq!(
            PlanBy::of(
                &Actor::Agent("SHOP-40".parse().unwrap()),
                Some("Milestone plan")
            )
            .label(),
            "Milestone plan"
        );
    }
}
