//! The header's search and filter field as the Tauri page draws it (T-0221; UI-02, EPC-01, KEY-09: `/` focuses it,
//! d0097; *03 · Board & columns → Search & filter*). The page keeps what the user typed and the chips per project ([`FilterState`]);
//! [`view`] turns it into the field: finished tokens become chips, the rest stays in the field, the "4 results" count,
//! the suggestions under it with their counts, and the matching tickets with their status, Done included (↵ opens
//! one). The board takes the same state with `board.view` and hides the cards that don't match
//! ([`FilterState::filter`]).

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::TicketKey;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::view_models::filter::{Filter, Scope, Token};
use crate::view_models::status::{self, PillView};

/// The field's placeholder, in the header and in the field.
pub const PLACEHOLDER: &str = "Search  ·  is:needs-you";

/// The suggestion list's label.
pub const SUGGESTIONS_HEADER: &str = "FILTERS";

/// The matching tickets' label.
pub const TICKETS_HEADER: &str = "TICKETS";

/// The most tickets the list shows; the count says how many match.
pub const TICKETS_SHOWN: usize = 50;

/// What the field typed in ↵ opens Recovery and Cleanup (*16 → Where Cleanup lives*).
pub const CLEANUP_WORD: &str = "cleanup";

/// The filter as the page keeps it: the chips (`"is:needs-you"`, `"dep:SHOP-18"`) and what is typed in the field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "BoardFilter")]
pub struct FilterState {
    pub tokens: Vec<String>,
    pub text: String,
}

impl FilterState {
    /// The filter it stands for. A finished token still in the text counts as a chip, so the board never shows a
    /// moment where `is:needs-you ` is searched for as words.
    pub fn filter(&self, scope: &Scope) -> Filter {
        self.read(scope).0
    }

    /// The filter, and what the field shows once finished tokens left it.
    fn read(&self, scope: &Scope) -> (Filter, String) {
        let mut filter = Filter::default();
        for token in self.tokens.iter().filter_map(|t| Token::parse(t, scope)) {
            filter.add(token);
        }
        let shown = filter.take_input(&self.text, scope);
        (filter, shown)
    }
}

/// One row under the field: `is:working` · "agent running now" · `3`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SearchSuggestion")]
pub struct Suggestion {
    /// The chip it adds.
    pub token: String,
    pub description: String,
    pub count: String,
}

/// One matching ticket in the list: `SHOP-18` · "Checkout API" · Review · its status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SearchTicket")]
pub struct ListedTicket {
    /// What ↵ or a click opens.
    pub key: String,
    pub title: String,
    /// The column its card is in ("Backlog", a stage's name, "Done").
    pub column: String,
    pub status: PillView,
}

/// The field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SearchView")]
pub struct SearchView {
    /// The filter with finished tokens moved into the chips; `text` is what the field shows.
    pub filter: FilterState,
    pub placeholder: String,
    /// The user's key for Filter the board (`/`), shown while no filter is active.
    pub keys: Option<String>,
    /// "4 results" while a filter is active.
    pub results: Option<String>,
    pub suggestions_header: String,
    pub suggestions: Vec<Suggestion>,
    pub tickets_header: String,
    /// The tickets the filter matches, Done included (last), while a filter is active.
    pub tickets: Vec<ListedTicket>,
    /// What ↵ adds: the first suggestion while a token is half typed (`is:wor`).
    pub enter_adds: Option<String>,
    /// ↵ opens Recovery and Cleanup: the field says "cleanup".
    pub enter_opens_cleanup: bool,
    /// What ↵ opens when it adds nothing and opens no Cleanup: the first ticket listed.
    pub enter_opens: Option<String>,
}

/// The field for `state` over the project's `tickets`; `selected` is the open ticket (its chain is suggested), `scope`
/// the project's columns and agents (`in:`, `by:`).
pub fn view(
    state: &FilterState,
    tickets: &[Ticket],
    selected: Option<&TicketKey>,
    scope: &Scope,
    keys: &KeyLabels,
) -> SearchView {
    let (filter, shown) = state.read(scope);
    let visible = filter.visible(tickets, scope);
    let results = visible.as_ref().map(|keys| {
        let n = keys.len();
        format!("{n} result{}", if n == 1 { "" } else { "s" })
    });
    let mut listed: Vec<&Ticket> = match &visible {
        Some(keys) => tickets.iter().filter(|t| keys.contains(&t.key)).collect(),
        None => Vec::new(),
    };
    listed.sort_by_key(|t| t.place.is_done());
    let listed: Vec<ListedTicket> = listed
        .into_iter()
        .take(TICKETS_SHOWN)
        .map(|t| ListedTicket {
            key: t.key.to_string(),
            title: t.title.clone(),
            column: scope.column_of(&t.key).unwrap_or_default().to_owned(),
            status: PillView::of(status::shown(t)),
        })
        .collect();
    let suggestions: Vec<Suggestion> = filter
        .suggestions(tickets, selected, scope)
        .into_iter()
        .map(|(token, description, count)| Suggestion {
            token: token.label(),
            description,
            count: count.to_string(),
        })
        .collect();
    let enter_adds = filter
        .text
        .contains(':')
        .then(|| suggestions.first().map(|s| s.token.clone()))
        .flatten();
    let enter_opens_cleanup =
        filter.tokens.is_empty() && filter.text.trim().eq_ignore_ascii_case(CLEANUP_WORD);
    let enter_opens = (enter_adds.is_none() && !enter_opens_cleanup)
        .then(|| listed.first().map(|t| t.key.clone()))
        .flatten();
    SearchView {
        enter_opens_cleanup,
        filter: FilterState {
            tokens: filter.tokens.iter().map(Token::label).collect(),
            text: shown,
        },
        placeholder: PLACEHOLDER.to_owned(),
        keys: results
            .is_none()
            .then(|| keys.label(KeyAction::FilterBoard))
            .flatten(),
        results,
        suggestions_header: SUGGESTIONS_HEADER.to_owned(),
        suggestions,
        tickets_header: TICKETS_HEADER.to_owned(),
        tickets: listed,
        enter_adds,
        enter_opens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_labels::fixtures;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::{Place, Status};

    const MAC: fn() -> KeyLabels = fixtures::base;

    fn sample() -> Vec<Ticket> {
        vec![
            ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
            ticket("SHOP-20", "Checkout page", Status::Queued),
            ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou),
            ticket("SHOP-19", "Price table", Status::Working),
        ]
    }

    fn state(tokens: &[&str], text: &str) -> FilterState {
        FilterState {
            tokens: tokens.iter().map(|t| (*t).to_owned()).collect(),
            text: text.to_owned(),
        }
    }

    fn scope(all: &[Ticket]) -> Scope {
        Scope::of(&otaq_core::flow::default_flow(), all, None, None)
    }

    fn view_of(state: &FilterState, all: &[Ticket], selected: Option<&TicketKey>) -> SearchView {
        view(state, all, selected, &scope(all), &MAC())
    }

    #[test]
    fn a_finished_token_becomes_a_chip_and_the_rest_stays_typed() {
        let all = sample();
        let v = view_of(&state(&[], "is:needs-you "), &all, None);
        assert_eq!(v.filter, state(&["is:needs-you"], ""));
        assert_eq!(v.results.as_deref(), Some("2 results"));
        assert_eq!(v.keys, None, "the count takes the key's place");
        let v = view_of(&state(&["is:needs-you"], "checkout"), &all, None);
        assert_eq!(v.filter, state(&["is:needs-you"], "checkout"));
        assert_eq!(v.results.as_deref(), Some("1 result"));
        let v = view_of(&state(&[], ""), &all, None);
        assert_eq!(v.results, None);
        assert_eq!(v.keys.as_deref(), Some("/"));
        assert_eq!(v.placeholder, PLACEHOLDER);
    }

    /// KEY-08, KEY-09: the field shows the user's key for Filter the board (`/` by default), none once removed or with
    /// letters off.
    #[test]
    fn the_field_shows_the_users_filter_key() {
        let all = sample();
        let keys =
            |labels: &KeyLabels| view(&state(&[], ""), &all, None, &scope(&all), labels).keys;
        assert_eq!(keys(&MAC()).as_deref(), Some("/"));
        assert_eq!(
            keys(&fixtures::replaced("filter_board", None, 0, "f")).as_deref(),
            Some("f")
        );
        assert_eq!(keys(&fixtures::removed("filter_board", None, 0)), None);
        assert_eq!(keys(&fixtures::letters_off()), None);
    }

    #[test]
    fn suggestions_carry_their_counts_and_enter_takes_the_first() {
        let all = sample();
        let key: TicketKey = "SHOP-18".parse().unwrap();
        let v = view_of(&state(&[], ""), &all, Some(&key));
        assert_eq!(v.suggestions_header, "FILTERS");
        assert_eq!(
            v.suggestions[0],
            Suggestion {
                token: "is:needs-you".into(),
                description: "needs you, asks, interrupted, review".into(),
                count: "2".into(),
            }
        );
        assert!(v.suggestions.iter().any(|s| s.token == "dep:SHOP-18"));
        assert_eq!(v.enter_adds, None, "nothing half typed");
        let v = view_of(&state(&[], "is:wor"), &all, None);
        assert_eq!(v.enter_adds.as_deref(), Some("is:working"));
        assert_eq!(v.enter_opens, None, "↵ adds the filter, it opens nothing");
        let v = view_of(&state(&["is:working"], ""), &all, None);
        assert!(
            v.suggestions.iter().all(|s| s.token != "is:working"),
            "a chip already there isn't suggested again"
        );
    }

    #[test]
    fn cleanup_opens_recovery_and_cleanup() {
        let all = sample();
        let v = view_of(&state(&[], "Cleanup"), &all, None);
        assert!(v.enter_opens_cleanup);
        assert_eq!(v.enter_opens, None);
        assert!(!view_of(&state(&["is:working"], "cleanup"), &all, None).enter_opens_cleanup);
    }

    #[test]
    fn the_board_filter_reads_chips_and_unfinished_text() {
        let all = sample();
        let scope = scope(&all);
        let shown = |s: FilterState| -> Vec<String> {
            s.filter(&scope)
                .visible(&all, &scope)
                .unwrap_or_default()
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        assert_eq!(shown(state(&["is:needs-you"], "")), ["SHOP-18", "SHOP-22"]);
        assert_eq!(shown(state(&[], "is:needs-you checkout")), ["SHOP-18"]);
        assert!(
            !state(&["not-a-token"], "").filter(&scope).is_active(),
            "an unknown chip is dropped, leaving no filter"
        );
        assert!(!state(&[], "  ").filter(&scope).is_active());
    }

    /// Search also lists the matching tickets with their column and status, Done ones included (after the rest), and
    /// ↵ opens the first; nothing is listed while no filter is active.
    #[test]
    fn search_lists_the_matching_tickets_done_included() {
        let mut all = sample();
        all.insert(0, ticket("SHOP-9", "Checkout copy", Status::Merged));
        let v = view_of(&state(&[], "checkout"), &all, None);
        assert_eq!(v.tickets_header, "TICKETS");
        let rows: Vec<(&str, &str, &str)> = v
            .tickets
            .iter()
            .map(|t| (t.key.as_str(), t.column.as_str(), t.status.label.as_str()))
            .collect();
        assert_eq!(
            rows[2],
            (
                "SHOP-9",
                "Done",
                PillView::of(Status::Merged).label.as_str()
            )
        );
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, "SHOP-18");
        assert_eq!(v.tickets[0].title, "Checkout API");
        assert_eq!(v.results.as_deref(), Some("3 results"));
        assert_eq!(v.enter_opens.as_deref(), Some("SHOP-18"));
        let v = view_of(&state(&[], ""), &all, None);
        assert!(v.tickets.is_empty());
        assert_eq!(v.enter_opens, None);
    }

    /// `in:` keeps the tickets of one column, named by the stage's name or id in any case, Backlog and Done included;
    /// a stage the flow doesn't have stays typed text, like any unknown filter.
    #[test]
    fn in_keeps_one_columns_tickets() {
        // SHOP-18 waits for the user's review in Review; SHOP-20 waits to begin, at the top of Dev.
        let mut all = sample();
        all.push(ticket("SHOP-31", "Someday", Status::Backlog));
        all.push(ticket("SHOP-32", "Shipped", Status::Merged));
        assert_eq!(all[1].place, Place::Waiting(None));
        let keys =
            |v: &SearchView| -> Vec<String> { v.tickets.iter().map(|t| t.key.clone()).collect() };
        let v = view_of(&state(&[], "in:review "), &all, None);
        assert_eq!(v.filter, state(&["in:review"], ""));
        assert_eq!(keys(&v), ["SHOP-18"]);
        let in_dev = keys(&view_of(&state(&[], "in:dev "), &all, None));
        assert_eq!(in_dev, ["SHOP-20", "SHOP-22", "SHOP-19"]);
        assert_eq!(v.tickets[0].column, "Review");
        let v = view_of(&state(&[], "IN:Review "), &all, None);
        assert_eq!(v.filter.tokens, ["in:review"], "any case, one chip");
        assert_eq!(
            keys(&view_of(&state(&[], "in:backlog "), &all, None)),
            ["SHOP-31"]
        );
        assert_eq!(
            keys(&view_of(&state(&[], "in:done "), &all, None)),
            ["SHOP-32"]
        );
        let v = view_of(&state(&[], "in:nowhere "), &all, None);
        assert!(v.filter.tokens.is_empty(), "an unknown stage stays text");
        assert_eq!(v.results.as_deref(), Some("0 results"));
        // Half typed, the stages are suggested.
        let v = view_of(&state(&[], "in:rev"), &all, None);
        assert_eq!(v.enter_adds.as_deref(), Some("in:review"));
        assert_eq!(v.suggestions[0].count, "1");
    }

    /// `by:` keeps the tickets whose current stage is worked by that agent (the ticket's own pick for the stage, else
    /// the stage's); an agent nobody has stays typed text.
    #[test]
    fn by_keeps_the_tickets_an_agent_works() {
        use otaq_core::agents::facts::{BoardAgents, CardAgents};
        let all = sample();
        let card = |key: &str, agent: Option<&str>| CardAgents {
            key: key.parse().unwrap(),
            running_developer: None,
            running_validator: None,
            running_developer_on: None,
            running_validator_on: None,
            holds: Vec::new(),
            starts: None,
            refused_agent: None,
            stage_agent: agent.map(Into::into),
        };
        let agents = BoardAgents {
            agent_count: 2,
            cards: vec![
                card("SHOP-18", Some("Strong")),
                card("SHOP-20", Some("Developer")),
                card("SHOP-22", Some("Strong")),
                card("SHOP-19", None),
            ],
            connected: Vec::new(),
        };
        let scope = Scope::of(&otaq_core::flow::default_flow(), &all, Some(&agents), None);
        let listed = |text: &str| {
            let v = view(&state(&[], text), &all, None, &scope, &MAC());
            (
                v.filter.tokens,
                v.tickets.into_iter().map(|t| t.key).collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            listed("by:Strong "),
            (
                vec!["by:strong".to_owned()],
                vec!["SHOP-18".to_owned(), "SHOP-22".to_owned()]
            )
        );
        assert_eq!(listed("by:developer ").1, ["SHOP-20"]);
        let (tokens, keys) = listed("by:nobody ");
        assert!(tokens.is_empty(), "an unknown agent stays text");
        assert!(keys.is_empty());
    }
}
