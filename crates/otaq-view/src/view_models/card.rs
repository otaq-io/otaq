//! What a ticket card shows: **Ticket Card v2** (SHL-07, d0087; *04 · Ticket card variants → Update U + V · Ticket
//! Card v2 · all variants (0086 · V)*, master *01 · Components → Ticket Card v2*). It replaces UI-03's anatomy (the
//! pill, the callout, the Entry row, the bars and the meta row); the states and flags of SDD §7.2 are unchanged and
//! each keeps a place on the card.
//!
//! - **Head:** the tool avatar of whoever **has the turn** (the validator while it validates or asks, else the
//!   developer; hidden while one tool is connected, [`tool_avatar::shown`]), the key, the pin (d0047), the priority
//!   chip (d0065) and **one short flag** (≤ 12 characters at every width).
//! - **Title**, two lines.
//! - **Situation line:** one line with a 2 px edge in the state's colour, mono only for a command. The flag that
//!   wins the head says its words here first; every flag that loses the head joins it with " · " in the flag order
//!   ("Allow rm -rf .next? · also running :4022"), then the quiet facts that never take the head (waits, background
//!   work, messages waiting, the lead's markers, the review entry, the diff).
//! - **Origin line** of a card an agent created ("Created by Checkout slowness", violet, d0068).
//! - **Footer, only when it has something to say:** while an agent runs, the tokens its tool reports and the last
//!   signal ("182k · 40s"; "not reported" when Otaq can't read the usage file, d0086); and the card's one action.
//!   Its **agent part** (M22, AGT-04, AGM-09; [`footer_agent`]): while an agent runs, the agent of whoever has the
//!   turn and its model ("Rust developer · sonnet"); a ticket waiting before an Agent stage says where it starts
//!   ("Grill · Griller · sonnet"); a held role names the agent it would start on. No choice line before the start
//!   (the panel and the form show the choices), and nothing names an agent while there is only one (d0086).
//! - **Hover:** the full status ("Needs you · permission"), and what the card has no room for.
//!
//! **The flag order** (head slot, highest first, [`Rank`]): Validator / Needs you / Asks you (also via lead) / Idle
//! → Interrupted · Resume fail · Start failed · Parent closed · Conflicts ×3 · Diverged · Dirty repo · Failed ×3 · No
//! result → Limit (neutral) → Validating… · Conflict → the review outcome (Validated · Skipped · Did not pass · PR
//! #481 · Closed) → :port (running) → Working. Turquoise stays the needs-you family's (d0077); Limit is neutral; the
//! tool never takes the slot. Everything else (Queued waits, Parked · has work, a quiet agent, Asked SHOP-40.1,
//! Updated by lead, Merged automatically…) goes to the situation line only (Q-D).
//!
//! **Several tools** (M23, d0086, d0087; *04 → Variants · several tools (M23)*, the fallback cards of *Ticket Card
//! v2 · all variants*): once two tools are connected the head's avatar shows the tool of whoever has the turn. A
//! role held before it starts ([`AgentFacts::holds`], T-0357's `agent_holds`) takes the head with **Unavailable**
//! (amber: its tool isn't available, or its model is no longer listed) or **Limit** (neutral: its tool is at its
//! limit), never turquoise and never in Needs you. A model that can't run (its tool not available, the model no
//! longer listed) says **Can't start** ("Griller's model can't run: Claude Code isn't available"); the card offers
//! **Use fallback** (when the tool's fallback can take it) and **Open agent…** (AGM-03, d0108 second gap review rule
//! 7). A session on its tool's fallback says so while it runs, amber ("On Codex · Claude Code limit", AGM-03). A
//! swapped model changes nothing on the card (the panel says it).
//!
//! Data that the service doesn't fill yet comes in through [`CardExtras`]; fixtures fill it.

use otaq_core::agents::facts::{BoardAgents, CardAgents, CardHold, CardStart, RunsOnTool};
use otaq_core::agents::{HoldReason, SessionRole as Role};
use otaq_core::board::epic_flight::is_never_started;
use otaq_core::board::human_check::Arrival;
use otaq_core::board::needs_you::change_agent;
use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::{Actor, HandOffKind, MergeCheckState, Priority, Status, TicketKey};
use otaq_core::flow::model::Delivers;
use otaq_core::signals::background::{minutes, wakeup_text};
use otaq_core::signals::{HELPER_PROMPT_PREFIX, is_stale};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::theme::{Palette, Status as Legend};
use crate::view_models::tool_avatar::{AvatarTool, ToolAvatarView};
use crate::view_models::{priority, status, ticket_comments};

/// The last signal in the footer turns amber after this long ("not you, but soon", *Foundations → amber*); past
/// the core's threshold ([`otaq_core::signals::STALE_AFTER_MS`], ATT-05) the situation line says so instead.
pub const SIGNAL_AMBER_MS: i64 = 3 * 60 * 1000;

/// A head flag is at most this many characters at every width (designer decision B12)…
pub const FLAG_MAX_CHARS: usize = 12;
/// …except "Parent closed", the design's own label (13)…
pub const PARENT_CLOSED: &str = "Parent closed";
/// …and "Validator asks" (14), the glossary's short flag for the validator's question (d0093).
pub const VALIDATOR_ASKS: &str = "Validator asks";

/// Facts about a ticket that aren't on [`Ticket`] yet or come from elsewhere on the board.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardExtras {
    /// The project's base branch; a ticket on another base says so ("on release/2.3").
    pub project_base: Option<String>,
    /// The epic child's name line (EPC-09, BRD-03): its epic's name and the epic's pin.
    pub sleeve: Option<Sleeve>,
    /// Queued: what it waits for ("waits for SHOP-20 to merge", T-0077, T-0083).
    pub waits_for: Option<String>,
    /// A parent only waits for the user's merge (Review, PR open).
    pub waits_for_merge: bool,
    /// Every dependency is merged and the scheduler is starting it ("Starting…").
    pub starting: bool,
    /// The question or prompt of a needs-you ticket.
    pub question: Option<String>,
    /// The prompt comes from a helper agent inside the session.
    pub helper_agent: bool,
    /// Parent closed: the parent's key.
    pub closed_parent: Option<TicketKey>,
    /// Held: why ("`develop` and `origin/develop` have diverged. Push or pull, then retry.").
    pub held_reason: Option<String>,
    /// Held by the plan's usage limit (SCH-04): it starts by itself after the reset, so no Retry.
    pub held_by_limit: bool,
    /// Start interrupted: the step that failed ("setup command failed").
    pub start_failure: Option<String>,
    /// Working in background: what runs ("test run, 1 helper agent · 6 min") or a wakeup time.
    pub background: Option<String>,
    pub background_wakeup: bool,
    /// The key the ticket had before Move to epic.
    pub old_key: Option<TicketKey>,
    /// Lines added, removed and files changed (Review).
    pub diff: Option<(u32, u32, u32)>,
    /// The PR provider's name ("GitHub").
    pub pr_provider: Option<String>,
    /// The user's terminal has focus on this ticket and the agent ended its turn: calm "your turn".
    pub your_turn: bool,
    /// The branch name shown on a parked ticket.
    pub branch: Option<String>,
    /// Interrupted while it was validating (the startup report's `was_validating`, REC-01's M12 row, d0063).
    pub was_validating: bool,
    /// The current name of the Project agent that created it (d0068).
    pub agent_name: Option<String>,
    /// The developer's tool (and model) for the head's avatar; `None` until it has a session.
    pub developer: Option<ToolAvatarView>,
    /// The validator's tool (and model), while a validator has a session.
    pub validator: Option<ToolAvatarView>,
    /// Its named agents (`board.agents`, T-0345): the footer's agent part.
    pub agents: AgentFacts,
    /// Two or more agent tools are connected (`board.agents`'s `connected`): the head shows the avatar (SHL-08).
    pub several_tools: bool,
    /// What its Delivery delivers, from its project's flow (an epic child always merges locally; FLW-15): a To
    /// deliver card is "Ready to merge" or "Ready for PR".
    pub delivers: Option<Delivers>,
    /// The agent of its project's Delivery stage, who opens and pushes the PR: "Opened by Shipper · 2 h ago".
    pub delivery_agent: Option<String>,
    /// Its project's flow isn't the default's shape ([`crate::view_models::flow_columns::keeps_default_look`]): its
    /// human checks, To deliver cards and failed checks read as `s03-44`…`s03-54` draw them. On the default flow
    /// those cards stay today's (d0108: "a project that never changes its flow sees today's board"; STG-06); the
    /// renames and new features (PR comments, conflicts) show on every flow.
    pub custom_flow: bool,
    /// Its project is paused (PAU-03, d0112): a ticket the pause stopped reads "Paused" in amber with what it was
    /// doing, its timer held; one that needs the user keeps its question.
    pub paused: bool,
    /// It hasn't started and waits for a parent (DPV-01): "Blocked · waits for SHOP-18 and SHOP-29 to merge" is its
    /// one line ([`crate::view_models::dependencies::blocked_line`]).
    pub blocked: Option<String>,
    /// Others wait for it (DPV-01): "Blocks SHOP-20", "blocks 1".
    pub blocks: Option<crate::view_models::dependencies::Blocks>,
}

/// What the card knows about its agents (`board.agents`, AGT-04): the footer's agent part ([`footer_agent`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentFacts {
    /// How many agents there are: with one, the card names none (d0086, "only what's needed").
    pub count: usize,
    /// The agent of the running developer session and of the running validator.
    pub running_developer: Option<String>,
    pub running_validator: Option<String>,
    /// The roles held before they start (M23): a Developer's while it waits in Ready for Dev, a Validator's in Dev.
    pub holds: Vec<CardHold>,
    /// The tool and model each running session runs on, and the fallback it runs on (AGM-03).
    pub developer_on: Option<RunsOnTool>,
    pub validator_on: Option<RunsOnTool>,
    /// Waiting before an Agent stage: where it starts (AGM-09).
    pub starts: Option<CardStart>,
    /// The agent whose model the tool refused (Open agent…).
    pub refused_agent: Option<String>,
}

impl AgentFacts {
    /// From the service's facts for one card, with the number of agents.
    pub fn of(card: &CardAgents, count: usize) -> Self {
        Self {
            count,
            running_developer: card.running_developer.clone(),
            running_validator: card.running_validator.clone(),
            holds: card.holds.clone(),
            developer_on: card.running_developer_on.clone(),
            validator_on: card.running_validator_on.clone(),
            starts: card.starts.clone(),
            refused_agent: card.refused_agent.clone(),
        }
    }

    /// The hold of `role`, if it has one.
    pub fn hold(&self, role: Role) -> Option<&CardHold> {
        self.holds.iter().find(|h| h.role == role)
    }
}

impl CardExtras {
    /// The card's agents from `board.agents` (T-0345, T-0362): the footer's facts, whether several tools are
    /// connected, and the avatars: each role's running session's tool and model, else a held role's tool.
    pub fn with_agents(mut self, board: &BoardAgents, key: &TicketKey) -> Self {
        self.several_tools = board.connected.len() > 1;
        let Some(card) = board.cards.iter().find(|c| &c.key == key) else {
            return self;
        };
        self.agents = AgentFacts::of(card, board.agent_count);
        let avatar = |on: &Option<RunsOnTool>, role: Role| match on {
            Some(on) => Some(ToolAvatarView::of(on.tool, on.model.as_deref())),
            None => card
                .holds
                .iter()
                .find(|h| h.role == role)
                .map(|h| ToolAvatarView::of(h.tool, h.model.as_deref())),
        };
        self.developer = avatar(&card.running_developer_on, Role::Developer);
        self.validator = avatar(&card.running_validator_on, Role::Validator);
        self
    }

    /// The extras that come from the agent's live signal (T-0075; d0042): the question, prompt or last line of
    /// a needs-you ticket (and whether a helper agent asks), or what a ticket works on in the background with how
    /// long it has run ("test run, 1 helper agent · 6 min"), or when it wakes up ("wakes up at 4:10pm").
    pub fn with_signal(mut self, ticket: &Ticket, now_ms: i64) -> Self {
        let Some(signal) = &ticket.signal else {
            return self;
        };
        if ticket.status.is_needs_you_family() {
            match signal
                .text
                .as_deref()
                .and_then(|t| t.strip_prefix(HELPER_PROMPT_PREFIX))
            {
                Some(prompt) => {
                    self.helper_agent = true;
                    self.question = Some(prompt.to_owned());
                }
                None => self.question = signal.text.clone(),
            }
        } else if ticket.status == Status::WorkingInBackground {
            match (&signal.text, signal.wakeup_at) {
                (Some(text), _) => {
                    let tasks = text.strip_prefix("background: ").unwrap_or(text);
                    let since = signal.since.unwrap_or(now_ms);
                    self.background = Some(format!("{tasks} · {}", minutes(now_ms - since)));
                }
                (None, Some(at)) => {
                    self.background = Some(wakeup_text(at));
                    self.background_wakeup = true;
                }
                (None, None) => {}
            }
        }
        self
    }
}

/// What an epic child's card says of its epic (EPC-09, BRD-03): the epic's name on the key's row ("Wishlist"); the
/// "2 of 4" band is gone (EPL-01). A pinned epic's children follow its pin (EPC-17). No sleeve: a standalone ticket.
/// `CardSleeve` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardSleeve")]
pub struct Sleeve {
    pub epic_key: String,
    pub name: String,
    pub pinned: bool,
}

/// The sleeve of `ticket` when it is an epic child; `all` is its project (its epic).
pub fn sleeve(ticket: &Ticket, all: &[Ticket]) -> Option<Sleeve> {
    let epic = all.iter().find(|t| Some(&t.key) == ticket.epic.as_ref())?;
    Some(Sleeve {
        epic_key: epic.key.to_string(),
        name: epic.title.clone(),
        pinned: epic.flags.is_pinned(),
    })
}

/// Who added a child that joined its whole-queued epic's queue (EPC-13): "SHOP-40.2", "you", a Project agent.
fn joined_by(actor: &Actor, ticket: &Ticket, agent_name: Option<&str>) -> String {
    match actor {
        Actor::User(_) => "you".to_owned(),
        Actor::Agent(key) => key.to_string(),
        // The board knows the current name of the agent that created the ticket (d0068).
        Actor::ProjectAgent(_) => match agent_name.filter(|_| &ticket.created_by == actor) {
            Some(name) => format!("Project agent \u{201c}{name}\u{201d}"),
            None => "a Project agent".to_owned(),
        },
    }
}

/// The hint on an unblocked Backlog card.
pub const READY_TO_START: &str = "Ready to start";

/// A colour of the situation line (its edge and its text) or of a hover's icon. `CardTone` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "CardTone")]
pub enum Tone {
    /// Neutral: the edge in `border-strong`, the text in `text-muted`.
    Muted,
    Attention,
    Error,
    /// Amber: "not you, but soon".
    Warning,
    Working,
    Review,
    Epic,
    Accent,
    /// Secondary text (the situation's words next to a coloured edge).
    Secondary,
    /// A green edge with calm text: how it passed ("Passed Security · round 2", `s03-46`).
    Passed,
    /// An amber edge with calm text: a skip, a move by the user, a conflict being fixed ("Skipped Security · by you",
    /// "Merge conflict · fixing", "Delivered without checks"; `s03-46`, `s03-48`).
    Paused,
}

impl Tone {
    /// The text's colour next to an edge of this tone: red, amber and violet lines are coloured, the rest calm.
    fn text(self) -> Tone {
        match self {
            Tone::Error | Tone::Warning | Tone::Epic => self,
            _ => Tone::Secondary,
        }
    }
}

/// The epic card's status pill (the Epic Card keeps its own anatomy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pill {
    pub legend: Legend,
    pub label: String,
}

/// A small card shown on hover (*04 → Row · Unread notes & same file (0083 · T) → HOVER*; *03 → 25*): an icon and a
/// title, then short sections. `CardHover` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardHover")]
pub struct Hover {
    pub icon: IconName,
    /// The icon's colour.
    pub tone: Tone,
    pub title: String,
    pub sections: Vec<HoverSection>,
}

/// One section of a [`Hover`]: an optional mono eyebrow ("LEAD'S VIEW") and its text. `CardHoverSection` on the
/// page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardHoverSection")]
pub struct HoverSection {
    pub label: Option<String>,
    pub text: String,
    /// Quiet text: the footnote.
    pub muted: bool,
}

/// On the page: `"resume"`, `{ "pick_option": 2 }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CardActionKind {
    /// "Deliver now": a message waits to be typed (SES-06).
    DeliverNow,
    /// A click on an answer option (ATT-12): the panel opens with it picked (the last number is Other…).
    PickOption(u32),
    /// "Send answer" with the preselected (recommended) option.
    SendAnswer,
    /// "Answer", "Reply" or "Open": the ticket's sheet, where the prompt, the question or the stop is dealt with
    /// (*Ticket Card v2*'s one action on a needs-you card).
    Open,
    Resume,
    /// Resume failed: the same resume again.
    RetryResume,
    /// Resume failed: a new session in the same worktree, after the panel's confirmation.
    StartFromScratch,
    RetryStart,
    Review,
    /// A ticket back from PR rework and submitted again: its new commits go to the PR (REV-14).
    PushToPr,
    MarkMerged,
    MarkClosed,
    Reopen,
    Retry,
    /// The run port: its menu (Open, Stop; RUN-01).
    RunChip,
    /// Validation ended without a result: a fresh validator (VAL-07).
    RetryValidation,
    /// Auto-merge stopped: run the merge again (AMG-03, AMG-04).
    RetryMerge,
    /// Auto-merge stopped: back to Ready for Review, still validated, not merged again.
    MoveToReview,
    /// A model that can't run (AGM-03; d0108 second gap review rule 7): Settings → Agents opens on that agent's page
    /// (the card's [`open_agent`]); replaces Change agent (Q280).
    OpenAgent,
    /// A Developer held for its tool (AGT-08, d0086): its fallback takes it (`ticket.use_fallback`).
    UseFallback,
    /// A Validator round held for its tool: its fallback takes it.
    UseValidatorFallback,
    /// Your check (FLW-14): Pass moves it on, or offers the delivery choices when a click Delivery comes next
    /// (`ticket.pass`).
    Pass,
    /// Your check: the Fail dialog ("Fail Review on SHOP-18?").
    Fail,
    /// A To deliver card (FLW-15): the Local merge confirmation…
    LocalMerge,
    /// …or the Open PR dialog.
    OpenPr,
    /// A PR open (FLW-16): the "Send SHOP-26 back with PR comments?" dialog.
    SendBackPr,
    /// An epic's PR open (FLW-16): a Backlog child holding the PR's unresolved comments.
    AddChildFromPr,
    /// An agent check's last round failed (FLW-13): on to the next stage; its Delivery waits for a click.
    MoveOnAnyway,
    /// A check out of rounds (BRD-03): the card's one action opens the ticket, where Move on anyway, another round
    /// and answering the agent are.
    Decide,
    /// An agent check held for the plan limit with no usable fallback (FLW-13, Q277).
    SkipCheck,
    /// "Merging · running the merge check" (d0120, Q475): Stop ends the check and nothing is merged.
    StopMergeCheck,
}

impl CardActionKind {
    /// The validation action a card button sends (T-0187, T-0189), if it is one.
    pub fn validation_action(self) -> Option<otaq_core::domain::ValidationAction> {
        use otaq_core::domain::ValidationAction as A;
        match self {
            Self::RetryValidation => Some(A::Retry),
            Self::RetryMerge => Some(A::RetryMerge),
            Self::MoveToReview => Some(A::MoveToReview),
            Self::MoveOnAnyway => Some(A::MoveOnAnyway),
            Self::SkipCheck => Some(A::Skip),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CardAction {
    pub kind: CardActionKind,
    pub label: &'static str,
    pub icon: Option<IconName>,
    pub primary: bool,
}

/// The card's outline. `CardOutline` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "CardOutline")]
pub enum Outline {
    Normal,
    /// Turquoise: blocked on the user.
    Attention,
    /// Red: a closed parent blocks it.
    Error,
    /// Violet: a never-started child the epic's lead added ("new · added by Wishlist lead", LEAD-08).
    Epic,
}

/// Where a flag stands in the race for the head slot, highest first (*Flag order* `Uuk99`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// Validator · Needs you · Asks you (also via lead) · Idle: turquoise.
    NeedsYou,
    /// Interrupted · Resume fail · Start failed · Parent closed · Conflicts ×3 · Diverged · Dirty repo · Failed ×3 ·
    /// No result.
    Stopped,
    /// Limit: neutral.
    Limit,
    /// Validating… · Conflict.
    Busy,
    /// How it got where it is in Review and Done: Validated · Skipped · Did not pass · PR #481 · Closed.
    Outcome,
    /// :4020.
    Running,
    /// Working.
    Working,
}

/// The one short flag in the head and what the hover says in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadFlag {
    pub rank: Rank,
    /// "Failed ×3" (≤ 12 characters).
    pub label: String,
    /// "Validation failed 3 times": the hover's status.
    pub full: String,
    pub palette: Palette,
    pub icon: IconName,
}

impl HeadFlag {
    fn new(rank: Rank, label: impl Into<String>, full: impl Into<String>, legend: Legend) -> Self {
        let (_, palette, icon) = legend.spec();
        Self {
            rank,
            label: label.into(),
            full: full.into(),
            palette,
            icon,
        }
    }

    /// The flag in another colour than its legend entry's (the frame's).
    fn colored(self, palette: Palette) -> Self {
        Self { palette, ..self }
    }

    pub fn pill(&self) -> status::PillView {
        status::PillView {
            palette: self.palette,
            icon: self.icon,
            label: self.label.clone(),
        }
    }
}

/// The situation line. `CardSituation` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardSituation")]
pub struct Situation {
    /// The words, already joined with " · ".
    pub text: String,
    /// The 2 px left edge's colour: the state's.
    pub edge: Tone,
    pub text_tone: Tone,
    /// Mono: the whole line is a command ("Allow rm -rf .next?").
    pub mono: bool,
    /// The lead's note, the files both children changed, the schema paths: shown on hover, in order.
    pub hovers: Vec<Hover>,
}

/// The footer's token count: "182k", or "not reported" with why on hover (d0086: never a made-up number).
/// `CardTokens` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardTokens")]
pub struct Tokens {
    pub text: String,
    pub tooltip: Option<String>,
}

/// The footer's last signal: "40s", amber once it is getting old. `CardSignal` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardSignal")]
pub struct Signal {
    pub text: String,
    pub stale: bool,
}

/// The footer's agent part, on its left: "Fable test · set by you" (neutral), "Light · picked by Checkout slowness"
/// (violet: an agent picked it, d0077), "Strong" while it runs. `CardAgent` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardAgent")]
pub struct FooterAgent {
    pub text: String,
    /// Violet.
    pub picked: bool,
    /// The agent's reason for its pick, on hover.
    pub tooltip: Option<String>,
}

/// The footer, only when it has something to say: the agent part, while an agent runs its tokens and last signal,
/// and the card's one action. `CardFooter` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardFooter")]
pub struct Footer {
    /// Boxed: most cards have none, and the board keeps every card's view model.
    pub agent: Option<Box<FooterAgent>>,
    pub tokens: Option<Tokens>,
    pub signal: Option<Signal>,
    pub action: Option<CardAction>,
    /// A second button after the action: Open agent… next to Use fallback (`s03-54`).
    pub second: Option<CardAction>,
    /// The two buttons stand one under the other, each its own width: Send back with PR comments over Mark merged,
    /// Skip this check over Open agent… (`s03-46`, `s03-54`).
    pub stacked: bool,
}

/// The full status on hover ("Needs you · permission") and the facts the card has no room for ("Tokens: 241k",
/// "Branch otaq/SHOP-25"). `CardStatusHover` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardStatusHover")]
pub struct StatusHover {
    pub status: String,
    pub details: Vec<String>,
}

/// The card's one status line in words (BRD-03; *24 · The two views*): "Needs you: allow rm -rf .next?", "Working ·
/// back from Security", "starts next". `CardStatusLine` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardStatusLine")]
pub struct StatusLine {
    pub text: String,
    /// The dot's colour: the state's.
    pub dot: Palette,
    /// The words' colour: secondary, or the edge's while the card needs the user.
    pub tone: Tone,
    /// An icon in place of the dot: the link of a blocked ticket (DPV-01).
    pub icon: Option<IconName>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardViewModel {
    pub key: TicketKey,
    /// "SHOP-51 → SHOP-40.4" after Move to epic.
    pub key_label: String,
    /// The avatar of whoever has the turn; `None` while avatars are hidden or nobody has a session yet.
    pub avatar: Option<ToolAvatarView>,
    pub pinned: bool,
    /// The Priority chip after the key: P1, P2 and P4; `None` for Normal (TKT-09, d0065). An epic child shows the
    /// priority it follows from its epic the same way.
    pub priority: Option<Priority>,
    pub flag: Option<HeadFlag>,
    pub title: String,
    pub situation: Option<Situation>,
    /// "Created by Checkout slowness" (d0068).
    pub origin: Option<String>,
    pub footer: Option<Footer>,
    pub hover: StatusHover,
    /// The epic child's sleeve (EPC-09); `None` for a standalone ticket.
    pub sleeve: Option<Sleeve>,
    pub outline: Outline,
    /// Done cards fade back: the opacity.
    pub faded: Option<f32>,
    /// Asks you with answer options (ATT-12, d0061): listed compactly under the title.
    pub options: Option<crate::view_models::answer_options::AnswerOptions>,
    /// A Project agent just created it: the subtle highlight that fades (*18 → live board feedback*).
    pub fresh: bool,
    /// The one-line card of BRD-03, from the parts above. Boxed: the board keeps every card's view model.
    pub one_line: Box<OneLine>,
}

/// What the one-line card shows (BRD-03, *24 · The two views*) besides its key, epic and title.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OneLine {
    /// The one status line: the head flag's words and the situation's, joined ([`status_line`]).
    pub line: Option<StatusLine>,
    /// Who: the agent of whoever has the turn, or where it starts ("Rust developer"); named only while there are two
    /// or more agents (d0086). The model stays in the sheet.
    pub who: Option<String>,
    /// How long ago the working agent last said something ("3m").
    pub time: Option<Signal>,
    /// The coloured edge of a card that needs the user: turquoise, the review colour for your check, red for a
    /// closed parent.
    pub edge: Option<Tone>,
    /// The card's one action: the needs-you card's (Answer, Reply, Review, **Decide** for a check out of rounds,
    /// Resume…) or another primary one (Open PR, Local merge).
    pub action: Option<CardAction>,
    /// DPV-01's blocked line, which takes the line's place ([`CardExtras::blocked`]).
    pub blocked: Option<String>,
    /// What waits for it (DPV-01).
    pub blocks: Option<crate::view_models::dependencies::Blocks>,
}

/// "setup command failed" → "Setup command failed".
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Why a token count shows "not reported" (USG-01): never an error, only a display that can't be filled.
pub const TOKENS_UNKNOWN: &str =
    "Token count unknown: the agent tool's session file couldn't be read";
/// The footer's words for it (d0086).
pub const TOKENS_NOT_REPORTED: &str = "not reported";

/// "182k", "1.84M", "12.4M", "950".
pub fn tokens_label(tokens: u64) -> String {
    if tokens >= 10_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.)
    } else if tokens >= 1_000_000 {
        format!("{:.2}M", tokens as f64 / 1_000_000.)
    } else if tokens >= 1_000 {
        format!("{}k", (tokens as f64 / 1_000.).round() as u64)
    } else {
        tokens.to_string()
    }
}

/// "40s", "4m", "2h".
pub fn age_label(ms: i64) -> String {
    let secs = (ms / 1000).max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

/// Whether a ticket shows a time that moves on its own: a last signal or a background run on a Dev card.
pub fn ticks(ticket: &Ticket) -> bool {
    otaq_core::signals::LIVE.contains(&ticket.status) && ticket.flags.last_signal_at.is_some()
}

/// The states in which an agent runs: the footer shows its tokens.
fn runs(status: Status) -> bool {
    matches!(
        status,
        Status::Working
            | Status::WorkingInBackground
            | Status::Checking
            | Status::MergeConflictFixing
    )
}

/// The states in which the agent's own last signal says something: the footer shows it.
fn shows_signal(status: Status) -> bool {
    matches!(status, Status::Working | Status::MergeConflictFixing)
}

/// One thing the card has to say, before the card decides where it goes: a flag that may take the head, and its
/// words on the situation line when it wins (`text`) or when it loses (`lost`).
#[derive(Debug, Clone)]
struct Part {
    flag: Option<HeadFlag>,
    text: Option<String>,
    lost: Option<String>,
    tone: Tone,
    mono: bool,
    hover: Option<Hover>,
}

impl Part {
    /// A fact for the situation line only.
    fn quiet(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            flag: None,
            text: Some(text.into()),
            lost: None,
            tone,
            mono: false,
            hover: None,
        }
    }

    /// A flag and its words; when it loses the head, the same words join the line.
    fn flag(flag: HeadFlag, text: Option<String>, tone: Tone) -> Self {
        Self {
            flag: Some(flag),
            lost: text.clone(),
            text,
            tone,
            mono: false,
            hover: None,
        }
    }

    fn lost(self, lost: impl Into<String>) -> Self {
        Self {
            lost: Some(lost.into()),
            ..self
        }
    }

    fn mono(self) -> Self {
        Self { mono: true, ..self }
    }

    fn hover(self, hover: Option<Hover>) -> Self {
        Self { hover, ..self }
    }
}

/// The head flag and the situation line from the parts: the highest flag takes the head (the first of equals),
/// its words lead the line, the flags that lost follow in the flag order, then the quiet facts in their order.
fn arrange(parts: Vec<Part>) -> (Option<HeadFlag>, Option<Situation>) {
    let winner = parts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.flag.as_ref().map(|f| (f.rank, i)))
        .min()
        .map(|(_, i)| i);
    let mut line: Vec<&Part> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    if let Some(w) = winner
        && let Some(text) = &parts[w].text
    {
        line.push(&parts[w]);
        texts.push(text.clone());
    }
    let mut losers: Vec<(usize, &Part)> = parts
        .iter()
        .enumerate()
        .filter(|(i, p)| Some(*i) != winner && p.flag.is_some())
        .collect();
    losers.sort_by_key(|(i, p)| (p.flag.as_ref().map(|f| f.rank), *i));
    for (_, part) in losers {
        if let Some(lost) = &part.lost {
            line.push(part);
            texts.push(lost.clone());
        }
    }
    for part in parts.iter().filter(|p| p.flag.is_none()) {
        if let Some(text) = &part.text {
            line.push(part);
            texts.push(text.clone());
        }
    }
    let flag = winner.and_then(|w| parts[w].flag.clone());
    let situation = line.first().map(|first| Situation {
        text: texts.join(" \u{b7} "),
        edge: first.tone,
        text_tone: first.tone.text(),
        mono: line.len() == 1 && first.mono,
        hovers: line.iter().filter_map(|p| p.hover.clone()).collect(),
    });
    (flag, situation)
}

/// The status's own part: its head flag (if it has one in the flag order) and its words.
fn status_part(ticket: &Ticket, status: Status, x: &CardExtras, now_ms: i64) -> Option<Part> {
    use Rank as R;
    use otaq_core::domain::{AutoMergeStop, validation};
    let flags = &ticket.flags;
    let flag = |rank, label: &str, full: &str, legend| HeadFlag::new(rank, label, full, legend);
    // Local merge runs the project's Merge check before the target moves (d0120): calm, with Stop
    // ([`action`]), whatever column the ticket waits in.
    if flags.merge_check == Some(MergeCheckState::Running) {
        return Some(Part::flag(
            flag(
                R::Busy,
                MERGING,
                validation::MERGE_CHECK_RUNNING,
                Legend::Validating,
            )
            .colored(Palette::Working),
            Some(validation::MERGE_CHECK_RUNNING.to_owned()),
            Tone::Muted,
        ));
    }
    Some(match status {
        Status::NeedsYou => {
            let text = x
                .question
                .clone()
                .unwrap_or_else(|| "Waiting for your permission".into());
            if x.helper_agent {
                Part::flag(
                    flag(
                        R::NeedsYou,
                        "Needs you",
                        "Needs you \u{b7} helper agent",
                        Legend::NeedsYou,
                    ),
                    Some(format!("helper agent asks: {text}")),
                    Tone::Attention,
                )
            } else {
                Part::flag(
                    flag(
                        R::NeedsYou,
                        "Needs you",
                        "Needs you \u{b7} permission",
                        Legend::NeedsYou,
                    ),
                    Some(text),
                    Tone::Attention,
                )
                .mono()
            }
        }
        // The validator's permission prompt (VAL-11).
        Status::CheckerNeedsYou => Part::flag(
            flag(
                R::NeedsYou,
                VALIDATOR_ASKS,
                validation::VALIDATOR_NEEDS_YOU,
                Legend::ValidatorNeedsYou,
            ),
            Some(format!(
                "validator: {}",
                x.question
                    .clone()
                    .unwrap_or_else(|| "Waiting for your permission".into())
            )),
            Tone::Attention,
        )
        .mono(),
        Status::AsksYou => {
            let question = x
                .question
                .clone()
                .unwrap_or_else(|| "Asks you a question".into());
            // A question the epic's lead passed on (LEAD-06): "via Wishlist lead: …".
            let text = match &flags.via_lead {
                Some(lead) => format!("via {lead}: {question}"),
                None => question,
            };
            Part::flag(
                flag(R::NeedsYou, "Asks you", "Asks you", Legend::AsksYou),
                Some(text),
                Tone::Attention,
            )
        }
        Status::Idle => Part::flag(
            flag(R::NeedsYou, "Idle", "Idle", Legend::Idle),
            Some(match &x.question {
                Some(q) => format!("Ended its turn: \u{201c}{q}\u{201d}"),
                None => "Ended its turn".into(),
            }),
            Tone::Attention,
        ),
        Status::Interrupted => Part::flag(
            flag(
                R::Stopped,
                "Interrupted",
                "Interrupted",
                Legend::Interrupted,
            ),
            // *16 → Validating ticket after a restart (SHOP-29)*.
            Some(if x.was_validating {
                "Validation stopped by restart \u{b7} Resume starts it fresh (not counted)".into()
            } else {
                "Session lost after reboot".into()
            }),
            Tone::Error,
        ),
        Status::ResumeFailed => Part::flag(
            flag(
                R::Stopped,
                "Resume fail",
                "Resume failed",
                Legend::Interrupted,
            ),
            Some("Resume failed \u{b7} retry or start from scratch".into()),
            Tone::Error,
        ),
        // Start failed on the model (AGT-05, SDD §7.2 `[M22]`): what couldn't run, the tool's message on hover.
        Status::StartInterrupted if let Some(message) = start_refusal(ticket) => Part::flag(
            flag(
                R::Stopped,
                "Start failed",
                "Start failed \u{b7} couldn\u{2019}t run the model",
                Legend::StartInterrupted,
            ),
            Some(couldnt_run_on(developer_tool(x), None)),
            Tone::Error,
        )
        // The tool's message names the model itself.
        .hover(Some(refusal_hover(
            couldnt_run_on(developer_tool(x), None),
            message,
        ))),
        Status::StartInterrupted => Part::flag(
            flag(
                R::Stopped,
                "Start failed",
                "Start failed",
                Legend::StartInterrupted,
            ),
            Some(match &x.start_failure {
                Some(step) => format!("{} \u{b7} agent never started", capitalized(step)),
                None => "Start didn\u{2019}t finish \u{b7} agent never started".into(),
            }),
            Tone::Error,
        ),
        Status::ParentClosed => Part::flag(
            flag(
                R::Stopped,
                PARENT_CLOSED,
                "Parent closed",
                Legend::ParentClosed,
            ),
            Some(match &x.closed_parent {
                Some(p) => format!(
                    "{p} was closed without merging. Can\u{2019}t be approved until you reopen it or remove the \
                     dependency."
                ),
                None => "A parent was closed without merging".into(),
            }),
            Tone::Error,
        ),
        // The last round failed: its verdict, pointing at the checker's comment (VAL-06, CMT-10; `s05-41`: "10,000 row
        // limit isn't enforced · see comment").
        Status::CheckFailed if !x.custom_flow => {
            let n = ticket.validation_round.max(1);
            Part::flag(
                flag(
                    R::Stopped,
                    &format!("Failed \u{d7}{n}"),
                    &validation::failed_times(n),
                    Legend::ValidationFailed,
                ),
                Some(
                    flags
                        .verdict
                        .as_deref()
                        .map(ticket_comments::verdict_line)
                        .unwrap_or_else(|| validation::failed_times(n)),
                ),
                Tone::Attention,
            )
        }
        Status::CheckFailed => {
            let n = ticket.validation_round.max(1);
            let times = check_failed_times(flags.check.as_deref(), n);
            Part::flag(
                flag(
                    R::Stopped,
                    &format!("Failed \u{d7}{n}"),
                    &times,
                    Legend::ValidationFailed,
                ),
                Some(match flags.verdict.as_deref() {
                    Some(verdict) => {
                        format!("{times} \u{b7} {}", ticket_comments::verdict_line(verdict))
                    }
                    None => times.clone(),
                }),
                Tone::Attention,
            )
        }
        // The validator's model was refused (AGT-05): not a round; its message on hover.
        Status::CheckNoResult if let Some((model, message)) = validator_refusal(ticket) => {
            Part::flag(
                flag(
                    R::Stopped,
                    "No result",
                    validation::NO_RESULT,
                    Legend::ValidationNoResult,
                ),
                Some(VALIDATOR_COULDNT_RUN_ITS_MODEL.to_owned()),
                Tone::Attention,
            )
            .hover(Some(refusal_hover(
                format!(
                    "The validator couldn\u{2019}t run {}",
                    model.as_deref().unwrap_or("its model")
                ),
                &message,
            )))
        }
        // No verdict, or a discarded one: the core's text says which (VAL-02, VAL-07).
        Status::CheckNoResult => Part::flag(
            flag(
                R::Stopped,
                "No result",
                validation::NO_RESULT,
                Legend::ValidationNoResult,
            ),
            Some(
                x.question
                    .clone()
                    .unwrap_or_else(|| validation::NO_RESULT.to_owned()),
            ),
            Tone::Attention,
        ),
        // "Keeps conflicting with main" / "… with the epic's branch" after 3 conflicts in a row (FLW-17, `s03-46`), or
        // the Local merge's message.
        Status::DeliveryStopped => {
            let n = ticket.auto_merge_conflicts.max(1);
            let (label, full) = match ticket.auto_merge_stop {
                Some(AutoMergeStop::BaseDiverged) => ("Diverged".to_owned(), "Base diverged"),
                Some(AutoMergeStop::CheckoutDirty) => ("Dirty repo".to_owned(), "Checkout dirty"),
                Some(AutoMergeStop::KeepsFailingMergeCheck) => (
                    format!("Check failed \u{d7}{n}"),
                    "Merge check keeps failing",
                ),
                Some(AutoMergeStop::DeliveryFailed) => {
                    ("Not delivered".to_owned(), "Couldn\u{2019}t deliver")
                }
                _ => (format!("Conflicts \u{d7}{n}"), "Keeps conflicting"),
            };
            Part::flag(
                flag(R::Stopped, &label, full, Legend::AutoMergeStopped),
                Some(match ticket.auto_merge_stop {
                    Some(AutoMergeStop::KeepsConflicting) => keeps_conflicting(ticket, x),
                    _ => x
                        .question
                        .clone()
                        .unwrap_or_else(|| Status::DeliveryStopped.label().to_owned()),
                }),
                Tone::Attention,
            )
        }
        // Stopped by the plan limit (neutral, not in Needs you).
        Status::WaitingForLimitReset => Part::flag(
            flag(
                R::Limit,
                "At limit",
                "Waiting for limit reset",
                Legend::WaitingForLimitReset,
            ),
            None,
            Tone::Muted,
        )
        .lost("waiting for limit reset"),
        // Held by the plan's usage limit (SCH-04): neutral like the other limit waits.
        Status::Held if x.held_by_limit => Part::flag(
            flag(R::Limit, "At limit", "Held", Legend::WaitingForLimitReset),
            Some(x.held_reason.clone().unwrap_or_else(|| "Held".into())),
            Tone::Muted,
        ),
        Status::Held => Part::quiet(
            x.held_reason.clone().unwrap_or_else(|| "Held".into()),
            Tone::Warning,
        ),
        // Calm, like working (*Validating… (0063)*): which round and how long. Any other agent check reads
        // "Checking" (FLW-13, `s03-44`).
        Status::Checking if is_validate(flags.check.as_deref()) => Part::flag(
            flag(
                R::Busy,
                "Validating\u{2026}",
                "Validating\u{2026}",
                Legend::Validating,
            )
            .colored(Palette::Queued),
            validating_text(ticket, now_ms),
            Tone::Muted,
        ),
        Status::Checking => Part::flag(
            flag(
                R::Busy,
                CHECKING,
                &format!(
                    "{CHECKING} \u{b7} {}",
                    flags.check.as_deref().unwrap_or_default()
                ),
                Legend::Validating,
            ),
            validating_text(ticket, now_ms),
            Tone::Muted,
        ),
        // A hand-off typed in that the agent hasn't picked up: what it waits for, not "Working" (d0118).
        Status::Working | Status::MergeConflictFixing if let Some(kind) = flags.hand_off => {
            Part::quiet(hand_off_line(kind), Tone::Muted)
        }
        // The Merge check failed on the rebased result (d0120): back with the agent, as after a conflict.
        Status::MergeConflictFixing if flags.merge_check == Some(MergeCheckState::Failed) => {
            let f = flag(
                R::Busy,
                CHECK_FAILED,
                validation::MERGE_CHECK_FAILED_FIXING,
                Legend::MergeConflictFixing,
            );
            Part::flag(
                if x.custom_flow {
                    f
                } else {
                    f.colored(Palette::Working)
                },
                Some(validation::MERGE_CHECK_FAILED_FIXING.to_owned()),
                if x.custom_flow {
                    Tone::Paused
                } else {
                    Tone::Muted
                },
            )
        }
        // The agent of the nearest earlier Agent stage rebases with restack, no click (FLW-17; amber, `s03-46`). A
        // conflict the test merge found in Review says which branch: "Conflicts with main · fixing" (d0127).
        Status::MergeConflictFixing => {
            let fixing = match flags.conflicts_with.as_deref() {
                Some(target) if flags.conflict_in_review => validation::conflicts_fixing(target),
                _ => validation::MERGE_CONFLICT_FIXING.to_owned(),
            };
            Part::flag(
                {
                    let f = flag(R::Busy, "Conflict", &fixing, Legend::MergeConflictFixing);
                    // Today's calm pill on the default flow.
                    if x.custom_flow {
                        f
                    } else {
                        f.colored(Palette::Working)
                    }
                },
                Some(match flags.conflict_files.as_slice() {
                    [] => fixing.clone(),
                    [file] => format!("{fixing} \u{b7} {file}"),
                    [file, rest @ ..] => format!("{fixing} \u{b7} {file} +{}", rest.len()),
                }),
                if x.custom_flow {
                    Tone::Paused
                } else {
                    Tone::Muted
                },
            )
        }
        Status::Working => Part::flag(
            flag(R::Working, "Working", "Working", Legend::AgentWorking),
            None,
            Tone::Working,
        ),
        Status::WorkingInBackground => Part::flag(
            flag(
                R::Working,
                "Working",
                "Working in background",
                Legend::AgentWorking,
            ),
            x.background.as_ref().map(|text| {
                if x.background_wakeup {
                    text.clone()
                } else {
                    format!("background: {text}")
                }
            }),
            Tone::Muted,
        ),
        Status::WaitingForYou if !x.custom_flow => return review_part(ticket),
        Status::WaitingForYou => return your_check_part(ticket),
        Status::ToDeliver if !x.custom_flow => return None,
        Status::ToDeliver => return delivery_part(ticket, x),
        // Stays in its Delivery stage until marked merged (FLW-16, `s03-46`): "PR #481 open · Opened by Shipper · 2 h
        // ago".
        Status::PrOpen => {
            let n = flags.pr_number?;
            let open = format!("PR #{n} open");
            Part::flag(
                flag(R::Outcome, &open, &open, Legend::PrOpen),
                Some(pr_opened_line(ticket, x, now_ms)),
                Tone::Muted,
            )
            .lost(open.clone())
        }
        // Closed by an agent's `otaq stage done` (AGM-10, Q276): "Nothing to merge · already fixed in SHOP-31".
        Status::ClosedNotMerged if let Some(reason) = &flags.nothing_to_merge => Part::flag(
            flag(
                R::Outcome,
                "Closed",
                "Closed \u{b7} nothing to merge",
                Legend::ClosedNotMerged,
            )
            .colored(Palette::Queued),
            Some(nothing_to_merge_line(reason)),
            Tone::Muted,
        ),
        Status::ClosedNotMerged => Part::flag(
            flag(
                R::Outcome,
                "Closed",
                "Closed \u{b7} not merged",
                Legend::ClosedNotMerged,
            )
            .colored(Palette::Queued),
            Some("closed without merging".into()),
            Tone::Muted,
        ),
        // An automatic Delivery with no check passed before it (d0108 rule 8, `s03-48`): amber, once.
        Status::Merged if flags.delivered_without_checks => {
            Part::quiet(validation::DELIVERED_WITHOUT_CHECKS, Tone::Paused)
        }
        Status::Merged if flags.merged_automatically => Part::quiet(
            match &x.sleeve {
                Some(s) => format!("{} \u{b7} {}", validation::MERGED_AUTOMATICALLY, s.name),
                None => validation::MERGED_AUTOMATICALLY.to_owned(),
            },
            Tone::Muted,
        ),
        Status::Parked => Part::quiet(PARKED, Tone::Warning),
        Status::WaitingForSlot => Part::quiet(WAITING_FOR_SLOT, Tone::Muted),
        Status::Queued if flags.resuming_conversation => {
            Part::quiet("Resuming conversation\u{2026}", Tone::Muted)
        }
        // A sibling's merge freed it; the epic lead's turn about that merge comes first (d0124).
        Status::Queued if flags.waiting_for_lead => Part::quiet(WAITING_FOR_LEAD, Tone::Muted),
        Status::Queued if x.starting => Part::quiet("Starting\u{2026}", Tone::Muted),
        Status::Queued => Part::quiet(x.waits_for.clone()?, Tone::Muted),
        Status::Backlog if flags.ready_to_start => Part::quiet(READY_TO_START, Tone::Muted),
        Status::Backlog | Status::Merged => return None,
    })
}

/// The line of a hand-off typed into the live session that its agent hasn't picked up yet (d0118).
pub const WAITING_FOR_PICK_UP: &str = "waiting for the agent to pick it up";

/// What the hand-off was, as the card names it: "Sent back", "Merge conflict", "Open PR".
pub fn hand_off_label(kind: HandOffKind) -> &'static str {
    match kind {
        HandOffKind::SendBack => "Sent back",
        HandOffKind::MergeConflict => "Merge conflict",
        HandOffKind::MergeCheck => "Merge check failed",
        HandOffKind::OpenPr => "Open PR",
        HandOffKind::Push => "Push to PR",
    }
}

/// "Sent back · waiting for the agent to pick it up" (d0118): the card's and the sheet's line in place of the working
/// one, until the agent's tool reports activity.
pub fn hand_off_line(kind: HandOffKind) -> String {
    format!("{} \u{b7} {WAITING_FOR_PICK_UP}", hand_off_label(kind))
}

/// The hand-off the ticket's agent hasn't picked up, while it is at work (Working, Merge conflict · fixing).
pub fn pending_hand_off(ticket: &Ticket, status: Status) -> Option<HandOffKind> {
    ticket
        .flags
        .hand_off
        .filter(|_| matches!(status, Status::Working | Status::MergeConflictFixing))
}

/// The situation line of a No result on the validator's model (update Y §3).
pub const VALIDATOR_COULDNT_RUN_ITS_MODEL: &str = "Validator couldn\u{2019}t run its model";

/// *Approved · PR open*'s line: Otaq doesn't follow the PR (d0017).
pub fn pr_not_watched() -> String {
    format!(
        "{} doesn\u{2019}t watch PRs. Mark it once it\u{2019}s merged on GitHub.",
        otaq_core::naming::PRODUCT_NAME
    )
}
/// A parked ticket's line (d0052).
pub const PARKED: &str = "Parked \u{b7} has work \u{b7} branch, files and conversation kept";
/// *Waiting for a free slot (0016)*.
pub const WAITING_FOR_SLOT: &str = "waiting for a free slot";
/// A child a sibling's merge freed, until the epic lead's turn about that merge ends (d0124).
pub const WAITING_FOR_LEAD: &str = "Waiting for the lead\u{2019}s update";

/// "round 1 of 3 · 2 min": which round the validator checks and how long it has run.
fn validating_text(ticket: &Ticket, now_ms: i64) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(r) = ticket.flags.validating_round {
        parts.push(format!("round {} of {}", r.round, r.of));
    }
    if let Some(since) = ticket.status_since {
        parts.push(minutes(now_ms - since));
    }
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
}

/// How a ticket in Ready for Review got there (*Note · M12*): Validated · Skipped · Did not pass take the head;
/// "Submitted by you · not validated" is a quiet line.
fn review_part(ticket: &Ticket) -> Option<Part> {
    use otaq_core::domain::ReviewEntry;
    let flag = ticket.flags.review_entry?;
    let label = flag.entry.label(flag.round);
    let f = |short: &str, legend| HeadFlag::new(Rank::Outcome, short, label.clone(), legend);
    Some(match flag.entry {
        ReviewEntry::Validated => Part::flag(
            f("Validated", Legend::Validated).colored(Palette::Approved),
            Some(format!("round {}", flag.round)),
            Tone::Muted,
        )
        .lost(label.clone()),
        ReviewEntry::Skipped => {
            Part::flag(f("Skipped", Legend::ValidationSkipped), None, Tone::Muted)
                .lost(label.clone())
        }
        // Reads as "did not pass": red, never like a pass.
        ReviewEntry::FailedMoved => {
            let mut rest = label.chars();
            let first = rest.next().map(|c| c.to_lowercase().to_string());
            let text = format!("{}{}", first.unwrap_or_default(), rest.as_str());
            Part::flag(
                f("Did not pass", Legend::FailedMoved),
                Some(text.clone()),
                Tone::Error,
            )
            .lost(format!("Did not pass \u{b7} {text}"))
        }
        // With a note, the note's line says it (see [`marker`]).
        ReviewEntry::SubmittedByUser if has_note(&ticket.flags) => return None,
        ReviewEntry::SubmittedByUser => {
            Part::quiet(format!("{label} \u{b7} not validated"), Tone::Muted)
        }
    })
}

/// A card in a human check (FLW-14, `s03-46`; `s03-54`): "Your check".
pub const YOUR_CHECK: &str = "Your check";
/// A To deliver card (FLW-15, `s03-46`): its Delivery waits for the user's Local merge (14 characters, the frame's
/// own flag like [`PARENT_CLOSED`])…
pub const READY_TO_MERGE: &str = "Ready to merge";
/// …or Open PR.
pub const READY_FOR_PR: &str = "Ready for PR";
/// A running agent check other than the default flow's Validate (FLW-13, `s03-44`).
pub const CHECKING: &str = "Checking";
/// The default flow's agent check, whose cards keep today's words ("Validating…", "Validation failed 3 times").
pub const VALIDATE: &str = "Validate";

/// Whether the check the card counts for is the default flow's Validate (or unnamed): today's words.
fn is_validate(check: Option<&str>) -> bool {
    check.is_none_or(|c| c == VALIDATE)
}

/// "1 time", "3 times".
fn times(n: u32) -> String {
    if n == 1 {
        "1 time".to_owned()
    } else {
        format!("{n} times")
    }
}

/// "Security failed 3 times" (`s03-44`); on Validate today's "Validation failed 3 times".
pub fn check_failed_times(check: Option<&str>, n: u32) -> String {
    match check.filter(|c| !is_validate(Some(c))) {
        Some(check) => format!("{check} failed {}", times(n)),
        None => otaq_core::domain::validation::failed_times(n),
    }
}

/// "Security failed · round 2 of 3" while the agent fixes the findings (`s03-44`); on Validate today's "Validation
/// failed · round 1 of 3".
pub fn check_failed_round(
    check: Option<&str>,
    round: otaq_core::domain::ValidationRound,
) -> String {
    match check.filter(|c| !is_validate(Some(c))) {
        Some(check) => format!(
            "{check} failed \u{b7} round {} of {}",
            round.round, round.of
        ),
        None => round.to_string(),
    }
}

/// The colour of how a card got where it is: green for a pass, amber for a skip or a move by the user (`s03-46`).
fn arrival_tone(arrival: &Arrival) -> Tone {
    match arrival {
        Arrival::Passed { .. } => Tone::Passed,
        Arrival::Finished { .. } => Tone::Muted,
        Arrival::Skipped { .. } | Arrival::FailedMovedOn { .. } | Arrival::MovedBack => {
            Tone::Paused
        }
    }
}

/// Your check (FLW-14, `s03-46`, `s03-54`): "Your check" and how it got here ("Passed Security · round 2", "Skipped
/// Security · by you", "Moved back by you"); Pass and Fail are its buttons.
fn your_check_part(ticket: &Ticket) -> Option<Part> {
    let flag = HeadFlag::new(
        Rank::Outcome,
        YOUR_CHECK,
        YOUR_CHECK,
        Legend::ReadyForReview,
    );
    Some(match &ticket.flags.arrival {
        Some(arrival) => Part::flag(flag, Some(arrival.label()), arrival_tone(arrival)),
        None => Part::flag(flag, None, Tone::Muted),
    })
}

/// A To deliver card (FLW-15, `s03-46`, `s03-54`): "Ready to merge" or "Ready for PR", with why it waits for a click
/// ("Failed Security 3 times · moved by you · open the PR when ready") or how it got here ("Passed Review"). An
/// automatic Delivery under way says its agent opens the PR.
fn delivery_part(ticket: &Ticket, x: &CardExtras) -> Option<Part> {
    let flags = &ticket.flags;
    let pr = x.delivers == Some(Delivers::Pr);
    let label = if pr { READY_FOR_PR } else { READY_TO_MERGE };
    let flag = HeadFlag::new(Rank::Outcome, label, label, Legend::ReadyForReview);
    if flags.delivering_automatically {
        let text = match &x.delivery_agent {
            Some(agent) => format!("{agent} is opening the PR"),
            None => "The PR is being opened".to_owned(),
        };
        return Some(Part::flag(flag, Some(text), Tone::Muted));
    }
    let when = if pr {
        "open the PR when ready"
    } else {
        "merge when ready"
    };
    Some(match (&flags.moved_past_check, &flags.arrival) {
        (Some(reason), _) => {
            Part::flag(flag, Some(format!("{reason} \u{b7} {when}")), Tone::Paused)
        }
        (None, Some(arrival)) => Part::flag(flag, Some(arrival.label()), arrival_tone(arrival)),
        (None, None) => Part::flag(flag, None, Tone::Muted),
    })
}

/// An age as a card says it: "now", "6 min", "3 h", "2 d".
fn age(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        0 => "now".to_owned(),
        m if m < 60 => format!("{m} min"),
        m if m < 24 * 60 => format!("{} h", m / 60),
        m => format!("{} d", m / (24 * 60)),
    }
}

/// "Opened by Shipper · 2 h ago" (FLW-15, `s03-46`): the Delivery stage's agent and when the PR opened; with one
/// agent (nobody to name, d0086) or no time, today's line that Otaq doesn't watch PRs.
fn pr_opened_line(ticket: &Ticket, x: &CardExtras, now_ms: i64) -> String {
    match (&x.delivery_agent, ticket.status_since) {
        (Some(agent), Some(since)) if x.agents.count > 1 => {
            let age = age(now_ms - since);
            let ago = if age == "now" {
                "just now".to_owned()
            } else {
                format!("{age} ago")
            };
            format!("Opened by {agent} \u{b7} {ago}")
        }
        _ => pr_not_watched(),
    }
}

/// "Keeps conflicting with main", "Keeps conflicting with the epic's branch" (FLW-17, `s03-46`).
fn keeps_conflicting(ticket: &Ticket, x: &CardExtras) -> String {
    let target = ticket
        .flags
        .conflicts_with
        .as_ref()
        .or(x.project_base.as_ref());
    match (&ticket.epic, target) {
        (Some(_), _) => "Keeps conflicting with the epic\u{2019}s branch".to_owned(),
        (None, Some(target)) => format!("Keeps conflicting with {target}"),
        (None, None) => "Keeps conflicting".to_owned(),
    }
}

impl CardViewModel {
    pub fn from(ticket: &Ticket, extras: &CardExtras, now_ms: i64) -> Self {
        Self::build(ticket, extras, now_ms, extras.several_tools)
    }

    /// [`Self::from`] with avatars shown or hidden on purpose.
    fn build(ticket: &Ticket, x: &CardExtras, now_ms: i64, avatars: bool) -> Self {
        // The user is in its terminal: the card stays calm, "your turn · in terminal" (ATT-03, d0042).
        let status = status::shown(ticket);
        let flags = &ticket.flags;
        let paused = paused_stop(ticket, status, x);
        // A needs-you ticket interrupted by a restart keeps its flag (d0045); answering resumes it. Only a question
        // is kept: a check or a working agent the restart stopped has nothing to answer (as `paused_stop`, T-0490).
        let kept = ticket
            .interrupted_from
            .filter(|was| was.is_needs_you_family() && status == Status::Interrupted && !paused);
        let stale = is_stale(status, flags.last_signal_at, now_ms);

        let mut parts = Vec::new();
        match kept {
            // A pause leaves the question as it was (PAU-03, `s24-07`); a restart says what ended the session.
            Some(was) if x.paused => parts.extend(status_part(ticket, was, x, now_ms)),
            Some(was) => {
                if let Some(part) = status_part(ticket, was, x, now_ms) {
                    parts.push(Part {
                        text: Some("Session ended by restart \u{b7} answering resumes it".into()),
                        mono: false,
                        ..part
                    });
                }
            }
            None if paused => parts.push(paused_part(ticket)),
            // A role held before it starts: the hold says why, not "Held", "Validating…" or the limit wait (M23).
            None if held(ticket, status, x).is_some() && !status.is_needs_you_family() => {}
            None => parts.extend(status_part(ticket, status, x, now_ms)),
        }
        if let Some(hold) = held(ticket, status, x) {
            parts.push(hold_part(hold, x, now_ms));
        }
        parts.extend(moved_part(status, x));
        if let Some(base) = flags.conflicts_with.as_ref().filter(|_| {
            !matches!(
                status,
                Status::MergeConflictFixing | Status::DeliveryStopped
            )
        }) {
            parts.push(
                Part::flag(
                    HeadFlag::new(
                        Rank::Busy,
                        "Conflict",
                        format!("Conflicts with {base}"),
                        Legend::Conflicts,
                    ),
                    Some(format!(
                        "Conflicts with {base} \u{b7} found after the last merge"
                    )),
                    Tone::Warning,
                )
                .lost(format!("conflicts with {base}")),
            );
        }
        if let Some(port) = flags.run_port {
            parts.push(
                Part::flag(
                    HeadFlag::new(
                        Rank::Running,
                        format!(":{port}"),
                        format!("Serving :{port}"),
                        Legend::Running,
                    ),
                    None,
                    Tone::Muted,
                )
                .lost(format!("also running :{port}")),
            );
        }
        if stale {
            let quiet = flags.last_signal_at.map_or(0, |at| now_ms - at) / 60_000;
            parts.push(Part::quiet(
                format!("agent last active {quiet} min ago"),
                Tone::Warning,
            ));
        }
        if ticket.flags.your_turn {
            parts.push(Part::quiet("your turn \u{b7} in terminal", Tone::Accent));
        }
        parts.extend(marker(ticket, x));
        if flags.messages_waiting > 0 {
            let n = flags.messages_waiting;
            parts.push(Part::quiet(
                format!("{n} message{} waiting", if n == 1 { "" } else { "s" }),
                Tone::Accent,
            ));
        }
        parts.extend(asked_parts(flags));
        parts.extend(lead_parts(ticket));
        if let Some(by) = &ticket.joined_queue_by {
            // A child that joined its whole-queued epic's queue on its own (EPC-13).
            parts.push(Part::quiet(
                format!(
                    "New \u{b7} added by {} \u{b7} in Ready for Dev",
                    joined_by(by, ticket, x.agent_name.as_deref())
                ),
                Tone::Epic,
            ));
        }
        parts.extend(fact_parts(ticket, x));
        let (flag, situation) = arrange(parts);

        let shown_status = kept.unwrap_or(status);
        let options = (status == Status::AsksYou)
            .then_some(ticket.question.as_ref())
            .flatten()
            .and_then(crate::view_models::answer_options::AnswerOptions::of);
        let (action, second) = if options.as_ref().is_some_and(|o| o.pick.is_some()) {
            (
                Some(CardAction {
                    kind: CardActionKind::SendAnswer,
                    label: "Send answer",
                    icon: Some(IconName::CornerDownLeft),
                    primary: true,
                }),
                None,
            )
        } else {
            (
                action(shown_status, kept.is_some(), ticket, x),
                second_action(shown_status, ticket, x),
            )
        };
        let footer = footer(
            ticket,
            status,
            stale,
            now_ms,
            (action, second),
            footer_agent(ticket, status, x).map(Box::new),
        );
        let outline = if status.is_needs_you_family() || kept.is_some() {
            Outline::Attention
        } else if status == Status::ParentClosed {
            Outline::Error
        } else if added_by_lead(ticket).is_some() {
            Outline::Epic
        } else {
            Outline::Normal
        };
        let hover = StatusHover {
            status: match &flag {
                Some(f) => f.full.clone(),
                None => status::pill(status).1.to_owned(),
            },
            details: details(ticket, status, x, footer.as_ref(), now_ms),
        };
        Self {
            key: ticket.key.clone(),
            key_label: match &x.old_key {
                Some(old) => format!("{old} \u{2192} {}", ticket.key),
                None => ticket.key.to_string(),
            },
            avatar: avatars.then(|| turn(ticket, status, x)).flatten(),
            pinned: flags.is_pinned(),
            priority: priority::on_card(ticket.priority),
            flag,
            title: ticket.title.clone(),
            situation,
            origin: origin(ticket, x),
            footer,
            hover,
            sleeve: x.sleeve.clone(),
            outline,
            faded: match status {
                // "Delivered without checks" is said at full strength (`s03-48`).
                Status::Merged if flags.delivered_without_checks => None,
                Status::Merged => Some(0.55),
                Status::ClosedNotMerged => Some(0.8),
                _ => None,
            },
            options,
            fresh: crate::view_models::agent_activity::fresh_from_project_agent(ticket, now_ms),
            one_line: Box::new(OneLine {
                who: who(ticket, status, x),
                blocked: x.blocked.clone(),
                blocks: x.blocks.clone(),
                ..OneLine::default()
            }),
        }
        .with_one_line(ticket, kept.is_some(), paused)
    }

    /// The one-line card of BRD-03 from the parts already worded: the line, the edge, the time and the one action.
    fn with_one_line(mut self, ticket: &Ticket, kept: bool, paused: bool) -> Self {
        let status = status::shown(ticket);
        let one = &mut self.one_line;
        if let Some(text) = one.blocked.clone() {
            // A ticket that hasn't started and waits for its parents (DPV-01): amber, the link in place of the dot,
            // nothing to do but wait (Start is refused, DPV-03).
            one.line = Some(StatusLine {
                text,
                dot: Palette::Paused,
                tone: Tone::Paused,
                icon: Some(crate::view_models::dependencies::ICON),
            });
            return self;
        }
        if paused {
            // What the pause stopped (PAU-03, `s24-07`): amber words, no edge and no action (Resume is the title
            // bar's, or the sheet's for this one ticket), and the time it had when it stopped.
            one.line = status_line(
                ticket,
                status,
                self.flag.as_ref(),
                self.situation.as_ref(),
                None,
            )
            .map(|line| StatusLine {
                dot: Palette::Paused,
                tone: Tone::Paused,
                ..line
            });
            one.time = held_time(ticket);
            return self;
        }
        one.edge = edge(ticket, status, kept, self.outline, self.footer.as_ref());
        one.line = status_line(
            ticket,
            status,
            self.flag.as_ref(),
            self.situation.as_ref(),
            one.edge,
        );
        one.time = self.footer.as_ref().and_then(|f| f.signal.clone());
        one.action = one_action(ticket, self.footer.as_ref(), one.edge.is_some());
        self
    }
}

/// "Paused": the card's word for a ticket the user's Pause stopped (PAU-03, d0112).
pub const PAUSED: &str = "Paused";

/// Whether the user's Pause stopped this ticket's agent (PAU-03): Interrupted in a paused project, unless it waited
/// for the user (then it keeps its question).
fn paused_stop(ticket: &Ticket, status: Status, x: &CardExtras) -> bool {
    x.paused
        && status == Status::Interrupted
        && !ticket
            .interrupted_from
            .is_some_and(|s| s.is_needs_you_family())
}

/// The pause's part (`s24-07`): "Paused", and for a check what it was doing ("validating · round 1 of 3",
/// "checking · round 1 of 3"); the card's other facts follow ("Paused · back from Security").
fn paused_part(ticket: &Ticket) -> Part {
    let flags = &ticket.flags;
    let text = (ticket.interrupted_from == Some(Status::Checking)).then(|| {
        let doing = if is_validate(flags.check.as_deref()) {
            "validating"
        } else {
            "checking"
        };
        match flags.validating_round {
            Some(r) => format!("{doing} \u{b7} round {} of {}", r.round, r.of),
            None => doing.to_owned(),
        }
    });
    Part::flag(
        HeadFlag::new(Rank::Busy, PAUSED, PAUSED, Legend::Idle).colored(Palette::Paused),
        text,
        Tone::Paused,
    )
}

/// The timer a pause holds (PAU-03): how long the agent had been quiet when it stopped, as the card last showed it.
fn held_time(ticket: &Ticket) -> Option<Signal> {
    let stopped = ticket.status_since?;
    let at = ticket.flags.last_signal_at?;
    Some(Signal {
        text: age_label(stopped - at),
        stale: false,
    })
}

/// The card's coloured edge (BRD-03): a card that needs the user (turquoise; red for a closed parent), and your
/// check (the review colour, `s24-03`'s SHOP-18), which waits for the user too.
fn edge(
    ticket: &Ticket,
    status: Status,
    kept: bool,
    outline: Outline,
    footer: Option<&Footer>,
) -> Option<Tone> {
    if outline == Outline::Error {
        return Some(Tone::Error);
    }
    if kept || ticket.needs_user() {
        return Some(Tone::Attention);
    }
    let reviews = footer
        .and_then(|f| f.action.as_ref())
        .is_some_and(|a| matches!(a.kind, CardActionKind::Pass | CardActionKind::Review));
    (status == Status::WaitingForYou && reviews).then_some(Tone::Review)
}

/// The card's one action (BRD-03): Review for your check (Pass and Fail are in the sheet), **Decide** for a check
/// that failed its rounds (Move on anyway, another round or answering the agent are in the sheet); otherwise the
/// action the card already had, when the card needs the user or the action is a primary one. Second buttons leave
/// the card.
fn one_action(ticket: &Ticket, footer: Option<&Footer>, edge: bool) -> Option<CardAction> {
    if ticket.status == Status::CheckFailed {
        return Some(CardAction {
            kind: CardActionKind::Decide,
            label: DECIDE,
            icon: None,
            primary: true,
        });
    }
    let action = footer.and_then(|f| f.action.clone())?;
    match action.kind {
        CardActionKind::Pass => Some(CardAction {
            kind: CardActionKind::Review,
            label: "Review",
            icon: Some(IconName::Eye),
            primary: true,
        }),
        // The answer options are the sheet's: Reply opens it.
        CardActionKind::SendAnswer => Some(CardAction {
            kind: CardActionKind::Open,
            label: "Reply",
            icon: None,
            primary: true,
        }),
        // Stop is quiet, but it is all the card offers while the Merge check runs (d0120).
        CardActionKind::StopMergeCheck => Some(action),
        _ if edge || action.primary => Some(action),
        _ => None,
    }
}

/// A check out of rounds: the card's one action opens the ticket, where the choices are (BRD-03).
pub const DECIDE: &str = "Decide";

/// Ready for Dev's next ticket while nothing starts (paused): the frame's "starts next" (`s24-03`, `s24-07`).
pub const STARTS_NEXT: &str = "starts next";

/// The one status line (BRD-03): the head flag's words, then the situation's ("Needs you: allow rm -rf .next?" after
/// a needs-you or stopped flag, "Checking · round 1 of 3" otherwise); the situation alone when it already says what the flag
/// says ("Security failed 3 times · …" under "Failed ×3"). The priority is the card's chip after its key, not part of
/// the line (#36). The words keep their case, as drawn ("Checking · round 1 of 3"); a blocked ticket's line is DPV-01's.
/// Nothing to say: no line (a calm Backlog card).
pub fn status_line(
    ticket: &Ticket,
    status: Status,
    flag: Option<&HeadFlag>,
    situation: Option<&Situation>,
    edge: Option<Tone>,
) -> Option<StatusLine> {
    // Waiting for the agent to pick up a hand-off: that alone, in the neutral tone (d0118).
    if let Some(kind) = pending_hand_off(ticket, status) {
        return Some(StatusLine {
            text: hand_off_line(kind),
            dot: Palette::Queued,
            tone: Tone::Secondary,
            icon: None,
        });
    }
    // An unblocked Backlog card says "Ready to start": nothing starts until it is moved, so "starts next" is only
    // Ready for Dev's (#36).
    let words = situation.map(|s| s.text.clone());
    let text = match (flag, words) {
        (None, None) => return None,
        (None, Some(words)) => words,
        (Some(flag), None) => flag_words(flag),
        (Some(flag), Some(words)) => {
            let lead = flag_words(flag);
            let words = words
                .strip_prefix("validator: ")
                .map_or(words.clone(), str::to_owned);
            if says(&words, &lead) {
                words
            } else if matches!(flag.rank, Rank::NeedsYou | Rank::Stopped) {
                format!("{lead}: {}", lowered(&words))
            } else {
                format!("{lead} \u{b7} {words}")
            }
        }
    };
    let dot = match (edge, flag) {
        (Some(Tone::Attention), _) => Palette::Attn,
        (Some(Tone::Error), _) => Palette::Error,
        (Some(Tone::Review), _) => Palette::Review,
        (_, Some(flag)) => flag.palette,
        (_, None) => status::pill(status).0.spec().1,
    };
    Some(StatusLine {
        text,
        dot,
        tone: edge.unwrap_or(Tone::Secondary),
        icon: None,
    })
}

/// A flag's words on the line: "Validating" without its dots, "Failed 3 times" for "Failed ×3".
fn flag_words(flag: &HeadFlag) -> String {
    if flag.rank == Rank::Running {
        // ":4020" says nothing alone: "Serving :4020".
        return flag.full.clone();
    }
    let label = flag.label.trim_end_matches('\u{2026}');
    match label.split_once(" \u{d7}") {
        Some((word, n)) => format!("{word} {}", times(n.parse().unwrap_or(0))),
        None => label.to_owned(),
    }
}

/// Whether `words` already say `lead` ("Merge conflict · fixing" says "Conflict"; "Security failed 3 times" says
/// "Failed 3 times").
fn says(words: &str, lead: &str) -> bool {
    let words = words.to_lowercase();
    let lead = lead.to_lowercase();
    let stem = lead.trim_end_matches('s');
    !stem.is_empty() && words.contains(stem)
}

/// The agent's own words after "Needs you:": "Allow rm -rf .next?" reads "allow rm -rf .next?"; a word in
/// capitals ("SHOP-18", "API") stays.
pub(crate) fn lowered(words: &str) -> String {
    let mut chars = words.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second)) if first.is_uppercase() && second.is_lowercase() => {
            first.to_lowercase().chain(words.chars().skip(1)).collect()
        }
        _ => words.to_owned(),
    }
}

/// Who the card names (BRD-03): the agent of whoever has the turn while one runs, a held role's agent, where a
/// waiting ticket starts; nobody while there is only one agent (d0086).
pub fn who(ticket: &Ticket, status: Status, x: &CardExtras) -> Option<String> {
    let a = &x.agents;
    if a.count < 2 {
        return None;
    }
    if let Some(hold) = held(ticket, status, x) {
        return hold.agent.clone();
    }
    match status {
        Status::Checking | Status::CheckerNeedsYou => a.running_validator.clone(),
        _ if a.running_developer.is_some() => a.running_developer.clone(),
        _ if ticket.place.is_waiting() => a.starts.as_ref().map(|s| s.agent.clone()),
        _ => None,
    }
}

/// Whose avatar the head shows (*Has the turn*): the validator while the ticket validates, the validator asks or
/// its round is held, else the developer.
fn turn(ticket: &Ticket, status: Status, x: &CardExtras) -> Option<ToolAvatarView> {
    let validator = matches!(status, Status::Checking | Status::CheckerNeedsYou)
        || held(ticket, status, x).is_some_and(|h| h.role == Role::Validator);
    match validator {
        true => x.validator.clone(),
        false => x.developer.clone(),
    }
}

/// The hold the card shows (M23, SCH-04): the Developer's while the ticket waits in Ready for Dev to start, the
/// Validator's while it is in Dev (the round didn't start). A needs-you state still wins the head.
pub fn held<'a>(ticket: &Ticket, status: Status, x: &'a CardExtras) -> Option<&'a CardHold> {
    match status {
        Status::Queued | Status::Held | Status::WaitingForSlot => x.agents.hold(Role::Developer),
        _ if crate::view_models::flow_columns::place_set(ticket).at_work() => {
            x.agents.hold(Role::Validator)
        }
        _ => None,
    }
}

/// "Codex not available", "Claude Code limit".
pub fn hold_reason_text(reason: HoldReason, tool: &str) -> String {
    match reason {
        HoldReason::ToolUnavailable => format!("{tool} not available"),
        HoldReason::ToolLimit => format!("{tool} limit"),
        HoldReason::ModelUnavailable => MODEL_UNAVAILABLE.to_owned(),
    }
}

/// The flag and line of a held role (0086, SDD §7.2; *03 → A model that can't run*): **Can't start** (red) for a
/// model that can't run, its tool not available or the model no longer listed ("Griller's model can't run: Claude
/// Code isn't available"); Limit (neutral) for a tool at its limit ("Claude Code limit · resets 3:45 pm"). The
/// buttons say what helps (Use fallback, Open agent…).
fn hold_part(h: &CardHold, x: &CardExtras, now_ms: i64) -> Part {
    let tool = AvatarTool::from(h.tool).name();
    let fallback = h
        .fallback
        .as_ref()
        .map(|f| AvatarTool::from(f.tool).name().to_owned());
    // One step, no chains: a fallback that can't start either leaves it waiting (0086).
    let blocked_too = fallback
        .as_deref()
        .filter(|_| h.fallback_held)
        .map(crate::view_models::tool_problems::fallback_blocked_too);
    let agent = h.agent.as_deref().unwrap_or("The agent");
    let cant_run = |why: String| {
        let mut text = format!("{agent}\u{2019}s model can\u{2019}t run: {why}");
        if let Some(too) = &blocked_too {
            text.push_str(&format!(" \u{b7} {too}"));
        }
        text
    };
    let flag = |label: &str, full: String, palette: Palette, icon: IconName| HeadFlag {
        rank: Rank::Limit,
        label: label.to_owned(),
        full,
        palette,
        icon,
    };
    match h.reason {
        HoldReason::ToolLimit => {
            // A limit with its tool's reset: "Claude Code limit · resets 3:45 pm" (Q154).
            let cause = match h.resets_at {
                Some(at) => format!(
                    "{} \u{b7} resets {}",
                    hold_reason_text(h.reason, tool),
                    crate::view_models::clock::reset_label(at, now_ms)
                ),
                None => hold_reason_text(h.reason, tool),
            };
            let text = match (&fallback, h.fallback_held) {
                (Some(fallback), true) => {
                    format!("{tool} and its fallback {fallback} are both at their limits")
                }
                // A check that can be skipped instead (FLW-13, `s03-54`).
                (None, _) if h.can_skip_check => {
                    format!("{cause} \u{b7} {tool} has no fallback")
                }
                _ => cause.clone(),
            };
            Part::flag(
                flag("At limit", cause, Palette::Queued, IconName::Hourglass),
                Some(text),
                Tone::Muted,
            )
        }
        HoldReason::ToolUnavailable => Part::flag(
            flag(
                CANT_START,
                format!("{CANT_START} \u{b7} {tool} isn\u{2019}t available"),
                Palette::Error,
                IconName::CircleAlert,
            ),
            Some(cant_run(format!("{tool} isn\u{2019}t available"))),
            Tone::Error,
        ),
        // The model on hover: "Old GPT · Codex · gpt-5.4 is no longer in Codex's list".
        HoldReason::ModelUnavailable => {
            let mut named = vec![agent.to_owned()];
            if x.several_tools && !agent.eq_ignore_ascii_case(tool) {
                named.push(tool.to_owned());
            }
            named.extend(h.model.clone());
            let why = match &h.model {
                Some(model) => format!("{tool} no longer lists {model}"),
                None => MODEL_UNAVAILABLE.to_owned(),
            };
            Part::flag(
                flag(
                    CANT_START,
                    format!("{CANT_START} \u{b7} {MODEL_UNAVAILABLE}"),
                    Palette::Error,
                    IconName::CircleAlert,
                ),
                Some(cant_run(why)),
                Tone::Error,
            )
            .hover(Some(Hover {
                icon: IconName::CircleAlert,
                tone: Tone::Error,
                title: format!(
                    "{} is no longer in {tool}\u{2019}s list",
                    named.join(" \u{b7} ")
                ),
                sections: Vec::new(),
            }))
        }
    }
}

/// The amber line of a session on its tool's fallback, while it runs (AGM-03): "On Codex · Claude Code limit",
/// "Validator on Codex · Copilot not available".
fn moved_part(status: Status, x: &CardExtras) -> Option<Part> {
    let (on, prefix) = match status {
        Status::Checking => (x.agents.validator_on.as_ref()?, "Validator on"),
        Status::Working | Status::WorkingInBackground => (x.agents.developer_on.as_ref()?, "On"),
        _ => return None,
    };
    let from = on.fallback.as_ref()?;
    let why = hold_reason_text(from.reason, AvatarTool::from(from.tool).name());
    Some(Part::quiet(
        format!("{prefix} {} \u{b7} {why}", AvatarTool::from(on.tool).name()),
        Tone::Warning,
    ))
}

/// "Nothing to merge · already fixed in SHOP-31": the agent's reason as the line goes on (its first line, without
/// the closing full stop, starting lower case).
pub fn nothing_to_merge_line(reason: &str) -> String {
    let first = reason
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .trim_end_matches('.');
    let mut chars = first.chars();
    match chars.next() {
        // "Already fixed" reads lower case after the dot; "PR #12 …" stays as it is.
        Some(c) if chars.clone().next().is_some_and(char::is_lowercase) => format!(
            "Nothing to merge \u{b7} {}{}",
            c.to_lowercase(),
            chars.as_str()
        ),
        Some(_) => format!("Nothing to merge \u{b7} {first}"),
        None => "Nothing to merge".to_owned(),
    }
}

/// The head flag of a held role whose model can't run: its tool isn't available, or its model is no longer listed
/// (≤ 12 characters; `s03-54`).
pub const CANT_START: &str = "Can\u{2019}t start";
/// The line's words for a model no longer in its tool's list (SDD §7.2).
pub const MODEL_UNAVAILABLE: &str = "Model unavailable";

/// The old callout's markers on a card that isn't stopped: the first that applies (they never pile up).
fn marker(ticket: &Ticket, x: &CardExtras) -> Option<Part> {
    let flags = &ticket.flags;
    if ticket.status == Status::WaitingForYou && flags.pr_rework {
        // PR rework (d0041): dragged back from Approved; Send back starts the agent.
        return Some(Part::quiet(
            "Back for rework \u{b7} Send back to start the agent",
            Tone::Review,
        ));
    }
    if ticket.status.is_needs_you_family()
        || matches!(
            ticket.status,
            Status::Interrupted
                | Status::StartInterrupted
                | Status::ResumeFailed
                | Status::ParentClosed
                | Status::Held
                | Status::MergeConflictFixing
        )
    {
        return None;
    }
    if let Some(answering) = &flags.answering {
        return Some(Part::quiet(
            format!("answering {answering} \u{b7} resumed, answer-only"),
            Tone::Working,
        ));
    }
    if flags.changed_since_review {
        // After a push the PR's content differs from the review (REV-06, T-0145).
        return Some(Part::quiet(
            "Changed since you reviewed it \u{b7} the pushed content differs",
            Tone::Warning,
        ));
    }
    if let Some(note) = flags
        .submitted_by_you
        .as_ref()
        // With no note, the review entry already says "Submitted by you · not validated" (d0063).
        .filter(|note| !note.trim().is_empty() || !submitted_entry(flags))
    {
        let line = crate::view_models::close::submitted_line(note);
        return Some(Part::quiet(
            match submitted_entry(flags) {
                true => format!("{line} \u{b7} not validated"),
                false => line,
            },
            Tone::Review,
        ));
    }
    if let Some(text) = lead_note_callout(ticket) {
        // LEAD-07: the lead's note answered the first stop; it carries the round counter, back at 0.
        return Some(Part::quiet(text, Tone::Epic));
    }
    if let Some(marker) = flags.validation_round {
        // The agent fixes what the check found (VAL-04): the verdict, pointing at the checker's comment (CMT-10,
        // `s05-41`); without one, the round, counted on the check that failed it (FLW-13).
        return Some(Part::quiet(
            match flags.verdict.as_deref() {
                Some(verdict) => ticket_comments::verdict_line(verdict),
                None => check_failed_round(flags.check.as_deref(), marker),
            },
            Tone::Muted,
        ));
    }
    if let Some(sent) = flags.sent_back {
        return Some(Part::quiet(sent_back_text(sent), Tone::Warning));
    }
    if let Some(old) = &x.old_key {
        let epic = ticket
            .epic
            .as_ref()
            .map_or_else(String::new, |e| format!(" {e}"));
        return Some(Part::quiet(
            format!("Moved into epic{epic} \u{b7} old key {old} still works"),
            Tone::Epic,
        ));
    }
    crate::view_models::queue::started_line(ticket).map(|line| Part::quiet(line, Tone::Working))
}

/// MSG-06, LEAD-06: the question this ticket's agent asked another agent or its lead, calm, never turquoise.
fn asked_parts(flags: &otaq_core::domain::Flags) -> Vec<Part> {
    let mut parts = Vec::new();
    if let Some(text) = asked_meta(flags) {
        parts.push(Part::quiet(
            match &flags.asked_question {
                Some(q) => format!("{text}: {}", quoted(q)),
                None => text,
            },
            Tone::Muted,
        ));
    }
    if let Some(asked) = &flags.asked_lead {
        parts.push(Part::quiet(
            format!("{ASKED_LEAD_WAITING}: {}", quoted(&asked.question)),
            Tone::Muted,
        ));
    }
    parts
}

/// The lead's markers (d0083, *04 → Row · Epic children & the lead*): "via Wishlist lead" (when the question isn't
/// in the line already), "Lead's note sent once" after the second stop, the unread note, the same file, and on a
/// never-started child "by Wishlist lead · …" or "new · added by Wishlist lead". The notes and files show on hover.
fn lead_parts(ticket: &Ticket) -> Vec<Part> {
    let flags = &ticket.flags;
    let running = otaq_core::epic_lead::overlap::is_running(ticket.status);
    let mut parts = Vec::new();
    if let Some(lead) = flags
        .via_lead
        .as_ref()
        .filter(|_| ticket.status != Status::AsksYou)
    {
        parts.push(Part::quiet(format!("via {lead}"), Tone::Epic));
    }
    if let Some(note) = second_stop(ticket) {
        parts.push(Part::quiet(LEAD_NOTE_ONCE, Tone::Epic).hover(Some(lead_note_hover(note))));
    }
    if let Some(notes) = flags.unread_notes.as_ref().filter(|_| running) {
        parts.push(
            Part::quiet(
                otaq_core::epic_lead::notes::count_text(notes.count as usize, &notes.from),
                Tone::Muted,
            )
            .hover(Some(unread_note_hover(notes))),
        );
    }
    if let Some(text) = same_file_text(&flags.same_file) {
        parts.push(Part::quiet(text, Tone::Muted).hover(same_file_hover(&flags.same_file)));
    }
    if let Some(by) = updated_by_lead(ticket) {
        parts.push(Part::quiet(updated_by_lead_text(ticket, by), Tone::Epic));
    } else if let Some(by) = added_by_lead(ticket) {
        parts.push(Part::quiet(
            format!("new \u{b7} added by {}", by.lead),
            Tone::Epic,
        ));
    }
    parts
}

/// The quiet facts at the end of the line: the diff, another base, open decisions, a schema overlap. The parents
/// are DPV-01's blocked line ([`crate::view_models::dependencies`]), no longer "after SHOP-18".
fn fact_parts(ticket: &Ticket, x: &CardExtras) -> Vec<Part> {
    let mut parts = Vec::new();
    if let Some((added, removed, files)) = x.diff {
        parts.push(Part::quiet(
            format!("+{added} \u{2212}{removed} \u{b7} {files} files"),
            Tone::Muted,
        ));
    }
    if let (Base::Branch(base), Some(project)) = (&ticket.base, &x.project_base)
        && base != project
        && !matches!(ticket.status, Status::Merged | Status::ClosedNotMerged)
    {
        parts.push(Part::quiet(format!("on {base}"), Tone::Muted));
    }
    // "1 decision": neutral, only in Ready for Review (DEC-03).
    if let Some(text) = decisions_label(ticket) {
        parts.push(Part::quiet(text, Tone::Muted));
    }
    // M19-e: another Review ticket adds files under the same schema path; the files on hover.
    if let Some(work) = &ticket.work
        && let Some(text) = schema_overlap_text(&work.schema_overlap)
    {
        parts.push(
            Part::quiet(text, Tone::Warning).hover(schema_overlap_hover(&work.schema_overlap)),
        );
    }
    // d0121: it went on without its remote; git's reason on hover. Gone at the next fetch that works.
    if let Some(warning) = ticket.work.as_ref().and_then(|w| w.remote_warning.as_ref())
        && !matches!(ticket.status, Status::Merged | Status::ClosedNotMerged)
    {
        parts.push(
            Part::quiet(remote_warning_text(warning), Tone::Warning)
                .hover(Some(remote_warning_hover(warning))),
        );
    }
    parts
}

/// "Couldn't update main from origin · started from your local main" (or "· merged into your local main"): the fetch
/// before the start or the Local merge failed and the ticket went on from the local base branch (d0121).
pub fn remote_warning_text(w: &otaq_core::domain::RemoteWarning) -> String {
    let went_on = match w.went_on {
        otaq_core::domain::WentOn::Started => "started from",
        otaq_core::domain::WentOn::Merged => "merged into",
    };
    format!(
        "Couldn\u{2019}t update {b} from {r} \u{b7} {went_on} your local {b}",
        b = w.branch,
        r = w.remote
    )
}

/// The hover of the remote warning: git's reason.
pub fn remote_warning_hover(w: &otaq_core::domain::RemoteWarning) -> Hover {
    Hover {
        icon: IconName::TriangleAlert,
        tone: Tone::Warning,
        title: remote_warning_text(w),
        sections: vec![HoverSection {
            label: None,
            text: w.reason.clone(),
            muted: false,
        }],
    }
}

/// The origin line of a card an agent created, while it waits in Backlog (d0019, d0068): "Created by Checkout
/// slowness" for a Project agent (its current name), "Created by SHOP-40.2" for a ticket's agent.
fn origin(ticket: &Ticket, x: &CardExtras) -> Option<String> {
    use crate::view_models::agent_activity::{origin_label, project_agent_label};
    if project_agent_label(ticket, x.agent_name.as_deref()).is_some() {
        return Some(match &x.agent_name {
            Some(name) => format!("Created by {name}"),
            None => "Created by a Project agent".to_owned(),
        });
    }
    origin_label(ticket)?;
    match &ticket.created_by {
        Actor::Agent(key) => Some(format!("Created by {key}")),
        _ => None,
    }
}

/// The footer: while an agent runs, its tokens and (while the agent itself is active and not quiet for long) its
/// last signal; and the card's one action. Nothing at all when it has nothing to say.
fn footer(
    ticket: &Ticket,
    status: Status,
    stale: bool,
    now_ms: i64,
    (action, second): (Option<CardAction>, Option<CardAction>),
    agent: Option<Box<FooterAgent>>,
) -> Option<Footer> {
    let flags = &ticket.flags;
    let running = runs(status);
    let tokens = running.then(|| tokens_of(flags)).flatten();
    let signal = flags
        .last_signal_at
        .filter(|_| running && shows_signal(status) && !stale)
        .map(|at| Signal {
            text: age_label(now_ms - at),
            stale: now_ms - at >= SIGNAL_AMBER_MS,
        });
    let stacked = second.is_some()
        && action.as_ref().is_some_and(|a| {
            matches!(
                a.kind,
                CardActionKind::SendBackPr | CardActionKind::SkipCheck
            )
        });
    (agent.is_some() || tokens.is_some() || signal.is_some() || action.is_some()).then_some(
        Footer {
            agent,
            tokens,
            signal,
            action,
            second,
            stacked,
        },
    )
}

/// The footer's agent part (update Y §8, AGT-04, AGM-09; *03 → Two projects, two flows*): never while there is only
/// one agent (d0086). While an agent runs, whoever has the turn's with its model ("Rust developer · sonnet", the
/// validator while it validates); a ticket waiting before an Agent stage says where it starts ("Grill · Griller ·
/// sonnet"); a held role names the agent it would start on. No choice line before the start.
pub fn footer_agent(ticket: &Ticket, status: Status, x: &CardExtras) -> Option<FooterAgent> {
    let a = &x.agents;
    if a.count < 2 {
        return None;
    }
    let plain = |text: String| FooterAgent {
        text,
        picked: false,
        tooltip: None,
    };
    if let Some(hold) = held(ticket, status, x) {
        return hold.agent.clone().map(plain);
    }
    if runs(status) {
        let (name, on) = match status {
            Status::Checking => (a.running_validator.as_ref(), a.validator_on.as_ref()),
            _ => (a.running_developer.as_ref(), a.developer_on.as_ref()),
        };
        let name = name?;
        return Some(plain(
            match on.and_then(|on| on.passed.as_deref().or(on.model.as_deref())) {
                Some(model) => format!("{name} \u{b7} {}", footer_model(Some(model))),
                None => name.clone(),
            },
        ));
    }
    if ticket.place.is_waiting() && !ticket.status.is_needs_you_family() {
        let start = a.starts.as_ref()?;
        return Some(plain(match &start.model {
            Some(model) => format!(
                "{} \u{b7} {} \u{b7} {}",
                start.stage,
                start.agent,
                footer_model(model.model.as_deref())
            ),
            None => format!("{} \u{b7} {}", start.stage, start.agent),
        }));
    }
    None
}

/// A model as the footer names it ([`crate::view_models::stage_choices::short_model`]; the card has no lists).
fn footer_model(model: Option<&str>) -> String {
    crate::view_models::stage_choices::short_model(model, None)
}

/// The tool the user's agents run on (M22: the one connected tool), by the name people read.
fn tool_name() -> &'static str {
    AvatarTool::from(otaq_core::agent_tools::Tool::ALL[0]).name()
}

/// The developer's tool by name: its session's or its row's ([`CardExtras::developer`]), else the first tool.
fn developer_tool(x: &CardExtras) -> &'static str {
    x.developer
        .as_ref()
        .map_or_else(tool_name, |a| a.tool.name())
}

/// A Start failed ticket's refusal (AGT-05): the tool's own message, kept on the start problem.
pub fn start_refusal(ticket: &Ticket) -> Option<&str> {
    (change_agent(ticket) == Some(Role::Developer))
        .then(|| ticket.start_problem.as_ref().map(|p| p.message.as_str()))
        .flatten()
}

/// A No result on the validator's model (AGT-05): the model and the tool's message, from the round's text ("The
/// validator couldn't run `claude-fable-5-1`: There's an issue…", `validation::couldnt_run_text`).
pub fn validator_refusal(ticket: &Ticket) -> Option<(Option<String>, String)> {
    if change_agent(ticket) != Some(Role::Validator) {
        return None;
    }
    let text = ticket.signal.as_ref()?.text.as_deref()?;
    let rest = text
        .strip_prefix(otaq_core::validation::VALIDATOR_COULDNT_RUN)?
        .trim_start();
    Some(
        match rest.strip_prefix('`').and_then(|r| r.split_once("`:")) {
            Some((model, message)) => (Some(model.to_owned()), message.trim().to_owned()),
            None => (None, rest.trim_start_matches(':').trim().to_owned()),
        },
    )
}

/// "Claude Code couldn't run claude-fable-5-1" (the panel's callout heading, the model from the refused session), or
/// "… the model" (the card's line and hover title, whose message names it).
pub fn couldnt_run(model: Option<&str>) -> String {
    couldnt_run_on(tool_name(), model)
}

/// [`couldnt_run`] on a given tool: "Codex couldn't run the model" (*Start failed · several tools*).
pub fn couldnt_run_on(tool: &str, model: Option<&str>) -> String {
    format!(
        "{tool} couldn\u{2019}t run {}",
        model.unwrap_or("the model")
    )
}

/// The hover of a refused model's line: what couldn't run, then the tool's own message.
fn refusal_hover(title: String, message: &str) -> Hover {
    Hover {
        icon: IconName::TriangleAlert,
        tone: Tone::Error,
        title,
        sections: vec![HoverSection {
            label: None,
            text: message.to_owned(),
            muted: false,
        }],
    }
}

fn tokens_of(flags: &otaq_core::domain::Flags) -> Option<Tokens> {
    match flags.tokens {
        Some(tokens) => Some(Tokens {
            text: tokens_label(tokens),
            tooltip: None,
        }),
        None => flags.tokens_unknown.then(|| Tokens {
            text: TOKENS_NOT_REPORTED.to_owned(),
            tooltip: Some(TOKENS_UNKNOWN.to_owned()),
        }),
    }
}

/// What the card has no room for, on hover: the tokens and last signal outside the footer, a parked branch, the PR
/// provider, how a Done ticket reached Review.
fn details(
    ticket: &Ticket,
    status: Status,
    x: &CardExtras,
    footer: Option<&Footer>,
    now_ms: i64,
) -> Vec<String> {
    let flags = &ticket.flags;
    let mut details = Vec::new();
    if footer.is_none_or(|f| f.tokens.is_none())
        && let Some(tokens) = tokens_of(flags)
    {
        details.push(format!("Tokens: {}", tokens.text));
    }
    if footer.is_none_or(|f| f.signal.is_none())
        && otaq_core::signals::LIVE.contains(&status)
        && let Some(at) = flags.last_signal_at
    {
        details.push(format!("Last signal: {} ago", age_label(now_ms - at)));
    }
    // The tool's own message when it refused the model: on the flag's hover too.
    if let Some(message) = start_refusal(ticket)
        .map(str::to_owned)
        .or_else(|| validator_refusal(ticket).map(|(_, m)| m))
    {
        details.push(message);
    }
    if let Some(branch) = &x.branch {
        details.push(format!("Branch {branch}"));
    }
    if let Some(provider) = x.pr_provider.clone().or_else(|| pr_provider_label(ticket)) {
        details.push(provider);
    }
    if status != Status::WaitingForYou
        && let Some(entry) = flags.review_entry
    {
        details.push(entry.entry.label(entry.round));
    }
    details
}

/// The Merge check's head flags (d0120): "Merging…" while it runs, "Check failed" while the agent fixes what it
/// found; the line says the rest.
pub const MERGING: &str = "Merging\u{2026}";
pub const CHECK_FAILED: &str = "Check failed";
/// Stop on "Merging · running the merge check" (d0120, Q475): nothing is merged.
pub const STOP: &str = "Stop";

/// "Use fallback": a held session starts on its tool's fallback (AGM-03).
pub const USE_FALLBACK: &str = "Use fallback";
/// "Open agent…": Settings → Agents on the agent whose model can't run (d0108 second gap review rule 7).
pub const OPEN_AGENT: &str = "Open agent\u{2026}";

/// "Skip this check" on a check held for the plan limit with no usable fallback (FLW-13, `s03-54`).
pub const SKIP_THIS_CHECK: &str = "Skip this check";
/// The buttons of a human check (FLW-14, `s03-46`).
pub const PASS: &str = "Pass";
pub const FAIL: &str = "Fail";
/// A PR open (FLW-16, `s03-46`).
pub const SEND_BACK_WITH_PR_COMMENTS: &str = "Send back with PR comments";
/// An agent check's last round failed (FLW-13, `s03-44`; d0108 "Move on anyway").
pub const MOVE_ON_ANYWAY: &str = "Move on anyway";

/// The second button: Open agent… after Use fallback or Skip this check on a held session (`s03-54`), Fail after
/// Pass, Mark merged after Send back with PR comments (`s03-46`).
fn second_action(status: Status, ticket: &Ticket, x: &CardExtras) -> Option<CardAction> {
    let quiet = |kind, label| {
        Some(CardAction {
            kind,
            label,
            icon: None,
            primary: false,
        })
    };
    if ticket.flags.merge_check == Some(MergeCheckState::Running) {
        return None;
    }
    if let Some(hold) = held(ticket, status, x) {
        return (hold.can_use_fallback || hold.can_skip_check)
            .then_some(())
            .and_then(|()| quiet(CardActionKind::OpenAgent, OPEN_AGENT));
    }
    match status {
        Status::WaitingForYou if x.custom_flow && your_check_buttons(ticket) => {
            quiet(CardActionKind::Fail, FAIL)
        }
        Status::PrOpen if ticket.flags.pr_number.is_some() && ticket.epic.is_none() => {
            quiet(CardActionKind::MarkMerged, "Mark merged")
        }
        _ => None,
    }
}

/// Whether a card in a human check offers Pass · Fail: not while it is back for PR rework or holds new commits for
/// its open PR (Push to PR, REV-14).
fn your_check_buttons(ticket: &Ticket) -> bool {
    !ticket.is_epic() && ticket.flags.pr_number.is_none() && !ticket.flags.pr_rework
}

/// The agent Open agent… opens: a held session's, else the one whose model the tool refused.
pub fn open_agent(ticket: &Ticket, x: &CardExtras) -> Option<String> {
    held(ticket, ticket.status, x)
        .and_then(|h| h.agent_id.clone())
        .or_else(|| x.agents.refused_agent.clone())
}

/// The card's one action (*Ticket Card v2 → footer*): Answer, Reply, Open, Resume, Review, Mark merged, Retry
/// start…; the others stay in the panel.
fn action(status: Status, kept: bool, ticket: &Ticket, x: &CardExtras) -> Option<CardAction> {
    let has_pr = ticket.flags.pr_number.is_some();
    let a = |kind, label, icon, primary| {
        Some(CardAction {
            kind,
            label,
            icon,
            primary,
        })
    };
    // Answer, Reply and Open unblock the card: the frame's filled button.
    let open = |label| a(CardActionKind::Open, label, None, true);
    // The only thing to do while the Merge check runs (d0120): the merge finishes on its own.
    if ticket.flags.merge_check == Some(MergeCheckState::Running) {
        return a(
            CardActionKind::StopMergeCheck,
            STOP,
            Some(IconName::Square),
            false,
        );
    }
    // A model the tool refused (AGT-05): open its agent; Retry start / Retry validation stay in the panel.
    // A held role (M23, AGM-03): its fallback when it can take it, then Open agent… ([`second_action`]); quiet
    // buttons.
    if let Some(hold) = held(ticket, status, x) {
        return match (hold.role, hold.can_use_fallback) {
            (Role::Validator, true) => a(
                CardActionKind::UseValidatorFallback,
                USE_FALLBACK,
                None,
                false,
            ),
            (_, true) => a(CardActionKind::UseFallback, USE_FALLBACK, None, false),
            // A check at the plan limit with no usable fallback can be skipped instead (FLW-13, Q277).
            (_, false) if hold.can_skip_check => {
                a(CardActionKind::SkipCheck, SKIP_THIS_CHECK, None, false)
            }
            (_, false) => a(CardActionKind::OpenAgent, OPEN_AGENT, None, false),
        };
    }
    if change_agent(ticket).is_some() {
        return a(CardActionKind::OpenAgent, OPEN_AGENT, None, true);
    }
    if kept {
        // Answered from the panel: that answer resumes it.
        return match status {
            Status::NeedsYou | Status::CheckerNeedsYou => open("Answer"),
            _ => open("Reply"),
        };
    }
    match status {
        Status::NeedsYou | Status::CheckerNeedsYou => open("Answer"),
        Status::AsksYou | Status::Idle => open("Reply"),
        Status::ParentClosed | Status::DeliveryStopped => open("Open"),
        Status::CheckFailed if !x.custom_flow => open("Open"),
        // Only Move on anyway (FLW-13, `s03-44`); another round and Answer the agent stay in the panel.
        Status::CheckFailed => a(CardActionKind::MoveOnAnyway, MOVE_ON_ANYWAY, None, false),
        Status::Interrupted => a(CardActionKind::Resume, "Resume", Some(IconName::Play), true),
        // *16 → Resume failed*: Start from scratch, the frame's primary; Retry stays in the panel.
        Status::ResumeFailed => a(
            CardActionKind::StartFromScratch,
            "Start from scratch",
            Some(IconName::RotateCcw),
            true,
        ),
        Status::StartInterrupted => a(
            CardActionKind::RetryStart,
            "Retry start",
            Some(IconName::RotateCcw),
            true,
        ),
        // Back from PR rework and submitted again: the new commits go to the PR (REV-14).
        Status::WaitingForYou if has_pr && !ticket.flags.pr_rework => a(
            CardActionKind::PushToPr,
            "Push to PR",
            Some(IconName::Upload),
            true,
        ),
        Status::WaitingForYou if has_pr => None,
        // Your check (FLW-14): Pass, then Fail ([`second_action`]).
        Status::WaitingForYou if x.custom_flow && !ticket.is_epic() => {
            a(CardActionKind::Pass, PASS, None, true)
        }
        Status::WaitingForYou => a(CardActionKind::Review, "Review", Some(IconName::Eye), true),
        // To deliver (FLW-15): the click the Delivery waits for; nothing while an automatic one runs.
        Status::ToDeliver if ticket.flags.delivering_automatically || !x.custom_flow => None,
        Status::ToDeliver if x.delivers == Some(Delivers::Pr) => {
            a(CardActionKind::OpenPr, "Open PR", None, true)
        }
        Status::ToDeliver => a(CardActionKind::LocalMerge, "Local merge", None, true),
        // Send back with PR comments, then Mark merged (FLW-16); Mark closed stays in the panel and the card's menu.
        // An epic child never has a PR.
        Status::PrOpen if has_pr && ticket.epic.is_none() => a(
            CardActionKind::SendBackPr,
            SEND_BACK_WITH_PR_COMMENTS,
            None,
            false,
        ),
        Status::PrOpen if has_pr => a(CardActionKind::MarkMerged, "Mark merged", None, false),
        Status::ClosedNotMerged => a(
            CardActionKind::Reopen,
            "Reopen",
            Some(IconName::RotateCcw),
            false,
        ),
        Status::Held if x.held_reason.is_some() && !x.held_by_limit => a(
            CardActionKind::Retry,
            "Retry",
            Some(IconName::RotateCcw),
            false,
        ),
        // VAL-07: a fresh validator; Move on anyway stays in the panel.
        Status::CheckNoResult => a(
            CardActionKind::RetryValidation,
            "Retry validation",
            Some(IconName::RotateCcw),
            true,
        ),
        _ if ticket.flags.messages_waiting > 0 => {
            a(CardActionKind::DeliverNow, "Deliver now", None, false)
        }
        _ => None,
    }
}

/// LEAD-06's words.
pub const ASKED_LEAD_WAITING: &str = "asked lead \u{b7} waiting";
/// The unread note's hover footnote (LEAD-10, CLI-08): "Read at the agent's next commit (otaq notes). Submit is refused
/// until then."
pub fn unread_note_hint() -> String {
    format!(
        "Read at the agent's next commit ({} notes). Submit is refused until then.",
        otaq_core::naming::COMMAND_NAME
    )
}
/// The same file's hover footnote (LEAD-09).
pub const SAME_FILE_HINT: &str =
    "Both changed it so far. Doesn't wake the lead; neither child is paused.";
/// After the second stop (*03 → 25*).
pub const LEAD_NOTE_ONCE: &str = "Lead's note sent once \u{b7} hover for it";

/// "“first line”".
fn quoted(text: &str) -> String {
    format!(
        "\u{201c}{}\u{201d}",
        text.lines().next().unwrap_or_default()
    )
}

/// What the lead did to a never-started child (LEAD-08); nothing once it has been to Dev.
fn by_lead(ticket: &Ticket) -> Option<&otaq_core::domain::ByLead> {
    ticket
        .flags
        .by_lead
        .as_ref()
        .filter(|_| ticket.epic.is_some() && is_never_started(ticket))
}

/// "Updated by lead": the lead edited or linked this never-started child since the user last looked.
fn updated_by_lead(ticket: &Ticket) -> Option<&otaq_core::domain::ByLead> {
    by_lead(ticket).filter(|b| b.updated)
}

/// "new · added by Wishlist lead": the lead created this never-started child.
fn added_by_lead(ticket: &Ticket) -> Option<&otaq_core::domain::ByLead> {
    by_lead(ticket).filter(|b| b.added)
}

/// "by Wishlist lead · description, now waits for SHOP-40.4": the fields the lead set and the parents it linked
/// that are still parents.
pub fn updated_by_lead_text(ticket: &Ticket, by: &otaq_core::domain::ByLead) -> String {
    let mut parts = by.fields.clone();
    let waits: Vec<String> = by
        .waits_for
        .iter()
        .filter(|k| ticket.depends_on.contains(k))
        .map(ToString::to_string)
        .collect();
    if !waits.is_empty() {
        parts.push(format!("now waits for {}", waits.join(", ")));
    }
    match parts.is_empty() {
        true => format!("by {}", by.lead),
        false => format!("by {} \u{b7} {}", by.lead, parts.join(", ")),
    }
}

/// The lead's one note shows as the second stop's hover once the child stopped again (LEAD-07, *03 → 25*).
fn second_stop(ticket: &Ticket) -> Option<&otaq_core::domain::LeadNoteSent> {
    let at = ticket.interrupted_from.unwrap_or(ticket.status);
    ticket.flags.lead_note_sent.as_ref().filter(|_| {
        matches!(
            at,
            Status::CheckFailed | Status::DeliveryStopped | Status::Idle
        )
    })
}

/// The hover of "Lead's note sent once": the lead, when it was sent, the note and the lead's view.
pub fn lead_note_hover(note: &otaq_core::domain::LeadNoteSent) -> Hover {
    use otaq_core::domain::StuckStop;
    let when = match (note.stop, note.after_rounds) {
        (StuckStop::ValidationFailed, Some(n)) => format!("SENT AFTER ROUND {n}"),
        (StuckStop::KeepsConflicting, _) => "SENT AFTER IT KEPT CONFLICTING".to_owned(),
        (StuckStop::KeepsFailingMergeCheck, _) => {
            "SENT AFTER ITS MERGE CHECK KEPT FAILING".to_owned()
        }
        (StuckStop::Idle, _) => "SENT WHEN IT STOPPED".to_owned(),
        (StuckStop::ValidationFailed, None) => "SENT AFTER VALIDATION FAILED".to_owned(),
    };
    let mut sections = vec![HoverSection {
        label: Some(format!("LEAD'S NOTE \u{b7} {when}")),
        text: format!("\u{201c}{}\u{201d}", note.text.trim()),
        muted: false,
    }];
    if let Some(view) = note.view.as_ref().filter(|v| !v.trim().is_empty()) {
        sections.push(HoverSection {
            label: Some("LEAD'S VIEW".to_owned()),
            text: view.trim().to_owned(),
            muted: true,
        });
    }
    Hover {
        icon: IconName::Bot,
        tone: Tone::Epic,
        title: note.lead.clone(),
        sections,
    }
}

/// The hover of "1 unread note from Wishlist lead": the latest note, its time and when the agent reads it.
pub fn unread_note_hover(notes: &otaq_core::domain::UnreadNotes) -> Hover {
    Hover {
        icon: IconName::Mail,
        tone: Tone::Epic,
        title: format!(
            "{} \u{b7} {} \u{b7} unread",
            notes.from,
            crate::view_models::clock::local_hhmm(notes.latest_at)
        ),
        sections: vec![
            HoverSection {
                label: None,
                text: format!("\u{201c}{}\u{201d}", notes.latest_text.trim()),
                muted: false,
            },
            HoverSection {
                label: None,
                text: unread_note_hint(),
                muted: true,
            },
        ],
    }
}

/// "same file as SHOP-40.4", "same file as SHOP-40.2 +1".
pub fn same_file_text(same: &[otaq_core::domain::SameFile]) -> Option<String> {
    let first = same.first()?;
    Some(match same.len() {
        1 => format!("same file as {}", first.other),
        n => format!("same file as {} +{}", first.other, n - 1),
    })
}

/// The hover of "same file as …": the files, per other child when there are several.
pub fn same_file_hover(same: &[otaq_core::domain::SameFile]) -> Option<Hover> {
    let title = same_file_text(same)?;
    let mut sections: Vec<HoverSection> = same
        .iter()
        .map(|s| HoverSection {
            label: (same.len() > 1).then(|| s.other.to_string()),
            text: s.files.join("\n"),
            muted: false,
        })
        .collect();
    sections.push(HoverSection {
        label: None,
        text: SAME_FILE_HINT.to_owned(),
        muted: true,
    });
    Some(Hover {
        icon: IconName::Files,
        tone: Tone::Muted,
        title,
        sections,
    })
}

/// Move to next stage came with a note (d0052).
fn has_note(flags: &otaq_core::domain::Flags) -> bool {
    flags
        .submitted_by_you
        .as_ref()
        .is_some_and(|note| !note.trim().is_empty())
}

/// The review entry is the user's Move to next stage.
fn submitted_entry(flags: &otaq_core::domain::Flags) -> bool {
    flags
        .review_entry
        .is_some_and(|e| e.entry == otaq_core::domain::ReviewEntry::SubmittedByUser)
}

/// "Lead's note sent · validation round 0 of 3: “…”" on a working card the lead's note sent back to work; "Lead's
/// note sent: “…”" after a conflict or an Idle stop (LEAD-07).
pub fn lead_note_callout(ticket: &Ticket) -> Option<String> {
    use otaq_core::domain::StuckStop;
    let note = ticket.flags.lead_note_sent.as_ref()?;
    if !matches!(ticket.status, Status::Working | Status::WorkingInBackground) {
        return None;
    }
    let round = ticket.flags.validation_round;
    let of = round.map(|r| r.of).or(note.after_rounds);
    Some(match (note.stop, of) {
        (StuckStop::ValidationFailed, Some(of)) => format!(
            "Lead's note sent \u{b7} validation round {} of {of}: {}",
            round.map_or(0, |r| r.round),
            quoted(&note.text)
        ),
        _ => format!("Lead's note sent: {}", quoted(&note.text)),
    })
}

/// "Sent back · 3 comments · round 2"; with decisions Change… sent back, "Sent back · 1 decision changed · round 2"
/// (comments named too when there are any; DEC-06); with neither, "Sent back · round 2" (the Send back dialog's own
/// comment is the hand-over, #20).
fn sent_back_text(sent: otaq_core::domain::flags::SentBack) -> String {
    let plural = |n: u32| if n == 1 { "" } else { "s" };
    let mut parts = Vec::new();
    if sent.comments > 0 {
        parts.push(format!(
            "{} comment{}",
            sent.comments,
            plural(sent.comments)
        ));
    }
    if sent.decisions > 0 {
        parts.push(format!(
            "{} decision{} changed",
            sent.decisions,
            plural(sent.decisions)
        ));
    }
    parts.push(format!("round {}", sent.round));
    format!("Sent back \u{b7} {}", parts.join(" \u{b7} "))
}

/// "adds migrations next to SHOP-21 (db/migrations/**)", "… next to SHOP-21 +1 (…)" (M19-e): another Review ticket
/// adds files under one of the project's schema paths too.
pub fn schema_overlap_text(overlap: &[otaq_core::domain::SchemaOverlap]) -> Option<String> {
    let first = overlap.first()?;
    let more = match overlap.len() {
        1 => String::new(),
        n => format!(" +{}", n - 1),
    };
    Some(format!(
        "adds migrations next to {}{more} ({})",
        first.other, first.path
    ))
}

/// The hover of "adds migrations next to …": per other ticket and schema path, the files it adds.
pub fn schema_overlap_hover(overlap: &[otaq_core::domain::SchemaOverlap]) -> Option<Hover> {
    let title = schema_overlap_text(overlap)?;
    let mut sections: Vec<HoverSection> = overlap
        .iter()
        .map(|o| HoverSection {
            label: Some(format!("{} \u{b7} {}", o.other, o.path)),
            text: o.files.join("\n"),
            muted: false,
        })
        .collect();
    sections.push(HoverSection {
        label: None,
        text: SCHEMA_OVERLAP_HINT.to_owned(),
        muted: true,
    });
    Some(Hover {
        icon: IconName::Database,
        tone: Tone::Warning,
        title,
        sections,
    })
}

/// Under the schema-path warning's files: nothing was changed, the merge order is the user's call.
pub const SCHEMA_OVERLAP_HINT: &str = "Whichever merges second may need its migration renumbered or regenerated. No files were changed.";

/// "N decisions" (DEC-03, d0084): the open recorded decisions of a card in Ready for Review; nothing in other columns
/// or after approval.
pub fn decisions_label(ticket: &Ticket) -> Option<String> {
    let n = ticket.flags.open_decisions;
    (n > 0 && ticket.status == Status::WaitingForYou)
        .then(|| format!("{n} decision{}", if n == 1 { "" } else { "s" }))
}

/// MSG-06: "asked SHOP-40.1 · waiting", "asked SHOP-40.1 +1 · waiting".
pub fn asked_meta(flags: &otaq_core::domain::Flags) -> Option<String> {
    let to = flags.asked.as_ref()?;
    Some(match flags.asked_more {
        0 => format!("asked {to} \u{b7} waiting"),
        n => format!("asked {to} +{n} \u{b7} waiting"),
    })
}

/// "GitHub" / "Azure DevOps" for a ticket with a PR, from the PR's URL (no setting, REV-11).
pub fn pr_provider_label(ticket: &Ticket) -> Option<String> {
    ticket.flags.pr_number?;
    let url = ticket.work.as_ref()?.pr_url.as_deref()?;
    otaq_core::pr::provider::Provider::detect(url).map(|p| p.name().to_owned())
}

/// Parents: the first `shown` keys, then "+N" ("SHOP-17, SHOP-18 +1").
pub fn deps_label(keys: &[String], shown: usize) -> String {
    if keys.len() <= shown {
        keys.join(", ")
    } else {
        format!("{} +{}", keys[..shown].join(", "), keys.len() - shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::{self, NOW, ticket};
    use crate::view_models::tool_avatar::AvatarTool;
    use otaq_core::domain::ValidationRound;

    fn main_project() -> CardExtras {
        CardExtras {
            project_base: Some("main".into()),
            ..Default::default()
        }
    }

    fn card(t: &Ticket) -> CardViewModel {
        CardViewModel::from(t, &main_project(), NOW)
    }

    fn flag_of(vm: &CardViewModel) -> Option<&str> {
        vm.flag.as_ref().map(|f| f.label.as_str())
    }

    fn line(vm: &CardViewModel) -> Option<&str> {
        vm.situation.as_ref().map(|s| s.text.as_str())
    }

    /// What the footer says: tokens, the last signal (with `!` when amber) and the action's label.
    fn foot(vm: &CardViewModel) -> Option<String> {
        let f = vm.footer.as_ref()?;
        let mut parts = Vec::new();
        parts.extend(
            f.agent
                .as_ref()
                .map(|a| format!("{}{}", a.text, if a.picked { " (violet)" } else { "" })),
        );
        parts.extend(f.tokens.as_ref().map(|t| t.text.clone()));
        parts.extend(
            f.signal
                .as_ref()
                .map(|s| format!("{}{}", s.text, if s.stale { "!" } else { "" })),
        );
        parts.extend(f.action.as_ref().map(|a| format!("[{}]", a.label)));
        parts.extend(f.second.as_ref().map(|a| format!("[{}]", a.label)));
        Some(parts.join(" "))
    }

    fn variant(name: &str) -> (Ticket, CardViewModel) {
        let v = card_fixtures::variants()
            .into_iter()
            .chain(card_fixtures::lead_variants())
            .chain(card_fixtures::sleeve_variants())
            .chain(card_fixtures::agent_variants())
            .chain(card_fixtures::flow_variants())
            .find(|v| v.name == name)
            .unwrap_or_else(|| panic!("no fixture {name}"));
        let vm = CardViewModel::from(&v.ticket, &v.extras, NOW);
        (v.ticket, vm)
    }

    /// Everywhere a card can say something: the head flag, the hover's status and details, the situation line,
    /// the origin line, the footer.
    fn places(vm: &CardViewModel) -> String {
        let mut all = vec![vm.hover.status.clone()];
        all.extend(vm.flag.as_ref().map(|f| f.label.clone()));
        all.extend(line(vm).map(str::to_owned));
        all.extend(vm.origin.clone());
        all.extend(foot(vm));
        all.extend(vm.hover.details.clone());
        all.join(" | ")
    }

    /// The head flag, the situation line and the footer of every M21 variant of *Ticket Card v2 · all variants*
    /// (`SPy9r`), in the frame's order.
    #[test]
    fn every_m21_variant_of_the_frame() {
        type Row = (
            &'static str,
            Option<&'static str>,
            Option<&'static str>,
            Option<&'static str>,
        );
        let rows: &[Row] = &[
            ("Backlog · calm", None, Some("on release/2.3"), None),
            ("Ready to start (0066)", None, Some("Ready to start"), None),
            ("Created by a Project agent (0068)", None, None, None),
            (
                "Waits for merge (0049)",
                None,
                Some("waits for SHOP-18, SHOP-19 to merge"),
                None,
            ),
            (
                "Start failed (0018)",
                Some("Start failed"),
                Some("Setup command failed · agent never started"),
                Some("[Retry start]"),
            ),
            (
                "Waiting for a free slot (0016)",
                None,
                Some("waiting for a free slot"),
                None,
            ),
            ("Working", Some("Working"), None, Some("182k 40s")),
            (
                "Working · stale signal (0027)",
                Some("Working"),
                Some("agent last active 6 min ago"),
                Some("64k"),
            ),
            (
                "Working in background (0042)",
                Some("Working"),
                Some("background: test run, 1 helper agent · 6 min"),
                Some("182k"),
            ),
            (
                "Running (Run, 0035) · pinned (0047)",
                Some(":4020"),
                None,
                Some("182k 40s"),
            ),
            (
                "Tokens not reported",
                Some("Working"),
                None,
                Some("not reported 12s"),
            ),
            (
                "Needs you · permission (0042)",
                Some("Needs you"),
                Some("Allow rm -rf .next?"),
                Some("[Answer]"),
            ),
            (
                "Asks you (0037)",
                Some("Asks you"),
                Some("Should dark mode follow the system setting?"),
                Some("[Reply]"),
            ),
            (
                "Idle (0042)",
                Some("Idle"),
                Some("Ended its turn: “Should I also update the tests?”"),
                Some("[Reply]"),
            ),
            (
                "Interrupted (0018)",
                Some("Interrupted"),
                Some("Session lost after reboot"),
                Some("[Resume]"),
            ),
            (
                "Needs you · validator (0063)",
                Some("Validator asks"),
                Some("validator: Allow Bash(pnpm test)?"),
                Some("[Answer]"),
            ),
            (
                "Validation failed 3 times (0063)",
                Some("Failed ×3"),
                Some("Arrow keys do nothing in the list \u{b7} see comment"),
                Some("[Open]"),
            ),
            (
                "Validation ended without a result",
                Some("No result"),
                Some("Validation ended without a result"),
                Some("[Retry validation]"),
            ),
            (
                "Parent closed (0033)",
                Some("Parent closed"),
                Some(
                    "SHOP-18 was closed without merging. Can’t be approved until you reopen it or remove the \
                     dependency.",
                ),
                Some("[Open]"),
            ),
            (
                "Validating… (0063)",
                Some("Validating…"),
                Some("round 1 of 3 · 2 min"),
                Some("23k"),
            ),
            (
                "Validation failed · round 2 of 3",
                Some("Working"),
                Some("the list isn't paginated \u{b7} see comment"),
                Some("141k 20s"),
            ),
            (
                "Merge conflict · fixing (0064)",
                Some("Conflict"),
                Some("Merge conflict · fixing · lib/wishlist.ts"),
                Some("77k 10s"),
            ),
            (
                "Keeps conflicting (0064)",
                Some("Conflicts ×3"),
                // FLW-17 (`s03-46`).
                Some("Keeps conflicting with the epic’s branch"),
                Some("[Open]"),
            ),
            (
                "Merging · running the merge check (0120)",
                Some("Merging\u{2026}"),
                Some("Merging \u{b7} running the merge check"),
                Some("[Stop]"),
            ),
            (
                "Merge check failed · fixing (0120)",
                Some("Check failed"),
                Some("Merge check failed \u{b7} fixing"),
                Some("91k 10s"),
            ),
            (
                "Picked by the lead (0083)",
                None,
                Some("waits for SHOP-40.2 to merge"),
                None,
            ),
            (
                "Unread note · same file (0083 · T)",
                Some("Working"),
                Some("1 unread note from Wishlist lead · same file as SHOP-40.4"),
                Some("58k 12s"),
            ),
            (
                "Asks you · via Wishlist lead (0083)",
                Some("Asks you"),
                Some("via Wishlist lead: Show the empty state on mobile too?"),
                Some("[Reply]"),
            ),
            (
                "Validated · round 2",
                Some("Validated"),
                Some("round 2 · +412 −38 · 9 files"),
                Some("[Review]"),
            ),
            (
                "Validation skipped by you",
                Some("Skipped"),
                Some("+412 −38 · 9 files"),
                Some("[Review]"),
            ),
            (
                "Failed 3 times · moved by you",
                Some("Did not pass"),
                Some("failed 3 times · moved by you"),
                Some("[Review]"),
            ),
            (
                "Approved · PR open (0017)",
                Some("PR #481 open"),
                Some("{pr line}"),
                Some("[Send back with PR comments] [Mark merged]"),
            ),
            ("Merged", None, None, None),
            (
                "Merged automatically (0064)",
                None,
                Some("Merged automatically · Wishlist"),
                None,
            ),
            (
                "Closed · not merged (0033)",
                Some("Closed"),
                Some("closed without merging"),
                Some("[Reopen]"),
            ),
            (
                "Only one tool connected (0086)",
                Some("Working"),
                None,
                Some("182k 40s"),
            ),
            (
                "Needs you + running + 2 dependencies",
                Some("Needs you"),
                Some("Allow rm -rf .next? · also running :4022"),
                Some("[Answer]"),
            ),
        ];
        let fixtures = card_fixtures::variants();
        for (name, flag, situation, footer) in rows {
            let (_, vm) = variant(name);
            assert_eq!(flag_of(&vm), *flag, "{name}: flag");
            // The name is spelled out only in `otaq_core::naming` (d0028).
            let situation = situation.map(|s| {
                s.replace("{epic branch}", &otaq_core::naming::branch("SHOP-40"))
                    .replace("{pr line}", &pr_not_watched())
            });
            assert_eq!(line(&vm), situation.as_deref(), "{name}: situation");
            assert_eq!(foot(&vm).as_deref(), *footer, "{name}: footer");
            assert_eq!(vm.avatar, None, "{name}: one tool, no avatar");
        }
        // Every plain fixture is in the table, in the frame's order; the rest are *Interaction states*.
        let plain: Vec<&str> = fixtures
            .iter()
            .filter(|v| v.look == card_fixtures::Look::Plain)
            .map(|v| v.name)
            .collect();
        let named: Vec<&str> = rows.iter().map(|r| r.0).collect();
        assert_eq!(plain, named);
        assert_eq!(fixtures.len(), rows.len() + 5);
    }

    /// The cards of checks and deliveries in any flow (FLW-13…FLW-17), as `s03-44`, `s03-46`, `s03-48` and `s03-54`
    /// word them: the head flag, the situation line and the footer.
    #[test]
    fn every_m33_card_of_the_frames() {
        type Row = (
            &'static str,
            Option<&'static str>,
            Option<&'static str>,
            Option<&'static str>,
        );
        let rows: &[Row] = &[
            (
                "Checking \u{b7} Security",
                Some("Checking"),
                Some("round 1 of 3"),
                Some("Security reviewer \u{b7} opus 18k"),
            ),
            (
                "Security failed \u{b7} back in Dev",
                Some("Working"),
                Some("Security failed \u{b7} round 2 of 3"),
                Some("Rust developer \u{b7} sonnet 141k 20s"),
            ),
            (
                "Security failed \u{d7}3",
                Some("Failed \u{d7}3"),
                Some("Security failed 3 times \u{b7} unescaped SQL in search \u{b7} see comment"),
                Some("[Move on anyway]"),
            ),
            (
                "Your check \u{b7} passed Security",
                Some("Your check"),
                Some("Passed Security \u{b7} round 2"),
                Some("[Pass] [Fail]"),
            ),
            (
                "Your check \u{b7} skipped Security",
                Some("Your check"),
                Some("Skipped Security \u{b7} by you"),
                Some("[Pass] [Fail]"),
            ),
            (
                "Your check \u{b7} skipped by the Project agent",
                Some("Your check"),
                Some("Skipped Grill \u{b7} by Planner"),
                Some("[Pass] [Fail]"),
            ),
            (
                "Your check \u{b7} moved back",
                Some("Your check"),
                Some("Moved back by you"),
                Some("[Pass] [Fail]"),
            ),
            (
                "Ready to merge \u{b7} passed Review",
                Some("Ready to merge"),
                Some("Passed Review"),
                Some("[Local merge]"),
            ),
            (
                "Ready to merge \u{b7} moved past Validate",
                Some("Ready to merge"),
                Some("Failed Validate 3 times \u{b7} moved by you \u{b7} merge when ready"),
                Some("[Local merge]"),
            ),
            (
                "Ready for PR \u{b7} moved past Security",
                Some("Ready for PR"),
                Some("Failed Security 3 times \u{b7} moved by you \u{b7} open the PR when ready"),
                Some("[Open PR]"),
            ),
            (
                "Ready for PR \u{b7} skipped Security",
                Some("Ready for PR"),
                Some("Skipped Security \u{b7} by you \u{b7} open the PR when ready"),
                Some("[Open PR]"),
            ),
            (
                "Ready for PR \u{b7} delivering automatically",
                Some("Ready for PR"),
                Some("Shipper is opening the PR"),
                None,
            ),
            (
                "PR #481 open \u{b7} opened by Shipper",
                Some("PR #481 open"),
                Some("Opened by Shipper \u{b7} 2 h ago"),
                Some("[Send back with PR comments] [Mark merged]"),
            ),
            (
                "Merge conflict \u{b7} fixing in Dev",
                Some("Conflict"),
                Some("Merge conflict \u{b7} fixing"),
                Some("Rust developer \u{b7} sonnet 77k 10s"),
            ),
            (
                "Conflicts \u{d7}3 \u{b7} the epic\u{2019}s branch",
                Some("Conflicts \u{d7}3"),
                Some("Keeps conflicting with the epic\u{2019}s branch"),
                Some("[Open]"),
            ),
            (
                "Conflicts \u{d7}3 \u{b7} main",
                Some("Conflicts \u{d7}3"),
                Some("Keeps conflicting with main"),
                Some("[Open]"),
            ),
            (
                "Delivered without checks",
                None,
                Some("Delivered without checks"),
                None,
            ),
            (
                "Plan limit \u{b7} a fallback is set",
                Some("At limit"),
                Some("Claude Code limit"),
                Some("Security reviewer [Use fallback] [Open agent\u{2026}]"),
            ),
            (
                "Plan limit \u{b7} no fallback can run",
                Some("At limit"),
                Some("Claude Code limit \u{b7} Claude Code has no fallback"),
                Some("Security reviewer [Skip this check] [Open agent\u{2026}]"),
            ),
        ];
        let names: Vec<&str> = card_fixtures::flow_variants()
            .iter()
            .map(|v| v.name)
            .collect();
        assert_eq!(names, rows.iter().map(|r| r.0).collect::<Vec<_>>());
        for (name, flag, situation, footer) in rows {
            let (_, vm) = variant(name);
            assert_eq!(flag_of(&vm), *flag, "{name}: flag");
            assert_eq!(line(&vm), *situation, "{name}: situation");
            assert_eq!(foot(&vm).as_deref(), *footer, "{name}: footer");
        }
    }

    /// The frames' colours for the M33 cards (`s03-44`, `s03-46`, `s03-48`): a green edge for a pass, amber for a skip,
    /// a move by you, a conflict being fixed and a delivery without checks; turquoise for the stops that need you.
    #[test]
    fn the_m33_cards_colours() {
        let look = |name| {
            let (_, vm) = variant(name);
            let s = vm.situation.expect("a line");
            (
                vm.flag.map(|f| f.palette),
                s.edge,
                s.text_tone,
                vm.outline,
                vm.faded,
            )
        };
        use Palette as P;
        assert_eq!(
            look("Checking \u{b7} Security"),
            (
                Some(P::Working),
                Tone::Muted,
                Tone::Secondary,
                Outline::Normal,
                None
            )
        );
        assert_eq!(
            look("Security failed \u{d7}3"),
            (
                Some(P::Attn),
                Tone::Attention,
                Tone::Secondary,
                Outline::Attention,
                None
            )
        );
        assert_eq!(
            look("Your check \u{b7} passed Security"),
            (
                Some(P::Review),
                Tone::Passed,
                Tone::Secondary,
                Outline::Normal,
                None
            )
        );
        for name in [
            "Your check \u{b7} skipped Security",
            "Your check \u{b7} moved back",
        ] {
            assert_eq!(
                look(name),
                (
                    Some(P::Review),
                    Tone::Paused,
                    Tone::Secondary,
                    Outline::Normal,
                    None
                ),
                "{name}"
            );
        }
        assert_eq!(
            look("Ready for PR \u{b7} moved past Security"),
            (
                Some(P::Review),
                Tone::Paused,
                Tone::Secondary,
                Outline::Normal,
                None
            )
        );
        assert_eq!(
            look("PR #481 open \u{b7} opened by Shipper").0,
            Some(P::Review)
        );
        assert_eq!(
            look("Merge conflict \u{b7} fixing in Dev"),
            (
                Some(P::Paused),
                Tone::Paused,
                Tone::Secondary,
                Outline::Normal,
                None
            )
        );
        assert_eq!(
            look("Conflicts \u{d7}3 \u{b7} main"),
            (
                Some(P::Attn),
                Tone::Attention,
                Tone::Secondary,
                Outline::Attention,
                None
            )
        );
        assert_eq!(
            look("Delivered without checks"),
            (None, Tone::Paused, Tone::Secondary, Outline::Normal, None),
            "amber, not faded"
        );
        // Send back with PR comments over Mark merged, Skip this check over Open agent…; Pass · Fail side by side.
        let stacked = |name| variant(name).1.footer.expect("a footer").stacked;
        assert!(stacked("PR #481 open \u{b7} opened by Shipper"));
        assert!(stacked("Plan limit \u{b7} no fallback can run"));
        assert!(!stacked("Your check \u{b7} passed Security"));
        assert!(!stacked("Plan limit \u{b7} a fallback is set"));
    }

    /// The colours and the mono line of the frame: the edge is the winning state's, the text coloured only for red,
    /// amber and violet lines; mono only for a lone command.
    #[test]
    fn the_situation_lines_edge_and_mono() {
        let look = |name| {
            let (_, vm) = variant(name);
            let s = vm.situation.unwrap();
            (s.edge, s.text_tone, s.mono)
        };
        assert_eq!(
            look("Needs you · permission (0042)"),
            (Tone::Attention, Tone::Secondary, true)
        );
        assert_eq!(
            look("Needs you · validator (0063)"),
            (Tone::Attention, Tone::Secondary, true)
        );
        assert_eq!(
            look("Needs you + running + 2 dependencies"),
            (Tone::Attention, Tone::Secondary, false),
            "a joined line is never mono"
        );
        assert_eq!(
            look("Start failed (0018)"),
            (Tone::Error, Tone::Error, false)
        );
        assert_eq!(
            look("Failed 3 times · moved by you"),
            (Tone::Error, Tone::Error, false)
        );
        assert_eq!(
            look("Working · stale signal (0027)"),
            (Tone::Warning, Tone::Warning, false),
            "amber: not you, but soon"
        );
        assert_eq!(
            look("Waits for merge (0049)"),
            (Tone::Muted, Tone::Secondary, false)
        );
        assert_eq!(
            look("Asks you (0037)"),
            (Tone::Attention, Tone::Secondary, false)
        );
    }

    /// The head flag's colours: turquoise only for the needs-you family (d0077), Limit neutral, the frame's
    /// colours for the rest.
    #[test]
    fn turquoise_stays_the_needs_you_familys() {
        for v in card_fixtures::variants()
            .into_iter()
            .chain(card_fixtures::lead_variants())
        {
            let vm = CardViewModel::from(&v.ticket, &v.extras, NOW);
            if let Some(f) = &vm.flag {
                assert_eq!(
                    f.palette == Palette::Attn,
                    f.rank == Rank::NeedsYou || v.ticket.status.is_needs_you_family(),
                    "{}",
                    v.name
                );
            }
            if vm
                .situation
                .as_ref()
                .is_some_and(|s| s.edge == Tone::Attention)
            {
                assert!(v.ticket.status.is_needs_you_family(), "{}", v.name);
            }
        }
        let limit = card(&ticket(
            "SHOP-19",
            "Price table",
            Status::WaitingForLimitReset,
        ));
        let f = limit.flag.unwrap();
        assert_eq!((f.label.as_str(), f.palette), ("At limit", Palette::Queued));
        assert_eq!(f.rank, Rank::Limit);
        let palette = |name| variant(name).1.flag.unwrap().palette;
        assert_eq!(palette("Validating… (0063)"), Palette::Queued);
        assert_eq!(palette("Merge conflict · fixing (0064)"), Palette::Working);
        assert_eq!(palette("Validated · round 2"), Palette::Approved);
        assert_eq!(palette("Failed 3 times · moved by you"), Palette::Error);
        assert_eq!(
            palette("Running (Run, 0035) · pinned (0047)"),
            Palette::Approved
        );
        assert_eq!(
            palette("Interrupted (0018)"),
            Palette::Error,
            "red, not turquoise"
        );
    }

    /// Every short flag in its place in the flag order (*Flag order* `Uuk99`), and ≤ 12 characters.
    #[test]
    fn every_short_flag_in_the_flag_order() {
        use otaq_core::domain::AutoMergeStop;
        let of = |t: Ticket, x: CardExtras| {
            let f = CardViewModel::from(&t, &x, NOW).flag.expect("a flag");
            (f.label, f.rank)
        };
        let st = |s| of(ticket("SHOP-1", "x", s), main_project());
        let stop = |stop| {
            let mut t = ticket("SHOP-1", "x", Status::DeliveryStopped);
            t.auto_merge_stop = Some(stop);
            t.auto_merge_conflicts = 3;
            of(t, main_project())
        };
        let mut failed = ticket("SHOP-1", "x", Status::CheckFailed);
        failed.validation_round = 3;
        let mut conflicts = ticket("SHOP-1", "x", Status::WaitingForYou);
        conflicts.flags.conflicts_with = Some("main".into());
        let mut port = ticket("SHOP-1", "x", Status::Working);
        port.flags.run_port = Some(4020);
        let mut pr = ticket("SHOP-1", "x", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        let review = |entry| {
            let mut t = ticket("SHOP-1", "x", Status::WaitingForYou);
            t.flags.review_entry = Some(otaq_core::domain::ReviewEntryFlag { entry, round: 3 });
            of(t, main_project())
        };
        use otaq_core::domain::ReviewEntry as E;
        let flags = [
            st(Status::CheckerNeedsYou),
            st(Status::NeedsYou),
            st(Status::AsksYou),
            st(Status::Idle),
            st(Status::Interrupted),
            st(Status::ResumeFailed),
            st(Status::StartInterrupted),
            st(Status::ParentClosed),
            stop(AutoMergeStop::KeepsConflicting),
            stop(AutoMergeStop::BaseDiverged),
            stop(AutoMergeStop::CheckoutDirty),
            of(failed, main_project()),
            st(Status::CheckNoResult),
            st(Status::WaitingForLimitReset),
            of(
                ticket("SHOP-1", "x", Status::Held),
                CardExtras {
                    held_by_limit: true,
                    ..main_project()
                },
            ),
            st(Status::Checking),
            st(Status::MergeConflictFixing),
            of(conflicts, main_project()),
            review(E::Validated),
            review(E::Skipped),
            review(E::FailedMoved),
            of(pr, main_project()),
            st(Status::ClosedNotMerged),
            of(port, main_project()),
            st(Status::Working),
            st(Status::WorkingInBackground),
        ];
        let labels: Vec<&str> = flags.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Validator asks",
                "Needs you",
                "Asks you",
                "Idle",
                "Interrupted",
                "Resume fail",
                "Start failed",
                "Parent closed",
                "Conflicts ×3",
                "Diverged",
                "Dirty repo",
                "Failed ×3",
                "No result",
                "At limit",
                "At limit",
                "Validating…",
                "Conflict",
                "Conflict",
                "Validated",
                "Skipped",
                "Did not pass",
                "PR #481 open",
                "Closed",
                ":4020",
                "Working",
                "Working",
            ]
        );
        let ranks: Vec<Rank> = flags.iter().map(|(_, r)| *r).collect();
        assert!(ranks.is_sorted(), "{ranks:?}");
        for (label, _) in &flags {
            let n = label.chars().count();
            assert!(
                n <= FLAG_MAX_CHARS || label == PARENT_CLOSED || label == VALIDATOR_ASKS,
                "{label}: {n} characters"
            );
        }
    }

    /// Each pair of neighbours in the flag order: on one card, the higher takes the head and the lower joins the
    /// situation line.
    #[test]
    fn the_flag_order_neighbour_by_neighbour() {
        // Needs you over a stop, through the head's own rule (two statuses never meet on one card).
        let part = |rank, label: &str, lost: &str| {
            Part::flag(
                HeadFlag::new(rank, label, label, Legend::NeedsYou),
                None,
                Tone::Muted,
            )
            .lost(lost)
        };
        let order = [
            Rank::NeedsYou,
            Rank::Stopped,
            Rank::Limit,
            Rank::Busy,
            Rank::Outcome,
            Rank::Running,
            Rank::Working,
        ];
        for pair in order.windows(2) {
            let (high, low) = (pair[0], pair[1]);
            // The lower one comes first on the card: the order decides, not the insertion.
            let (flag, situation) =
                arrange(vec![part(low, "low", "lost low"), part(high, "high", "")]);
            assert_eq!(flag.unwrap().rank, high, "{high:?} over {low:?}");
            assert_eq!(situation.unwrap().text, "lost low", "{high:?} over {low:?}");
        }

        // On real cards: a stop over Conflicts with main (Busy).
        let mut t = ticket("SHOP-25", "Upgrade", Status::Interrupted);
        t.flags.conflicts_with = Some("main".into());
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("Interrupted"));
        assert_eq!(
            line(&vm),
            Some("Session lost after reboot · conflicts with main")
        );
        // Limit over Conflict.
        let mut t = ticket("SHOP-19", "Price table", Status::WaitingForLimitReset);
        t.flags.conflicts_with = Some("main".into());
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("At limit"));
        assert_eq!(line(&vm), Some("conflicts with main"));
        // Conflict over the review outcome.
        let (mut t, _) = variant("Validated · round 2");
        t.flags.conflicts_with = Some("main".into());
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("Conflict"));
        assert_eq!(
            line(&vm),
            Some("Conflicts with main · found after the last merge · Validated · round 2")
        );
        // The outcome over the running port.
        let (mut t, _) = variant("Approved · PR open (0017)");
        t.flags.run_port = Some(4020);
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("PR #481 open"));
        assert_eq!(
            line(&vm),
            Some(format!("{} · also running :4020", pr_not_watched()).as_str())
        );
        // The port over Working.
        let (_, vm) = variant("Running (Run, 0035) · pinned (0047)");
        assert_eq!(flag_of(&vm), Some(":4020"));
        assert_eq!(vm.hover.status, "Serving :4020");
        // Needs you over the port (the frame's combined card).
        let (_, vm) = variant("Needs you + running + 2 dependencies");
        assert_eq!(flag_of(&vm), Some("Needs you"));
    }

    /// Flags that lose the head join the situation line in the flag order, after the winner's words and before
    /// the quiet facts.
    #[test]
    fn losers_join_the_line_in_order() {
        let mut t = ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou);
        t.depends_on = vec!["SHOP-17".parse().unwrap()];
        t.flags.run_port = Some(4024);
        t.flags.conflicts_with = Some("main".into());
        t.flags.messages_waiting = 2;
        let x = CardExtras {
            question: Some("Allow rm -rf .next?".into()),
            ..main_project()
        };
        let vm = CardViewModel::from(&t, &x, NOW);
        assert_eq!(flag_of(&vm), Some("Needs you"));
        assert_eq!(
            line(&vm),
            Some(
                "Allow rm -rf .next? · conflicts with main · also running :4024 · 2 messages waiting"
            )
        );
        // Synthetic: whatever order the card collects them in.
        let part = |rank, lost: &str| {
            Part::flag(
                HeadFlag::new(rank, "f", "f", Legend::AgentWorking),
                None,
                Tone::Muted,
            )
            .lost(lost)
        };
        let (_, s) = arrange(vec![
            Part::quiet("quiet", Tone::Muted),
            part(Rank::Working, "working"),
            part(Rank::Running, "running"),
            part(Rank::Busy, "busy"),
            part(Rank::NeedsYou, "winner"),
        ]);
        assert_eq!(s.unwrap().text, "busy · running · working · quiet");
    }

    /// The avatar follows whoever has the turn: the validator while it validates or asks, else the developer.
    #[test]
    fn the_avatar_follows_the_turn() {
        let x = CardExtras {
            developer: Some(ToolAvatarView::of(AvatarTool::ClaudeCode, None)),
            validator: Some(ToolAvatarView::of(AvatarTool::Codex, Some("gpt-5.4"))),
            ..main_project()
        };
        let shown = |status| {
            CardViewModel::build(&ticket("SHOP-29", "Coupon field", status), &x, NOW, true)
                .avatar
                .map(|a| a.letters)
        };
        assert_eq!(shown(Status::Checking).as_deref(), Some("CX"));
        assert_eq!(shown(Status::CheckerNeedsYou).as_deref(), Some("CX"));
        for status in [
            Status::Working,
            Status::NeedsYou,
            Status::CheckFailed,
            Status::CheckNoResult,
            Status::WaitingForYou,
        ] {
            assert_eq!(shown(status).as_deref(), Some("CC"), "{status}");
        }
        // Hidden while one tool is connected.
        let hidden = CardViewModel::from(
            &ticket("SHOP-29", "Coupon field", Status::Checking),
            &x,
            NOW,
        );
        assert_eq!(hidden.avatar, None);
        // Nobody has a session yet: no avatar.
        let none = CardViewModel::build(
            &ticket("SHOP-29", "Coupon field", Status::Backlog),
            &main_project(),
            NOW,
            true,
        );
        assert_eq!(none.avatar, None);
    }

    /// The footer only when it has something to say: an agent runs, or there is one action.
    #[test]
    fn the_footer_only_when_it_has_something_to_say() {
        for status in [
            Status::Backlog,
            Status::Queued,
            Status::WaitingForSlot,
            Status::Parked,
            Status::Merged,
            Status::WaitingForLimitReset,
        ] {
            let mut t = ticket("SHOP-19", "Price table", status);
            t.flags.tokens = Some(10_000);
            assert_eq!(card(&t).footer, None, "{status}");
            assert!(
                card(&t).hover.details.contains(&"Tokens: 10k".to_owned()),
                "{status}"
            );
        }
        // A waiting agent: its action, no tokens.
        let (_, vm) = variant("Needs you · permission (0042)");
        let footer = vm.footer.unwrap();
        assert_eq!((footer.tokens, footer.signal), (None, None));
        let answer = footer.action.unwrap();
        assert_eq!(answer.kind, CardActionKind::Open);
        // Answer, Reply and Open are the frame's filled button; Mark merged is the outlined one (T-0323).
        assert!(answer.primary);
        let (_, pr) = variant("Approved · PR open (0017)");
        assert!(!pr.footer.unwrap().action.unwrap().primary);
        // A running agent: tokens and the last signal, amber after 3 min, gone to the line once quiet.
        let mut t = ticket("BILL-4", "Invoice PDF export", Status::Working);
        t.flags.tokens = Some(205_000);
        t.flags.last_signal_at = Some(NOW);
        let at = |ms| foot(&CardViewModel::from(&t, &main_project(), NOW + ms));
        assert_eq!(at(40_000).as_deref(), Some("205k 40s"));
        assert_eq!(at(4 * 60_000).as_deref(), Some("205k 4m!"));
        assert_eq!(
            at(5 * 60_000).as_deref(),
            Some("205k 5m!"),
            "exactly 5 min isn't stale yet"
        );
        assert_eq!(at(6 * 60_000).as_deref(), Some("205k"));
        assert_eq!(
            line(&CardViewModel::from(&t, &main_project(), NOW + 6 * 60_000)),
            Some("agent last active 6 min ago")
        );
        // Tokens Otaq can't read: "not reported", with why on hover; never a made-up number (d0086).
        let (_, vm) = variant("Tokens not reported");
        let tokens = vm.footer.unwrap().tokens.unwrap();
        assert_eq!(tokens.text, TOKENS_NOT_REPORTED);
        assert_eq!(tokens.tooltip.as_deref(), Some(TOKENS_UNKNOWN));
        // A message waiting on a working card: Deliver now next to the tokens.
        let mut t = ticket("SHOP-19", "Price table", Status::Working);
        t.flags.messages_waiting = 1;
        t.flags.tokens = Some(1_000);
        let vm = card(&t);
        assert_eq!(foot(&vm).as_deref(), Some("1k [Deliver now]"));
        assert_eq!(line(&vm), Some("1 message waiting"));
    }

    /// The origin line of an agent-made card (d0068), in Backlog only, as before.
    #[test]
    fn the_origin_line() {
        let (mut t, vm) = variant("Created by a Project agent (0068)");
        assert_eq!(vm.origin.as_deref(), Some("Created by Checkout slowness"));
        assert_eq!(
            card(&t).origin.as_deref(),
            Some("Created by a Project agent"),
            "an agent that isn't listed any more"
        );
        t.created_by = Actor::Agent("SHOP-40.2".parse().unwrap());
        assert_eq!(card(&t).origin.as_deref(), Some("Created by SHOP-40.2"));
        t.status = Status::Working;
        t.place = otaq_core::domain::Place::stage_of("dev");
        assert_eq!(card(&t).origin, None);
        t.created_by = Actor::User("me".into());
        t.status = Status::Backlog;
        t.place = otaq_core::domain::Place::Backlog;
        assert_eq!(card(&t).origin, None);
    }

    /// SDD §7.2 is unchanged: every state and every flag keeps a place on the card (head, situation line, footer or
    /// hover), so no state silently loses its signal.
    #[test]
    fn no_state_of_7_2_without_a_place() {
        for status in Status::ALL {
            let vm = card(&ticket("SHOP-1", "x", *status));
            let want = match &vm.flag {
                Some(f) => f.full.clone(),
                None => status::pill(*status).1.to_owned(),
            };
            assert_eq!(vm.hover.status, want, "{status}");
            assert!(!want.is_empty(), "{status}");
        }
        use otaq_core::domain::{ReviewEntry as E, ReviewEntryFlag};
        type Setup = fn(&mut Ticket, &mut CardExtras);
        let cases: &[(&str, Status, Setup, &str)] = &[
            ("parked", Status::Parked, |_, _| {}, "Parked · has work"),
            (
                "ready to start",
                Status::Backlog,
                |t, _| t.flags.ready_to_start = true,
                "Ready to start",
            ),
            (
                "waits",
                Status::Queued,
                |_, x| x.waits_for = Some("waits for SHOP-18 to merge".into()),
                "waits for SHOP-18",
            ),
            (
                "slot",
                Status::WaitingForSlot,
                |_, _| {},
                "waiting for a free slot",
            ),
            (
                "held",
                Status::Held,
                |_, x| x.held_reason = Some("diverged".into()),
                "diverged",
            ),
            (
                "held by limit",
                Status::Held,
                |_, x| x.held_by_limit = true,
                "At limit",
            ),
            (
                "parent closed",
                Status::ParentClosed,
                |_, _| {},
                "Parent closed",
            ),
            (
                "start interrupted",
                Status::StartInterrupted,
                |_, _| {},
                "Start failed",
            ),
            ("working", Status::Working, |_, _| {}, "Working"),
            (
                "background",
                Status::WorkingInBackground,
                |_, _| {},
                "Working in background",
            ),
            ("asks you", Status::AsksYou, |_, _| {}, "Asks you"),
            ("needs you", Status::NeedsYou, |_, _| {}, "Needs you"),
            ("idle", Status::Idle, |_, _| {}, "Idle"),
            (
                "limit",
                Status::WaitingForLimitReset,
                |_, _| {},
                "Waiting for limit reset",
            ),
            ("interrupted", Status::Interrupted, |_, _| {}, "Interrupted"),
            (
                "resume failed",
                Status::ResumeFailed,
                |_, _| {},
                "Resume failed",
            ),
            ("validating", Status::Checking, |_, _| {}, "Validating…"),
            (
                "round",
                Status::Working,
                |t, _| t.flags.validation_round = Some(ValidationRound { round: 1, of: 3 }),
                "round 1 of 3",
            ),
            (
                "validator",
                Status::CheckerNeedsYou,
                |_, _| {},
                "Needs you · validator",
            ),
            (
                "failed",
                Status::CheckFailed,
                |t, _| t.validation_round = 3,
                "Validation failed 3 times",
            ),
            (
                "no result",
                Status::CheckNoResult,
                |_, _| {},
                "without a result",
            ),
            (
                "fixing",
                Status::MergeConflictFixing,
                |_, _| {},
                "Merge conflict · fixing",
            ),
            (
                "keeps",
                Status::DeliveryStopped,
                |t, _| t.auto_merge_stop = Some(otaq_core::domain::AutoMergeStop::KeepsConflicting),
                "Keeps conflicting",
            ),
            (
                "diverged",
                Status::DeliveryStopped,
                |t, _| t.auto_merge_stop = Some(otaq_core::domain::AutoMergeStop::BaseDiverged),
                "Base diverged",
            ),
            (
                "dirty",
                Status::DeliveryStopped,
                |t, _| t.auto_merge_stop = Some(otaq_core::domain::AutoMergeStop::CheckoutDirty),
                "Checkout dirty",
            ),
            (
                "review",
                Status::WaitingForYou,
                |_, _| {},
                "Ready for Review",
            ),
            (
                "pr open",
                Status::PrOpen,
                |t, _| t.flags.pr_number = Some(481),
                "PR #481",
            ),
            ("merged", Status::Merged, |_, _| {}, "Merged"),
            (
                "merged automatically",
                Status::Merged,
                |t, _| t.flags.merged_automatically = true,
                "Merged automatically",
            ),
            (
                "closed",
                Status::ClosedNotMerged,
                |_, _| {},
                "Closed · not merged",
            ),
            // The flags.
            (
                "priority",
                Status::Backlog,
                |t, _| t.priority = Priority::High,
                "",
            ),
            (
                "validated",
                Status::WaitingForYou,
                |t, _| {
                    t.flags.review_entry = Some(ReviewEntryFlag {
                        entry: E::Validated,
                        round: 2,
                    })
                },
                "Validated · round 2",
            ),
            (
                "skipped",
                Status::WaitingForYou,
                |t, _| {
                    t.flags.review_entry = Some(ReviewEntryFlag {
                        entry: E::Skipped,
                        round: 1,
                    })
                },
                "Validation skipped by you",
            ),
            (
                "submitted",
                Status::WaitingForYou,
                |t, _| {
                    t.flags.review_entry = Some(ReviewEntryFlag {
                        entry: E::SubmittedByUser,
                        round: 0,
                    })
                },
                "Submitted by you",
            ),
            (
                "failed moved",
                Status::WaitingForYou,
                |t, _| {
                    t.flags.review_entry = Some(ReviewEntryFlag {
                        entry: E::FailedMoved,
                        round: 3,
                    })
                },
                "Did not pass",
            ),
            (
                "entry after review",
                Status::Merged,
                |t, _| {
                    t.flags.review_entry = Some(ReviewEntryFlag {
                        entry: E::Validated,
                        round: 2,
                    })
                },
                "Validated · round 2",
            ),
            (
                "sent back",
                Status::Working,
                |t, _| {
                    t.flags.sent_back = Some(otaq_core::domain::SentBack {
                        comments: 3,
                        round: 2,
                        decisions: 0,
                    })
                },
                "Sent back · 3 comments",
            ),
            (
                "conflicts",
                Status::WaitingForYou,
                |t, _| t.flags.conflicts_with = Some("main".into()),
                "Conflicts with main",
            ),
            (
                "changed",
                Status::PrOpen,
                |t, _| t.flags.changed_since_review = true,
                "Changed since you reviewed it",
            ),
            (
                "messages",
                Status::Working,
                |t, _| t.flags.messages_waiting = 1,
                "1 message waiting",
            ),
            (
                "run",
                Status::WaitingForYou,
                |t, _| t.flags.run_port = Some(4020),
                "Serving :4020",
            ),
            (
                "answering",
                Status::Merged,
                |t, _| t.flags.answering = Some("SHOP-40.2".parse().unwrap()),
                "answering SHOP-40.2",
            ),
            (
                "asked",
                Status::Working,
                |t, _| t.flags.asked = Some("SHOP-40.1".parse().unwrap()),
                "asked SHOP-40.1 · waiting",
            ),
            (
                "last signal (running)",
                Status::Working,
                |t, _| t.flags.last_signal_at = Some(NOW - 40_000),
                "40s",
            ),
            (
                "last signal (waiting)",
                Status::AsksYou,
                |t, _| t.flags.last_signal_at = Some(NOW - 40_000),
                "Last signal: 40s ago",
            ),
            (
                "tokens (running)",
                Status::Working,
                |t, _| t.flags.tokens = Some(182_000),
                "182k",
            ),
            (
                "tokens (stopped)",
                Status::Interrupted,
                |t, _| t.flags.tokens = Some(182_000),
                "Tokens: 182k",
            ),
            (
                "pr provider",
                Status::PrOpen,
                |t, x| {
                    t.flags.pr_number = Some(1);
                    x.pr_provider = Some("GitHub".into())
                },
                "GitHub",
            ),
            (
                "your turn",
                Status::Idle,
                |t, _| t.flags.your_turn = true,
                "your turn · in terminal",
            ),
            (
                "parked branch",
                Status::Parked,
                |_, x| x.branch = Some(otaq_core::naming::branch("SHOP-25")),
                "Branch ",
            ),
            (
                "decisions",
                Status::WaitingForYou,
                |t, _| t.flags.open_decisions = 1,
                "1 decision",
            ),
        ];
        for (what, status, setup, want) in cases {
            let mut t = ticket("SHOP-1", "x", *status);
            let mut x = main_project();
            setup(&mut t, &mut x);
            let vm = CardViewModel::from(&t, &x, NOW);
            assert!(places(&vm).contains(want), "{what}: {:?}", places(&vm));
            if *what == "priority" {
                assert_eq!(vm.priority, Some(Priority::High));
            }
        }
        // Pinned is not a flag: the pin icon after the key (d0047).
        let mut t = ticket("SHOP-19", "Price table", Status::Idle);
        t.flags.pinned_at = Some(1);
        let vm = card(&t);
        assert!(vm.pinned);
        assert_eq!(flag_of(&vm), Some("Idle"));
    }

    fn live(key: &str, status: Status, text: Option<&str>, since_min: i64) -> Ticket {
        let mut t = ticket(key, "Price table", status);
        t.flags.last_signal_at = Some(NOW - 40_000);
        t.signal = Some(otaq_core::board::tickets::AgentSignal {
            text: text.map(Into::into),
            since: Some(NOW - since_min * 60_000),
            wakeup_at: None,
        });
        t
    }

    fn from_signal(t: &Ticket, now: i64) -> CardViewModel {
        CardViewModel::from(t, &main_project().with_signal(t, now), now)
    }

    /// T-0075: the live signal states map to the card.
    #[test]
    fn signals_map_to_card_content() {
        let vm = from_signal(
            &live(
                "SHOP-22",
                Status::NeedsYou,
                Some("Allow `rm -rf .next`?"),
                0,
            ),
            NOW,
        );
        assert_eq!(vm.outline, Outline::Attention);
        assert_eq!(flag_of(&vm), Some("Needs you"));
        let s = vm.situation.unwrap();
        assert_eq!((s.text.as_str(), s.mono), ("Allow `rm -rf .next`?", true));

        let vm = from_signal(
            &live(
                "SHOP-19",
                Status::NeedsYou,
                Some("helper agent asks: Allow pnpm test --update-snapshots?"),
                0,
            ),
            NOW,
        );
        assert_eq!(
            line(&vm),
            Some("helper agent asks: Allow pnpm test --update-snapshots?")
        );
        assert!(!vm.situation.unwrap().mono);
        assert_eq!(vm.hover.status, "Needs you · helper agent");

        let t = live(
            "SHOP-19",
            Status::WorkingInBackground,
            Some("background: test run, 1 helper agent"),
            6,
        );
        let vm = from_signal(&t, NOW);
        assert_eq!(vm.outline, Outline::Normal);
        assert_eq!(
            line(&vm),
            Some("background: test run, 1 helper agent · 6 min")
        );
        assert!(
            line(&from_signal(&t, NOW + 60_000))
                .unwrap()
                .ends_with("· 7 min")
        );

        let mut t = live("SHOP-19", Status::WorkingInBackground, None, 0);
        let at = otaq_core::signals::background::next_fire("10 16 * * *", NOW).unwrap();
        t.signal.as_mut().unwrap().wakeup_at = Some(at);
        assert_eq!(line(&from_signal(&t, NOW)), Some("wakes up at 4:10pm"));
        assert!(super::ticks(&t));
        assert!(!super::ticks(&ticket("SHOP-17", "x", Status::Merged)));
    }

    #[test]
    fn held_and_parked_and_queued_lines() {
        let held = ticket("SHOP-30", "Coupons", Status::Held);
        let x = CardExtras {
            held_reason: Some("Claude Code limit reached".into()),
            held_by_limit: true,
            ..main_project()
        };
        let vm = CardViewModel::from(&held, &x, NOW);
        assert_eq!(flag_of(&vm), Some("At limit"));
        assert_eq!(line(&vm), Some("Claude Code limit reached"));
        assert_eq!(vm.footer, None, "no Retry: it starts by itself");
        let x = CardExtras {
            held_reason: Some("develop and origin/develop have diverged.".into()),
            ..main_project()
        };
        let vm = CardViewModel::from(&held, &x, NOW);
        assert_eq!(flag_of(&vm), None);
        assert_eq!(vm.situation.as_ref().unwrap().edge, Tone::Warning);
        assert_eq!(
            vm.footer.unwrap().action.unwrap().kind,
            CardActionKind::Retry
        );

        let vm = card(&ticket("SHOP-25", "Upgrade", Status::Parked));
        assert_eq!((flag_of(&vm), line(&vm)), (None, Some(PARKED)));
        assert_eq!(vm.hover.status, "Parked · has work");
        let mut back = ticket("SHOP-25", "Upgrade", Status::Queued);
        back.flags.resuming_conversation = true;
        assert_eq!(line(&card(&back)), Some("Resuming conversation…"));
        let starting = CardViewModel::from(
            &ticket("SHOP-34", "Zoom", Status::Queued),
            &CardExtras {
                starting: true,
                ..main_project()
            },
            NOW,
        );
        assert_eq!(line(&starting), Some("Starting…"));
    }

    #[test]
    fn a_question_with_options_lists_them_on_the_card() {
        let mut t = ticket("SHOP-21", "Order confirmation email", Status::AsksYou);
        t.question = Some(otaq_core::board::questions::HumanQuestion {
            id: "q".into(),
            text: "Which email provider should the order confirmation use?".into(),
            options: vec!["Postmark".into(), "SES".into(), "Resend".into()],
            recommended: Some(1),
            notes: Vec::new(),
            asked_at: 0,
            via_lead: None,
        });
        let vm = card(&t);
        let rows = vm.options.as_ref().unwrap().rows();
        assert_eq!(rows.len(), 4);
        assert!(rows[0].recommended && rows[0].picked);
        assert_eq!(foot(&vm).as_deref(), Some("[Send answer]"));
        // Without a recommendation: Reply, which opens the panel.
        t.question.as_mut().unwrap().recommended = None;
        assert_eq!(foot(&card(&t)).as_deref(), Some("[Reply]"));
        t.question.as_mut().unwrap().options.clear();
        assert!(card(&t).options.is_none());
    }

    #[test]
    fn a_focused_waiting_ticket_stays_calm() {
        let mut t = ticket("SHOP-19", "Price table", Status::Idle);
        t.flags.your_turn = true;
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("Working"));
        assert_eq!(vm.outline, Outline::Normal);
        assert_eq!(line(&vm), Some("your turn · in terminal"));
        t.flags.your_turn = false;
        assert_eq!(card(&t).outline, Outline::Attention);
    }

    /// An interrupted question keeps its flag and is answered, not resumed, from the card (d0045).
    #[test]
    fn restarts_keep_the_needs_you_flag() {
        let mut asked = ticket("SHOP-23", "Split user full name", Status::Interrupted);
        asked.interrupted_from = Some(Status::AsksYou);
        let vm = card(&asked);
        assert_eq!(flag_of(&vm), Some("Asks you"));
        assert_eq!(vm.outline, Outline::Attention);
        assert_eq!(
            line(&vm),
            Some("Session ended by restart · answering resumes it")
        );
        assert_eq!(foot(&vm).as_deref(), Some("[Reply]"));
        let validating = CardViewModel::from(
            &ticket("SHOP-29", "Coupon field", Status::Interrupted),
            &CardExtras {
                was_validating: true,
                ..main_project()
            },
            NOW,
        );
        assert_eq!(
            line(&validating),
            Some("Validation stopped by restart · Resume starts it fresh (not counted)")
        );
        let failed = card(&ticket(
            "BILL-4",
            "Invoice PDF export",
            Status::ResumeFailed,
        ));
        assert_eq!(flag_of(&failed), Some("Resume fail"));
        assert_eq!(
            failed.footer.unwrap().action.unwrap().kind,
            CardActionKind::StartFromScratch
        );
    }

    /// A check the restart stopped (`interrupted_from` Checking) keeps no question on its card: no "answering resumes
    /// it", no Reply; it says the validation stopped and Resume starts it fresh (#58).
    #[test]
    fn a_check_interrupted_by_a_restart_keeps_no_question() {
        let mut checking = ticket("SHOP-15", "Weekly summary", Status::Interrupted);
        checking.interrupted_from = Some(Status::Checking);
        let vm = CardViewModel::from(
            &checking,
            &CardExtras {
                was_validating: true,
                ..main_project()
            },
            NOW,
        );
        assert_eq!(
            line(&vm),
            Some("Validation stopped by restart · Resume starts it fresh (not counted)")
        );
        assert_ne!(foot(&vm).as_deref(), Some("[Reply]"));
        assert_ne!(vm.outline, Outline::Attention);
    }

    /// MSG-06, LEAD-06: asking another agent or the lead is calm, on the situation line.
    #[test]
    fn asked_another_agent_or_the_lead() {
        let mut t = ticket("SHOP-40.2", "Wishlist button", Status::Working);
        t.flags.asked = Some("SHOP-40.1".parse().unwrap());
        t.flags.asked_question = Some("Which endpoint returns the wishlist?".into());
        assert_eq!(
            line(&card(&t)),
            Some("asked SHOP-40.1 · waiting: “Which endpoint returns the wishlist?”")
        );
        t.flags.asked_more = 1;
        t.status = Status::Idle;
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("Idle"));
        assert!(
            line(&vm)
                .unwrap()
                .ends_with("asked SHOP-40.1 +1 · waiting: “Which endpoint returns the wishlist?”")
        );
        let (_, vm) = variant("Dev · asked lead · waiting");
        assert_eq!(flag_of(&vm), Some("Working"));
        assert_eq!(vm.outline, Outline::Normal, "not turquoise");
        assert!(
            line(&vm)
                .unwrap()
                .starts_with("asked lead · waiting: “Should the button")
        );
    }

    #[test]
    fn answering_moved_and_rework_lines() {
        let mut t = ticket("SHOP-40.1", "Wishlist API", Status::Merged);
        t.flags.answering = Some("SHOP-40.2".parse().unwrap());
        assert_eq!(
            line(&card(&t)),
            Some("answering SHOP-40.2 · resumed, answer-only")
        );

        let mut moved = ticket("SHOP-40.4", "Wishlist share link", Status::Backlog);
        moved.epic = Some("SHOP-40".parse().unwrap());
        let x = CardExtras {
            old_key: Some("SHOP-51".parse().unwrap()),
            ..main_project()
        };
        let vm = CardViewModel::from(&moved, &x, NOW);
        assert_eq!(vm.key_label, "SHOP-51 → SHOP-40.4");
        assert_eq!(
            line(&vm),
            Some("Moved into epic SHOP-40 · old key SHOP-51 still works")
        );

        let mut t = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        t.flags.pr_number = Some(481);
        t.work = Some(otaq_core::board::tickets::Work {
            pr_url: Some("https://github.com/acme/shop-web/pull/481".into()),
            ..Default::default()
        });
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("PR #481 open"));
        assert!(vm.hover.details.contains(&"GitHub".to_owned()));
        // FLW-16 on every flow.
        assert_eq!(
            foot(&vm).as_deref(),
            Some("[Send back with PR comments] [Mark merged]")
        );
        t.flags.changed_since_review = true;
        assert!(
            line(&card(&t))
                .unwrap()
                .contains("Changed since you reviewed it")
        );
        t.flags.changed_since_review = false;

        t.status = Status::WaitingForYou;
        t.place = otaq_core::domain::Place::stage_of("dev");
        t.flags.pr_rework = true;
        let vm = card(&t);
        assert_eq!(
            line(&vm),
            Some("Back for rework · Send back to start the agent")
        );
        assert_eq!(vm.footer, None, "no Review / Push until the fix");
        t.flags.pr_rework = false;
        assert_eq!(
            card(&t).footer.unwrap().action.unwrap().kind,
            CardActionKind::PushToPr
        );
    }

    /// *04 → Row · priority & Ready for Dev*: P1, P2 and P4 sit after the key; Normal shows nothing; the priority
    /// never takes the flag.
    #[test]
    fn priority_chip_per_priority() {
        let mut t = ticket("SHOP-31", "Hotfix: checkout crash", Status::Backlog);
        assert_eq!(card(&t).priority, None, "P3 Normal is hidden on cards");
        for p in [Priority::Urgent, Priority::High, Priority::Low] {
            t.priority = p;
            assert_eq!(card(&t).priority, Some(p));
            assert_eq!(card(&t).flag, None);
        }
        let mut child = ticket("SHOP-40.2", "Wishlist button", Status::Backlog);
        child.epic = Some("SHOP-40".parse().unwrap());
        child.priority = Priority::High;
        child.priority_inherited = true;
        assert_eq!(card(&child).priority, Some(Priority::High));
    }

    /// SHOP-40 Wishlist with children 40.1…40.4 (40.10 to check the key order), and SHOP-51 on its own.
    fn wishlist_project() -> Vec<Ticket> {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = otaq_core::domain::Kind::Epic;
        let mut all = vec![
            epic,
            ticket("SHOP-51", "Fix checkout rounding", Status::Working),
        ];
        for (key, status) in [
            ("SHOP-40.10", Status::Backlog),
            ("SHOP-40.2", Status::Working),
            ("SHOP-40.1", Status::Merged),
            ("SHOP-40.3", Status::Queued),
        ] {
            let mut child = ticket(key, "Child", status);
            child.epic = Some("SHOP-40".parse().unwrap());
            all.push(child);
        }
        all
    }

    fn find<'a>(all: &'a [Ticket], key: &str) -> &'a Ticket {
        all.iter().find(|t| t.key.to_string() == key).unwrap()
    }

    /// EPC-09, BRD-03: every child names its epic; the "2 of 4" band is gone (EPL-01).
    #[test]
    fn the_sleeve_names_the_epic() {
        let all = wishlist_project();
        let at = |key| super::sleeve(find(&all, key), &all);
        assert_eq!(
            at("SHOP-40.2"),
            Some(Sleeve {
                epic_key: "SHOP-40".into(),
                name: "Wishlist".into(),
                pinned: false,
            })
        );
        assert_eq!(at("SHOP-40.1").unwrap().name, "Wishlist", "merged too");
        assert_eq!(at("SHOP-51"), None, "a standalone ticket");
        assert_eq!(at("SHOP-40"), None, "the epic itself");
        let x = CardExtras {
            sleeve: at("SHOP-40.2"),
            ..main_project()
        };
        let vm = CardViewModel::from(find(&all, "SHOP-40.2"), &x, NOW);
        assert_eq!(vm.sleeve.unwrap().name, "Wishlist");
        assert_eq!(
            flag_of(&variant("Dev · epic child · asks you").1),
            Some("Asks you")
        );
    }

    /// EPC-17: a pinned epic's pin is in the sleeve; its children show their own pin only when pinned themselves.
    #[test]
    fn a_pinned_epic_pins_the_sleeve_not_the_child() {
        let mut all = wishlist_project();
        all[0].flags.pinned_at = Some(NOW);
        let child = find(&all, "SHOP-40.3").clone();
        let sleeve = super::sleeve(&child, &all);
        assert!(sleeve.as_ref().unwrap().pinned);
        let x = CardExtras {
            sleeve,
            ..main_project()
        };
        assert!(!CardViewModel::from(&child, &x, NOW).pinned);
        let mut own = child.clone();
        own.flags.pinned_at = Some(NOW);
        assert!(CardViewModel::from(&own, &x, NOW).pinned);
    }

    /// d0124: a child a sibling's merge freed says it waits for the lead's update, until its pass ends.
    #[test]
    fn a_freed_child_waits_for_the_leads_update() {
        let mut t = ticket("SHOP-40.3", "Wishlist page", Status::Queued);
        t.epic = Some("SHOP-40".parse().unwrap());
        t.flags.waiting_for_lead = true;
        assert_eq!(
            line(&card(&t)),
            Some("Waiting for the lead\u{2019}s update")
        );
        t.flags.waiting_for_lead = false;
        assert_ne!(
            line(&card(&t)),
            Some("Waiting for the lead\u{2019}s update")
        );
    }

    /// EPC-13: a child that joined its whole-queued epic's queue says who added it.
    #[test]
    fn a_child_that_joined_the_queue_says_who_added_it() {
        let mut t = ticket("SHOP-40.4", "Wishlist share link", Status::Queued);
        t.epic = Some("SHOP-40".parse().unwrap());
        assert_eq!(card(&t).situation, None);
        t.joined_queue_by = Some(Actor::Agent("SHOP-40.2".parse().unwrap()));
        let vm = card(&t);
        assert_eq!(
            line(&vm),
            Some("New · added by SHOP-40.2 · in Ready for Dev")
        );
        assert_eq!(vm.situation.unwrap().edge, Tone::Epic);
        t.joined_queue_by = Some(Actor::User("me".into()));
        assert_eq!(
            line(&card(&t)),
            Some("New · added by you · in Ready for Dev")
        );
        let agent = Actor::ProjectAgent("pa-1".into());
        t.created_by = agent.clone();
        t.joined_queue_by = Some(agent);
        let named = CardExtras {
            agent_name: Some("Checkout slowness".into()),
            ..main_project()
        };
        assert_eq!(
            line(&CardViewModel::from(&t, &named, NOW)),
            Some("New · added by Project agent “Checkout slowness” · in Ready for Dev")
        );
        assert_eq!(
            line(&card(&t)),
            Some("New · added by a Project agent · in Ready for Dev")
        );
    }

    /// M19-e and DEC-03: the schema overlap (with its files on hover) and the open decisions join the line.
    #[test]
    fn schema_overlap_and_decisions() {
        use otaq_core::domain::SchemaOverlap;
        let mut t = ticket("SHOP-18", "Add orders table", Status::WaitingForYou);
        assert_eq!(line(&card(&t)), None);
        let overlap = |other: &str, file: &str| SchemaOverlap {
            other: other.parse().unwrap(),
            path: "db/migrations/**".into(),
            files: vec![file.into()],
        };
        t.work = Some(otaq_core::board::tickets::Work {
            schema_overlap: vec![overlap("SHOP-21", "db/migrations/0042_coupons.sql")],
            ..Default::default()
        });
        t.flags.open_decisions = 1;
        let vm = card(&t);
        assert_eq!(
            line(&vm),
            Some("1 decision · adds migrations next to SHOP-21 (db/migrations/**)")
        );
        let hover = &vm.situation.as_ref().unwrap().hovers[0];
        assert_eq!(
            hover.sections[0].label.as_deref(),
            Some("SHOP-21 · db/migrations/**")
        );
        assert_eq!(hover.sections[1].text, SCHEMA_OVERLAP_HINT);
        t.work
            .as_mut()
            .unwrap()
            .schema_overlap
            .push(overlap("SHOP-22", "x.sql"));
        assert!(line(&card(&t)).unwrap().contains("next to SHOP-21 +1"));
        t.flags.open_decisions = 3;
        assert_eq!(decisions_label(&t).as_deref(), Some("3 decisions"));
        for status in [Status::Working, Status::Merged] {
            t.status = status;
            assert_eq!(decisions_label(&t), None, "{status:?}");
        }
    }

    /// d0121: a ticket that went on without its remote says so, git's reason on hover, until a fetch for it works
    /// (the core clears it then); a Done card doesn't.
    #[test]
    fn remote_warning() {
        use otaq_core::domain::{RemoteWarning, WentOn};
        let reason = "git fetch --quiet origin failed (exit 128): Host key verification failed.";
        let mut t = ticket("SHOP-18", "Checkout API", Status::Working);
        t.work = Some(otaq_core::board::tickets::Work {
            remote_warning: Some(RemoteWarning {
                went_on: WentOn::Started,
                branch: "main".into(),
                remote: "origin".into(),
                reason: reason.into(),
            }),
            ..Default::default()
        });
        let vm = card(&t);
        assert_eq!(
            line(&vm),
            Some("Couldn\u{2019}t update main from origin \u{b7} started from your local main")
        );
        let hover = &vm.situation.as_ref().unwrap().hovers[0];
        assert_eq!(hover.tone, Tone::Warning);
        assert_eq!(hover.sections[0].text, reason);

        let warning = t.work.as_mut().unwrap().remote_warning.as_mut().unwrap();
        warning.went_on = WentOn::Merged;
        warning.branch = "develop".into();
        t.status = Status::WaitingForYou;
        assert_eq!(
            line(&card(&t)),
            Some(
                "Couldn\u{2019}t update develop from origin \u{b7} merged into your local develop"
            )
        );
        t.status = Status::Merged;
        assert!(!line(&card(&t)).unwrap_or_default().contains("origin"));
        // After a good fetch.
        t.status = Status::Working;
        t.work.as_mut().unwrap().remote_warning = None;
        assert_eq!(line(&card(&t)), None);
    }

    /// d0121: an automatic Delivery whose git step failed: "Couldn't deliver · <reason>", Retry merge.
    #[test]
    fn a_failed_delivery() {
        use otaq_core::domain::AutoMergeStop;
        let mut t = ticket("BILL-6", "Late fee rule", Status::DeliveryStopped);
        t.auto_merge_stop = Some(AutoMergeStop::DeliveryFailed);
        let text =
            otaq_core::domain::validation::delivery_failed("git commit failed (exit 1): lint");
        let x = CardExtras {
            question: Some(text.clone()),
            ..main_project()
        };
        let vm = CardViewModel::from(&t, &x, NOW);
        assert_eq!(
            (flag_of(&vm), line(&vm)),
            (Some("Not delivered"), Some(text.as_str()))
        );
        assert_eq!(
            text,
            "Couldn\u{2019}t deliver \u{b7} git commit failed (exit 1): lint"
        );
        assert_eq!(vm.outline, Outline::Attention);
    }

    #[test]
    fn labels() {
        assert_eq!(tokens_label(182_400), "182k");
        assert_eq!(tokens_label(1_250_000), "1.25M");
        assert_eq!(tokens_label(12_400_000), "12.4M");
        assert_eq!(tokens_label(950), "950");
        assert_eq!(age_label(40_000), "40s");
        assert_eq!(age_label(3 * 60_000), "3m");
        assert_eq!(age_label(2 * 3_600_000), "2h");
        let keys: Vec<String> = ["SHOP-17", "SHOP-18", "SHOP-19"].map(String::from).to_vec();
        assert_eq!(deps_label(&keys, 2), "SHOP-17, SHOP-18 +1");
        assert_eq!(deps_label(&keys[..2], 2), "SHOP-17, SHOP-18");
    }

    #[test]
    fn done_cards_fade() {
        assert_eq!(variant("Merged").1.faded, Some(0.55));
        assert_eq!(variant("Closed · not merged (0033)").1.faded, Some(0.8));
        assert_eq!(
            variant("Merged automatically (0064)").1.hover.details,
            ["Validated · round 1"]
        );
    }

    #[test]
    fn the_validation_and_merge_stops() {
        // Without a report, the count.
        let mut t = ticket("SHOP-32", "Address autocomplete", Status::CheckFailed);
        t.validation_round = 1;
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("Failed ×1"));
        assert_eq!(line(&vm), Some("Validation failed 1 time"));
        // A discarded verdict says why (VAL-02).
        let mut t = ticket("SHOP-34", "Tax display", Status::CheckNoResult);
        t.signal = Some(otaq_core::board::tickets::AgentSignal {
            text: Some(otaq_core::domain::validation::DISCARDED.into()),
            since: None,
            wakeup_at: None,
        });
        assert_eq!(
            line(&from_signal(&t, NOW)),
            Some("Validation discarded: the validator changed files")
        );
        let (_, vm) = variant("Validation ended without a result");
        let action = vm.footer.unwrap().action.unwrap();
        assert_eq!(
            action.kind.validation_action(),
            Some(otaq_core::domain::ValidationAction::Retry)
        );
        // Merge conflict · fixing never shows a stop; more files count up.
        let mut t = ticket(
            "SHOP-40.4",
            "Wishlist share link",
            Status::MergeConflictFixing,
        );
        t.flags.conflict_files = vec!["a.ts".into(), "b.ts".into(), "c.ts".into()];
        t.flags.conflicts_with = Some(otaq_core::naming::branch("SHOP-40"));
        let vm = card(&t);
        assert_eq!(line(&vm), Some("Merge conflict · fixing · a.ts +2"));
        assert_eq!(vm.outline, Outline::Normal);
        // The test merge's conflict, found while it waited in Review, names the branch (d0127); the Review flag is
        // gone while the agent fixes it.
        let mut t = ticket("SHOP-19", "Price table", Status::MergeConflictFixing);
        t.flags.conflicts_with = Some("main".into());
        t.flags.conflict_in_review = true;
        let vm = card(&t);
        assert_eq!(line(&vm), Some("Conflicts with main · fixing"));
        assert_eq!(
            vm.flag.as_ref().map(|f| f.full.as_str()),
            Some("Conflicts with main · fixing")
        );
        t.flags.conflict_files = vec!["lib/pricing.ts".into()];
        assert_eq!(
            line(&card(&t)),
            Some("Conflicts with main · fixing · lib/pricing.ts")
        );
        // Still in Review (its PR is open, or the project is paused): only the flag.
        t.status = Status::WaitingForYou;
        assert!(
            !line(&card(&t)).unwrap_or_default().contains("fixing"),
            "{:?}",
            line(&card(&t))
        );
        // The other Auto-merge stops show the Local merge's message.
        use otaq_core::domain::AutoMergeStop;
        for (stop, label) in [
            (AutoMergeStop::BaseDiverged, "Diverged"),
            (AutoMergeStop::CheckoutDirty, "Dirty repo"),
        ] {
            let mut t = ticket("BILL-6", "Late fee rule", Status::DeliveryStopped);
            t.auto_merge_stop = Some(stop);
            let x = CardExtras {
                question: Some("the message".into()),
                ..main_project()
            };
            let vm = CardViewModel::from(&t, &x, NOW);
            assert_eq!(
                (flag_of(&vm), line(&vm)),
                (Some(label), Some("the message"))
            );
            assert_eq!(vm.outline, Outline::Attention);
        }
        // A submitted note keeps its line next to the entry (d0052).
        let mut t = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        t.flags.review_entry = Some(otaq_core::domain::ReviewEntryFlag {
            entry: otaq_core::domain::ReviewEntry::SubmittedByUser,
            round: 0,
        });
        t.flags.submitted_by_you = Some(String::new());
        assert_eq!(line(&card(&t)), Some("Submitted by you · not validated"));
        t.flags.submitted_by_you = Some("Paging works, tests pending".into());
        assert_eq!(
            line(&card(&t)),
            Some("Submitted by you · “Paging works, tests pending” · not validated")
        );
    }

    /// d0120: "Merging · running the merge check" with Stop (and nothing else) while Local merge runs the check, in
    /// Review or To deliver; "Merge check failed · fixing" while the agent fixes what it found.
    #[test]
    fn the_merge_check_states() {
        for (status, custom) in [
            (Status::WaitingForYou, false),
            (Status::WaitingForYou, true),
            (Status::ToDeliver, true),
        ] {
            let mut t = ticket("SHOP-2", "Card", status);
            t.flags.merge_check = Some(MergeCheckState::Running);
            let x = CardExtras {
                custom_flow: custom,
                ..main_project()
            };
            let vm = CardViewModel::from(&t, &x, NOW);
            assert_eq!(flag_of(&vm), Some("Merging\u{2026}"), "{status:?}");
            assert_eq!(line(&vm), Some("Merging \u{b7} running the merge check"));
            let footer = vm.footer.as_ref().unwrap();
            let stop = footer.action.as_ref().unwrap();
            assert_eq!(
                (stop.kind, stop.label, stop.primary),
                (CardActionKind::StopMergeCheck, "Stop", false)
            );
            assert_eq!(footer.second, None, "no Fail beside it");
            let one = &vm.one_line;
            assert_eq!(
                one.line.as_ref().map(|l| l.text.as_str()),
                Some("Merging \u{b7} running the merge check")
            );
            assert_eq!(
                one.action.as_ref().map(|a| a.kind),
                Some(CardActionKind::StopMergeCheck)
            );
            assert_eq!(one.edge, None, "nothing to review while it merges");
        }

        let mut t = ticket("SHOP-2", "Card", Status::MergeConflictFixing);
        t.flags.merge_check = Some(MergeCheckState::Failed);
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("Check failed"));
        assert_eq!(line(&vm), Some("Merge check failed \u{b7} fixing"));
        assert_eq!(
            vm.one_line.line.as_ref().map(|l| l.text.as_str()),
            Some("Merge check failed \u{b7} fixing")
        );
        assert_eq!(vm.outline, Outline::Normal, "the agent has it");
        assert_eq!(vm.one_line.action, None);
    }

    // T-0291's lead rows, on the v2 card (LEAD-06…LEAD-10).

    #[test]
    fn the_leads_markers() {
        let (_, vm) = variant("Ready for Dev · Updated by Wishlist lead");
        assert_eq!(flag_of(&vm), None);
        assert_eq!(
            line(&vm),
            Some("by Wishlist lead · description, now waits for SHOP-40.4"),
            "the parent the lead linked isn't repeated"
        );
        assert_eq!(vm.situation.unwrap().edge, Tone::Epic);
        let (_, vm) = variant("Ready for Dev · new · added by Wishlist lead");
        assert_eq!(line(&vm), Some("new · added by Wishlist lead"));
        assert_eq!(vm.outline, Outline::Epic);
        // Only on a never-started child.
        let (mut t, _) = variant("Ready for Dev · Updated by Wishlist lead");
        t.status = Status::Working;
        t.place = otaq_core::domain::Place::stage_of("review");
        assert_eq!(
            line(&card(&t)),
            None,
            "no more \"after SHOP-40.4\" (DPV-01)"
        );

        let (mut t, vm) = variant("Dev · Lead's note sent (round 0 of 3)");
        assert_eq!(
            line(&vm),
            Some(
                "Lead's note sent · validation round 0 of 3: “The API returns items[] (SHOP-40.1). Fix the test, \
                 don't change the API.”"
            )
        );
        t.flags.validation_round = Some(ValidationRound { round: 1, of: 3 });
        assert!(
            line(&card(&t))
                .unwrap()
                .starts_with("Lead's note sent · validation round 1 of 3: ")
        );
        t.status = Status::WaitingForYou;
        assert_eq!(lead_note_callout(&t), None);

        // After the second stop, the note is on hover.
        let (_, vm) = variant("Dev · Failed 3 times after the lead's note");
        assert_eq!(flag_of(&vm), Some("Failed ×3"));
        assert_eq!(
            line(&vm),
            Some(
                "orders.test.ts expects list · see comment · Lead's note sent once · hover for it"
            )
        );
        let hover = &vm.situation.as_ref().unwrap().hovers[0];
        assert_eq!(hover.title, "Wishlist lead");
        assert_eq!(
            hover.sections[0].label.as_deref(),
            Some("LEAD'S NOTE · SENT AFTER ROUND 3")
        );
        assert_eq!(hover.sections[1].label.as_deref(), Some("LEAD'S VIEW"));

        // The unread note and the same file, each with its hover, quiet (not turquoise).
        let (mut t, vm) = variant("Dev · both on one card");
        assert_eq!(
            line(&vm),
            Some("1 unread note from Wishlist lead · same file as SHOP-40.2")
        );
        let s = vm.situation.unwrap();
        assert_eq!(s.edge, Tone::Muted);
        assert_eq!(s.hovers.len(), 2);
        assert!(s.hovers[0].title.ends_with("· unread"));
        assert_eq!(s.hovers[1].sections.last().unwrap().text, SAME_FILE_HINT);
        // The unread note only on a running child; the same file "+1" with two others.
        t.status = Status::WaitingForYou;
        t.place = otaq_core::domain::Place::of(t.status, None, None);
        t.flags.same_file.push(otaq_core::domain::SameFile {
            other: "SHOP-40.3".parse().unwrap(),
            files: vec!["app/wishlist/page.tsx".into()],
        });
        assert_eq!(line(&card(&t)), Some("same file as SHOP-40.2 +1"));
    }

    /// "Sent back · 1 decision changed · round 2" (DEC-06); no line comments, "Sent back · round 2" (#20).
    #[test]
    fn sent_back_counts_changed_decisions() {
        use otaq_core::domain::flags::SentBack;
        let sent = |comments, decisions| SentBack {
            comments,
            round: 2,
            decisions,
        };
        assert_eq!(
            sent_back_text(sent(3, 0)),
            "Sent back · 3 comments · round 2"
        );
        assert_eq!(sent_back_text(sent(0, 0)), "Sent back · round 2");
        assert_eq!(
            sent_back_text(sent(0, 1)),
            "Sent back · 1 decision changed · round 2"
        );
        assert_eq!(
            sent_back_text(sent(1, 2)),
            "Sent back · 1 comment · 2 decisions changed · round 2"
        );
    }

    /// The M22 cards (`uUqnz`, the picked-by cards of `SPy9r`; update Y §3, §8): the footer's agent part, Start
    /// failed and No result on a refused model, each with its flag, line, hover, action and the row it opens.
    #[test]
    fn the_m22_cards_name_their_agents() {
        let rows = [
            (
                "Start failed on the model",
                Some("Start failed"),
                Some("Claude Code couldn\u{2019}t run the model"),
                Some("[Open agent\u{2026}]"),
            ),
            (
                "No result \u{b7} the validator\u{2019}s model",
                Some("No result"),
                Some("Validator couldn\u{2019}t run its model"),
                Some("[Open agent\u{2026}]"),
            ),
            (
                "Project default \u{b7} no footer",
                None,
                Some("Ready to start"),
                None,
            ),
            (
                "Working \u{b7} two or more named agents",
                Some("Working"),
                None,
                Some("Strong 182k 40s"),
            ),
            // Q-A: with one named agent, M21's footer.
            (
                "Working \u{b7} one named agent",
                Some("Working"),
                None,
                Some("182k 40s"),
            ),
        ];
        for (name, flag, situation, footer) in rows {
            let (_, vm) = variant(name);
            assert_eq!(
                (flag_of(&vm), line(&vm), foot(&vm).as_deref()),
                (flag, situation, footer),
                "{name}"
            );
        }
    }

    /// Start failed on the model (AGT-05): red, the tool's message on the line's hover and the flag's, Open agent…
    /// opens the agent whose model the tool refused; No result keeps turquoise and opens the validator's.
    #[test]
    fn a_refused_model_says_what_couldnt_run_and_opens_the_agent() {
        let (t, vm) = variant("Start failed on the model");
        let flag = vm.flag.as_ref().unwrap();
        assert_eq!(flag.palette, Palette::Error);
        assert_eq!(
            vm.hover.status,
            "Start failed \u{b7} couldn\u{2019}t run the model"
        );
        assert!(
            vm.hover
                .details
                .contains(&card_fixtures::MODEL_ISSUE_MESSAGE.to_owned())
        );
        let situation = vm.situation.as_ref().unwrap();
        assert_eq!(
            (situation.edge, situation.text_tone),
            (Tone::Error, Tone::Error)
        );
        let hover = &situation.hovers[0];
        assert_eq!(hover.title, "Claude Code couldn\u{2019}t run the model");
        assert_eq!(hover.sections[0].text, card_fixtures::MODEL_ISSUE_MESSAGE);
        let action = vm.footer.as_ref().unwrap().action.as_ref().unwrap();
        assert_eq!(
            (action.kind, action.primary, action.label),
            (CardActionKind::OpenAgent, true, "Open agent\u{2026}")
        );
        assert_eq!(start_refusal(&t), Some(card_fixtures::MODEL_ISSUE_MESSAGE));

        let (t, vm) = variant("No result \u{b7} the validator\u{2019}s model");
        assert_eq!(vm.flag.as_ref().unwrap().palette, Palette::Attn);
        assert_eq!(vm.outline, Outline::Attention);
        let hover = &vm.situation.as_ref().unwrap().hovers[0];
        assert_eq!(
            hover.title,
            "The validator couldn\u{2019}t run claude-fable-5-1"
        );
        let action = vm.footer.as_ref().unwrap().action.as_ref().unwrap();
        assert_eq!(action.kind, CardActionKind::OpenAgent);
        assert_eq!(
            validator_refusal(&t),
            Some((
                Some("claude-fable-5-1".into()),
                card_fixtures::MODEL_ISSUE_MESSAGE.into()
            ))
        );
        assert_eq!(
            serde_json::to_value(CardActionKind::OpenAgent).unwrap(),
            "open_agent"
        );

        // Another start problem and another No result keep today's line and action.
        let mut other = ticket("SHOP-28", "Cart badge count", Status::StartInterrupted);
        other.start_problem = Some(otaq_core::board::tickets::StartProblem {
            step: otaq_core::domain::StartStep::Setup,
            message: "exit 1".into(),
        });
        assert_eq!(start_refusal(&other), None);
        let vm = card(&other);
        assert_eq!(
            vm.footer.unwrap().action.unwrap().kind,
            CardActionKind::RetryStart
        );
        let vm = card(&ticket("SHOP-34", "Tax display", Status::CheckNoResult));
        assert_eq!(
            vm.footer.unwrap().action.unwrap().kind,
            CardActionKind::RetryValidation
        );
    }

    /// d0086, "only what's needed": with one named agent no card names an agent, not even a refused one; and with
    /// one tool no card names the tool beside an agent.
    #[test]
    fn one_named_agent_names_nothing() {
        for v in card_fixtures::agent_variants() {
            let extras = CardExtras {
                agents: AgentFacts {
                    count: 1,
                    ..v.extras.agents.clone()
                },
                ..v.extras.clone()
            };
            let vm = CardViewModel::from(&v.ticket, &extras, NOW);
            assert!(
                vm.footer.as_ref().is_none_or(|f| f.agent.is_none()),
                "{}",
                v.name
            );
            assert!(vm.avatar.is_none(), "{}: one tool, no avatar", v.name);
        }
        for v in card_fixtures::agent_variants() {
            let vm = CardViewModel::from(&v.ticket, &v.extras, NOW);
            let said = vm
                .footer
                .and_then(|f| f.agent)
                .map(|a| a.text)
                .unwrap_or_default();
            assert!(
                !said.contains(" \u{b7} Claude Code"),
                "{}: no tool name beside an agent",
                v.name
            );
        }
    }

    /// A limit's hold says when its tool resets, when that is known (Q154): "Claude Code limit · resets 3:45 pm".
    #[test]
    fn a_limits_hold_says_when_its_tool_resets() {
        let v = card_fixtures::tool_variants()
            .into_iter()
            .find(|v| v.name == "Tool at its limit, no fallback (0086)")
            .unwrap();
        let at = NOW + 2 * 3_600_000;
        let mut extras = v.extras.clone();
        extras.agents.holds[0].resets_at = Some(at);
        let vm = CardViewModel::from(&v.ticket, &extras, NOW);
        assert_eq!(
            vm.situation.unwrap().text,
            format!(
                "Claude Code limit \u{b7} resets {}",
                crate::view_models::clock::reset_label(at, NOW)
            )
        );
    }

    /// What each M23 card says (*04 → Variants · several tools (M23)*, the fallback and limit cards of `SPy9r`):
    /// avatar, head flag and its colour, the situation line and its edge, the hover's title, the footer, and the
    /// action's kind. A model that can't run is red (Can't start, with Use fallback and Open agent…, `s03-54`), Limit
    /// neutral, moves amber; never turquoise.
    #[test]
    fn the_m23_cards_hold_move_and_name_the_tool() {
        type Row = (
            &'static str,
            &'static str,
            Option<(&'static str, Palette)>,
            &'static str,
            Tone,
            Option<&'static str>,
            &'static str,
            Option<(CardActionKind, bool)>,
        );
        use CardActionKind as K;
        let want: [Row; 12] = [
            (
                "Start failed \u{b7} several tools",
                "CC",
                Some(("Start failed", Palette::Error)),
                "Claude Code couldn\u{2019}t run the model",
                Tone::Error,
                Some("Claude Code couldn\u{2019}t run the model"),
                "[Open agent\u{2026}]",
                Some((K::OpenAgent, true)),
            ),
            (
                "Start failed \u{b7} Codex",
                "CX",
                Some(("Start failed", Palette::Error)),
                "Codex couldn\u{2019}t run the model",
                Tone::Error,
                Some("Codex couldn\u{2019}t run the model"),
                "[Open agent\u{2026}]",
                Some((K::OpenAgent, true)),
            ),
            (
                "Model unavailable",
                "CX",
                Some(("Can\u{2019}t start", Palette::Error)),
                "Old GPT\u{2019}s model can\u{2019}t run: Codex no longer lists gpt-5.4",
                Tone::Error,
                Some("Old GPT \u{b7} Codex \u{b7} gpt-5.4 is no longer in Codex\u{2019}s list"),
                "Old GPT [Open agent\u{2026}]",
                Some((K::OpenAgent, false)),
            ),
            (
                "Model unavailable \u{b7} the validator",
                "CX",
                Some(("Can\u{2019}t start", Palette::Error)),
                "Old GPT\u{2019}s model can\u{2019}t run: Codex no longer lists gpt-5.4",
                Tone::Error,
                Some("Old GPT \u{b7} Codex \u{b7} gpt-5.4 is no longer in Codex\u{2019}s list"),
                "Old GPT [Open agent\u{2026}]",
                Some((K::OpenAgent, false)),
            ),
            (
                "Swapped model \u{b7} not on the card",
                "CP",
                Some(("Validating\u{2026}", Palette::Queued)),
                "round 1 of 3 \u{b7} 2 min",
                Tone::Muted,
                None,
                "Verifier 23k",
                None,
            ),
            (
                "Tool at its limit, no fallback (0086)",
                "CC",
                Some(("At limit", Palette::Queued)),
                "Claude Code limit",
                Tone::Muted,
                None,
                "Light [Open agent\u{2026}]",
                Some((K::OpenAgent, false)),
            ),
            (
                "Tool at its limit \u{b7} fallback off",
                "CC",
                Some(("At limit", Palette::Queued)),
                "Claude Code limit",
                Tone::Muted,
                None,
                "Strong [Use fallback] [Open agent\u{2026}]",
                Some((K::UseFallback, false)),
            ),
            (
                "Tool not available \u{b7} fallback off (0086)",
                "CX",
                Some(("Can\u{2019}t start", Palette::Error)),
                "Standard\u{2019}s model can\u{2019}t run: Codex isn\u{2019}t available",
                Tone::Error,
                None,
                "Standard [Use fallback] [Open agent\u{2026}]",
                Some((K::UseFallback, false)),
            ),
            (
                "Validator held \u{b7} tool not available",
                "CP",
                Some(("Can\u{2019}t start", Palette::Error)),
                "Verifier\u{2019}s model can\u{2019}t run: GitHub Copilot isn\u{2019}t available",
                Tone::Error,
                None,
                "Verifier [Use fallback] [Open agent\u{2026}]",
                Some((K::UseValidatorFallback, false)),
            ),
            (
                "On its tool\u{2019}s fallback (AGM-03)",
                "CX",
                Some(("Working", Palette::Working)),
                "On Codex \u{b7} Claude Code limit",
                Tone::Warning,
                None,
                "Strong GPT 80k 30s",
                None,
            ),
            (
                "Fallback also at its limit",
                "CC",
                Some(("At limit", Palette::Queued)),
                "Claude Code and its fallback Codex are both at their limits",
                Tone::Muted,
                None,
                "Strong [Open agent\u{2026}]",
                Some((K::OpenAgent, false)),
            ),
            (
                "Validator on its tool\u{2019}s fallback",
                "CC",
                Some(("Validating\u{2026}", Palette::Queued)),
                "Validator on Claude Code \u{b7} GitHub Copilot not available",
                Tone::Warning,
                None,
                "Strong 6k",
                None,
            ),
        ];
        let variants = card_fixtures::tool_variants();
        assert_eq!(variants.len(), want.len());
        for (v, (name, avatar, flag, text, edge, hover, footer, act)) in variants.iter().zip(want) {
            assert_eq!(v.name, name);
            let vm = CardViewModel::from(&v.ticket, &v.extras, NOW);
            assert_eq!(
                vm.avatar.as_ref().map(|a| a.letters.as_str()),
                Some(avatar),
                "{name}"
            );
            assert_eq!(
                vm.flag.as_ref().map(|f| (f.label.as_str(), f.palette)),
                flag,
                "{name}"
            );
            let situation = vm.situation.as_ref().unwrap();
            assert_eq!(
                (situation.text.as_str(), situation.edge),
                (text, edge),
                "{name}"
            );
            assert_eq!(
                situation.hovers.first().map(|h| h.title.as_str()),
                hover,
                "{name}"
            );
            assert_eq!(foot(&vm).as_deref(), Some(footer), "{name}");
            assert_eq!(
                vm.footer
                    .as_ref()
                    .and_then(|f| f.action.as_ref())
                    .map(|a| (a.kind, a.primary)),
                act,
                "{name}"
            );
            // Tools and fallbacks are never turquoise (d0077), and holds never outline the card.
            assert_ne!(
                vm.flag.as_ref().map(|f| f.palette),
                Some(Palette::Attn),
                "{name}"
            );
            assert_eq!(vm.outline, Outline::Normal, "{name}");
            assert!(
                vm.flag
                    .as_ref()
                    .is_none_or(|f| f.label.chars().count() <= FLAG_MAX_CHARS)
            );
        }
    }

    /// AGM-09 (`s03-38`, `s03-42`): a running card's footer names the agent and the model passed; a ticket waiting
    /// before an Agent stage says where it starts; nothing names a choice before it starts (Backlog).
    #[test]
    fn footers_name_the_agent_and_its_model() {
        use otaq_core::agent_tools::Tool;
        use otaq_core::agents::ModelRef;
        let running = CardExtras {
            agents: AgentFacts {
                running_developer: Some("Rust developer".into()),
                developer_on: Some(RunsOnTool {
                    tool: Tool::Claude,
                    model: Some("claude-sonnet-5-5".into()),
                    passed: Some("sonnet".into()),
                    fallback: None,
                }),
                running_validator: Some("Validator".into()),
                validator_on: Some(RunsOnTool {
                    tool: Tool::Codex,
                    model: Some("gpt-5.5".into()),
                    passed: Some("gpt-5.5".into()),
                    fallback: None,
                }),
                ..card_fixtures::named("Rust developer")
            },
            ..main_project()
        };
        let agent_of = |status, x: &CardExtras| {
            CardViewModel::from(&ticket("SHOP-33", "Shipping estimate", status), x, NOW)
                .footer
                .and_then(|f| f.agent)
                .map(|a| a.text)
        };
        assert_eq!(
            agent_of(Status::Working, &running).as_deref(),
            Some("Rust developer \u{b7} sonnet")
        );
        assert_eq!(
            agent_of(Status::Checking, &running).as_deref(),
            Some("Validator \u{b7} gpt-5.5")
        );
        let waiting = CardExtras {
            agents: AgentFacts {
                starts: Some(CardStart {
                    stage: "Grill".into(),
                    agent: "Griller".into(),
                    model: Some(ModelRef {
                        tool: Tool::Claude,
                        model: Some("sonnet".into()),
                    }),
                }),
                ..card_fixtures::named("Griller")
            },
            ..main_project()
        };
        assert_eq!(
            agent_of(Status::Queued, &waiting).as_deref(),
            Some("Grill \u{b7} Griller \u{b7} sonnet")
        );
        assert_eq!(agent_of(Status::Backlog, &waiting), None);
        // One agent only: nothing (d0086).
        let one = CardExtras {
            agents: AgentFacts {
                count: 1,
                ..running.agents.clone()
            },
            ..main_project()
        };
        assert_eq!(agent_of(Status::Working, &one), None);
    }

    /// AGM-03, `s03-54`: a held session whose fallback can take it offers Use fallback and Open agent…, which opens
    /// the held agent; a refused model opens the agent that ran it.
    #[test]
    fn a_card_that_cant_run_offers_the_fallback_and_its_agent() {
        use otaq_core::agent_tools::Tool;
        use otaq_core::agents::HoldReason as H;
        let mut hold = card_fixtures::hold(
            Role::Developer,
            H::ToolUnavailable,
            Tool::Claude,
            None,
            "Griller",
            Some(Tool::Codex),
            true,
            false,
        );
        hold.agent_id = Some("griller".into());
        let x = CardExtras {
            agents: AgentFacts {
                holds: vec![hold],
                ..card_fixtures::named("Griller")
            },
            ..main_project()
        };
        let t = ticket("SHOP-44", "Saved carts", Status::Queued);
        let vm = CardViewModel::from(&t, &x, NOW);
        assert_eq!(flag_of(&vm), Some("Can\u{2019}t start"));
        assert_eq!(
            line(&vm),
            Some("Griller\u{2019}s model can\u{2019}t run: Claude Code isn\u{2019}t available")
        );
        let footer = vm.footer.unwrap();
        assert_eq!(
            (
                footer.action.map(|a| (a.kind, a.label)),
                footer.second.map(|a| (a.kind, a.label))
            ),
            (
                Some((CardActionKind::UseFallback, "Use fallback")),
                Some((CardActionKind::OpenAgent, "Open agent\u{2026}"))
            )
        );
        assert_eq!(open_agent(&t, &x).as_deref(), Some("griller"));
        let refused = CardExtras {
            agents: AgentFacts {
                refused_agent: Some("fable".into()),
                ..card_fixtures::named("Fable test")
            },
            ..main_project()
        };
        assert_eq!(
            open_agent(
                &ticket("SHOP-48", "Gift wrap", Status::StartInterrupted),
                &refused
            )
            .as_deref(),
            Some("fable")
        );
    }

    /// Q276, AGM-10 (`s03-48`): a ticket closed by `otaq stage done` says "Closed · nothing to merge" with the
    /// agent's reason.
    #[test]
    fn a_ticket_closed_with_nothing_to_merge_says_so() {
        let mut t = ticket(
            "SHOP-48",
            "Fix rounding in cart total",
            Status::ClosedNotMerged,
        );
        t.flags.nothing_to_merge = Some("Already fixed in SHOP-31.".into());
        let vm = card(&t);
        assert_eq!(flag_of(&vm), Some("Closed"));
        assert_eq!(vm.hover.status, "Closed \u{b7} nothing to merge");
        assert_eq!(
            line(&vm),
            Some("Nothing to merge \u{b7} already fixed in SHOP-31")
        );
        assert_eq!(
            nothing_to_merge_line("PR #12 already did it"),
            "Nothing to merge \u{b7} PR #12 already did it"
        );
        assert_eq!(nothing_to_merge_line(""), "Nothing to merge");
        t.flags.nothing_to_merge = None;
        assert_eq!(line(&card(&t)), Some("closed without merging"));
    }

    /// With one tool connected the same cards show no avatar and name no tool beside the agent (d0086): the model's
    /// hover reads "Old GPT · gpt-5.4 …".
    #[test]
    fn one_tool_connected_shows_no_avatar() {
        for v in card_fixtures::tool_variants() {
            let one = CardExtras {
                several_tools: false,
                ..v.extras.clone()
            };
            let vm = CardViewModel::from(&v.ticket, &one, NOW);
            assert_eq!(vm.avatar, None, "{}", v.name);
            let two = CardViewModel::from(&v.ticket, &v.extras, NOW);
            assert_eq!(vm.flag, two.flag, "{}", v.name);
            assert_eq!(vm.footer, two.footer, "{}", v.name);
        }
        let (t, x) = card_fixtures::tool_variants()
            .into_iter()
            .find(|v| v.name == "Model unavailable")
            .map(|v| (v.ticket, v.extras))
            .unwrap();
        let vm = CardViewModel::from(
            &t,
            &CardExtras {
                several_tools: false,
                ..x
            },
            NOW,
        );
        assert_eq!(
            vm.situation.unwrap().hovers[0].title,
            "Old GPT \u{b7} gpt-5.4 is no longer in Codex\u{2019}s list"
        );
    }

    /// A hold shows only where its role waits: the Developer's in Ready for Dev, the Validator's in Dev; a needs-you
    /// state still wins the head, and the user's own move says nothing; a move says nothing once it no longer runs.
    #[test]
    fn holds_and_moves_show_where_they_apply() {
        use otaq_core::agent_tools::Tool;
        use otaq_core::agents::HoldReason as H;
        let developer = card_fixtures::hold(
            Role::Developer,
            H::ToolUnavailable,
            Tool::Codex,
            None,
            "Standard",
            None,
            false,
            false,
        );
        let x = CardExtras {
            several_tools: true,
            agents: AgentFacts {
                holds: vec![developer],
                ..card_fixtures::named("Standard")
            },
            ..main_project()
        };
        for status in [Status::Queued, Status::Held, Status::WaitingForSlot] {
            let vm = CardViewModel::from(&ticket("SHOP-36", "Cache", status), &x, NOW);
            assert_eq!(flag_of(&vm), Some(CANT_START), "{status}");
            assert_eq!(
                line(&vm),
                Some("Standard\u{2019}s model can\u{2019}t run: Codex isn\u{2019}t available"),
                "{status}"
            );
        }
        for status in [Status::Backlog, Status::Working, Status::WaitingForYou] {
            let vm = CardViewModel::from(&ticket("SHOP-36", "Cache", status), &x, NOW);
            assert_ne!(flag_of(&vm), Some(CANT_START), "{status}");
        }
        // A validator's hold while the developer's agent asks: the question wins the head, the hold joins the line.
        let validator = card_fixtures::hold(
            Role::Validator,
            H::ToolLimit,
            Tool::Copilot,
            None,
            "Verifier",
            None,
            false,
            false,
        );
        let x = CardExtras {
            agents: AgentFacts {
                holds: vec![validator],
                ..card_fixtures::named("Standard")
            },
            question: Some("Which Stripe API version?".into()),
            ..x
        };
        let vm = CardViewModel::from(&ticket("SHOP-33", "Shipping", Status::AsksYou), &x, NOW);
        assert_eq!(flag_of(&vm), Some("Asks you"));
        assert_eq!(
            line(&vm),
            Some("Which Stripe API version? \u{b7} GitHub Copilot limit")
        );
        // The held round of a validator at its tool's limit waits as "Waiting for limit reset" (T-0357): the hold's
        // words, neutral.
        let vm = CardViewModel::from(
            &ticket("SHOP-33", "Shipping", Status::WaitingForLimitReset),
            &x,
            NOW,
        );
        assert_eq!(flag_of(&vm), Some("At limit"));
        assert_eq!(line(&vm), Some("GitHub Copilot limit"));
        assert_eq!(
            vm.footer.unwrap().action.unwrap().kind,
            CardActionKind::OpenAgent
        );
        // On the fallback: said while it runs; nothing once the ticket no longer runs.
        let on = |reason| CardExtras {
            several_tools: true,
            agents: AgentFacts {
                developer_on: Some(card_fixtures::on_fallback(
                    Tool::Codex,
                    Tool::Claude,
                    reason,
                )),
                ..card_fixtures::named("Strong GPT")
            },
            ..main_project()
        };
        let working = ticket("SHOP-31", "Hotfix", Status::Working);
        assert_eq!(
            line(&CardViewModel::from(&working, &on(H::ToolUnavailable), NOW)),
            Some("On Codex \u{b7} Claude Code not available")
        );
        let review = ticket("SHOP-31", "Hotfix", Status::WaitingForYou);
        assert_eq!(
            line(&CardViewModel::from(&review, &on(H::ToolLimit), NOW)),
            None
        );
    }

    /// The swapped model changes nothing on the card (*Swapped model · not on the card*): only the avatar's hover
    /// says the model that ran.
    #[test]
    fn a_swapped_model_leaves_the_card_unchanged() {
        let v = card_fixtures::tool_variants()
            .into_iter()
            .find(|v| v.name.starts_with("Swapped"))
            .unwrap();
        let asked = CardExtras {
            validator: Some(ToolAvatarView::of(AvatarTool::Copilot, Some("gpt-5.5"))),
            ..v.extras.clone()
        };
        let ran = CardViewModel::from(&v.ticket, &v.extras, NOW);
        let would = CardViewModel::from(&v.ticket, &asked, NOW);
        assert_eq!(
            ran.avatar.as_ref().unwrap().hover,
            "GitHub Copilot \u{b7} auto"
        );
        assert_eq!(
            CardViewModel {
                avatar: None,
                ..ran
            },
            CardViewModel {
                avatar: None,
                ..would
            }
        );
    }

    /// `board.agents` reaches the card (T-0362): avatars only with two tools connected; the running session's tool
    /// and model, else a held role's tool; the head follows whoever has the turn.
    #[test]
    fn the_board_brings_tools_holds_and_fallbacks() {
        use otaq_core::agent_tools::Tool;
        use otaq_core::agents::HoldReason as H;
        use otaq_core::agents::facts::{CardAgents, RunsOnTool};
        let card = CardAgents {
            key: "SHOP-29".parse().unwrap(),
            running_developer: Some("Standard".into()),
            running_validator: Some("Verifier".into()),
            running_developer_on: Some(card_fixtures::on_fallback(
                Tool::Codex,
                Tool::Claude,
                H::ToolLimit,
            )),
            running_validator_on: Some(RunsOnTool {
                tool: Tool::Copilot,
                model: Some("auto".into()),
                passed: Some("gpt-5.5".into()),
                fallback: None,
            }),
            holds: Vec::new(),
            starts: None,
            refused_agent: None,
        };
        let board = |connected: Vec<Tool>| BoardAgents {
            agent_count: 3,
            cards: vec![card.clone()],
            connected,
        };
        let key: TicketKey = "SHOP-29".parse().unwrap();
        let x = main_project().with_agents(&board(vec![Tool::Claude, Tool::Codex]), &key);
        assert!(x.several_tools);
        assert_eq!(x.agents.developer_on, card.running_developer_on);
        assert_eq!(x.developer.as_ref().unwrap().hover, "Codex");
        assert_eq!(
            x.validator.as_ref().unwrap().hover,
            "GitHub Copilot \u{b7} auto"
        );
        let head = |status, x: &CardExtras| {
            CardViewModel::from(&ticket("SHOP-29", "Coupon field", status), x, NOW)
                .avatar
                .map(|a| a.letters)
        };
        assert_eq!(head(Status::Checking, &x).as_deref(), Some("CP"));
        assert_eq!(head(Status::Working, &x).as_deref(), Some("CX"));
        // One tool connected, or an older service that sends none: no avatar.
        for connected in [vec![Tool::Claude], Vec::new()] {
            let one = main_project().with_agents(&board(connected), &key);
            assert!(!one.several_tools);
            assert_eq!(head(Status::Checking, &one), None);
        }
        // A card the board doesn't list keeps its extras.
        let other: TicketKey = "SHOP-1".parse().unwrap();
        assert_eq!(
            main_project()
                .with_agents(&board(vec![Tool::Claude]), &other)
                .agents,
            AgentFacts::default()
        );
    }

    /// A hand-off typed in that the agent hasn't picked up (d0118): the one line says what it waits for, in the
    /// neutral tone, in place of the working line; once picked up the card reads as before.
    #[test]
    fn a_hand_off_waiting_for_the_agent_replaces_the_working_line() {
        let words = [
            (HandOffKind::SendBack, "Sent back"),
            (HandOffKind::MergeConflict, "Merge conflict"),
            (HandOffKind::MergeCheck, "Merge check failed"),
            (HandOffKind::OpenPr, "Open PR"),
            (HandOffKind::Push, "Push to PR"),
        ];
        for (kind, label) in words {
            let status = match kind {
                HandOffKind::MergeConflict | HandOffKind::MergeCheck => Status::MergeConflictFixing,
                _ => Status::Working,
            };
            let mut t = ticket("SHOP-18", "Login redirect", status);
            t.flags.hand_off = Some(kind);
            let vm = card(&t);
            let one = vm.one_line.line.as_ref().unwrap();
            assert_eq!(
                one.text,
                format!("{label} \u{b7} waiting for the agent to pick it up"),
                "{kind:?}"
            );
            assert_eq!(one.tone, Tone::Secondary, "{kind:?}");
            assert_eq!(one.dot, Palette::Queued, "{kind:?}");
            assert_eq!(vm.one_line.edge, None, "{kind:?}: not the user's turn");

            t.flags.hand_off = None;
            let one = card(&t).one_line.line.unwrap();
            assert!(
                !one.text.contains("waiting for the agent"),
                "{kind:?}: {}",
                one.text
            );
        }
        // Only while its agent is at it.
        let mut t = ticket("SHOP-18", "Login redirect", Status::WaitingForYou);
        t.flags.hand_off = Some(HandOffKind::SendBack);
        assert!(
            !card(&t)
                .one_line
                .line
                .is_some_and(|l| l.text.contains("waiting for the agent"))
        );
    }
}
