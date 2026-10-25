//! Search and filter (UI-02, EPC-01; KEY-09: `/` focuses it; *03 · Board & columns → Search & filter*): one field does
//! both. Free text matches the key (old keys from Move to epic too), the title and the epic's title; tokens
//! filter by state (`is:needs-you`, `is:working`, `has:run`, …), by chain (`dep:SHOP-18`) or by epic (`epic:Wishlist`).
//! Everything combines (all must match). Non-matching cards hide; columns show "shown/total".
//!
//! An `epic:` token is the same as focusing the epic (EPC-18, d0080, Q55): on the board it **dims** the other cards
//! instead of hiding them, so the board keeps its shape ([`Filter::hiding`], [`Filter::strong`]); the other words
//! still hide. Focusing an epic (its track's name, `g` `e`) puts its token in the filter ([`Filter::focus`]); Esc takes
//! it out ([`Filter::unfocus`]).

use crate::view_models::flow_columns::place_set;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::StatusSet;
use otaq_core::domain::{Status, TicketKey};

use crate::view_models::chain::chain;

/// A state filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Is {
    /// Needs you: the needs-you family, Interrupted, Start interrupted, Resume failed, Parent closed, and review.
    NeedsMe,
    /// The agent works: Working, Working in background, Validating…, Merge conflict · fixing.
    Working,
    /// The M12 validation states: Validating…, Needs you · validator, Validation failed N times, Validation ended
    /// without a result (d0063).
    Validating,
    Queued,
    Parked,
    /// A Run process is serving (`has:run`; d0093: "running" is for agents only).
    Running,
    Backlog,
    Review,
    Approved,
    Done,
}

impl Is {
    pub const ALL: &[Is] = &[
        Is::NeedsMe,
        Is::Working,
        Is::Validating,
        Is::Queued,
        Is::Parked,
        Is::Running,
        Is::Backlog,
        Is::Review,
        Is::Approved,
        Is::Done,
    ];

    /// The word before the colon: `is`, or `has` for a Run process (`has:run`).
    pub fn kind(self) -> &'static str {
        match self {
            Self::Running => "has",
            _ => "is",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::NeedsMe => "needs-you",
            Self::Working => "working",
            Self::Validating => "validating",
            Self::Queued => "queued",
            Self::Parked => "parked",
            Self::Running => "run",
            Self::Backlog => "backlog",
            Self::Review => "review",
            Self::Approved => "approved",
            Self::Done => "done",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::NeedsMe => "needs you, asks, interrupted, review",
            Self::Working => "agent running now",
            Self::Validating => "validating, or its validation needs you",
            Self::Queued => "waiting to start",
            Self::Parked => "parked in Backlog, has work",
            Self::Running => "a Run process is serving",
            Self::Backlog => "not started",
            Self::Review => "ready for your review",
            Self::Approved => "approved, PR open",
            Self::Done => "merged or closed",
        }
    }

    fn matches(self, t: &Ticket) -> bool {
        match self {
            Self::NeedsMe => t.needs_user() || t.status == Status::WaitingForYou,
            Self::Working => matches!(
                t.status,
                Status::Working
                    | Status::WorkingInBackground
                    | Status::Checking
                    | Status::MergeConflictFixing
            ),
            Self::Validating => matches!(
                t.status,
                Status::Checking
                    | Status::CheckerNeedsYou
                    | Status::CheckFailed
                    | Status::CheckNoResult
            ),
            Self::Queued => t.status.is_queued(),
            Self::Parked => t.status == Status::Parked,
            Self::Running => t.flags.run_port.is_some(),
            Self::Backlog => t.place.is_backlog(),
            Self::Review => place_set(t) == StatusSet::HumanCheck,
            Self::Approved => place_set(t) == StatusSet::Delivery,
            Self::Done => t.place.is_done(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    Is(Is),
    /// In this ticket's chain (parents and children, transitively).
    Dep(TicketKey),
}

impl Token {
    /// Reads `is:needs-you`, `has:run`, `dep:SHOP-18`. The old `is:needs-me` still reads as
    /// `is:needs-you`, and `is:running` (agents only, d0093) as `is:working`.
    pub fn parse(word: &str) -> Option<Self> {
        let (kind, value) = word.split_once(':')?;
        let (kind, value) = (kind.to_lowercase(), value.to_lowercase());
        match (kind.as_str(), value.as_str()) {
            ("is", "needs-me") => return Some(Self::Is(Is::NeedsMe)),
            ("is", "running") => return Some(Self::Is(Is::Working)),
            _ => {}
        }
        match kind.as_str() {
            "is" | "has" => Is::ALL
                .iter()
                .find(|i| i.kind() == kind && i.name() == value)
                .map(|i| Self::Is(*i)),
            "dep" => value.to_uppercase().parse().ok().map(Self::Dep),
            _ => None,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Is(i) => format!("{}:{}", i.kind(), i.name()),
            Self::Dep(k) => format!("dep:{k}"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub tokens: Vec<Token>,
    pub text: String,
}

impl Filter {
    pub fn is_active(&self) -> bool {
        !self.tokens.is_empty() || !self.text.trim().is_empty()
    }

    /// Takes the field's text: complete tokens (followed by a space) become chips, the rest stays text.
    pub fn take_input(&mut self, input: &str) -> String {
        let ends_with_space = input.ends_with(' ');
        let words: Vec<&str> = input.split_whitespace().collect();
        let mut rest = Vec::new();
        for (i, word) in words.iter().enumerate() {
            let complete = i + 1 < words.len() || ends_with_space;
            match Token::parse(word) {
                Some(token) if complete => self.add(token),
                _ => rest.push(*word),
            }
        }
        let mut text = rest.join(" ");
        if ends_with_space && !text.is_empty() {
            text.push(' ');
        }
        self.text = text.trim().to_owned();
        text
    }

    pub fn add(&mut self, token: Token) {
        if !self.tokens.contains(&token) {
            self.tokens.push(token);
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.tokens.len() {
            self.tokens.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.tokens.clear();
        self.text.clear();
    }

    /// Whether a ticket stays visible. `all` is the project (for chains).
    pub fn matches(&self, t: &Ticket, all: &[Ticket]) -> bool {
        let text = self.text.trim().to_lowercase();
        let text_ok = text.is_empty()
            || text.split_whitespace().all(|word| {
                t.key.to_string().to_lowercase().contains(word)
                    || t.title.to_lowercase().contains(word)
            });
        text_ok
            && self.tokens.iter().all(|token| match token {
                Token::Is(i) => i.matches(t),
                Token::Dep(k) => chain(k, all).is_some_and(|c| c.members.contains(&t.key)),
            })
    }

    /// The keys that stay visible, or `None` when no filter is active.
    pub fn visible(&self, all: &[Ticket]) -> Option<Vec<TicketKey>> {
        self.is_active().then(|| {
            all.iter()
                .filter(|t| self.matches(t, all))
                .map(|t| t.key.clone())
                .collect()
        })
    }

    /// The suggestions under the field: every state filter, the selected ticket's chain, and each epic,
    /// with how many cards each would show.
    pub fn suggestions(
        &self,
        all: &[Ticket],
        selected: Option<&TicketKey>,
    ) -> Vec<(Token, String, usize)> {
        let count = |token: &Token| {
            let f = Filter {
                tokens: vec![token.clone()],
                text: String::new(),
            };
            all.iter().filter(|t| f.matches(t, all)).count()
        };
        let mut out: Vec<(Token, String)> = Is::ALL
            .iter()
            .map(|i| (Token::Is(*i), i.description().to_owned()))
            .collect();
        if let Some(k) = selected {
            out.push((Token::Dep(k.clone()), format!("in {k}’s chain")));
        }
        let typed = self.text.trim().to_lowercase();
        out.into_iter()
            .filter(|(t, _)| !self.tokens.contains(t))
            .filter(|(t, _)| typed.is_empty() || t.label().to_lowercase().starts_with(&typed))
            .map(|(t, d)| {
                let n = count(&t);
                (t, d, n)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;

    fn sample() -> Vec<Ticket> {
        let mut checkout = ticket("SHOP-20", "Checkout page", Status::Queued);
        checkout.depends_on = vec!["SHOP-18".parse().unwrap()];
        let mut running = ticket("SHOP-19", "Price table", Status::Working);
        running.flags.run_port = Some(4020);
        vec![
            ticket("SHOP-40", "Wishlist", Status::Backlog),
            ticket("SHOP-44", "Wishlist share link", Status::Backlog),
            ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
            checkout,
            ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou),
            running,
            ticket("SHOP-25", "Upgrade to Next.js 15", Status::Parked),
        ]
    }

    fn shown(filter: &Filter, all: &[Ticket]) -> Vec<String> {
        filter
            .visible(all)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn text_matches_key_and_title() {
        let all = sample();
        let mut f = Filter::default();
        f.take_input("checkout");
        assert_eq!(shown(&f, &all), ["SHOP-18", "SHOP-20"]);
        f.take_input("SHOP-44");
        assert_eq!(shown(&f, &all), ["SHOP-44"]);
        f.take_input("wishlist");
        assert_eq!(shown(&f, &all), ["SHOP-40", "SHOP-44"]);
        f.clear();
        assert_eq!(f.visible(&all), None);
    }

    #[test]
    fn tokens_and_combinations() {
        let all = sample();
        let mut f = Filter::default();
        let rest = f.take_input("is:needs-you ");
        assert_eq!(rest, "");
        assert_eq!(f.tokens, vec![Token::Is(Is::NeedsMe)]);
        assert_eq!(shown(&f, &all), ["SHOP-18", "SHOP-22"]);
        f.take_input("checkout");
        assert_eq!(shown(&f, &all), ["SHOP-18"], "token and text combine");
        assert_eq!(Token::parse("epic:wish"), None);
        let mut f = Filter::default();
        f.take_input("dep:shop-20 ");
        assert_eq!(shown(&f, &all), ["SHOP-18", "SHOP-20"]);
        let mut f = Filter::default();
        f.take_input("has:run is:working ");
        assert_eq!(shown(&f, &all), ["SHOP-19"]);
        assert_eq!(f.tokens[0].label(), "has:run");
        f.remove(0);
        assert_eq!(f.tokens, vec![Token::Is(Is::Working)]);
        // An unfinished token stays text until a space follows.
        let mut f = Filter::default();
        assert_eq!(f.take_input("is:park"), "is:park");
        assert!(f.tokens.is_empty());
    }

    /// d0093: the old `is:needs-me` keeps working and becomes `is:needs-you`; `is:running` is agents only.
    #[test]
    fn old_and_agent_words_still_read() {
        let all = sample();
        let mut f = Filter::default();
        assert_eq!(f.take_input("is:needs-me "), "");
        assert_eq!(f.tokens, vec![Token::Is(Is::NeedsMe)]);
        assert_eq!(f.tokens[0].label(), "is:needs-you");
        assert_eq!(shown(&f, &all), ["SHOP-18", "SHOP-22"]);
        f.take_input("is:needs-you ");
        assert_eq!(
            f.tokens.len(),
            1,
            "the alias and the new word are one token"
        );
        let mut f = Filter::default();
        f.take_input("is:running ");
        assert_eq!(f.tokens, vec![Token::Is(Is::Working)]);
        assert_eq!(Token::parse("is:run"), None, "a Run process is has:run");
        assert_eq!(Token::parse("has:running"), None);
    }

    #[test]
    fn suggestions_count_and_follow_typing() {
        let all = sample();
        let mut f = Filter::default();
        let key: TicketKey = "SHOP-18".parse().unwrap();
        let s = f.suggestions(&all, Some(&key));
        let needs = s
            .iter()
            .find(|(t, _, _)| *t == Token::Is(Is::NeedsMe))
            .unwrap();
        assert_eq!(needs.2, 2);
        assert!(s.iter().any(|(t, d, n)| *t == Token::Dep(key.clone())
            && d == "in SHOP-18’s chain"
            && *n == 2));
        f.take_input("is:p");
        let labels: Vec<String> = f
            .suggestions(&all, None)
            .iter()
            .map(|(t, _, _)| t.label())
            .collect();
        assert_eq!(labels, ["is:parked"]);
    }

    /// The M12 states (d0063, d0064): the needs-you ones need you, Validating… and Merge conflict · fixing are
    /// working, and `is:validating` gathers the validation states.
    #[test]
    fn m12_states_in_the_status_filter() {
        let all = vec![
            ticket("SHOP-29", "Coupon field", Status::Checking),
            ticket("SHOP-33", "Shipping estimate", Status::CheckerNeedsYou),
            ticket("SHOP-32", "Address autocomplete", Status::CheckFailed),
            ticket("SHOP-34", "Tax display", Status::CheckNoResult),
            ticket(
                "SHOP-44",
                "Wishlist share link",
                Status::MergeConflictFixing,
            ),
            ticket("SHOP-41", "Wishlist sync", Status::DeliveryStopped),
        ];
        let only = |token: &str| {
            let mut f = Filter::default();
            f.take_input(&format!("{token} "));
            shown(&f, &all)
        };
        assert_eq!(
            only("is:needs-you"),
            ["SHOP-33", "SHOP-32", "SHOP-34", "SHOP-41"]
        );
        assert_eq!(only("is:working"), ["SHOP-29", "SHOP-44"]);
        assert_eq!(
            only("is:validating"),
            ["SHOP-29", "SHOP-33", "SHOP-32", "SHOP-34"]
        );
        let f = Filter::default();
        let suggested = f.suggestions(&all, None);
        assert!(
            suggested
                .iter()
                .any(|(t, _, n)| *t == Token::Is(Is::Validating) && *n == 4)
        );
    }
}
