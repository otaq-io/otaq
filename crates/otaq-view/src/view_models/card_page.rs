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

/// The card's width in the frames (*24 · The two views*: a 7-stage flow at 1440, about 180 px; the narrowest a column
/// gets, BRD-02) and the width the gallery draws next to it.
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
    /// The one status line; `null` on a calm Backlog card.
    pub status: Option<StatusLine>,
    /// "Blocks SHOP-20" under the line, muted, with the List's "blocks 1" (DPV-01).
    pub blocks: Option<crate::view_models::dependencies::Blocks>,
    /// The tool mark of whoever has the turn; `null` while only one tool is connected (d0086).
    pub avatar: Option<ToolAvatarView>,
    /// The agent's name ("Rust developer"); `null` while there is only one agent (d0086).
    pub who: Option<String>,
    /// How long ago the working agent last said something ("3m").
    pub time: Option<Signal>,
    /// The coloured edge of a card that needs the user: its colour.
    pub edge: Option<Tone>,
    /// The card's one action (Answer, Reply, Review, Decide…).
    pub action: Option<CardAction>,
    /// After the action, the one check agent it acts on: "for Performance only", "re-runs Performance only" (d0130).
    pub action_note: Option<String>,
    /// A Check stage with several check agents (d0130, `s29-04`): a row per check agent under the line, which is the
    /// round's ("Validate · round 1 of 3"), and its note; the rows name the agents, so `avatar` and `who` are empty.
    pub check: Option<crate::view_models::check_agents::RoundView>,
    /// "Plan · 4 pieces", "Plan · 2 pieces so far": quiet, under the line (d0133, `s30-01`); none without pieces.
    pub plan: Option<String>,
    /// A build stage's pieces, a row each (d0133, `s30-02`): "PIECES · 4", "40.2 · Wishlist button · working".
    pub pieces: Option<crate::view_models::pieces::CardPieces>,
    /// "Created by Noyan · Checkout slowness": violet, a card a Project agent created names it (d0068; Q-E).
    pub origin: Option<String>,
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
        let check = vm.one_line.check.map(|c| *c);
        let several = check.is_some();
        Self {
            key: vm.key.to_string(),
            priority: priority::on_card(ticket.priority).map(priority::view),
            title: vm.title,
            status: vm.one_line.line,
            blocks: vm.one_line.blocks,
            avatar: vm.avatar.filter(|_| !several),
            who: vm.one_line.who.filter(|_| !several),
            time: vm.one_line.time,
            edge: vm.one_line.edge,
            action: vm.one_line.action,
            action_note: vm.one_line.action_note,
            check,
            plan: vm.plan,
            pieces: vm.pieces.map(|p| *p),
            origin: vm.origin,
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
    /// *04 → Variants · several tools (M23)* and the fallback and limit cards (T-0362): avatars on, Unavailable,
    /// waiting on a tool, moved by a fallback.
    pub tools: Vec<GalleryCell>,
    /// *03 → cards in an agent check, your check, deliveries, conflicts, Done* (`s03-44`…`s03-54`, T-0424).
    pub flows: Vec<GalleryCell>,
    /// *29 → On the card*, *30 → When a check agent doesn't finish*: a Check stage with two check agents (d0130).
    pub checks: Vec<GalleryCell>,
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
        checks: card_fixtures::check_agent_variants()
            .iter()
            .map(cell)
            .collect(),
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

    /// PAU-03, PAU-08 (`s24-07`): a ticket the pause stopped reads "Paused" in amber, with what it was doing, its
    /// timer held, no edge and no action; one waiting for the user keeps its question; a ticket in a stage that
    /// would start waits ("Paused · starts when you resume").
    #[test]
    fn a_paused_card_reads_paused_in_amber_with_its_timer_held() {
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
        assert_eq!(line(&working), ("Paused", Palette::Paused, Tone::Paused));
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
            "Paused \u{b7} validating \u{b7} round 1 of 3"
        );
        validating.flags.check = Some("Security".into());
        assert_eq!(
            line(&TicketCardView::of(&validating, &paused, NOW)).0,
            "Paused \u{b7} checking \u{b7} round 1 of 3"
        );

        // A question keeps its words and its edge; answering starts that agent only (PAU-05).
        let mut asks = stopped(Some(Status::AsksYou));
        asks.title = "Add dark mode toggle".into();
        let question = CardExtras {
            question: Some("Follow the system setting?".into()),
            ..paused.clone()
        };
        let card = TicketCardView::of(&asks, &question, NOW);
        assert_eq!(line(&card).0, "Asks you: follow the system setting?");
        assert_eq!(card.edge, Some(Tone::Attention));

        // The same ticket outside a pause is interrupted, as after a restart.
        let card = TicketCardView::of(&stopped(None), &CardExtras::default(), NOW);
        assert_ne!(line(&card).0, "Paused");
    }

    #[test]
    fn a_needs_you_card_has_one_line_an_edge_and_one_action() {
        let g = gallery();
        let card = cell(&g, "Needs you + running + 2 dependencies");
        assert_eq!(
            line(card),
            (
                "Needs you: allow rm -rf .next? \u{b7} also running :4022",
                Palette::Attn,
                Tone::Attention
            )
        );
        assert_eq!(card.edge, Some(Tone::Attention));
        assert_eq!(card.action.as_ref().unwrap().label, "Answer");
        assert_eq!(card.hover.status, "Needs you \u{b7} permission");
        assert!(card.avatar.is_none(), "one tool connected: no tool mark");
    }

    #[test]
    fn the_pill_the_model_and_the_tokens_leave_the_card() {
        let g = gallery();
        let json = serde_json::to_value(cell(&g, "Working")).unwrap();
        let card = json.as_object().unwrap();
        for gone in [
            "flag",
            "situation",
            "footer",
            "options",
            "pinned",
            "outline",
        ] {
            assert!(!card.contains_key(gone), "{gone}");
        }
        assert_eq!(json["status"]["text"], "Working");
        assert_eq!(json["time"]["text"], "40s");
        assert_eq!(json["edge"], serde_json::Value::Null);
        assert_eq!(json["action"], serde_json::Value::Null);
    }

    #[test]
    fn a_check_out_of_rounds_says_decide() {
        let g = gallery();
        let card = cell(&g, "Security failed \u{d7}3");
        assert_eq!(
            line(card).0,
            "Security failed 3 times \u{b7} unescaped SQL in search \u{b7} see comment"
        );
        let action = card.action.as_ref().unwrap();
        assert_eq!(
            (action.kind, action.label),
            (CardActionKind::Decide, "Decide")
        );
        let card = cell(&g, "Validation failed 3 times (0063)");
        assert_eq!(card.action.as_ref().unwrap().kind, CardActionKind::Decide);
        assert_eq!(
            line(card).0,
            "Failed 3 times: arrow keys do nothing in the list \u{b7} see comment"
        );
    }

    #[test]
    fn your_check_has_the_review_edge_and_review() {
        let g = gallery();
        let card = cell(&g, "Your check \u{b7} passed Security");
        assert_eq!(
            line(card),
            (
                "Your check \u{b7} Passed Security \u{b7} round 2",
                Palette::Review,
                Tone::Review
            )
        );
        assert_eq!(card.edge, Some(Tone::Review));
        assert_eq!(card.action.as_ref().unwrap().kind, CardActionKind::Review);
    }

    #[test]
    fn quiet_buttons_leave() {
        let g = gallery();
        // Send back with PR comments is a quiet button: the sheet has it.
        let pr = cell(&g, "PR #481 open \u{b7} opened by Shipper");
        assert_eq!(
            line(pr).0,
            "PR #481 open \u{b7} Opened by Shipper \u{b7} 2 h ago"
        );
        assert_eq!(pr.action, None);
        assert_eq!(pr.edge, None);
    }

    /// d0068 (Q-E): a card a Project agent created keeps naming it.
    #[test]
    fn an_agent_made_card_names_its_agent() {
        let g = gallery();
        let card = cell(&g, "Created by Noyan (0068)");
        assert!(
            card.origin
                .as_deref()
                .is_some_and(|o| o.starts_with("Created by")),
            "{:?}",
            card.origin
        );
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
