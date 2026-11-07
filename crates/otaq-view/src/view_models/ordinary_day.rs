//! An ordinary day on shop-web: fifteen tickets on the default flow, the board the design draws for a project in
//! everyday use. Two need the user (a question, a failed start), two wait for the user's review, one waits for
//! another to merge, one works on its own agent for Dev, and Done has a merge from today and two from yesterday. The
//! gallery and the tests build on it.

use otaq_core::agents::facts::{BoardAgents, CardAgents};
use otaq_core::board::human_check::Arrival;
use otaq_core::board::tickets::{AgentSignal, StartProblem, Ticket};
use otaq_core::domain::{Place, Priority, StartStep, Status};

use crate::view_models::card_fixtures::{NOW, ticket};

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;

fn at(key: &str, title: &str, status: Status, place: Place, since_ms: Option<i64>) -> Ticket {
    let mut t = ticket(key, title, status);
    t.place = place;
    t.status_since = since_ms.map(|ago| NOW - ago);
    t.updated_at = NOW - since_ms.unwrap_or(DAY);
    t
}

/// The fifteen tickets, in board order.
pub fn tickets() -> Vec<Ticket> {
    let mut coupon = at(
        "SHOP-31",
        "Coupon field rejects lowercase codes",
        Status::Backlog,
        Place::Backlog,
        None,
    );
    coupon.priority = Priority::High;
    let mut summary = at(
        "SHOP-30",
        "Checkout summary page",
        Status::Queued,
        Place::Waiting(None),
        Some(30 * MIN),
    );
    summary.depends_on = vec!["SHOP-27".parse().expect("key")];
    summary.queued_at = Some(1);
    let mut login = at(
        "SHOP-22",
        "Fix flaky login redirect",
        Status::AsksYou,
        Place::stage_of("dev"),
        Some(12 * MIN),
    );
    login.signal = Some(AgentSignal {
        text: Some("Keep the old session cookie?".into()),
        since: Some(NOW - 2 * MIN),
        wakeup_at: None,
    });
    let mut badge = at(
        "SHOP-24",
        "Cart badge count",
        Status::StartInterrupted,
        Place::stage_of("dev"),
        Some(5 * MIN),
    );
    badge.start_problem = Some(StartProblem {
        step: StartStep::Setup,
        message: "pnpm install exited 1".into(),
    });
    let mut wishlist = at(
        "SHOP-27",
        "Wishlist share link",
        Status::Working,
        Place::stage_of("dev"),
        Some(26 * MIN),
    );
    wishlist.priority = Priority::Urgent;
    wishlist.flags.last_signal_at = Some(NOW - 20_000);
    let mut typo = at(
        "SHOP-29",
        "Typo sweep on the help pages",
        Status::Working,
        Place::stage_of("dev"),
        Some(3 * MIN),
    );
    typo.flags.last_signal_at = Some(NOW - 10_000);
    let mut rounding = at(
        "SHOP-26",
        "Price table rounding",
        Status::Checking,
        Place::stage_of("validate"),
        Some(2 * MIN),
    );
    rounding.flags.check = Some("Validate".into());
    rounding.flags.last_signal_at = Some(NOW - 5_000);
    let ready = |key: &str, title: &str, ago: i64| {
        let mut t = at(
            key,
            title,
            Status::WaitingForYou,
            Place::stage_of("review"),
            Some(ago),
        );
        t.flags.arrival = Some(Arrival::Passed {
            stage: "Validate".into(),
            round: Some(1),
        });
        t
    };
    let mut pr = at(
        "SHOP-17",
        "Search results empty state",
        Status::PrOpen,
        Place::stage_of("review"),
        Some(2 * HOUR),
    );
    pr.flags.pr_number = Some(481);
    let mut not_merged = at(
        "SHOP-15",
        "Remove the old banner",
        Status::ClosedNotMerged,
        Place::Done,
        Some(DAY),
    );
    not_merged.flags.check = None;
    vec![
        coupon,
        at(
            "SHOP-33",
            "Add sitemap.xml",
            Status::Backlog,
            Place::Backlog,
            None,
        ),
        at(
            "SHOP-34",
            "Order history pagination",
            Status::Backlog,
            Place::Backlog,
            None,
        ),
        summary,
        login,
        badge,
        wishlist,
        typo,
        rounding,
        ready("SHOP-18", "Checkout API timeout", HOUR),
        ready("SHOP-21", "Footer links", 40 * MIN),
        pr,
        at(
            "SHOP-16",
            "Product image zoom",
            Status::Merged,
            Place::Done,
            Some(0),
        ),
        not_merged,
        at(
            "SHOP-14",
            "Fix the 404 page",
            Status::Merged,
            Place::Done,
            Some(DAY),
        ),
    ]
}

/// The project's agents: Strong and Light work in Dev (Strong is Dev's own; SHOP-29 chose Light), Verifier checks.
pub fn agents() -> BoardAgents {
    let card = |key: &str| CardAgents {
        key: key.parse().expect("key"),
        running_developer: None,
        running_validator: None,
        running_developer_on: None,
        running_validator_on: None,
        holds: Vec::new(),
        starts: None,
        refused_agent: None,
        stage_agent: None,
        instead_of: None,
        skips: Vec::new(),
    };
    let working = |key: &str, agent: &str| CardAgents {
        running_developer: Some(agent.into()),
        stage_agent: Some(agent.into()),
        ..card(key)
    };
    BoardAgents {
        agent_count: 3,
        cards: vec![
            working("SHOP-22", "Strong"),
            working("SHOP-27", "Strong"),
            CardAgents {
                instead_of: Some("Strong".into()),
                ..working("SHOP-29", "Light")
            },
            CardAgents {
                running_validator: Some("Verifier".into()),
                stage_agent: Some("Verifier".into()),
                ..card("SHOP-26")
            },
        ],
        connected: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::settings::ViewPrefs;
    use otaq_core::flow::default_flow;

    use super::*;
    use crate::key_labels::KeyLabels;
    use crate::keymap::Platform;
    use crate::view_models::board_page::{self, BoardCard, BoardPage, Params, ProjectFacts};
    use crate::view_models::card::Tone;
    use crate::view_models::card_page::TicketCardView;

    fn board() -> BoardPage {
        let flow = default_flow();
        let view = ViewPrefs::default();
        let agents = agents();
        board_page::page(
            &tickets(),
            &ProjectFacts {
                flow: &flow,
                name: "shop-web",
                repo_path: "/Users/me/code/shop-web",
                base_branch: "main",
                agent_names: &[],
                was_validating: &[],
                agent_filter: None,
                agents: Some(&agents),
                agent_list: None,
                starts_held: None,
                view: &view,
                paused: false,
                helpers: &[],
            },
            &Params {
                project: "SHOP".into(),
                width: 1440.,
                height: 900.,
                ..Params::default()
            },
            NOW,
            &KeyLabels::base(Platform::Mac),
        )
    }

    fn cards(page: &BoardPage, column: usize) -> Vec<&TicketCardView> {
        let c = &page.columns[column];
        let dev = c.dev.iter().flat_map(|d| d.top.iter().chain(&d.working));
        c.cards
            .iter()
            .chain(dev)
            .map(|b| match b {
                BoardCard::Ticket { card, .. } => &**card,
            })
            .collect()
    }

    /// A card as the design reads it: key, P1/P2, title, status line, the note, who and how long, the action.
    fn read(card: &TicketCardView) -> String {
        let mut parts = vec![card.key.clone()];
        parts.extend(card.priority.as_ref().map(|p| p.short.clone()));
        parts.push(card.title.clone());
        parts.extend(card.status.as_ref().map(|s| s.text.clone()));
        parts.extend(card.note.as_ref().map(|n| n.text.clone()));
        parts.extend(card.who.clone());
        parts.extend(card.time.as_ref().map(|t| t.text.clone()));
        parts.extend(card.action.as_ref().map(|a| format!("[{}]", a.label)));
        parts.join(" | ")
    }

    #[test]
    fn the_ordinary_day_board_reads_as_drawn() {
        let page = board();
        let heads: Vec<(&str, &str, &str)> = page
            .columns
            .iter()
            .map(|c| (c.title.as_str(), c.count.as_str(), c.who_acts.as_str()))
            .collect();
        assert_eq!(
            heads,
            [
                ("Backlog", "3", "Not started"),
                ("Dev", "5", "An agent does the work"),
                ("Validate", "1", "A checker tests it"),
                ("Review", "3", "You review and approve"),
                ("Done", "3", "Merged"),
            ]
        );
        let column = |n: usize| -> Vec<String> { cards(&page, n).into_iter().map(read).collect() };
        assert_eq!(
            column(0),
            [
                "SHOP-31 | P2 | Coupon field rejects lowercase codes | Not started | [Start]",
                "SHOP-33 | Add sitemap.xml | Not started | [Start]",
                "SHOP-34 | Order history pagination | Not started | [Start]",
            ]
        );
        assert_eq!(
            column(1),
            [
                "SHOP-30 | Checkout summary page | Waits for SHOP-27 to merge",
                "SHOP-24 | Cart badge count | Start failed \u{b7} setup command failed | [Retry]",
                "SHOP-22 | Fix flaky login redirect | Asks you \u{b7} keep the old session cookie? | Strong \
                 | 12m | [Answer]",
                "SHOP-27 | P1 | Wishlist share link | Working | Strong | 26m",
                "SHOP-29 | Typo sweep on the help pages | Working | Dev: Light instead of Strong | Light | 3m",
            ]
        );
        assert_eq!(
            column(2),
            ["SHOP-26 | Price table rounding | Checking | Verifier | 2m"]
        );
        assert_eq!(
            column(3),
            [
                "SHOP-18 | Checkout API timeout | Ready for you \u{b7} passed Validate | 1h | [Review]",
                "SHOP-21 | Footer links | Ready for you \u{b7} passed Validate | 40m | [Review]",
                "SHOP-17 | Search results empty state | PR #481 open | 2h | [Mark merged]",
            ]
        );
        assert_eq!(
            column(4),
            [
                "SHOP-14 | Fix the 404 page | Merged into main | yesterday",
                "SHOP-15 | Remove the old banner | Done \u{b7} not merged | yesterday",
                "SHOP-16 | Product image zoom | Merged into main | today",
            ]
        );
    }

    /// Coloured edges only where the card needs the user: turquoise asks, the review colour is your review, red broke
    /// and waits; the waiting card's line is an amber hourglass.
    #[test]
    fn edges_only_where_the_user_is_needed() {
        let page = board();
        let edges: Vec<(String, Tone)> = (0..page.columns.len())
            .flat_map(|n| cards(&page, n))
            .filter_map(|c| c.edge.map(|e| (c.key.clone(), e)))
            .collect();
        assert_eq!(
            edges,
            [
                ("SHOP-24".to_owned(), Tone::Error),
                ("SHOP-22".to_owned(), Tone::Attention),
                ("SHOP-18".to_owned(), Tone::Review),
                ("SHOP-21".to_owned(), Tone::Review),
            ]
        );
        let waiting = cards(&page, 1)[0].status.clone().unwrap();
        assert_eq!(waiting.icon, Some(crate::icons::IconName::Hourglass));
    }

    /// Hovering SHOP-27 lights SHOP-30, which waits for it, and its card gains "SHOP-30 waits for it".
    #[test]
    fn hovering_shop_27_lights_what_waits_for_it() {
        let page = board();
        let chain = page.chains.iter().find(|c| c.key == "SHOP-27").unwrap();
        assert_eq!(chain.members, ["SHOP-27", "SHOP-30"]);
        assert_eq!(page.chains.len(), 1, "nothing else waits for anything");
        let shop_27 = cards(&page, 1)
            .into_iter()
            .find(|c| c.key == "SHOP-27")
            .unwrap();
        assert_eq!(
            shop_27.blocks.as_ref().map(|b| b.line.as_str()),
            Some("SHOP-30 waits for it")
        );
    }

    /// Dragging SHOP-30 ahead to Validate is refused, and says why.
    #[test]
    fn dragging_a_waiting_ticket_ahead_is_refused_with_why() {
        let flow = default_flow();
        let all = tickets();
        let shop_30 = all.iter().find(|t| t.key.to_string() == "SHOP-30").unwrap();
        let answer = board_page::drag_answer_in(&flow, shop_30, &all, None);
        let validate = answer
            .targets
            .iter()
            .find(|t| t.target.as_str() == "validate")
            .unwrap();
        assert_eq!(
            validate.verdict,
            board_page::Verdict::Forbidden {
                title: "SHOP-30 can\u{2019}t go to Validate yet".into(),
                detail: Some("It waits for SHOP-27 to merge, then starts by itself.".into()),
            }
        );
    }
}
