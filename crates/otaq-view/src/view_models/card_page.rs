//! A card as the Tauri page draws it (T-0218; Ticket Card v2 since T-0322; the one-line card of BRD-03 since T-0460):
//! [`CardViewModel`] turned into a plain, serialized shape (`TicketCardView`) with every text the card shows already
//! worded. A ticket card is its key, its title, one status line, who and when, a coloured edge when it needs the user, and one action; the
//! pill, the model, the tokens and the answer options are the ticket's sheet's. The page only lays them out.

use otaq_core::board::tickets::Ticket;
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::card::{
    CardAction, CardExtras, CardViewModel, Signal, StatusHover, StatusLine, Tone,
};
use crate::view_models::card_fixtures::{self, Look, NOW};
use crate::view_models::priority::{self, PriorityView};
use crate::view_models::tool_avatar::ToolAvatarView;

/// The hover quick actions' tooltips and key (*Interaction states → Hover*: open terminal, open sheet).
pub const OPEN_SHEET: &str = "Open panel";
pub const OPEN_SHEET_KEYS: &str = "\u{21b5}";
pub const OPEN_TERMINAL: &str = "Open terminal";

/// The card's width in the frames (*24 · The two views*: a 7-stage flow at 1440, about 180 px, drawn before BRD-02's
/// minimum became 240 px) and the width the gallery draws next to it.
pub const CARD_WIDTH: u32 = 220;
pub const CARD_WIDTH_1280: u32 = 164;

/// A ticket card for the page (BRD-03; *24 · The two views*): key, title, one status line, who and
/// when, the coloured edge and one action. `faded` is the Done cards' opacity.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "TicketCardView")]
pub struct TicketCardView {
    pub key: String,
    /// The priority chip after the key: P1, P2 and P4, none for Normal (d0065; #36), in every column.
    pub priority: Option<PriorityView>,
    pub title: String,
    /// The one status line: the state's dot (an amber hourglass while it waits) and its words.
    pub status: Option<StatusLine>,
    /// One muted line under it, only when set: the ticket's own agent for a stage, or the stages it skips.
    pub note: Option<crate::view_models::card_line::Note>,
    /// What waits for it: the line the card gains while hovered ("SHOP-30 waits for it"), the List's "blocks 1".
    pub blocks: Option<crate::view_models::dependencies::Blocks>,
    /// The tool mark of whoever has the turn; `null` while only one tool is connected (d0086).
    pub avatar: Option<ToolAvatarView>,
    /// The agent's name ("Strong"); `null` while there is only one agent (d0086).
    pub who: Option<String>,
    /// How long the agent has been at it ("26m"), or when a finished card finished ("today").
    pub time: Option<Signal>,
    /// The coloured edge, only while the card needs the user: turquoise asks or decides, the review colour is
    /// your review, red broke and waits.
    pub edge: Option<Tone>,
    /// The card's one action (Answer, Open, Review, Decide, Retry, Mark merged…).
    pub action: Option<CardAction>,
    /// The helper whose terminal the action opens ("40.3"), when a helper's permission prompt waits.
    pub opens: Option<String>,
    /// The full status and what the card has no room for.
    pub hover: StatusHover,
    pub faded: Option<f32>,
    /// A Project agent just created it: the page plays the fading highlight once.
    pub fresh: bool,
    pub open_sheet: String,
    pub open_sheet_keys: String,
    pub open_terminal: String,
}

impl TicketCardView {
    pub fn of(ticket: &Ticket, extras: &CardExtras, now_ms: i64) -> Self {
        Self::from_vm(CardViewModel::from(ticket, extras, now_ms), ticket)
    }

    /// From a view model already built (the board's, with its queue facts and signal).
    pub fn from_vm(vm: CardViewModel, ticket: &Ticket) -> Self {
        Self {
            key: vm.key.to_string(),
            priority: priority::on_card(ticket.priority).map(priority::view),
            title: vm.title,
            status: vm.one_line.line,
            note: vm.one_line.note,
            blocks: vm.one_line.blocks,
            avatar: vm.avatar,
            who: vm.one_line.who,
            time: vm.one_line.time,
            edge: vm.one_line.edge,
            action: vm.one_line.action,
            opens: crate::view_models::pieces::opens(ticket),
            hover: vm.hover,
            faded: vm.faded,
            fresh: vm.fresh,
            open_sheet: OPEN_SHEET.to_owned(),
            open_sheet_keys: OPEN_SHEET_KEYS.to_owned(),
            open_terminal: OPEN_TERMINAL.to_owned(),
        }
    }
}

/// One cell of the gallery's variant grid: the frame's variant name and the look it is drawn in.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "CardGalleryCell")]
pub struct GalleryCell {
    pub name: String,
    pub look: Look,
    pub card: TicketCardView,
}

/// The gallery's cards: *04 → Ticket Card v2 · all variants* in the frame's order (the M21 ones), drawn at
/// [`CARD_WIDTH`] and at [`CARD_WIDTH_1280`]. From [`card_fixtures`].
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "CardGallery")]
pub struct CardGallery {
    pub variants: Vec<GalleryCell>,
    /// The normal card width and the width at 1280 (*Cards at 1280*): the page draws the variants at both.
    pub width: u32,
    pub width_1280: u32,
    /// *04 → Variants · one tool (M22)* and the picked-by cards (T-0345): the footer's agent part, Start failed and
    /// No result on a refused model.
    pub agents: Vec<GalleryCell>,
    /// *04 → Variants · several tools (M23)* and the held and limit cards (T-0362): avatars on, Unavailable, waiting
    /// on a tool.
    pub tools: Vec<GalleryCell>,
    /// *03 → cards in an agent check, your check, deliveries, conflicts, Done* (`s03-44`…`s03-54`, T-0424).
    pub flows: Vec<GalleryCell>,
    /// *30 → The card in Dev*: a build stage's piece rows (d0133, T-0541).
    pub pieces: Vec<GalleryCell>,
    /// *30 → Planning on the board*: one card per ticket while it plans (d0129, T-0548).
    pub planning: Vec<GalleryCell>,
}

pub fn gallery() -> CardGallery {
    let cell = |v: &card_fixtures::Variant| GalleryCell {
        name: v.name.to_owned(),
        look: v.look,
        card: TicketCardView::of(&v.ticket, &v.extras, NOW),
    };
    CardGallery {
        variants: card_fixtures::variants().iter().map(cell).collect(),
        width: CARD_WIDTH,
        width_1280: CARD_WIDTH_1280,
        agents: card_fixtures::agent_variants().iter().map(cell).collect(),
        tools: card_fixtures::tool_variants().iter().map(cell).collect(),
        flows: card_fixtures::flow_variants().iter().map(cell).collect(),
        pieces: card_fixtures::piece_variants().iter().map(cell).collect(),
        planning: card_fixtures::planning_variants()
            .iter()
            .map(cell)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;
    use crate::view_models::card::CardActionKind;

    fn cell<'a>(g: &'a CardGallery, name: &str) -> &'a TicketCardView {
        g.variants
            .iter()
            .chain(&g.flows)
            .find(|c| c.name == name)
            .map(|c| &c.card)
            .unwrap_or_else(|| panic!("no variant {name}"))
    }

    fn line(card: &TicketCardView) -> (&str, Palette, Tone) {
        let line = card.status.as_ref().expect("a status line");
        (line.text.as_str(), line.dot, line.tone)
    }

    /// Every cell the gallery draws.
    fn every(g: &CardGallery) -> impl Iterator<Item = &GalleryCell> {
        g.variants
            .iter()
            .chain(&g.agents)
            .chain(&g.tools)
            .chain(&g.flows)
            .chain(&g.pieces)
            .chain(&g.planning)
    }

    /// Every card state the fixtures hold fits the card: its key (with at most a P1/P2 chip), its title, one status
    /// line, at most one muted note, one row of who and how long, and at most one action. The card has no other line
    /// to fill, and only three edge colours.
    #[test]
    fn every_card_state_fits_four_lines_and_one_action() {
        let g = gallery();
        for cell in every(&g) {
            let card = &cell.card;
            let json = serde_json::to_value(card).unwrap();
            let keys: Vec<&str> = json
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            for gone in [
                "plan",
                "pieces",
                "origin",
                "skill_warning",
                "flag",
                "situation",
                "footer",
                "options",
                "tokens",
                "model",
            ] {
                assert!(!keys.contains(&gone), "{}: {gone}", cell.name);
            }
            let lines = 1 // key
                + 1 // title, two lines at most (the page clamps it)
                + usize::from(card.status.is_some())
                + usize::from(card.note.is_some())
                + usize::from(card.who.is_some() || card.time.is_some() || card.action.is_some());
            assert!(lines <= 5, "{}: {lines} lines", cell.name);
            assert!(
                lines - usize::from(card.note.is_some()) <= 4,
                "{}: four lines besides the note",
                cell.name
            );
            assert!(
                card.priority.as_ref().is_none_or(|p| p.number <= 2),
                "{}: only P1 and P2 show",
                cell.name
            );
            assert!(
                matches!(
                    card.edge,
                    None | Some(Tone::Attention | Tone::Review | Tone::Error)
                ),
                "{}: edge {:?}",
                cell.name,
                card.edge
            );
            if let Some(status) = &card.status {
                for never in [" round ", "tokens", ":40", "decisions open"] {
                    assert!(
                        !status.text.contains(never),
                        "{}: {:?} says {never:?}",
                        cell.name,
                        status.text
                    );
                }
            }
        }
    }

    /// A stage held because its tool doesn't have its skill is red: the line says, word for word, "Codex doesn't
    /// have the skill `/team-validate`".
    #[test]
    fn a_stage_held_for_its_skill_says_which_on_its_line() {
        use crate::view_models::card_fixtures::{NOW, ticket};
        use otaq_core::agent_tools::Tool;
        use otaq_core::domain::{MissingSkill, Status};
        let mut t = ticket("SHOP-41", "Cart totals", Status::Checking);
        let calm = TicketCardView::of(&t, &CardExtras::default(), NOW);
        assert_eq!(line(&calm).0, "Checking");

        t.flags.missing_skill = Some(MissingSkill {
            tool: Tool::Codex,
            skill: "team-validate".into(),
        });
        let card = TicketCardView::of(&t, &CardExtras::default(), NOW);
        assert_eq!(
            line(&card),
            (
                "Codex doesn\u{2019}t have the skill `/team-validate`",
                Palette::Error,
                Tone::Error
            )
        );
        assert_eq!(card.edge, Some(Tone::Error));
    }

    /// A ticket the pause stopped reads "Paused · resumes where it was", with its timer held, no edge and no
    /// action; one waiting for the user keeps its question and its edge.
    #[test]
    fn a_paused_card_resumes_where_it_was() {
        use crate::view_models::card_fixtures::{NOW, ticket};
        use otaq_core::domain::{Status, ValidationRound};
        let paused = CardExtras {
            paused: true,
            ..CardExtras::default()
        };
        let stopped = |status_from: Option<Status>| {
            let mut t = ticket("SHOP-33", "Shipping estimate", Status::Interrupted);
            t.interrupted_from = status_from;
            t.status_since = Some(NOW - 600_000);
            t.flags.last_signal_at = Some(NOW - 600_000 - 180_000);
            t
        };

        let working = TicketCardView::of(&stopped(None), &paused, NOW);
        assert_eq!(
            line(&working),
            (
                "Paused \u{b7} resumes where it was",
                Palette::Paused,
                Tone::Secondary
            )
        );
        assert_eq!(
            working.time.as_ref().unwrap().text,
            "3m",
            "the time it had when it stopped"
        );
        assert_eq!(working.edge, None);
        assert!(working.action.is_none());

        let mut validating = stopped(Some(Status::Checking));
        validating.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
        assert_eq!(
            line(&TicketCardView::of(&validating, &paused, NOW)).0,
            "Paused \u{b7} resumes where it was"
        );

        // A question keeps its words and its edge.
        let mut asks = stopped(Some(Status::AsksYou));
        asks.title = "Add dark mode toggle".into();
        let question = CardExtras {
            question: Some("Follow the system setting?".into()),
            ..paused.clone()
        };
        let card = TicketCardView::of(&asks, &question, NOW);
        assert_eq!(line(&card).0, "Asks you \u{b7} follow the system setting?");
        assert_eq!(card.edge, Some(Tone::Attention));

        // The same ticket outside a pause was stopped by a restart: red, with Resume.
        let card = TicketCardView::of(&stopped(None), &CardExtras::default(), NOW);
        assert_eq!(line(&card).0, "Stopped by a restart");
        assert_eq!(card.edge, Some(Tone::Error));
        assert_eq!(card.action.as_ref().unwrap().label, "Resume");
    }

    /// A permission prompt asks you, turquoise, with Open; the app it runs isn't on the card.
    #[test]
    fn a_permission_prompt_asks_you_and_opens() {
        let g = gallery();
        let card = cell(&g, "Needs you + running + 2 dependencies");
        assert_eq!(
            line(card),
            (
                "Asks you \u{b7} allow rm -rf .next?",
                Palette::Attn,
                Tone::Attention
            )
        );
        assert_eq!(card.edge, Some(Tone::Attention));
        assert_eq!(card.action.as_ref().unwrap().label, "Open");
        assert!(card.avatar.is_none(), "one tool connected: no tool mark");
    }

    #[test]
    fn a_working_card_is_calm() {
        let g = gallery();
        let card = cell(&g, "Working");
        assert_eq!(line(card), ("Working", Palette::Working, Tone::Secondary));
        assert_eq!(card.edge, None);
        assert_eq!(card.action, None);
    }

    #[test]
    fn a_check_out_of_rounds_says_decide() {
        let g = gallery();
        let card = cell(&g, "Security failed \u{d7}3");
        assert_eq!(line(card).0, "Security failed 3 times \u{b7} decide");
        assert_eq!(card.edge, Some(Tone::Attention));
        let action = card.action.as_ref().unwrap();
        assert_eq!(
            (action.kind, action.label.as_ref()),
            (CardActionKind::Decide, "Decide")
        );
        let card = cell(&g, "Validation failed 3 times (0063)");
        assert_eq!(card.action.as_ref().unwrap().kind, CardActionKind::Decide);
        assert_eq!(line(card).0, "Validate failed 3 times \u{b7} decide");
    }

    #[test]
    fn your_review_has_the_review_edge_and_review() {
        let g = gallery();
        let card = cell(&g, "Your check \u{b7} passed Security");
        assert_eq!(
            line(card),
            (
                "Ready for you \u{b7} passed Security",
                Palette::Review,
                Tone::Review
            )
        );
        assert_eq!(card.edge, Some(Tone::Review));
        assert_eq!(card.action.as_ref().unwrap().kind, CardActionKind::Review);
    }

    /// An open PR is the user's to mark merged; the card has no edge.
    #[test]
    fn an_open_pr_offers_mark_merged() {
        let g = gallery();
        let pr = cell(&g, "PR #481 open \u{b7} opened by Shipper");
        assert_eq!(line(pr).0, "PR #481 open");
        assert_eq!(
            pr.action.as_ref().map(|a| (a.kind, a.label.as_ref())),
            Some((CardActionKind::MarkMerged, "Mark merged"))
        );
        assert_eq!(pr.edge, None);
    }

    /// The card never says who created it (Noyan's panel lists what it created).
    #[test]
    fn an_agent_made_card_says_nothing_of_its_maker() {
        let g = gallery();
        let card = cell(&g, "Created by Noyan (0068)");
        let json = serde_json::to_value(card).unwrap();
        assert!(json.get("origin").is_none());
        assert!(!json.to_string().contains("Created by"));
    }

    #[test]
    fn every_fixture_reaches_the_page() {
        let g = gallery();
        assert_eq!(g.variants.len(), card_fixtures::variants().len());
        assert_eq!(g.agents.len(), card_fixtures::agent_variants().len());
        assert_eq!(g.tools.len(), card_fixtures::tool_variants().len());
        assert!(
            g.tools.iter().all(|c| c.card.avatar.is_some()),
            "several tools: tool marks on"
        );
        assert_eq!((g.width, g.width_1280), (CARD_WIDTH, CARD_WIDTH_1280));
        let looks: Vec<Look> = g.variants.iter().map(|c| c.look).collect();
        for look in [
            Look::Plain,
            Look::Selected,
            Look::Hovered,
            Look::Dragging,
            Look::Chain,
            Look::Dimmed,
        ] {
            assert!(looks.contains(&look), "{look:?}");
        }
    }
}
