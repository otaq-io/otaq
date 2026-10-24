//! A card as the Tauri page draws it (T-0218; Ticket Card v2 since T-0322; the one-line card of BRD-03 since T-0460):
//! [`CardViewModel`] and [`EpicViewModel`] turned into plain, serialized shapes (`TicketCardView`, `EpicCardView`)
//! with every text the card shows already worded. A ticket card is its key (with its epic's name when it is a
//! child), its title, one status line, who and when, a coloured edge when it needs the user, and one action; the
//! pill, the model, the tokens and the answer options are the ticket's sheet's. The page only lays them out.

use otaq_core::board::tickets::Ticket;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::card::{
    CardAction, CardExtras, CardViewModel, Signal, StatusHover, StatusLine, Tone,
};
use crate::view_models::card_fixtures::{self, Look, NOW};
use crate::view_models::epic_card::{ChildState, EpicViewModel};
use crate::view_models::priority::{self, PriorityView};
use crate::view_models::status::PillView;
use crate::view_models::tool_avatar::ToolAvatarView;

/// The hover quick actions' tooltips and key (*Interaction states → Hover*: open terminal, open panel).
pub const OPEN_PANEL: &str = "Open panel";
pub const OPEN_PANEL_KEYS: &str = "\u{21b5}";
pub const OPEN_TERMINAL: &str = "Open terminal";
/// The epic card's last row.
pub const ADD_TO_EPIC: &str = "Add ticket to epic";

/// The card's width in the frames (*24 · The two views*: a 7-stage flow at 1440, about 180 px; the narrowest a column
/// gets, BRD-02) and the width the gallery draws next to it.
pub const CARD_WIDTH: u32 = 220;
pub const CARD_WIDTH_1280: u32 = 164;

/// A ticket card for the page (BRD-03; *24 · The two views*): key and epic name, title, one status line, who and
/// when, the coloured edge and one action. `faded` is the Done cards' opacity.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "TicketCardView")]
pub struct TicketCardView {
    pub key: String,
    /// "SHOP-51 → SHOP-40.4" after Move to epic.
    pub key_label: String,
    /// The epic's name on the key's row, for an epic child ("Wishlist").
    pub epic: Option<String>,
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
    /// "Created by Checkout slowness": violet, a card a Project agent created names it (d0068; Q-E).
    pub origin: Option<String>,
    /// The full status and what the card has no room for.
    pub hover: StatusHover,
    pub faded: Option<f32>,
    /// A Project agent just created it: the page plays the fading highlight once.
    pub fresh: bool,
    pub open_panel: String,
    pub open_panel_keys: String,
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
            key_label: vm.key_label,
            epic: vm.sleeve.map(|s| s.name),
            priority: priority::on_card(ticket.priority).map(priority::view),
            title: vm.title,
            status: vm.one_line.line,
            blocks: vm.one_line.blocks,
            avatar: vm.avatar,
            who: vm.one_line.who,
            time: vm.one_line.time,
            edge: vm.one_line.edge,
            action: vm.one_line.action,
            origin: vm.origin,
            hover: vm.hover,
            faded: vm.faded,
            fresh: vm.fresh,
            open_panel: OPEN_PANEL.to_owned(),
            open_panel_keys: OPEN_PANEL_KEYS.to_owned(),
            open_terminal: OPEN_TERMINAL.to_owned(),
        }
    }
}

/// A child row of the Epic Card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EpicChildRow")]
pub struct ChildView {
    pub key: String,
    pub title: String,
    pub state: ChildState,
    pub icon: IconName,
    pub status_label: String,
    /// The row can be dragged out to queue only this child (d0080).
    pub draggable: bool,
    /// "added by Wishlist lead" (d0083).
    pub origin: Option<String>,
}

/// An Epic Card for the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "EpicCardView")]
pub struct EpicCardView {
    pub key: String,
    pub pinned: bool,
    pub external_id: Option<String>,
    pub priority: Option<PriorityView>,
    pub pill: PillView,
    pub title: String,
    pub segments: Vec<ChildState>,
    pub progress: String,
    pub branch: String,
    pub children: Vec<ChildView>,
    /// "Add ticket to epic", when it can still take a child.
    pub add: Option<String>,
    pub actions: Vec<CardAction>,
    pub faded: Option<f32>,
    /// The Lead row under the branch line, or Open lead (d0083, T-0289).
    pub lead: Option<crate::view_models::lead_status::LeadStatusView>,
    /// Its lead asks you: the border is turquoise.
    pub lead_asks: bool,
}

impl EpicCardView {
    pub fn of(epic: &Ticket, tickets: &[Ticket], branch: Option<&str>) -> Self {
        Self::from_vm(EpicViewModel::from(epic, tickets, branch))
    }

    pub fn from_vm(vm: EpicViewModel) -> Self {
        let lead_asks = vm.lead_asks();
        Self {
            key: vm.key.to_string(),
            pinned: vm.pinned,
            external_id: vm.external_id,
            priority: vm.priority.map(priority::view),
            pill: PillView::legend(vm.pill.legend, Some(&vm.pill.label)),
            title: vm.title,
            segments: vm.segments,
            progress: vm.progress,
            branch: vm.branch,
            children: vm
                .children
                .into_iter()
                .map(|c| ChildView {
                    key: c.key.to_string(),
                    title: c.title,
                    state: c.state,
                    icon: c.state.icon(),
                    status_label: c.status_label.to_owned(),
                    draggable: c.draggable,
                    origin: c.origin,
                })
                .collect(),
            add: vm.can_add.then(|| ADD_TO_EPIC.to_owned()),
            actions: vm.actions,
            faded: vm.faded,
            lead: vm.lead,
            lead_asks,
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

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "EpicGalleryCell")]
pub struct EpicCell {
    pub name: String,
    pub card: EpicCardView,
}

/// The gallery's cards: *04 → Ticket Card v2 · all variants* in the frame's order (the M21 ones), drawn at
/// [`CARD_WIDTH`] and at [`CARD_WIDTH_1280`]; the epic rows; the sleeved children (*Epic Sleeve v2*); and the
/// lead's rows. From [`card_fixtures`].
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "CardGallery")]
pub struct CardGallery {
    pub variants: Vec<GalleryCell>,
    /// The normal card width and the width at 1280 (*Cards at 1280*): the page draws the variants at both.
    pub width: u32,
    pub width_1280: u32,
    pub epics: Vec<EpicCell>,
    /// *Epic Sleeve v2*: sleeved epic children (EPC-09).
    pub sleeves: Vec<GalleryCell>,
    /// *04 → Row · Epic children & the lead (0083)* and *Row · Unread notes & same file (0083 · T)* (T-0291).
    pub leads: Vec<GalleryCell>,
    /// *04 → Variants · one tool (M22)* and the picked-by cards (T-0345): the footer's agent part, Start failed and
    /// No result on a refused model.
    pub agents: Vec<GalleryCell>,
    /// *04 → Variants · several tools (M23)* and the fallback and limit cards (T-0362): avatars on, Unavailable,
    /// waiting on a tool, moved by a fallback.
    pub tools: Vec<GalleryCell>,
    /// *03 → cards in an agent check, your check, deliveries, conflicts, Done* (`s03-44`…`s03-54`, T-0424).
    pub flows: Vec<GalleryCell>,
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
        epics: card_fixtures::epic_variants()
            .iter()
            .map(|v| EpicCell {
                name: v.name.to_owned(),
                card: EpicCardView::of(&v.epic, &v.tickets, v.branch),
            })
            .collect(),
        sleeves: card_fixtures::sleeve_variants().iter().map(cell).collect(),
        leads: card_fixtures::lead_variants().iter().map(cell).collect(),
        agents: card_fixtures::agent_variants().iter().map(cell).collect(),
        tools: card_fixtures::tool_variants().iter().map(cell).collect(),
        flows: card_fixtures::flow_variants().iter().map(cell).collect(),
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
            .chain(&g.sleeves)
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
            "sleeve",
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
    fn an_epic_child_names_its_epic_and_quiet_buttons_leave() {
        let g = gallery();
        let card = cell(&g, "Dev \u{b7} epic child \u{b7} working");
        assert_eq!(card.epic.as_deref(), Some("Wishlist"));
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
        let card = cell(&g, "Created by a Project agent (0068)");
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
        assert_eq!(g.epics.len(), card_fixtures::epic_variants().len());
        assert_eq!(g.sleeves.len(), card_fixtures::sleeve_variants().len());
        assert_eq!(g.leads.len(), card_fixtures::lead_variants().len());
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

    #[test]
    fn epic_rows_are_worded_here() {
        let g = gallery();
        let epics = &g.epics;
        assert_eq!(epics[0].card.add.as_deref(), Some(ADD_TO_EPIC));
        assert_eq!(epics[0].card.children[0].status_label, "Merged");
        let done = epics.iter().find(|e| e.name == "Epic \u{b7} Done").unwrap();
        assert_eq!(done.card.add, None);
        let json = serde_json::to_value(&epics[2].card).unwrap();
        assert_eq!(json["actions"][0]["kind"], "mark_merged");
        assert_eq!(json["segments"][0], "merged");
    }
}
