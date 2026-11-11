//! `sheet.*`: the ticket sheet (UI-04, CLS-01, CLS-03, CLS-05, CLS-06, TKT-08, SES-02, ATT-12, REV-06, GIT-04;
//! d0008, d0033, d0044, d0052, d0060, d0061, d0043; *05 · Ticket side sheet*). One view per open ticket: the header
//! (key, title, priority, status, branch, base, last signal, tokens), previous / next in the column, the action bar
//! and the "…" menu of its state, the answer options of an Asks-you question, the queued notice, the conflict strip,
//! the Start interrupted callout and the tab strip with its indicators. The buttons that talk to the service land
//! here too, so their wording (toasts, refusals) stays in Rust; the ones that open a tab or a dialog are the page's.
//!
//! From M38 the panel is the ticket sheet (SHT-01, SHT-02, T-0464): [`View::sheet`] carries its header and
//! tabs (`view_models/sheet.rs`); the rest of the view is what today's tabs and Overview's top still show until
//! T-0465, T-0466 and T-0468 move it. `sheet.move_menu` is the header's Move….

use std::time::{SystemTime, UNIX_EPOCH};

use otaq_core::board::conflicts::Conflict;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::questions::Choice;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{TicketKey, ValidationAction};
use otaq_core::agents::facts::TicketAgents;
use otaq_core::usage::TicketUsage;
use otaq_core::protocol::{
    CloseParams, CommentsParams, DeliverNowParams, DepParams, KeyParams, MoveParams,
    ProjectParams, Reply, Request, SessionListParams, SessionTarget, SetChoiceParams, SubmitByUserParams, UsageParams,
    ValidationActionParams,
};
use otaq_core::session::deliveries::Delivery;
use otaq_core::session::{SessionInfo, SessionKind};
use otaq_view::icons::IconName;
use otaq_view::key_labels::KeyLabels;
use otaq_view::keymap::KeyAction;
use otaq_view::view_models::answer::{self, AnswerOutcome};
use otaq_view::view_models::answer_options::{self, Answer, question_bar};
use otaq_view::view_models::permission_prompt::PermissionPrompt;
use otaq_view::view_models::stage_choices;
use otaq_view::view_models::tool_problems::{Callout, Tools};
use otaq_view::view_models::approve;
use otaq_view::view_models::hook_refusal as hook_refusals;
use otaq_view::view_models::build_aside::{self, BuildAsideLine};
use otaq_view::view_models::comments;
use otaq_view::view_models::card::{
    CardActionKind, TOKENS_UNKNOWN, age_label, tokens_label,
};
use otaq_view::view_models::close;
use otaq_view::view_models::flow_columns::Columns;
use otaq_view::view_models::menu::MenuItem;
use otaq_view::view_models::session_tab::{self, Dot, PartView};
use otaq_view::view_models::sheet::{self, Content, Crumb, DetailFacts, SheetTab, Step};
use otaq_view::view_models::sheet_actions::{self as actions, Action, Look};
use otaq_view::view_models::pr_dialog;
use otaq_view::view_models::priority::{self, PriorityView};
use otaq_view::view_models::queue;
use otaq_view::view_models::send_back;
use otaq_view::view_models::status::PillView;
use otaq_view::view_models::terminal_strip::{self as strip, UnsentPrompt};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use otaq_view::view_models::validation_texts;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "sheet.view" => reply(&view(app, parse(params)?).await?),
        "sheet.action" => reply(&action(app, parse(params)?).await?),
        "sheet.menu" => reply(&menu_choice(app, parse(params)?).await?),
        "sheet.priority" => reply(&set_priority(app, parse(params)?).await?),
        "sheet.close_dialog" => reply(&close_dialog(app, parse(params)?).await?),
        "sheet.close" => reply(&close_ticket(app, parse(params)?).await?),
        "sheet.submit_dialog" => reply(&submit_dialog(app, parse(params)?).await?),
        "sheet.submit" => reply(&submit(app, parse(params)?).await?),
        "sheet.mark_closed" => reply(&mark_closed(app, parse(params)?).await?),
        "sheet.answer" => reply(&answer(app, parse(params)?).await?),
        "sheet.card" => reply(&card(app, parse(params)?).await?),
        "sheet.send_again" => reply(&send_again(app, parse(params)?).await?),
        "sheet.reopen_parent" => reply(&reopen_parent(app, parse(params)?).await?),
        "sheet.remove_dependency" => reply(&remove_dependency(app, parse(params)?).await?),
        "sheet.setup_log" => reply(&setup_log(parse(params)?)),
        "sheet.answer_agent" => reply(&answer_agent(app, parse(params)?).await?),
        "sheet.set_choice" => reply(&set_choice(app, parse(params)?).await?),
        "sheet.check_again" => reply(&check_again(app, parse(params)?).await?),
        "sheet.change_agent_dialog" => reply(&change_agent_dialog(app, parse(params)?).await?),
        "sheet.move_menu" => reply(&move_menu(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn key_of(text: &str) -> Result<TicketKey, RequestError> {
    text.parse()
        .map_err(|_| RequestError::plain(format!("not a ticket key: {text}")))
}

/// What most `sheet.*` requests take: the open ticket.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetParams")]
pub struct Params {
    pub key: String,
}

/// `sheet.view`: the open ticket; `opened` on the sheet's first load of it (it opened, or j / k moved to it), when the
/// Files count is taken again (Q421).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetViewParams")]
pub struct ViewParams {
    pub key: String,
    #[serde(default)]
    pub opened: bool,
}

/// `sheet.set_choice`: the user's choice for one stage, until it starts (AGM-08, AGM-09).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetChoiceParams")]
pub struct ChoiceParams {
    pub key: String,
    /// The stage's id.
    pub stage: String,
    /// An agent's name, `skip`, or `required` on a check of yours.
    pub choice: String,
}

/// `sheet.change_agent_dialog`: the ticket, and the stage (its id) whose agent to change; without one, the stage that
/// waits for its tool.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetChangeAgentParams")]
pub struct ChangeAgentParams {
    pub key: String,
    #[serde(default)]
    #[ts(optional)]
    pub stage: Option<String>,
}

/// A tooltip: its name and the user's key for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetHint")]
pub struct Hint {
    pub text: String,
    pub keys: Option<String>,
}

impl Hint {
    fn new(text: impl Into<String>, action: KeyAction, keys: &KeyLabels) -> Self {
        Self {
            text: text.into(),
            keys: keys.label(action),
        }
    }
}

/// The branch chip: the name shortened in the middle, the whole name on hover and for Copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetBranch")]
pub struct Branch {
    pub shown: String,
    pub full: String,
}

/// "40s ago", in the warning colour when a working agent has been silent too long (ATT-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetSignal")]
pub struct Signal {
    pub text: String,
    pub stale: bool,
}

/// "182k", or "—" with the reason when Claude Code's file can't be read (USG-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetTokens")]
pub struct Tokens {
    pub text: String,
    pub tooltip: Option<&'static str>,
}

/// A button of the action bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetButton")]
pub struct Button {
    pub action: Action,
    pub label: String,
    /// "Moves it into the first stage" on Start.
    pub tooltip: Option<&'static str>,
    pub icon: Option<IconName>,
    pub look: Look,
}

/// The dot after a tab's name (the page's `TabIndicator`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "SheetTabMark")]
pub enum Mark {
    Live,
    LiveAttention,
    Problem,
}

impl Mark {
    fn of(dot: Dot) -> Self {
        match dot {
            Dot::Waiting => Self::LiveAttention,
            Dot::Running => Self::Live,
            Dot::Problem => Self::Problem,
        }
    }
}

/// One tab of the sheet (SHT-02): its name, icon, key and dot, and what it shows (`contents`); Session's switch is
/// its `parts` (Agent · the check's agent · Run · Shell, SHT-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetTabView")]
pub struct TabView {
    pub tab: SheetTab,
    pub label: &'static str,
    pub icon: IconName,
    /// Its `SwitchTab` number: 1–4 (`g o` / `s` / `f` / `a`).
    pub number: Option<u8>,
    /// The user's first key for it (`1`).
    pub keys: Option<String>,
    /// The live dot (Session: its agents').
    pub mark: Option<Mark>,
    /// The short note after its name: Files "3" (`s25-03`).
    pub count: Option<String>,
    /// What it can show: itself, or Session's parts (the check only once one ran).
    pub contents: Vec<Content>,
    /// What it shows when the selection names none of its contents: Session the check while one runs or needs you,
    /// else its first.
    pub lands_on: Content,
    /// Session's switch: empty for the other tabs.
    pub parts: Vec<PartView>,
    /// Session of a ticket whose build stage split its work (d0133, `s30-03`): the list of its agent in charge and its
    /// helpers, each opening its terminal, in place of the switch's Agent; Run and Shell stay in `parts`.
    pub list: Option<otaq_view::view_models::pieces::SessionList>,
}

/// The sheet's frame (SHT-01): breadcrumb, the line under the title, tabs, the tab it opens on and its widths.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "SheetFrame")]
pub struct SheetFrame {
    pub crumbs: Vec<Crumb>,
    /// The line under the title: "In Dev · Strong · started by you 12 min ago" (`sheet::subtitle`).
    pub subtitle: String,
    /// The flow path under the title.
    pub steps: Vec<Step>,
    pub tabs: Vec<TabView>,
    pub opens_on: SheetTab,
    /// "Move…": the drag's columns (`sheet.move_menu`).
    pub move_label: &'static str,
    /// ⤢ and ⤡ (`e`).
    pub full_page: Hint,
    pub back: Hint,
    /// "← Board" at full page.
    pub board: &'static str,
    pub width: f32,
    pub width_beside_agent: f32,
    /// What it keeps clear on each side over the board.
    pub margin: f32,
}

/// Reopen parent / Remove dependency for a child of a closed parent (CLS-04), with the removal's confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetParentChoice")]
pub struct ParentChoice {
    pub parent: String,
    pub reopen: String,
    pub remove: String,
    pub confirm_title: String,
    pub confirm_body: String,
    pub confirm_label: &'static str,
}

/// What a Queued ticket waits for, as a sentence (T-0083).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetNotice")]
pub struct Notice {
    pub text: String,
    pub parent_closed: bool,
    pub choices: Vec<ParentChoice>,
}

/// "Conflicts with main · 2 files (…)" in the header (GIT-04), or "Merge check failed on main". A Delivery's problem
/// goes to the agent with no click (FLW-17), so the strip only says it: what moved, that nothing was merged, and
/// where it stands among the ones in a row that stop it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetConflict")]
pub struct ConflictStrip {
    pub line: String,
    /// "SHOP-6 changed README.md", when git could say.
    pub moved: Option<String>,
    /// "Nothing was merged into main. SHOP-5 returns to Review when it's fixed.", once it went back to its agent.
    pub outcome: Option<String>,
    /// "1 of 3", once it went back to its agent.
    pub count: Option<String>,
    pub count_hint: &'static str,
}

/// A refused model's callout above the bar (AGT-05; T-0345): "Claude Code couldn't run claude-fable-5-1" and the
/// tool's message in quotes; red for Start failed, turquoise for No result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetRefusal")]
pub struct Refusal {
    pub heading: String,
    pub message: String,
    pub error: bool,
}

/// The Start interrupted callout (SES-02, *16 → Start interrupted · SHOP-28 panel*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetStartProblem")]
pub struct StartProblem {
    pub heading: String,
    pub message: String,
    pub has_log: bool,
    pub show_log: &'static str,
    pub hide_log: &'static str,
    pub retry_heading: &'static str,
    pub retry_does: Vec<&'static str>,
    /// The user's hook that refused the start: its whole output under the message.
    pub hook: Option<HookOutput>,
}

/// A hook's refusal on the sheet: "Output of your post-checkout hook", a sentence above it when the hook needs one,
/// and the hook's whole output, paths under the home shortened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetHookOutput")]
pub struct HookOutput {
    pub name: String,
    pub label: String,
    pub note: Option<String>,
    pub output: String,
}

impl From<sheet::HookOutput> for HookOutput {
    fn from(h: sheet::HookOutput) -> Self {
        Self {
            name: h.name,
            label: h.label,
            note: h.note,
            output: h.output,
        }
    }
}

/// A project's git hook refused Approve & merge (the click or the automatic delivery), or a PR's commit message: "Your
/// commit-msg hook refused the merge", the step, the refused message, the hook's output in the same box as a refused
/// start's, and the way on (the bar's Edit message and retry, or Edit messages and retry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetHookRefusal")]
pub struct HookRefusal {
    pub heading: String,
    pub step: Option<String>,
    pub hook: HookOutput,
    /// The PR's commit message the commit-msg hook refused, above its output.
    pub message: Option<RefusedMessage>,
    pub way_on: String,
}

/// "The message it refused" and the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetRefusedMessage")]
pub struct RefusedMessage {
    pub label: String,
    pub text: String,
}

impl From<hook_refusals::HookRefusalView> for HookRefusal {
    fn from(v: hook_refusals::HookRefusalView) -> Self {
        Self {
            heading: v.heading,
            step: v.step,
            hook: v.hook.into(),
            message: v.message.map(|m| RefusedMessage {
                label: m.label,
                text: m.text,
            }),
            way_on: v.way_on,
        }
    }
}

/// *SHOP-32 · Validation failed 3 times*: the note box Answer the agent… opens (VAL-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetAnswerAgent")]
pub struct AnswerAgentBox {
    pub heading: &'static str,
    pub placeholder: &'static str,
    pub footer: String,
    pub cancel: &'static str,
    pub send: &'static str,
}

/// A plain confirmation's texts (Mark closed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetConfirm")]
pub struct Confirm {
    pub title: String,
    pub body: String,
    pub confirm: String,
}

/// The open ticket's sheet.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "SheetView")]
pub struct View {
    pub key: String,
    pub title: String,
    pub priority: PriorityView,
    pub priority_hint: String,
    pub priority_menu: Vec<MenuItem>,
    pub pill: PillView,
    pub branch: Option<Branch>,
    pub copy_branch: &'static str,
    pub base: String,
    pub signal: Option<Signal>,
    pub tokens: Option<Tokens>,
    /// The tickets before and after it in its column (j / k, ⌘[ / ⌘]).
    pub previous: Option<String>,
    pub next: Option<String>,
    pub close_hint: Hint,
    pub more_hint: &'static str,
    pub buttons: Vec<Button>,
    /// The "▾" beside Approve: the approve choices (Local merge / Open PR) in the `local_merge` overlay.
    pub approve_more: Option<&'static str>,
    /// Approve opens those choices too: with open recorded decisions Local merge / Open PR are only in ▾ (DEC-05).
    pub approve_opens_menu: bool,
    /// Why Auto-merge stopped, under the bar (AMG-03, AMG-04).
    pub stop_message: Option<String>,
    /// A project's git hook refused one of Otaq's own git steps: which hook, the step, its whole output and the way
    /// on, above the bar (whose primary button is that way on).
    pub hook_refusal: Option<HookRefusal>,
    /// Validation failed N times: Answer the agent…'s note box.
    pub answer_agent: Option<AnswerAgentBox>,
    /// The first parent, for View parent.
    pub parent: Option<String>,
    pub menu: Vec<MenuItem>,
    /// What Ask Noyan about this puts in Noyan's box for a new conversation: "About SHOP-19: ".
    pub ask_noyan: String,
    pub answer: Option<Answer>,
    /// A permission prompt the agent (or its checker) waits at: the command, above Answer in terminal.
    pub permission_prompt: Option<PermissionPrompt>,
    pub notice: Option<Notice>,
    pub conflict: Option<ConflictStrip>,
    pub start_problem: Option<StartProblem>,
    /// The tool refused a model (Start failed, No result): its callout above the bar, in place of `start_problem`.
    pub refusal: Option<Refusal>,
    /// The held roles (§7.2, SCH-04 `[M23]`): "Model unavailable", "Codex isn't available · logged out", "Claude
    /// Code limit reached · resets 3:45 pm", with Open agent…, Sign in….
    pub holds: Vec<Callout>,
    pub mark_closed: Option<Confirm>,
    /// The compact running pill next to the tokens while the Run is up (*13 → Card & sheet-overview-header controls*).
    pub run: Option<RunPill>,
    /// The sheet's header and tabs (SHT-01, SHT-02).
    pub sheet: SheetFrame,
    /// The ticket's agent session runs.
    pub live_agent: bool,
    /// The agent Open agent… opens (a held session's, or the one whose model the tool refused).
    pub open_agent: Option<String>,
    /// "The build was set aside to save disk · Run or Shell brings it back", or "Bringing the build back…" (d0125):
    /// on the Overview and at the top of the Run and Shell.
    pub build_aside: Option<BuildAsideLine>,
}

/// "● :4020 ↗ ▢": the Run's port, Open (only with an Open URL) and Stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetRunPill")]
pub struct RunPill {
    pub port: String,
    pub url: Option<String>,
    pub open_hint: String,
    pub stop_hint: &'static str,
}

/// What `sheet.view` answers: the view, or "SHOP-25 is gone." once the ticket was deleted.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "SheetLoad")]
pub struct Load {
    pub view: Option<View>,
    pub gone: String,
}

/// What the view is made of, gathered from the service (one ticket, its column's neighbours, its sessions).
pub struct Facts<'a> {
    pub ticket: &'a Ticket,
    pub tickets: &'a [Ticket],
    pub sessions: &'a [SessionInfo],
    pub conflict: Option<&'a Conflict>,
    pub has_log: bool,
    /// Pending line comments: "Send back (2) · to Dev…" (T-0226).
    pub pending: usize,
    /// The stage a human check's Send back returns it to (d0131, `send_back::back_stage`): "Send back · to Product
    /// spec…" (d0093); `None` outside a human check.
    pub send_back_to: Option<&'a str>,
    /// The ticket's tokens with the validators' share (`usage.summary`).
    pub usage: Option<&'a TicketUsage>,
    /// The project's Validation rounds (for Answer the agent).
    pub rounds: u32,
    pub now: i64,
    /// The ticket's Run (T-0232): the header's running pill and the Run tab's marker.
    pub run: Option<&'a otaq_core::terminals::run::RunInfo>,
    /// Open recorded decisions Approve keeps: "Approve · keeps N decisions" (DEC-05, T-0287).
    pub decisions: u32,
    /// The rows and the sessions' named agents (`ticket.agents`), with the agent tools and their model lists.
    pub agents: Option<&'a TicketAgents>,
    pub tools: &'a Tools,
    /// The user's keys: the tabs', Expand's and the arrows' tooltips (KEY-08).
    pub keys: KeyLabels,
    /// Its project's flow isn't the default's shape: a human check's Approve is its Pass (FLW-14), Send back with PR
    /// comments (FLW-16).
    pub custom_flow: bool,
    /// It stands in an agent check its flow marks skippable (`otaq_core::flow::engine::user_may_move_past`): only
    /// there do Skip this check and Move on anyway after no result show. A check out of rounds offers Move on anyway
    /// on every check.
    pub skippable_check: bool,
    /// Its project's name: the breadcrumb (SHT-01).
    pub project_name: &'a str,
    /// The Files tab's count (Q421): "Files 3"; `None` with no changes or before it started.
    pub files_count: Option<u32>,
    /// The ticket's helpers (d0133): the header "Dev · 2 helpers working" and Session's list.
    pub helpers: &'a [otaq_core::helpers::Helper],
}

fn running_agent(sessions: &[SessionInfo]) -> bool {
    sessions
        .iter()
        .any(|s| s.kind == SessionKind::Agent && s.is_running())
}

/// The callout of an agent whose tool stopped on an error (red, with why and what to do), on the tool of its latest
/// agent session; `None` for any other ticket.
fn stopped_callout(f: &Facts) -> Option<Callout> {
    let error = otaq_core::signals::stopped_on_error(f.ticket)?;
    let tool = f
        .sessions
        .iter()
        .filter(|s| s.kind == SessionKind::Agent)
        .max_by_key(|s| s.started_at)
        .and_then(|s| s.tool)
        .or_else(|| f.tools.connected.first().copied())
        .unwrap_or(otaq_core::agent_tools::Tool::ALL[0]);
    Some(otaq_view::view_models::tool_problems::stopped_callout(
        tool, error, f.tools,
    ))
}

/// The permission prompt the ticket waits at, named by the agent of the session that asks: the checker's for a
/// checker's prompt, else the ticket's agent's ("Developer wants to run a command").
fn permission_prompt_of(f: &Facts) -> Option<PermissionPrompt> {
    // A session's prompt shown without changing the ticket's status shows as any prompt, named by that session's
    // agent.
    if let Some(waiting) = f.ticket.as_needs_you() {
        let session = f.ticket.session_prompt().map(|p| p.session.as_str());
        let agent = f
            .sessions
            .iter()
            .find(|s| Some(s.id.as_str()) == session)
            .and_then(|s| s.agent.as_ref())
            .map(|a| a.name.as_str());
        return otaq_view::view_models::permission_prompt::of(&waiting, agent);
    }
    let kind = if f.ticket.status == otaq_core::domain::Status::CheckerNeedsYou {
        SessionKind::Validator
    } else {
        SessionKind::Agent
    };
    let agent = f
        .sessions
        .iter()
        .filter(|s| s.kind == kind)
        .max_by_key(|s| s.started_at)
        .and_then(|s| s.agent.as_ref())
        .map(|a| a.name.as_str());
    otaq_view::view_models::permission_prompt::of(f.ticket, agent)
}

fn answer_of(ticket: &Ticket) -> Option<Answer> {
    if ticket.status != otaq_core::domain::Status::AsksYou || ticket.flags.your_turn {
        return None;
    }
    question_bar(ticket.question.as_ref()?)
}

fn notice_of(ticket: &Ticket, tickets: &[Ticket]) -> Option<Notice> {
    let text = queue::sheet_notice(ticket, tickets)?;
    let choices = close::closed_parent_choices(ticket, tickets)
        .into_iter()
        .map(|c| {
            let (confirm_title, confirm_body) =
                close::remove_dependency_text(&ticket.key, &c.parent);
            ParentChoice {
                parent: c.parent.to_string(),
                reopen: c.reopen,
                remove: c.remove,
                confirm_title,
                confirm_body,
                confirm_label: "Remove dependency",
            }
        })
        .collect();
    Some(Notice {
        text,
        parent_closed: ticket.status == otaq_core::domain::Status::ParentClosed,
        choices,
    })
}

/// "Validate waits for Codex checker": a check held before its checker started (its tool lacks the skill, isn't
/// available…), as Change agent… says it; `None` when it isn't held.
fn held_check_label(f: &Facts) -> Option<String> {
    if f.ticket.status != otaq_core::domain::Status::Checking {
        return None;
    }
    let agents = f.agents?;
    let hold = agents
        .holds
        .iter()
        .find(|h| h.role == otaq_core::agents::SessionRole::Validator)?;
    let agent = hold
        .agent_id
        .as_deref()
        .and_then(|id| agents.agents.iter().find(|a| a.id == id))
        .map_or("its agent", |a| a.name.as_str());
    Some(format!("{} waits for {agent}", stage_name(f).unwrap_or("The check")))
}

pub fn view_of(f: &Facts) -> View {
    let ticket = f.ticket;
    let live = running_agent(f.sessions);
    let held_check = held_check_label(f);
    // A check held before its checker started has no checker's terminal to open.
    let no_checker = held_check.is_some();
    // Run is built here (T-0232); `actions::buttons` leaves it out.
    // In your check the review bar under every tab holds Approve and Send back: Overview doesn't repeat them.
    let in_review = otaq_view::view_models::files_tab::waits_for_you(ticket);
    // Run shows once the project has a run command (or a Run is up).
    let runnable = session_tab::runnable(f.run);
    let hook_refusal = otaq_view::view_models::card::hook_refusal(ticket)
        .map(|r| HookRefusal::from(hook_refusals::view(r, ticket.base.branch())));
    let buttons: Vec<Button> = actions::all_buttons_in(ticket, f.custom_flow, f.skippable_check)
        .into_iter()
        .filter(|(a, _)| a.arrives_with().is_none() || *a == Action::Run)
        .filter(|(a, _)| !(*a == Action::OpenValidator && no_checker))
        .filter(|(a, _)| {
            !(in_review && matches!(a, Action::Approve | Action::Pass | Action::SendBack))
        })
        .filter(|(a, _)| *a != Action::Run || runnable)
        .map(|(action, look)| Button {
            action,
            label: match action {
                Action::SendBack => f.send_back_to.map_or_else(
                    || comments::send_back_label(f.pending),
                    |to| send_back::button_label(to, f.pending),
                ),
                // To deliver's Approve is the Delivery's click, not a human check's (d0093).
                Action::Approve if ticket.status == otaq_core::domain::Status::ToDeliver => {
                    action.label().to_owned()
                }
                Action::Pass | Action::Approve => actions::approve_label(f.decisions),
                Action::ViewParent => ticket.depends_on.first().map_or_else(
                    || action.label().to_owned(),
                    |p| format!("View parent {p}"),
                ),
                _ => action.label().to_owned(),
            },
            tooltip: action.tooltip(),
            icon: action.icon(),
            look,
        })
        .collect();
    let has_branch = ticket.work.as_ref().is_some_and(|w| w.branch.is_some());
    let signal = ticket.flags.last_signal_at.map(|at| Signal {
        text: format!("{} ago", age_label(f.now - at)),
        stale: otaq_core::signals::is_stale(ticket.status, Some(at), f.now),
    });
    let tokens = match (ticket.flags.tokens, ticket.flags.tokens_unknown) {
        (Some(t), _) => Some(Tokens {
            text: tokens_label(t),
            tooltip: None,
        }),
        (None, true) => Some(Tokens {
            text: "\u{2014}".to_owned(),
            tooltip: Some(TOKENS_UNKNOWN),
        }),
        (None, false) => None,
    };
    let refusal = actions::refusal(ticket, f.agents).map(|r| Refusal {
        heading: r.heading,
        message: r.message,
        error: r.error,
    });
    let start_problem = sheet::start_problem(ticket).filter(|_| refusal.is_none()).map(|p| StartProblem {
        heading: p.heading,
        message: p.message,
        has_log: f.has_log,
        show_log: "View the setup log",
        hide_log: "Hide the setup log",
        retry_heading: "RETRY START DOES",
        retry_does: sheet::RETRY_START_DOES.to_vec(),
        hook: p.hook.map(HookOutput::from),
    });
    let mark_closed = (ticket.status == otaq_core::domain::Status::PrOpen)
        .then_some(ticket.flags.pr_number)
        .flatten()
        .map(|n| {
            let (title, body) = pr_dialog::mark_closed_confirm(n);
            Confirm {
                title,
                body: body.to_owned(),
                confirm: Action::MarkClosed.label().to_owned(),
            }
        });
    View {
        key: ticket.key.to_string(),
        title: ticket.title.clone(),
        priority: priority::view(ticket.priority),
        priority_hint: "Priority".to_owned(),
        priority_menu: priority::menu_items(ticket.priority),
        pill: {
            // One badge on every tab: the ticket's own status, even while its terminal has the keyboard (being there
            // holds back only the alert), as its card. A Delivery stop that kept it in Review, or a session's prompt
            // shown without changing its status, reads as its card does.
            let mut pill = PillView::of(otaq_view::view_models::status::shown(ticket));
            // Its tool stopped on an error: red with why, as its card ("Stopped · login expired").
            if let Some(error) = otaq_core::signals::stopped_on_error(ticket) {
                pill = PillView {
                    palette: otaq_view::theme::Palette::Error,
                    icon: IconName::CircleX,
                    label: otaq_view::view_models::card_line::stopped_line(error),
                };
            }
            if let Some(label) = validation_texts::pill_label(ticket, f.custom_flow) {
                pill.label = label;
            }
            if let Some(label) = held_check.clone() {
                pill.label = label;
            }
            // A build stage that split its work (d0133, `s30-03`): "Dev · 2 helpers working".
            if let Some(label) = stage_name(f).and_then(|stage| {
                otaq_view::view_models::pieces::header(
                    &otaq_view::view_models::pieces::Facts::new(ticket, f.helpers),
                    stage,
                )
            }) {
                pill.label = label;
            }
            pill
        },
        branch: sheet::branch_name(ticket).map(|full| Branch {
            shown: sheet::middle_truncate(&full, 28),
            full,
        }),
        copy_branch: "Copy branch name",
        base: ticket.base.branch().to_owned(),
        signal,
        tokens,
        previous: sheet::neighbour(f.tickets, &ticket.key, -1).map(|k| k.to_string()),
        next: sheet::neighbour(f.tickets, &ticket.key, 1).map(|k| k.to_string()),
        close_hint: Hint::new("Close", KeyAction::CloseSheet, &f.keys),
        more_hint: "More",
        approve_more: buttons
            .iter()
            .any(|b| b.action == Action::Approve)
            .then_some("Approve choices"),
        approve_opens_menu: actions::approve_opens_menu(f.decisions),
        // A hook's refusal says it all in its own callout.
        stop_message: validation_texts::stop_message(ticket).filter(|_| hook_refusal.is_none()),
        hook_refusal,
        answer_agent: validation_texts::answer_agent_text(ticket, f.rounds).map(|t| AnswerAgentBox {
            heading: t.heading,
            placeholder: t.placeholder,
            footer: t.footer,
            cancel: t.cancel,
            send: t.send,
        }),
        buttons,
        parent: ticket.depends_on.first().map(ToString::to_string),
        menu: actions::menu(ticket, has_branch, f.skippable_check),
        ask_noyan: actions::ask_noyan_draft(&ticket.key),
        answer: answer_of(ticket),
        permission_prompt: permission_prompt_of(f),
        notice: notice_of(ticket, f.tickets),
        conflict: conflict_strip(ticket, f.conflict),
        start_problem,
        refusal,
        holds: stopped_callout(f)
            .into_iter()
            .chain(
                f.agents
                    .map(|t| stage_choices::callouts(t, f.tools))
                    .unwrap_or_default(),
            )
            .collect(),
        mark_closed,
        run: otaq_view::view_models::run::header_pill(f.run).map(|(port, url)| RunPill {
            port: format!(":{port}"),
            open_hint: url
                .as_deref()
                .map_or_else(String::new, otaq_view::view_models::run::open_label),
            url,
            stop_hint: "Stop",
        }),
        sheet: sheet_of(f),
        live_agent: live,
        open_agent: f.agents.and_then(|t| open_agent(ticket, t)),
        build_aside: build_aside::line(ticket),
    }
}

/// The name of the stage the ticket is in, from its path (`ticket.agents`).
fn stage_name<'a>(f: &Facts<'a>) -> Option<&'a str> {
    let id = f.ticket.place.stage()?;
    f.agents?
        .path
        .iter()
        .find(|s| s.stage_id == id)
        .map(|s| s.stage.as_str())
}

/// The sheet's frame for `f`'s ticket, with Session's switch and the tabs' dots.
fn sheet_of(f: &Facts) -> SheetFrame {
    let ticket = f.ticket;
    let stages = f
        .agents
        .map(|a| {
            a.path
                .iter()
                .map(|s| (s.stage_id.clone(), s.stage.clone()))
                .collect()
        })
        .unwrap_or_default();
    let runs = f.agents.map_or(&[][..], |a| a.runs.as_slice());
    let checks = session_tab::check_sides(ticket, f.sessions, runs, &stages);
    let parts = session_tab::parts(&session_tab::Facts {
        ticket,
        sessions: f.sessions,
        checks: &checks,
        run: f.run,
    });
    let tabs = sheet::tabs(ticket)
        .into_iter()
        .map(|tab| {
            let list = match tab {
                SheetTab::Session => otaq_view::view_models::pieces::session_list_of(
                    &otaq_view::view_models::pieces::Facts::new(ticket, f.helpers),
                    session_tab::agent_session(f.sessions),
                ),
                _ => None,
            };
            let (contents, lands_on, parts, dot) = match tab {
                SheetTab::Session => (
                    {
                        // One check side per check agent, one content (d0130).
                        let mut contents: Vec<Content> = parts.iter().map(|p| p.content).collect();
                        contents.dedup();
                        contents
                    },
                    session_tab::lands_on(ticket.status),
                    parts.clone(),
                    session_tab::tab_dot(&parts),
                ),
                _ => (tab.contents().to_vec(), tab.contents()[0], Vec::new(), None),
            };
            TabView {
                tab,
                label: tab.label(),
                icon: tab.icon(),
                number: tab.number(),
                keys: tab
                    .number()
                    .and_then(|n| f.keys.label(KeyAction::SwitchTab(n))),
                mark: dot.map(Mark::of),
                count: match tab {
                    SheetTab::Files => sheet::files_count_label(f.files_count),
                    _ => None,
                },
                contents,
                lands_on,
                parts,
                list,
            }
        })
        .collect();
    SheetFrame {
        crumbs: sheet::breadcrumb(ticket, f.project_name),
        subtitle: sheet::subtitle(&DetailFacts {
            ticket,
            agents: f.agents,
            tools: f.tools,
            tickets: f.tickets,
            now: f.now,
            helpers: f.helpers,
            usage: f.usage,
        }),
        steps: sheet::steps(ticket, f.agents),
        tabs,
        opens_on: sheet::opens_on(ticket),
        move_label: sheet::MOVE,
        full_page: Hint::new(sheet::FULL_PAGE, KeyAction::ExpandSheet, &f.keys),
        back: Hint::new(sheet::BACK_TO_SHEET, KeyAction::ExpandSheet, &f.keys),
        board: sheet::BOARD,
        width: sheet::WIDTH,
        width_beside_agent: sheet::WIDTH_BESIDE_AGENT,
        margin: sheet::MARGIN,
    }
}

/// `sheet.move_menu`: the sheet's Move… (SHT-01), the board's columns with the drag's verdicts; a pick is
/// `board.drop`.
pub async fn move_menu(app: &App, params: Params) -> Result<Vec<MenuItem>, RequestError> {
    let key = key_of(&params.key)?;
    let answer = super::board::drag(
        app,
        super::board::TicketParams {
            key: params.key.clone(),
        },
    )
    .await?;
    let Some(ticket) = ticket(app, &params.key).await? else {
        return Ok(Vec::new());
    };
    let (flow, agents) = tokio::join!(
        super::board::flow(app, key.project.as_str()),
        super::board::agents(app, key.project.as_str())
    );
    let columns = Columns::of(&flow).knowing(agents.as_ref());
    Ok(sheet::move_menu(&columns, &ticket, &answer))
}

async fn ticket(app: &App, key: &str) -> Result<Option<Ticket>, RequestError> {
    match app
        .service
        .send(Request::TicketGet(KeyParams { key: key.to_owned() }))
        .await
    {
        Ok(Reply::Ticket(t)) => Ok(Some(*t)),
        Ok(other) => Err(unexpected("ticket.get", &other)),
        Err(e) if e.not_found => Ok(None),
        Err(e) => Err(e),
    }
}

pub(super) async fn tickets(app: &App, project: &str) -> Result<Vec<Ticket>, RequestError> {
    match app
        .service
        .send(Request::TicketList(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::Tickets(list) => Ok(list),
        other => Err(unexpected("ticket.list", &other)),
    }
}

async fn sessions(app: &App, key: &str) -> Result<Vec<SessionInfo>, RequestError> {
    match app
        .service
        .send(Request::SessionList(SessionListParams {
            key: Some(key.to_owned()),
        }))
        .await?
    {
        Reply::Sessions(list) => Ok(list),
        other => Err(unexpected("session.list", &other)),
    }
}

/// The ticket with the rest of its project, or a "gone" refusal.
async fn ticket_and_project(app: &App, key: &str) -> Result<(Ticket, Vec<Ticket>), RequestError> {
    let Some(ticket) = ticket(app, key).await? else {
        return Err(RequestError::plain(format!("{key} is gone.")));
    };
    let all = tickets(app, &ticket.key.project.to_string()).await?;
    Ok((ticket, all))
}

fn setup_log_path(key: &str) -> Option<std::path::PathBuf> {
    otaq_core::data_dir::DataDir::from_env()
        .ok()
        .and_then(|d| d.setup_log_path(key).ok())
        .filter(|p| p.is_file())
}

/// The Files tab's count (Q421): kept from the last time it was taken unless the sheet just opened on the ticket or its
/// status moved on since (its agent's turn ended); only then does it ask git (`files.changes`). Never before the ticket
/// started. The sheet doesn't wait for git (a big worktree takes seconds): it shows the last count while the next is
/// taken in the background, and loads again (`files_counted`) once a different one is in.
fn files_count(app: &App, ticket: &Ticket, opened: bool) -> Option<u32> {
    let counts = app.files_counts.clone();
    let lock = || counts.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(kept) = lock().kept(ticket, opened) {
        return kept;
    }
    if !sheet::counts_files(ticket) {
        lock().keep(ticket, None);
        return None;
    }
    let shown = lock().last(ticket);
    if !lock().start(ticket) {
        return shown;
    }
    // On a connection of its own: the shared one answers one request at a time, and the Overview's and Activity's
    // requests, sent as the sheet shows, would wait behind git.
    let service = app.service.alone();
    let events = app.events.clone();
    let ticket = ticket.clone();
    tokio::spawn(async move {
        let key = ticket.key.to_string();
        let count = match service
            .send(Request::FilesChanges(KeyParams { key: key.clone() }))
            .await
        {
            Ok(Reply::FilesChanges(changes)) => sheet::files_count(&changes),
            _ => None,
        };
        counts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keep(&ticket, count);
        if count != shown {
            events.publish(&crate::events::PageEvent::Service {
                name: "files_counted".to_owned(),
                event: serde_json::json!({ "kind": "files_counted", "key": key }),
            });
        }
    });
    shown
}

/// The conflict strip on Overview: a conflict says what conflicts and what moved, a failed merge check says so in its
/// own words. Once it went back to its agent (it counts in a row), it also says nothing was merged and the count.
fn conflict_strip(ticket: &Ticket, conflict: Option<&Conflict>) -> Option<ConflictStrip> {
    let check_failed = ticket.flags.merge_check == Some(otaq_core::domain::MergeCheckState::Failed);
    let target = conflict.map_or_else(|| ticket.base.branch().to_owned(), |c| c.target.clone());
    let (line, moved) = if check_failed {
        (format!("Merge check failed on {target}"), None)
    } else {
        let c = conflict.filter(|_| ticket.flags.conflicts_with.is_some())?;
        (approve::conflict_line(c), approve::moved_line(c))
    };
    let went_back = ticket.auto_merge_conflicts > 0
        && !matches!(
            ticket.status,
            otaq_core::domain::Status::WaitingForYou
                | otaq_core::domain::Status::ToDeliver
                | otaq_core::domain::Status::PrOpen
        );
    Some(ConflictStrip {
        line,
        moved,
        outcome: went_back.then(|| approve::nothing_merged_line(&ticket.key, &target)),
        count: went_back.then(|| approve::in_a_row(ticket.auto_merge_conflicts)),
        count_hint: approve::IN_A_ROW_HINT,
    })
}

pub async fn view(app: &App, params: ViewParams) -> Result<Load, RequestError> {
    let gone = format!("{} is gone.", params.key);
    let Some(ticket) = ticket(app, &params.key).await? else {
        return Ok(Load { view: None, gone });
    };
    let all = tickets(app, &ticket.key.project.to_string()).await?;
    let sessions = sessions(app, &params.key).await?;
    let conflict = if ticket.flags.conflicts_with.is_some()
        || ticket.flags.merge_check == Some(otaq_core::domain::MergeCheckState::Failed)
    {
        match app
            .service
            .send(Request::TicketConflict(KeyParams {
                key: params.key.clone(),
            }))
            .await
        {
            Ok(Reply::Conflict(c)) => c,
            _ => None,
        }
    } else {
        None
    };
    // The Run, once there is a worktree to run in (T-0232).
    let run = if ticket.work.as_ref().is_some_and(|w| w.worktree.is_some()) {
        match app
            .service
            .send(Request::RunApps(KeyParams {
                key: params.key.clone(),
            }))
            .await
        {
            // The pill shows the first app that is up, else the first app (agents run the app).
            Ok(Reply::Runs(all)) => {
                let live = all.iter().position(|r| r.state.is_live()).unwrap_or(0);
                all.into_iter().nth(live)
            }
            _ => None,
        }
    } else {
        None
    };
    let has_log = ticket.status == otaq_core::domain::Status::StartInterrupted
        && setup_log_path(&params.key).is_some();
    let pending = if ticket.status == otaq_core::domain::Status::WaitingForYou {
        match app
            .service
            .send(Request::ReviewComments(CommentsParams {
                key: params.key.clone(),
                round: None,
            }))
            .await
        {
            Ok(Reply::ReviewComments(list)) => list.pending as usize,
            _ => 0,
        }
    } else {
        0
    };
    let usage = if ticket.started_at.is_some() {
        ticket_usage(app, &ticket).await
    } else {
        None
    };
    let rounds = if ticket.status == otaq_core::domain::Status::CheckFailed {
        check_rounds(app, &ticket).await
    } else {
        0
    };
    let decisions = super::review::open_decisions(app, &ticket).await;
    let agents = super::editor::agent_facts(app, super::editor::of_ticket(&params.key)).await;
    let tools = super::editor::agent_tools(app).await;
    let keys = super::keymap::labels(app).await;
    let flow = super::board::flow(app, ticket.key.project.as_str()).await;
    let custom_flow = !otaq_view::view_models::flow_columns::keeps_default_look(&flow);
    let skippable_check = otaq_core::flow::engine::user_may_move_past(&flow, &ticket.place);
    let send_back_to = send_back::available(&ticket)
        .then(|| super::review::send_back_stage(&ticket, &flow, agents.as_ref()))
        .flatten();
    let project_name = project_name(app, ticket.key.project.as_str()).await;
    let files_count = files_count(app, &ticket, params.opened);
    let helpers: Vec<otaq_core::helpers::Helper> = if ticket.started_at.is_some() {
        super::board::helpers(app, ticket.key.project.as_str())
            .await
            .into_iter()
            .filter(|h| h.ticket == ticket.key)
            .collect()
    } else {
        Vec::new()
    };
    let view = view_of(&Facts {
        ticket: &ticket,
        tickets: &all,
        sessions: &sessions,
        conflict: conflict.as_ref(),
        has_log,
        pending,
        send_back_to: send_back_to.as_deref(),
        usage: usage.as_ref(),
        rounds,
        now: now_ms(),
        run: run.as_ref(),
        decisions,
        agents: agents.as_ref(),
        tools: &tools,
        keys,
        custom_flow,
        skippable_check,
        project_name: &project_name,
        files_count,
        helpers: &helpers,
    });
    Ok(Load {
        view: Some(view),
        gone,
    })
}

/// The project's name ("shop-web"), else its key.
async fn project_name(app: &App, project: &str) -> String {
    match app.service.send(Request::ProjectList).await {
        Ok(Reply::Projects(list)) => list
            .into_iter()
            .find(|p| p.key == project)
            .map_or_else(|| project.to_owned(), |p| p.name),
        _ => project.to_owned(),
    }
}

/// The ticket's row of `usage.summary`; `None` when it can't be read (the header then shows no split).
async fn ticket_usage(app: &App, ticket: &Ticket) -> Option<TicketUsage> {
    match app
        .service
        .send(Request::UsageSummary(UsageParams {
            project: Some(ticket.key.project.to_string()),
        }))
        .await
    {
        Ok(Reply::Usage(summary)) => summary.tickets.into_iter().find(|u| u.id == ticket.id),
        _ => None,
    }
}

/// The rounds of the agent check the ticket counts (its project's flow; d0063), without the extra rounds.
pub(super) async fn check_rounds(app: &App, ticket: &Ticket) -> u32 {
    match app
        .service
        .send(Request::FlowCurrent(ProjectParams {
            project: ticket.key.project.to_string(),
        }))
        .await
    {
        Ok(Reply::FlowVersion(version)) => version.file().map_or(
            otaq_core::flow::check::DEFAULT_VALIDATION_ROUNDS,
            |f| f.flow.check_rounds(ticket.place.stage(), 0),
        ),
        _ => otaq_core::flow::check::DEFAULT_VALIDATION_ROUNDS,
    }
}

/// The setup log of a Start interrupted ticket, when there is one.
pub fn setup_log(params: Params) -> Option<String> {
    setup_log_path(&params.key).and_then(|p| std::fs::read_to_string(p).ok())
}

fn toast_error(app: &App, title: String, error: &RequestError) {
    app.notices.toast(NewToast::new(
        ToastLevel::Error,
        title,
        Some(error.message.clone()),
    ));
}

/// What Resume says when the agent's terminal has a line the user typed: it is never cleared without asking.
pub const RESUME_UNSENT: &str =
    "Its terminal has a line you typed. Send or clear it there, then Resume again.";

/// Resume on an agent whose tool stopped on an error: "continue" typed into its session as the user's click, so it
/// goes on where it stopped (resumed first when a restart ended it meanwhile).
async fn resume_after_error(app: &App, key: &str) -> Result<(), RequestError> {
    let failed = format!("{key} can\u{2019}t be resumed");
    let choice = Choice::Other(otaq_core::usage::limit::CONTINUE.to_owned());
    match app
        .service
        .send(answer::request(&key_of(key)?, &choice, false))
        .await
    {
        Ok(Reply::Delivery(Delivery::Sent { .. })) => Ok(()),
        Ok(Reply::Delivery(_)) => {
            let e = RequestError::plain(RESUME_UNSENT);
            toast_error(app, failed, &e);
            Err(e)
        }
        Ok(other) => Err(unexpected("needs_you.answer", &other)),
        Err(e) => {
            toast_error(app, failed, &e);
            Err(e)
        }
    }
}

/// What `sheet.action` takes: a button that talks to the service.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetActionParams")]
pub struct ActionParams {
    pub key: String,
    pub action: Action,
}

/// Start, Retry start, Resume, Retry, Reopen, Mark merged and Open PR in browser. A refusal shows as a toast (and
/// rejects, so the button stops spinning). The other buttons open a tab or a dialog on the page.
pub async fn action(app: &App, params: ActionParams) -> Result<(), RequestError> {
    let key = params.key;
    if params.action == Action::Resume
        && ticket(app, &key)
            .await?
            .is_some_and(|t| otaq_core::signals::stopped_on_error(&t).is_some())
    {
        return resume_after_error(app, &key).await;
    }
    let k = KeyParams { key: key.clone() };
    let (request, failed) = match params.action {
        Action::Start => (
            Request::TicketMove(MoveParams {
                key: key.clone(),
                target: DropTarget::Dev,
                confirmed: false,
            }),
            format!("{key} can\u{2019}t start"),
        ),
        Action::RetryStart => (
            Request::TicketRetryStart(k),
            format!("{key}\u{2019}s start can\u{2019}t be retried"),
        ),
        Action::Resume => (
            Request::TicketResume(k),
            format!("{key} can\u{2019}t be resumed"),
        ),
        Action::RetryResume => (
            Request::TicketRetryResume(k),
            format!("{key} can\u{2019}t be resumed"),
        ),
        Action::Reopen => (
            Request::TicketReopen(k),
            format!("{key} can\u{2019}t be reopened"),
        ),
        Action::MarkMerged => (
            Request::TicketMarkMerged(k),
            format!("Mark merged of {key} failed"),
        ),
        Action::MoveOnAnyway => (
            validation_request(&key, ValidationAction::MoveOnAnyway, ""),
            format!("{key} can\u{2019}t be moved on"),
        ),
        Action::AnotherRound => (
            validation_request(&key, ValidationAction::AnotherRound, ""),
            format!("{key} can\u{2019}t get another round"),
        ),
        Action::RetryValidation => (
            validation_request(&key, ValidationAction::Retry, ""),
            format!("{key}\u{2019}s validation can\u{2019}t be retried"),
        ),
        Action::RetryMerge => (
            validation_request(&key, ValidationAction::RetryMerge, ""),
            format!("{key} can\u{2019}t be merged"),
        ),
        Action::MoveToReview => (
            validation_request(&key, ValidationAction::MoveToReview, ""),
            format!("{key} can\u{2019}t be moved to Review"),
        ),
        Action::StopMergeCheck => return stop_merge_check(app, &key).await,
        Action::StopGitStep => return stop_git_step(app, &key).await,
        Action::OpenPrInBrowser => {
            let url = ticket(app, &key)
                .await?
                .and_then(|t| t.work)
                .and_then(|w| w.pr_url)
                .ok_or_else(|| RequestError::plain(format!("{key} has no PR")))?;
            return open::that_detached(&url)
                .map_err(|e| RequestError::plain(format!("couldn't open {url}: {e}")));
        }
        other => {
            return Err(RequestError::plain(format!(
                "{other:?} is the page's (a tab or a dialog)"
            )));
        }
    };
    match app.service.send(request).await {
        Ok(Reply::MarkedMerged(marked)) => {
            app.notices.toast(NewToast::new(
                ToastLevel::Success,
                pr_dialog::merged_toast(&key, marked.started.len()),
                None,
            ));
            Ok(())
        }
        Ok(_) => Ok(()),
        Err(e) => {
            toast_error(app, failed, &e);
            Err(e)
        }
    }
}

/// Stop on a running git step of Approve & merge or a start ("Merging · running your pre-commit hook · 3 min"): on a
/// connection of its own, since the merge or start it stops may be waiting on the shared one. The merge answers that
/// nothing was merged, a start becomes Start failed; a step that already ended is a toast.
async fn stop_git_step(app: &App, key: &str) -> Result<(), RequestError> {
    match app
        .service
        .send_alone(Request::TicketGitStop(KeyParams {
            key: key.to_owned(),
        }))
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            toast_error(app, format!("{key}\u{2019}s git step can\u{2019}t be stopped"), &e);
            Err(e)
        }
    }
}

/// Stop on "Merging · running the merge check" (d0120, Q475): on a connection of its own, since the Local merge it
/// stops may be waiting on the shared one (an automatic Delivery after a move). The merge answers that nothing was
/// merged; a check that already ended is a toast.
async fn stop_merge_check(app: &App, key: &str) -> Result<(), RequestError> {
    match app
        .service
        .send_alone(Request::TicketMergeCheckStop(KeyParams {
            key: key.to_owned(),
        }))
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            toast_error(app, format!("{key}\u{2019}s merge check can\u{2019}t be stopped"), &e);
            Err(e)
        }
    }
}

fn validation_request(key: &str, action: ValidationAction, note: &str) -> Request {
    Request::TicketValidationAction(ValidationActionParams {
        key: key.to_owned(),
        action,
        note: note.to_owned(),
    })
}

/// What `sheet.menu` takes: a "…" menu item the service carries out (Pin / Unpin, Stop this check, Skip this
/// check).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetMenuParams")]
pub struct MenuParams {
    pub key: String,
    pub item: String,
}

pub async fn menu_choice(app: &App, params: MenuParams) -> Result<(), RequestError> {
    use otaq_view::view_models::card_menu::{PIN, UNPIN};
    let k = KeyParams {
        key: params.key.clone(),
    };
    let key = params.key.clone();
    let (request, failed) = match params.item.as_str() {
        PIN => (Request::TicketPin(k), format!("{key} wasn\u{2019}t pinned")),
        UNPIN => (Request::TicketUnpin(k), format!("{key} wasn\u{2019}t unpinned")),
        actions::STOP_VALIDATION => (
            validation_request(&key, ValidationAction::Stop, ""),
            format!("{key}\u{2019}s validation can\u{2019}t be stopped"),
        ),
        actions::SKIP_TO_REVIEW => (
            validation_request(&key, ValidationAction::Skip, ""),
            format!("{key} can\u{2019}t skip to Review"),
        ),
        other => return Err(RequestError::plain(format!("not a service item: {other}"))),
    };
    match app.service.send(request).await {
        Ok(_) => Ok(()),
        Err(e) => {
            toast_error(app, failed, &e);
            Err(e)
        }
    }
}

/// What `sheet.answer_agent` takes: the user's note for the agent.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetAnswerAgentParams")]
pub struct AnswerAgentParams {
    pub key: String,
    pub note: String,
}

/// Answer the agent (VAL-07): the note goes with the last findings and the round counter restarts. A refusal
/// stays in the box (the page shows it).
pub async fn answer_agent(app: &App, params: AnswerAgentParams) -> Result<(), RequestError> {
    let note = params.note.trim();
    if note.is_empty() {
        return Err(RequestError::plain("Write what the agent should do first."));
    }
    app.service
        .send(validation_request(&params.key, ValidationAction::AnswerAgent, note))
        .await
        .map(|_| ())
}

/// What `sheet.priority` takes: the chosen row of the header's Priority menu.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetPriorityParams")]
pub struct PriorityParams {
    pub key: String,
    pub item: String,
}

/// A pick of the header's Priority menu is a `ticket.update` (TKT-09, d0065).
pub async fn set_priority(app: &App, params: PriorityParams) -> Result<(), RequestError> {
    let Some(p) = priority::from_menu_id(&params.item) else {
        return Err(RequestError::plain(format!(
            "not a priority: {}",
            params.item
        )));
    };
    let key = key_of(&params.key)?;
    match app.service.send(priority::request(&key, p)).await {
        Ok(_) => Ok(()),
        Err(e) => {
            app.notices
                .toast(NewToast::new(ToastLevel::Error, e.message.clone(), None));
            Err(e)
        }
    }
}

/// One box of the close confirmation: the running agent (amber) or a dependent ticket (red).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetCloseLine")]
pub struct CloseLine {
    pub agent: bool,
    /// activity (the agent), shield-off (agent and validator, d0063) or link-2 (a dependent).
    pub icon: IconName,
    pub text: String,
}

/// *Close ticket · confirm* (d0033): it names the running agent and the tickets that wait for this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetCloseDialog")]
pub struct CloseDialog {
    pub title: String,
    pub lines: Vec<CloseLine>,
    pub footer: String,
    pub confirm: String,
}

pub fn close_dialog_of(ticket: &Ticket, tickets: &[Ticket], running: bool) -> CloseDialog {
    let text = close::close_text(ticket, tickets, running);
    let validating = close::validating_stop_line(ticket).is_some();
    CloseDialog {
        title: text.title,
        lines: text
            .lines
            .into_iter()
            .enumerate()
            .map(|(i, text)| {
                // `close_text` puts the running agent (or agent and validator) first.
                let agent = (running || validating) && i == 0;
                CloseLine {
                    agent,
                    icon: match (agent, validating) {
                        (true, true) => IconName::ShieldOff,
                        (true, false) => IconName::Activity,
                        (false, _) => IconName::Link2,
                    },
                    text,
                }
            })
            .collect(),
        footer: text.footer,
        confirm: text.confirm,
    }
}

pub async fn close_dialog(app: &App, params: Params) -> Result<CloseDialog, RequestError> {
    let (ticket, all) = ticket_and_project(app, &params.key).await?;
    let running = running_agent(&sessions(app, &params.key).await?);
    Ok(close_dialog_of(&ticket, &all, running))
}

/// Close ticket's confirm (CLS-01): the service stops the agent first. A refusal shows inside the dialog.
pub async fn close_ticket(app: &App, params: Params) -> Result<(), RequestError> {
    app.service
        .send(Request::TicketClose(CloseParams {
            key: params.key,
            confirm: true,
        }))
        .await
        .map(|_| ())
}

/// *Submit for review · dialog* (CLS-05, d0052): the user's Move to next stage (d0108 rule 10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetSubmitDialog")]
pub struct SubmitDialog {
    pub title: String,
    pub intro: String,
    pub note_label: &'static str,
    pub note_placeholder: &'static str,
    pub footer: String,
    pub confirm: &'static str,
}

pub async fn submit_dialog(app: &App, params: Params) -> Result<SubmitDialog, RequestError> {
    let Some(ticket) = ticket(app, &params.key).await? else {
        return Err(RequestError::plain(format!("{} is gone.", params.key)));
    };
    let text = close::submit_text(&ticket, now_ms());
    Ok(SubmitDialog {
        title: text.title,
        intro: text.intro,
        note_label: "Note for the review summary (optional)",
        note_placeholder: "What the reviewer should know",
        footer: text.footer,
        confirm: "Move to next stage",
    })
}

/// What `sheet.submit` takes: the note for the review summary.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetSubmitParams")]
pub struct SubmitParams {
    pub key: String,
    pub note: String,
}

/// Move to next stage: a refusal ("2 files are not committed…") shows inside the dialog.
pub async fn submit(app: &App, params: SubmitParams) -> Result<(), RequestError> {
    app.service
        .send(Request::TicketSubmitByUser(SubmitByUserParams {
            key: params.key,
            note: params.note,
        }))
        .await
        .map(|_| ())
}

/// Mark closed, once confirmed (REV-13).
pub async fn mark_closed(app: &App, params: Params) -> Result<(), RequestError> {
    let key = params.key;
    match app
        .service
        .send(Request::TicketMarkClosed(CloseParams {
            key: key.clone(),
            confirm: true,
        }))
        .await
    {
        Ok(_) => {
            let (title, text) = pr_dialog::marked_closed_toast(&key);
            app.notices.toast(NewToast::new(
                ToastLevel::Success,
                title,
                Some(text.to_owned()),
            ));
            Ok(())
        }
        Err(e) => {
            toast_error(app, format!("Mark closed of {key} failed"), &e);
            Err(e)
        }
    }
}

/// The unsent-text question (SES-07, d0048): Clear it and send, or Keep it and cancel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetUnsent")]
pub struct Unsent {
    pub title: String,
    pub body: String,
    /// "Clear it and send"; `None` for a multi-line draft the tool can't clear: only Keep it and cancel (Q-N).
    pub clear: Option<&'static str>,
    /// The other button: "Cancel", or "Keep it and cancel" when it is the only one.
    pub cancel: &'static str,
}

impl Unsent {
    pub(super) fn of(prompt: &UnsentPrompt) -> Self {
        // Over a permission prompt: it says so, and only Keep it and cancel (a clear would answer it).
        if let Some(note) = prompt.note().filter(|_| prompt.prompt.is_some()) {
            return Self {
                title: prompt.title(),
                body: note,
                clear: None,
                cancel: "Keep it and cancel",
            };
        }
        Self::asking(
            prompt.title(),
            format!("{} {}.", prompt.explanation(), prompt.sending),
            prompt.unclearable_by.as_deref(),
        )
    }

    /// The question titled `title` with `body`; a multi-line draft `unclearable_by` can't clear replaces the body
    /// with the note and drops Clear it and send: the user clears it (SES-07, Q-N).
    pub(super) fn asking(title: String, body: String, unclearable_by: Option<&str>) -> Self {
        match unclearable_by {
            Some(tool) => Self {
                title,
                body: strip::unclearable_note(tool),
                clear: None,
                cancel: "Keep it and cancel",
            },
            None => Self {
                title,
                body,
                clear: Some("Clear it and send"),
                cancel: "Cancel",
            },
        }
    }
}

/// How a send went: done, or the terminal has unsent text and the user is asked first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "SheetSent")]
pub enum Sent {
    Done,
    Ask(Unsent),
}

/// What `sheet.answer` takes: the open question, the picked number (Other… is the last), its text, and whether
/// the terminal's typed line is cleared first.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetAnswerParams")]
pub struct AnswerParams {
    pub key: String,
    pub question: String,
    pub pick: u32,
    #[serde(default)]
    pub other: String,
    #[serde(default)]
    pub clear: bool,
}

/// The choice an answer sends, by the options' own rules (`answer_options.rs`), or an open question's own words;
/// `None` when there is nothing to send.
pub fn choice_of(ticket: &Ticket, params: &AnswerParams) -> Option<Choice> {
    let question = ticket.question.as_ref().filter(|q| q.id == params.question)?;
    answer_options::choice_for(question, Some(params.pick), &params.other)
}

/// Send answer (ATT-12): typed into the session as the options word it.
pub async fn answer(app: &App, params: AnswerParams) -> Result<Sent, RequestError> {
    let key = key_of(&params.key)?;
    let Some(ticket) = ticket(app, &params.key).await? else {
        return Err(RequestError::plain(format!("{key} is gone.")));
    };
    let Some(choice) = choice_of(&ticket, &params) else {
        return Err(RequestError::plain(format!(
            "{key}\u{2019}s question has changed"
        )));
    };
    match app
        .service
        .send(answer::request(&key, &choice, params.clear))
        .await
    {
        Ok(Reply::Delivery(delivery)) => Ok(match answer::outcome(&key, &choice, &delivery) {
            AnswerOutcome::Sent => Sent::Done,
            AnswerOutcome::Ask(prompt) => Sent::Ask(Unsent::of(&prompt)),
        }),
        Ok(other) => Err(unexpected("needs_you.answer", &other)),
        Err(e) => {
            toast_error(app, format!("The answer to {key} wasn\u{2019}t sent"), &e);
            Err(e)
        }
    }
}

/// What `sheet.card` takes: a board card's button (T-0220) the sheet carries out once it shows the ticket, and
/// whether the terminal's typed line is cleared first (Deliver now).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetCardParams")]
pub struct CardParams {
    pub key: String,
    pub kind: CardActionKind,
    #[serde(default)]
    pub clear: bool,
}

/// A card button that talks to the service, as the GPUI app's card buttons did: Deliver now (with the unsent-text
/// question), Resume, Retry resume, Retry start and Retry (SES-02, d0059), Reopen, Mark merged, and the validation
/// buttons (VAL-07, AMG-03, AMG-04). An answer option, Start from scratch, Push to PR and Mark closed are the page's.
pub async fn card(app: &App, params: CardParams) -> Result<Sent, RequestError> {
    let key = key_of(&params.key)?;
    let panel_action = |action| ActionParams {
        key: params.key.clone(),
        action,
    };
    let validation = |action| {
        Request::TicketValidationAction(ValidationActionParams {
            key: params.key.clone(),
            action,
            note: String::new(),
        })
    };
    use otaq_core::domain::ValidationAction as V;
    let (request, failed) = match params.kind {
        CardActionKind::Resume => return action(app, panel_action(Action::Resume)).await.map(|()| Sent::Done),
        CardActionKind::RetryResume => {
            return action(app, panel_action(Action::RetryResume)).await.map(|()| Sent::Done);
        }
        CardActionKind::RetryStart | CardActionKind::Retry => {
            return action(app, panel_action(Action::RetryStart)).await.map(|()| Sent::Done);
        }
        CardActionKind::Reopen => return action(app, panel_action(Action::Reopen)).await.map(|()| Sent::Done),
        CardActionKind::MarkMerged => {
            return action(app, panel_action(Action::MarkMerged)).await.map(|()| Sent::Done);
        }
        CardActionKind::StopMergeCheck => return stop_merge_check(app, &params.key).await.map(|()| Sent::Done),
        CardActionKind::StopGitStep => return stop_git_step(app, &params.key).await.map(|()| Sent::Done),
        CardActionKind::DeliverNow => (
            strip::deliver_now_request(
                &key,
                params.clear.then_some(strip::Choice::Clear),
            )
            .ok_or_else(|| RequestError::plain("nothing to deliver"))?,
            format!("{key}\u{2019}s messages weren\u{2019}t delivered"),
        ),
        CardActionKind::RetryValidation => (
            validation(V::Retry),
            format!("{key}\u{2019}s validation can\u{2019}t be retried"),
        ),
        CardActionKind::RetryMerge => (validation(V::RetryMerge), format!("{key} can\u{2019}t be merged")),
        CardActionKind::MoveToReview => (
            validation(V::MoveToReview),
            format!("{key} can\u{2019}t be moved to Review"),
        ),
        // FLW-13: Move on anyway after the last failed round; Skip this check at the plan limit.
        CardActionKind::MoveOnAnyway => (validation(V::MoveOnAnyway), format!("{key} can\u{2019}t be moved on")),
        CardActionKind::SkipCheck => (validation(V::Skip), format!("{key}\u{2019}s check can\u{2019}t be skipped")),
        // A stage held for its skill: the finder is asked again and the stage starts if the skill is found.
        CardActionKind::CheckAgain => (
            Request::TicketCheckAgain(KeyParams { key: params.key.clone() }),
            format!("{key} can\u{2019}t be checked again"),
        ),
        other => {
            return Err(RequestError::plain(format!(
                "{other:?} is the page's (a dialog or the answer list)"
            )));
        }
    };
    match app.service.send(request).await {
        Ok(Reply::Delivery(Delivery::NeedsConfirmation { unclearable_by, .. })) => {
            let waiting = ticket(app, &params.key)
                .await?
                .map_or(1, |t| t.flags.messages_waiting);
            Ok(Sent::Ask(Unsent::of(
                &UnsentPrompt::deliver_now(key, None, waiting).unclearable_by(unclearable_by),
            )))
        }
        Ok(_) => Ok(Sent::Done),
        Err(e) => {
            toast_error(app, failed, &e);
            Err(e)
        }
    }
}

/// `sheet.check_again`: Check again on the ticket's held stage (`ticket.check_again`), from its sheet's callout. A
/// refusal shows as a toast.
pub async fn check_again(app: &App, params: Params) -> Result<(), RequestError> {
    let key = params.key;
    match app.service.send(Request::TicketCheckAgain(KeyParams { key: key.clone() })).await {
        Ok(_) => Ok(()),
        Err(e) => {
            toast_error(app, format!("{key} can\u{2019}t be checked again"), &e);
            Err(e)
        }
    }
}

/// What `sheet.send_again` takes: the ticket, and whether the terminal's typed line is cleared first.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetSendAgainParams")]
pub struct SendAgainParams {
    pub key: String,
    #[serde(default)]
    pub clear: bool,
}

/// `sheet.send_again` (d0118): the hand-off its agent hasn't picked up, typed again as the user's click; unsent text in
/// its terminal asks first (d0048).
pub async fn send_again(app: &App, params: SendAgainParams) -> Result<Sent, RequestError> {
    let key = key_of(&params.key)?;
    let request = Request::SessionSendAgain(DeliverNowParams {
        target: SessionTarget {
            key: Some(params.key.clone()),
            ..Default::default()
        },
        clear_first: params.clear,
    });
    match app.service.send(request).await {
        Ok(Reply::Delivery(Delivery::NeedsConfirmation { unclearable_by, .. })) => {
            let kind = ticket(app, &params.key)
                .await?
                .and_then(|t| t.flags.hand_off)
                .unwrap_or(otaq_core::domain::HandOffKind::SendBack);
            Ok(Sent::Ask(Unsent::of(
                &UnsentPrompt::for_click(
                    key,
                    None,
                    actions::send_again_sending(kind),
                    strip::Click::SendAgain,
                )
                .unclearable_by(unclearable_by),
            )))
        }
        Ok(_) => Ok(Sent::Done),
        Err(e) => {
            toast_error(app, format!("{key}\u{2019}s hand-off wasn\u{2019}t sent again"), &e);
            Err(e)
        }
    }
}

/// The agent Open agent… opens (AGM-03, d0108 second gap review rule 7): a held session's agent, else the agent of the
/// last session of the role whose model the tool refused (AGT-05).
fn open_agent(ticket: &Ticket, t: &TicketAgents) -> Option<String> {
    t.holds.iter().find_map(|h| h.agent_id.clone()).or_else(|| {
        let role = otaq_core::board::needs_you::change_agent(ticket)?;
        t.sessions
            .iter()
            .rev()
            .find(|s| s.role == role)
            .and_then(|s| s.agent_id.clone())
    })
}

/// `sheet.set_choice`: the user's pick in the sheet's Choices (`ticket.set_choice`); refused (a stage that started
/// meanwhile) with a toast. The sheet follows the ticket's change.
pub async fn set_choice(app: &App, params: ChoiceParams) -> Result<(), RequestError> {
    let key = params.key.clone();
    match app
        .service
        .send(Request::TicketSetChoice(SetChoiceParams {
            key: params.key,
            stage: params.stage,
            choice: params.choice,
        }))
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            toast_error(app, format!("{key}\u{2019}s choice wasn\u{2019}t changed"), &e);
            Err(e)
        }
    }
}

/// `sheet.change_agent_dialog` (Change agent…, on a card or a hold's callout): the dialog for the stage that waits for
/// its tool, offering the agents whose tool has its skill and can run now; `None` when no stage waits (it started
/// meanwhile). The pick is a `sheet.set_choice`: the service starts the waiting stage on it.
pub async fn change_agent_dialog(
    app: &App,
    params: ChangeAgentParams,
) -> Result<Option<otaq_view::view_models::stage_choices::ChangeAgentDialog>, RequestError> {
    let Some(choices) = super::editor::stage_choices_of(app, Some(&params.key), None).await else {
        return Err(RequestError::plain(format!(
            "{}\u{2019}s stage choices can\u{2019}t be read",
            params.key
        )));
    };
    let tools = super::editor::agent_tools(app).await;
    Ok(otaq_view::view_models::stage_choices::change_agent_dialog(
        &choices,
        params.stage.as_deref(),
        &tools,
    ))
}

/// What `sheet.reopen_parent` and `sheet.remove_dependency` take.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SheetParentParams")]
pub struct ParentParams {
    pub key: String,
    pub parent: String,
}

/// Reopen parent (CLS-04): the closed parent goes back to Backlog.
pub async fn reopen_parent(app: &App, params: ParentParams) -> Result<(), RequestError> {
    let parent = params.parent;
    match app
        .service
        .send(Request::TicketReopen(KeyParams {
            key: parent.clone(),
        }))
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            toast_error(app, format!("{parent} can\u{2019}t be reopened"), &e);
            Err(e)
        }
    }
}

/// Remove dependency, once confirmed.
pub async fn remove_dependency(app: &App, params: ParentParams) -> Result<(), RequestError> {
    match app
        .service
        .send(Request::DepRemove(DepParams {
            child: params.key,
            parent: params.parent,
        }))
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            toast_error(
                app,
                "The dependency wasn\u{2019}t removed".to_owned(),
                &e,
            );
            Err(e)
        }
    }
}

/// The answer options with notes for the Gallery (`gallery.option_notes`, ATT-17): the sheets and the cards of
/// NOTE-1's and NOTE-11's questions.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "OptionNotesGallery")]
pub struct NotesGallery {
    pub sheets: Vec<View>,
    pub cards: Vec<otaq_view::view_models::card_page::TicketCardView>,
}

pub fn notes_gallery() -> NotesGallery {
    use otaq_core::domain::Status;
    use otaq_view::view_models::answer_options::note_samples;
    use otaq_view::view_models::card::CardExtras;
    use otaq_view::view_models::card_fixtures::{self, NOW};
    use otaq_view::view_models::card_page::TicketCardView;

    static TOOLS: std::sync::OnceLock<Tools> = std::sync::OnceLock::new();
    let tools = TOOLS.get_or_init(|| Tools::one(None));
    let [asks, passed] = note_samples();
    let tickets: Vec<Ticket> = [
        ("NOTE-1", "Add export", asks),
        ("NOTE-11", "Invoice export", passed),
    ]
    .into_iter()
    .map(|(key, title, question)| {
        let mut t = card_fixtures::ticket(key, title, Status::AsksYou);
        t.question = Some(question);
        t
    })
    .collect();
    NotesGallery {
        sheets: tickets
            .iter()
            .map(|ticket| {
                view_of(&Facts {
                    ticket,
                    tickets: &tickets,
                    sessions: &[],
                    conflict: None,
                    has_log: false,
                    pending: 0,
                    usage: None,
                    rounds: 3,
                    now: NOW,
                    run: None,
                    decisions: 0,
                    agents: None,
                    tools,
                    keys: KeyLabels::default(),
                    custom_flow: false,
                    skippable_check: false,
                    project_name: "shop-web",
                    files_count: None,
                    send_back_to: None,
                    helpers: &[],
                })
            })
            .collect(),
        cards: tickets
            .iter()
            .map(|t| TicketCardView::of(t, &CardExtras::default(), NOW))
            .collect(),
    }
}

/// The sheet's header for the gallery (T-0464; `s25-03`, `s25-09`, `s25-11`), with two tools
/// connected: SHOP-22 needing you in Dev on Rust developer (Codex · gpt-5.5, 3 minutes, 64k tokens), SHOP-18 in your
/// check for 12 minutes, SHOP-24 working, and SHOP-41 in Backlog with Product spec's agent to pick.
pub fn sheet_gallery() -> Vec<View> {
    use otaq_core::agents::SessionRole;
    use otaq_core::board::tickets::Work;
    use otaq_core::domain::{Place, Priority, Status};
    use otaq_view::view_models::card_fixtures::{self, NOW};
    use otaq_view::view_models::stage_choices::fixtures as agents;

    const MIN: i64 = 60_000;
    static TOOLS: std::sync::OnceLock<Tools> = std::sync::OnceLock::new();
    let tools = TOOLS.get_or_init(|| {
        let mut tools = agents::tools();
        tools.connected = vec![agents::CLAUDE, agents::CODEX];
        tools
    });
    let ticket = |key: &str, title: &str, status: Status, place: Place| {
        let mut t = card_fixtures::ticket(key, title, status);
        t.place = place;
        t
    };
    let mut shop_22 = ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou, Place::InStage("dev".into()));
    shop_22.priority = Priority::High;
    shop_22.flags.tokens = Some(64_000);
    shop_22.work = Some(Work {
        branch: Some(otaq_core::naming::branch("SHOP-22")),
        ..Default::default()
    });
    let mut shop_18 = ticket("SHOP-18", "Checkout API", Status::WaitingForYou, Place::InStage("review".into()));
    shop_18.priority = Priority::Urgent;
    shop_18.status_since = Some(NOW - 12 * MIN);
    shop_18.work = Some(Work {
        branch: Some(otaq_core::naming::branch("SHOP-18")),
        ..Default::default()
    });
    let mut button = ticket("SHOP-24", "Wishlist button", Status::Working, Place::InStage("dev".into()));
    button.flags.tokens = Some(21_000);
    let shop_41 = ticket("SHOP-41", "Fix README typo", Status::Backlog, Place::Backlog);
    let tickets = vec![shop_22, shop_18, button, shop_41];
    let running = |agent: &str, tool, model: &str, minutes: i64| {
        let mut s = agents::session("s1", SessionRole::Developer, agent, tool, None, Some(model));
        s.running = true;
        s.started_at = NOW - minutes * MIN;
        s.ended_at = None;
        vec![s]
    };
    let sessions = [
        running("Rust developer", agents::CODEX, "gpt-5.5", 3),
        Vec::new(),
        running("Rust developer", agents::CLAUDE, "claude-sonnet-5-5", 8),
        Vec::new(),
    ];
    tickets[..4]
        .iter()
        .zip(sessions)
        .map(|(ticket, sessions)| {
            let facts = agents::ticket(
                &ticket.key.to_string(),
                ticket.status,
                ticket.place.clone(),
                agents::shop_path("rust"),
                agents::shop_options(),
                Vec::new(),
                sessions,
            );
            view_of(&Facts {
                ticket,
                tickets: &tickets,
                sessions: &[],
                conflict: None,
                has_log: false,
                pending: 0,
                usage: None,
                rounds: 3,
                now: NOW,
                run: None,
                decisions: 0,
                agents: Some(&facts),
                tools,
                keys: KeyLabels::default(),
                custom_flow: true,
                skippable_check: false,
                send_back_to: None,
                project_name: "shop-web",
                // `s25-03`: "Files 3" once it started.
                files_count: ticket.started_at.map(|_| 3),
                helpers: &[],
            })
        })
        .collect()
}

#[cfg(test)]
mod login_expired_tests;

#[cfg(test)]
mod tests {
    use otaq_core::board::questions::HumanQuestion;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::domain::Status;
    use otaq_core::git::testing::TempRepos;
    use otaq_core::board::projects::AddProject;
    use otaq_core::session::SessionState;
    use otaq_view::view_models::card_fixtures::ticket as fixture;
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    /// SES-07, Q-N: a multi-line draft the tool can't clear offers only Keep it and cancel, with the tool's note;
    /// otherwise both buttons and the explanation.
    #[test]
    fn the_unsent_question_drops_clear_for_a_draft_the_tool_cant_clear() {
        let key: TicketKey = "SHOP-18".parse().unwrap();
        let prompt = UnsentPrompt::for_click(
            key,
            None,
            "The address-review skill".into(),
            strip::Click::SendBack {
                note: String::new(),
            },
        );
        let both = Unsent::of(&prompt);
        assert_eq!(
            (both.clear, both.cancel),
            (Some("Clear it and send"), "Cancel")
        );
        assert!(both.body.ends_with("The address-review skill."), "{}", both.body);
        let only = Unsent::of(&prompt.clone().unclearable_by(Some("Codex".into())));
        assert_eq!((only.clear, only.cancel), (None, "Keep it and cancel"));
        assert_eq!(
            only.body,
            "Codex can\u{2019}t clear a multi-line draft. Clear it in the terminal, then send again."
        );
        let asking = Unsent::asking("Noyan".into(), "…".into(), Some("Copilot"));
        assert_eq!(asking.clear, None);
        // Over a permission prompt: it names the prompt, never "unsent text", and only Keep it and cancel.
        let over = Unsent::of(&prompt.over_prompt(Some("Run command: git push origin main".into())));
        assert_eq!(
            over.title,
            "A permission prompt is waiting in SHOP-18\u{2019}s terminal"
        );
        assert!(over.body.contains("`git push origin main`"), "{}", over.body);
        assert_eq!((over.clear, over.cancel), (None, "Keep it and cancel"));
    }

    fn agent(state: SessionState) -> SessionInfo {
        SessionInfo {
            id: "s1".into(),
            kind: SessionKind::Agent,
            project: "SHOP".into(),
            ticket: Some("SHOP-19".parse().unwrap()),
            state,
            started_at: 0,
            ended_at: None,
            end_reason: None,
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            model_ran: None,
            size: None,
        }
    }

    fn facts<'a>(
        ticket: &'a Ticket,
        tickets: &'a [Ticket],
        sessions: &'a [SessionInfo],
    ) -> Facts<'a> {
        Facts {
            ticket,
            tickets,
            sessions,
            conflict: None,
            has_log: false,
            pending: 0,
            usage: None,
            rounds: 3,
            now: 60_000,
            run: None,
            decisions: 0,
            agents: None,
            tools: one_tool(),
            keys: KeyLabels::base(otaq_view::keymap::Platform::Mac),
            custom_flow: false,
            skippable_check: false,
            project_name: "shop-web",
            files_count: None,
            send_back_to: None,
            helpers: &[],
        }
    }

    fn one_tool() -> &'static Tools {
        static ONE: std::sync::OnceLock<Tools> = std::sync::OnceLock::new();
        ONE.get_or_init(|| Tools::one(None))
    }

    /// ATT-17 (T-0375): the sheet's rows carry each option's note whole; the Gallery's long note is about 300
    /// characters, one option has none, and the passed-on question keeps its notes.
    #[test]
    fn the_answer_rows_carry_each_options_note() {
        let g = notes_gallery();
        let notes = |v: &View| -> Vec<Option<usize>> {
            v.answer
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|r| r.note.as_ref().map(|n| n.chars().count()))
                .collect()
        };
        let asks = notes(&g.sheets[0]);
        assert_eq!(asks.len(), 5, "four options and Other…");
        assert!(asks[0].is_some() && asks[1].is_some());
        assert_eq!((asks[2], asks[4]), (None, None));
        assert!((250..=320).contains(&asks[3].unwrap()), "{asks:?}");
        assert_eq!(notes(&g.sheets[1]).iter().flatten().count(), 2);
        // The answer options left the card for the sheet (BRD-03, T-0460): its one action is Answer.
        assert_eq!(g.cards[0].action.as_ref().map(|a| a.label.as_ref()), Some("Answer"));
    }

    fn labels(view: &View) -> Vec<&str> {
        view.buttons.iter().map(|b| b.label.as_str()).collect()
    }

    /// A session that ended on its tool's error: Overview says why in red and what to do next (sign in, then
    /// Resume), and the header's badge names it as the card does.
    #[test]
    fn a_tool_error_says_the_next_step_on_overview() {
        let mut t = fixture("SHOP-1", "Tidy the README", Status::Idle);
        t.signal = Some(otaq_core::board::tickets::AgentSignal {
            text: Some(otaq_core::signals::tool_error_text(
                "authentication_failed",
                Some("Login expired \u{b7} Please run /login"),
            )),
            ..Default::default()
        });
        let sessions = [agent(SessionState::Running)];
        let view = view_of(&facts(&t, std::slice::from_ref(&t), &sessions));
        assert_eq!(view.pill.label, "Stopped \u{b7} login expired");
        assert_eq!(view.pill.palette, otaq_view::theme::Palette::Error);
        let callout = &view.holds[0];
        assert_eq!(callout.tone, otaq_view::view_models::tool_problems::Tone::Error);
        assert!(callout.heading.ends_with("stopped \u{b7} login expired"));
        assert!(callout.text.as_deref().unwrap().contains(", then Resume: the agent goes on"));
        assert_eq!(labels(&view), ["Resume", "Answer in terminal"]);

        // A plain Idle keeps its turquoise answer bar and no callout.
        let idle = fixture("SHOP-1", "Tidy the README", Status::Idle);
        let view = view_of(&facts(&idle, std::slice::from_ref(&idle), &sessions));
        assert!(view.holds.is_empty());
        assert_eq!(labels(&view), ["Answer in terminal"]);
    }

    /// A ticket at a permission prompt shows the command on Overview, named by its agent, with Answer in terminal
    /// still its button (Otaq doesn't answer the prompt); a checker's prompt names the checker.
    #[test]
    fn a_permission_prompt_shows_its_command_above_answer_in_terminal() {
        use otaq_core::session::SessionAgent;
        let named = |kind: SessionKind, name: &str| {
            let mut s = agent(SessionState::Running);
            s.kind = kind;
            s.agent = Some(SessionAgent {
                id: name.to_lowercase(),
                name: name.into(),
            });
            s
        };
        let mut t = fixture("SHOP-7", "Check total for an empty list", Status::NeedsYou);
        t.signal = Some(otaq_core::board::tickets::AgentSignal {
            text: Some("Run command: npm install".into()),
            ..Default::default()
        });
        let developer = named(SessionKind::Agent, "Developer");
        let view = view_of(&facts(&t, std::slice::from_ref(&t), std::slice::from_ref(&developer)));
        let prompt = view.permission_prompt.as_ref().unwrap();
        assert_eq!(prompt.heading, "Developer wants to run a command");
        assert_eq!((prompt.text.as_str(), prompt.command), ("npm install", true));
        assert!(view.answer.is_none());
        assert_eq!(labels(&view), ["Answer in terminal"]);

        // The checker's prompt names the checker, not the Dev agent; one with no command shows the tool's words.
        let mut checker_at_prompt = t.clone();
        checker_at_prompt.status = Status::CheckerNeedsYou;
        checker_at_prompt.signal = Some(otaq_core::board::tickets::AgentSignal {
            text: Some("Claude needs your permission to use Bash".into()),
            ..Default::default()
        });
        let sessions = [developer, named(SessionKind::Validator, "Checker")];
        let view = view_of(&facts(
            &checker_at_prompt,
            std::slice::from_ref(&checker_at_prompt),
            &sessions,
        ));
        let prompt = view.permission_prompt.unwrap();
        assert_eq!(prompt.heading, "Checker wants your permission");
        assert_eq!(prompt.text, "Claude needs your permission to use Bash");
        assert!(!prompt.command);

        // A working ticket has none.
        let working = fixture("SHOP-7", "Check total for an empty list", Status::Working);
        let view = view_of(&facts(&working, std::slice::from_ref(&working), &[]));
        assert!(view.permission_prompt.is_none());
    }

    /// A check held before its checker started says it waits ("Validate waits for Codex checker", as Change agent…
    /// says it), not "Validating · round 1 of 1", and offers no checker's terminal: there is no session to open.
    /// Once a checker runs, both are back.
    #[test]
    fn a_held_check_waits_and_has_no_terminal_to_open() {
        use otaq_core::agent_tools::Tool;
        use otaq_core::agents::facts::{PathKind, PathStage};
        use otaq_core::agents::{AgentHold, HoldReason, SessionRole};
        use otaq_core::domain::{Place, ValidationRound};
        use otaq_view::view_models::agents::fixtures::agent as named_agent;
        let mut checking = fixture("SHOP-19", "Price table", Status::Checking);
        checking.place = Place::of(Status::Checking, Some("validate"), None);
        checking.flags.validating_round = Some(ValidationRound { round: 1, of: 1 });
        let all = vec![checking.clone()];
        let held = TicketAgents {
            key: Some(checking.key.clone()),
            status: Some(Status::Checking),
            agents: vec![named_agent("cx", "Codex checker", None)],
            stages: Vec::new(),
            sessions: Vec::new(),
            tokens: Vec::new(),
            holds: vec![AgentHold {
                id: "h1".into(),
                ticket_id: Some(checking.id.clone()),
                project_id: None,
                role: SessionRole::Validator,
                reason: HoldReason::MissingSkill,
                tool: Tool::Codex,
                model: None,
                agent_id: Some("cx".into()),
                skill: Some("team-validate".into()),
                since: 1,
            }],
            place: None,
            path: vec![PathStage {
                stage_id: "validate".into(),
                stage: "Validate".into(),
                kind: PathKind::AgentCheck,
                rounds: Some(1),
                delivery: None,
                agent: None,
                skipped: false,
            }],
            runs: Vec::new(),
            choices: Vec::new(),
        };
        let view = view_of(&Facts { agents: Some(&held), ..facts(&checking, &all, &[]) });
        assert_eq!(view.pill.label, "Validate waits for Codex checker");
        assert!(!labels(&view).contains(&"Open terminal \u{b7} Checker"), "{:?}", labels(&view));

        // Running: the round and the checker's terminal.
        let mut checker = agent(SessionState::Running);
        checker.kind = SessionKind::Validator;
        let running = TicketAgents { holds: Vec::new(), ..held.clone() };
        let view = view_of(&Facts {
            agents: Some(&running),
            ..facts(&checking, &all, std::slice::from_ref(&checker))
        });
        assert_eq!(view.pill.label, "Checking \u{b7} round 1 of 1");
        assert!(labels(&view).contains(&"Open terminal \u{b7} Checker"), "{:?}", labels(&view));
    }

    /// KEY-08: the sheet's tabs show `1`–`4` (the user's keys), full page `e`, Close Esc; a recorded key shows, a
    /// removed one and letters off show none.
    #[test]
    fn the_tabs_and_buttons_show_the_users_keys() {
        use otaq_view::key_labels::fixtures as keys;
        let working = fixture("SHOP-19", "Price table", Status::Working);
        let all = vec![working.clone()];
        let with = |labels: KeyLabels| view_of(&Facts { keys: labels, ..facts(&working, &all, &[]) });
        let tab_keys = |view: &View| view.sheet.tabs.iter().map(|t| t.keys.clone()).collect::<Vec<_>>();
        let base = with(keys::base());
        assert_eq!(
            tab_keys(&base),
            [Some("1".to_owned()), Some("2".to_owned()), Some("3".to_owned()), Some("4".to_owned())]
        );
        assert_eq!(base.sheet.full_page.keys.as_deref(), Some("e"));
        assert_eq!(base.close_hint.keys.as_deref(), Some("Esc"));
        let custom = with(keys::with(
            true,
            &[("switch_tab", Some(2), 0, Some("⌥2")), ("expand_sheet", None, 0, Some("x"))],
        ));
        assert_eq!(custom.sheet.tabs[1].keys.as_deref(), Some("⌥2"));
        assert_eq!(custom.sheet.full_page.keys.as_deref(), Some("x"));
        let removed = with(keys::removed("switch_tab", Some(1), 0));
        assert_eq!(removed.sheet.tabs[0].keys.as_deref(), Some("g o"), "its second key");
        assert_eq!(removed.sheet.tabs[1].keys.as_deref(), Some("2"));
        let off = with(keys::letters_off());
        assert!(tab_keys(&off).iter().all(Option::is_none));
        assert_eq!((off.sheet.full_page.keys, off.sheet.back.keys), (None, None));
    }

    /// The gallery's sheets (T-0464): SHOP-22 as drawn in `s25-03`, SHOP-18 waiting for you, SHOP-24 working.
    #[test]
    fn the_sheet_gallery_draws_the_frames() {
        let views = sheet_gallery();
        let crumbs: Vec<&str> = views[2].sheet.crumbs.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(crumbs, ["shop-web", "SHOP-24"]);
        assert_eq!(views[0].sheet.steps.len(), 5, "Delivery is no step");
    }

    #[test]
    fn the_header_and_bar_follow_the_state() {
        let mut working = fixture("SHOP-19", "Price table", Status::Working);
        working.flags.last_signal_at = Some(20_000);
        working.flags.tokens = Some(182_000);
        let all = vec![working.clone()];
        let sessions = [agent(SessionState::Running)];
        let view = view_of(&facts(&working, &all, &sessions));
        assert_eq!((view.key.as_str(), view.title.as_str()), ("SHOP-19", "Price table"));
        assert_eq!(labels(&view), ["Open agent session"], "no run command: no Run");
        assert_eq!(view.run, None, "no Run up, no pill");
        assert_eq!(view.signal.as_ref().unwrap().text, "40s ago");
        assert_eq!(view.tokens.as_ref().unwrap().text, "182k");
        // The sheet (SHT-01, SHT-02): breadcrumb, Session's live dot and its switch (SHT-04).
        let crumbs: Vec<&str> = view.sheet.crumbs.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(crumbs, ["shop-web", "SHOP-19"]);
        let tabs: Vec<&str> = view.sheet.tabs.iter().map(|t| t.label).collect();
        assert_eq!(tabs, ["Overview", "Session", "Changes", "History"]);
        assert_eq!(view.sheet.tabs[1].mark, Some(Mark::Live));
        assert_eq!(view.sheet.tabs[3].mark, None, "no count on History");
        let parts: Vec<&str> = view.sheet.tabs[1].parts.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(parts, ["Agent", "Terminal"], "no check ran: no check side; no run command: no Run");
        assert_eq!(view.sheet.tabs[1].parts[0].dot, Some(Dot::Running));
        assert_eq!(
            view.sheet.tabs[1].contents,
            [Content::Agent, Content::Shell]
        );
        assert_eq!(view.sheet.tabs[1].lands_on, Content::Agent);
        assert_eq!(view.sheet.tabs[0].contents, [Content::Overview]);
        assert_eq!(view.sheet.opens_on, SheetTab::Overview);
        assert!(view.live_agent);
        assert_eq!(view.ask_noyan, "About SHOP-19: ");
        let ids: Vec<&str> = view.menu.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"ask-noyan") && ids.contains(&close::CLOSE_TICKET));
        assert!(!ids.contains(&"copy-attach"), "the attach command is on Session's strip");
        assert_eq!(view.base, "main");

        let mut unknown = fixture("SHOP-30", "New", Status::Backlog);
        unknown.flags.tokens_unknown = true;
        let view = view_of(&facts(&unknown, std::slice::from_ref(&unknown), &[]));
        assert_eq!(labels(&view), ["Start", "Edit"]);
        assert_eq!(view.tokens.unwrap().tooltip, Some(TOKENS_UNKNOWN));
        assert_eq!(view.branch, None);
        assert!(!view.live_agent);
    }

    #[test]
    fn previous_and_next_walk_the_column() {
        let mut a = fixture("SHOP-1", "a", Status::Backlog);
        a.created_at = 1;
        let mut b = fixture("SHOP-2", "b", Status::Backlog);
        b.created_at = 2;
        let all = vec![a.clone(), b.clone()];
        let view = view_of(&facts(&a, &all, &[]));
        assert_eq!((view.previous, view.next), (None, Some("SHOP-2".into())));
    }

    #[test]
    fn a_queued_child_names_its_parent() {
        let parent = fixture("SHOP-18", "Checkout API", Status::WaitingForYou);
        let mut child = fixture("SHOP-20", "Checkout page", Status::Queued);
        child.depends_on = vec![parent.key.clone()];
        let all = vec![parent, child.clone()];
        let view = view_of(&facts(&child, &all, &[]));
        assert_eq!(labels(&view), ["Edit", "View parent SHOP-18"]);
        assert_eq!(view.parent.as_deref(), Some("SHOP-18"));
        let notice = view.notice.unwrap();
        assert!(notice.text.contains("SHOP-18"), "{}", notice.text);
        assert!(!notice.parent_closed && notice.choices.is_empty());
    }

    #[test]
    fn asks_you_with_options_shows_the_list() {
        let mut asks = fixture("SHOP-21", "Order confirmation email", Status::AsksYou);
        asks.question = Some(HumanQuestion {
            id: "q1".into(),
            text: "Which email provider?".into(),
            options: vec!["Postmark".into(), "SES".into(), "Resend".into()],
            recommended: Some(1),
            notes: Vec::new(),
            asked_at: 0,
            send_back: None,
        });
        let view = view_of(&facts(&asks, std::slice::from_ref(&asks), &[]));
        let answer = view.answer.unwrap();
        assert_eq!(answer.rows.len(), 4);
        assert!(answer.rows[0].recommended && answer.rows[0].picked);
        assert!(answer.rows[3].other);

        let pick = |pick: u32, other: &str| AnswerParams {
            key: "SHOP-21".into(),
            question: "q1".into(),
            pick,
            other: other.into(),
            clear: false,
        };
        assert_eq!(choice_of(&asks, &pick(2, "")), Some(Choice::Option(2)));
        assert_eq!(choice_of(&asks, &pick(4, "  ")), None, "Other… needs text");
        assert_eq!(
            choice_of(&asks, &pick(4, "Mailgun")),
            Some(Choice::Other("Mailgun".into()))
        );
        assert_eq!(choice_of(&asks, &pick(9, "")), None);
        let stale = AnswerParams {
            question: "old".into(),
            ..pick(1, "")
        };
        assert_eq!(choice_of(&asks, &stale), None, "a newer question wins");

        asks.flags.your_turn = true;
        let view = view_of(&facts(&asks, std::slice::from_ref(&asks), &[]));
        assert_eq!(view.answer, None, "focused in its terminal: answered there");
    }

    #[test]
    fn conflicts_show_in_review_with_the_fix() {
        let mut review = fixture("SHOP-18", "Checkout API", Status::WaitingForYou);
        let conflict = Conflict {
            target: "main".into(),
            files: vec!["a.ts".into(), "b.ts".into()],
            reason: otaq_core::board::conflicts::ConflictReason::Merge,
            moved: Vec::new(),
        };
        let all = vec![review.clone()];
        let with = |t: &Ticket| {
            view_of(&Facts {
                conflict: Some(&conflict),
                ..facts(t, &all, &[])
            })
        };
        assert_eq!(with(&review).conflict, None, "no flag: no strip");
        review.flags.conflicts_with = Some("main".into());
        let strip = with(&review).conflict.unwrap();
        assert_eq!(strip.line, "Conflicts with main \u{b7} 2 files (a.ts, b.ts)");
        assert_eq!(strip.outcome, None, "still waiting in Review: nothing went back");
        assert_eq!(strip.count, None);
    }

    /// A failed merge check and a conflict each show their own words on the badge and on Overview.
    #[test]
    fn a_failed_merge_check_and_a_conflict_read_as_themselves() {
        use otaq_core::board::conflicts::{ConflictReason, Moved};
        // The conflict the test merge found after SHOP-6 merged, back with SHOP-5's agent, the first in a row.
        let mut shop5 = fixture("SHOP-5", "Fix the word", Status::MergeConflictFixing);
        shop5.flags.conflicts_with = Some("main".into());
        shop5.flags.conflict_in_review = true;
        shop5.flags.conflict_files = vec!["README.md".into()];
        shop5.auto_merge_conflicts = 1;
        let conflict = Conflict {
            target: "main".into(),
            files: vec!["README.md".into()],
            reason: ConflictReason::TestMerge,
            moved: vec![Moved {
                by: "SHOP-6".into(),
                files: vec!["README.md".into()],
            }],
        };
        let all = vec![shop5.clone()];
        let view = view_of(&Facts {
            conflict: Some(&conflict),
            ..facts(&shop5, &all, &[])
        });
        assert_eq!(view.pill.label, "Conflicts with main \u{b7} fixing");
        let strip = view.conflict.unwrap();
        assert_eq!(strip.line, "Conflicts with main \u{b7} 1 file (README.md)");
        assert_eq!(strip.moved.as_deref(), Some("SHOP-6 changed README.md"));
        assert_eq!(
            strip.outcome.as_deref(),
            Some("Nothing was merged into main. SHOP-5 returns to Review when it\u{2019}s fixed.")
        );
        assert_eq!(strip.count.as_deref(), Some("1 of 3"));

        // The merge check failed on the result with the target merged in: no conflict, its own words.
        let mut note1 = fixture("NOTE-1", "Fix the typo", Status::MergeConflictFixing);
        note1.flags.merge_check = Some(otaq_core::domain::MergeCheckState::Failed);
        note1.auto_merge_conflicts = 2;
        let failed = Conflict {
            target: "main".into(),
            files: Vec::new(),
            reason: ConflictReason::MergeCheck,
            moved: Vec::new(),
        };
        let all = vec![note1.clone()];
        let view = view_of(&Facts {
            conflict: Some(&failed),
            ..facts(&note1, &all, &[])
        });
        assert_eq!(view.pill.label, "Merge check failed \u{b7} fixing");
        let strip = view.conflict.unwrap();
        assert_eq!(strip.line, "Merge check failed on main");
        assert_eq!(strip.moved, None);
        assert_eq!(
            strip.outcome.as_deref(),
            Some("Nothing was merged into main. NOTE-1 returns to Review when it\u{2019}s fixed.")
        );
        assert_eq!(strip.count.as_deref(), Some("2 of 3"));
    }

    #[test]
    fn a_running_run_shows_its_pill_and_a_failed_one_marks_its_tab() {
        use otaq_core::terminals::run::{RunInfo, RunState};
        let t = fixture("SHOP-19", "Price table", Status::Working);
        let mut run = RunInfo {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            key: t.key.clone(),
            title: t.title.clone(),
            app: otaq_core::board::settings::DEFAULT_APP.into(),
            state: RunState::Running,
            port: Some(4020),
            command: "pnpm dev --port $PORT".into(),
            open_url: Some("http://localhost:$PORT".into()),
            session: None,
            started_at: None,
            ended_at: None,
            exit_code: None,
            stopped_by: None,
            started_by: None,
            command_set_by: None,
        };
        let all = vec![t.clone()];
        let view = view_of(&Facts {
            run: Some(&run),
            ..facts(&t, &all, &[])
        });
        assert_eq!(
            view.run,
            Some(RunPill {
                port: ":4020".into(),
                url: Some("http://localhost:4020".into()),
                open_hint: "Open localhost:4020".into(),
                stop_hint: "Stop",
            })
        );
        assert_eq!(view.sheet.tabs[1].parts[1].dot, Some(Dot::Running));
        run.state = RunState::FailedToStart;
        let view = view_of(&Facts {
            run: Some(&run),
            ..facts(&t, &all, &[])
        });
        assert_eq!(view.run, None);
        assert_eq!(view.sheet.tabs[1].parts[1].dot, Some(Dot::Problem));
    }

    /// In your check the review bar under every tab holds Approve and Send back: Overview doesn't repeat them, and
    /// Run shows only once the project has a run command.
    #[test]
    fn overview_leaves_approve_and_send_back_to_the_review_bar() {
        let review = fixture("SHOP-18", "Checkout API", Status::WaitingForYou);
        let all = vec![review.clone()];
        for custom_flow in [false, true] {
            let view = view_of(&Facts {
                pending: 2,
                decisions: 1,
                custom_flow,
                send_back_to: Some("Dev"),
                ..facts(&review, &all, &[])
            });
            assert!(labels(&view).is_empty(), "{:?}", labels(&view));
            assert_eq!(view.approve_more, None);
        }
        use otaq_core::terminals::run::{RunInfo, RunState};
        let mut run = RunInfo {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            key: review.key.clone(),
            title: review.title.clone(),
            app: otaq_core::board::settings::DEFAULT_APP.into(),
            state: RunState::NotStarted,
            port: Some(4020),
            command: "npm run dev".into(),
            open_url: None,
            session: None,
            started_at: None,
            ended_at: None,
            exit_code: None,
            stopped_by: None,
            started_by: None,
            command_set_by: None,
        };
        let view = view_of(&Facts {
            run: Some(&run),
            ..facts(&review, &all, &[])
        });
        assert_eq!(labels(&view), ["Run"]);
        let switch = |view: &View| -> Vec<String> {
            view.sheet.tabs[1].parts.iter().map(|p| p.label.clone()).collect()
        };
        assert!(switch(&view).contains(&"Run".to_owned()), "Session's switch has Run too");
        run.command = String::new();
        let view = view_of(&Facts {
            run: Some(&run),
            ..facts(&review, &all, &[])
        });
        assert!(labels(&view).is_empty(), "no run command, no Run");
        assert!(!switch(&view).contains(&"Run".to_owned()), "nor in Session's switch");
        // Its badge is the ticket's own status, in plain words.
        assert_eq!(view_of(&facts(&review, &all, &[])).pill.label, "Ready for you");
    }

    /// *Row · validation (0063)*: the round in the header, the token split, Answer the agent's box, the Auto-merge
    /// stop message and the Approve caret.
    #[test]
    fn the_validation_states_carry_their_extras() {
        use otaq_core::domain::ValidationRound;
        use otaq_core::usage::Tokens;
        let mut t = fixture("SHOP-29", "Coupon field", Status::Checking);
        t.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
        let tokens = |input| {
            Some(Tokens {
                input,
                ..Tokens::default()
            })
        };
        let usage = TicketUsage {
            id: t.id.clone(),
            tokens: tokens(67_000),
            validation: tokens(9_000),
            rounds: 0,
            helpers: None,
        };
        let view = view_of(&Facts {
            usage: Some(&usage),
            ..facts(&t, std::slice::from_ref(&t), &[])
        });
        assert_eq!(view.pill.label, "Checking \u{b7} round 1 of 3");
        assert_eq!(labels(&view), ["Open terminal \u{b7} Checker"]);
        assert_eq!(view.menu[0].label, "Stop this check");
        assert_eq!((view.answer_agent, view.approve_more), (None, None));
        // Session's switch (SHT-04): the validator's side, running, named after its agent; Session lands on it.
        let sessions = [
            SessionInfo {
                state: SessionState::Ended,
                ..agent(SessionState::Ended)
            },
            SessionInfo {
                id: "v1".into(),
                kind: SessionKind::Validator,
                agent: Some(otaq_core::session::SessionAgent {
                    id: "validator".into(),
                    name: "Validator".into(),
                }),
                ..agent(SessionState::Running)
            },
        ];
        let view = view_of(&facts(&t, std::slice::from_ref(&t), &sessions));
        let session = &view.sheet.tabs[1];
        let parts: Vec<_> = session.parts.iter().map(|p| (p.label.as_str(), p.dot)).collect();
        assert_eq!(
            parts,
            [("Agent", None), ("Validator", Some(Dot::Running)), ("Terminal", None)]
        );
        assert_eq!(session.lands_on, Content::Check);
        assert_eq!(session.mark, Some(Mark::Live));

        let mut failed = fixture("SHOP-32", "Address autocomplete", Status::CheckFailed);
        failed.validation_round = 3;
        let view = view_of(&facts(&failed, std::slice::from_ref(&failed), &[]));
        assert_eq!(view.pill.label, "Check failed 3 times");
        // A check out of rounds is sent on whether or not the flow marks it skippable (the default flow's Validate
        // isn't).
        let ways_on = ["Move on anyway", "Give it another round", "Answer the agent\u{2026}"];
        assert_eq!(labels(&view), ways_on);
        let view = view_of(&Facts {
            skippable_check: true,
            ..facts(&failed, std::slice::from_ref(&failed), &[])
        });
        assert_eq!(labels(&view), ways_on);
        assert!(view.answer_agent.unwrap().footer.ends_with("(3 new rounds)."));

        let mut stopped = fixture("SHOP-42", "Wishlist page", Status::DeliveryStopped);
        stopped.auto_merge_stop = Some(otaq_core::domain::AutoMergeStop::CheckoutDirty);
        stopped.signal = Some(otaq_core::board::tickets::AgentSignal {
            text: Some("shop-web has uncommitted changes on main. Clean up and retry.".into()),
            ..Default::default()
        });
        let view = view_of(&facts(&stopped, std::slice::from_ref(&stopped), &[]));
        assert_eq!(labels(&view), ["Retry merge", "Move to Review"]);
        assert_eq!(
            view.stop_message.as_deref(),
            Some("shop-web has uncommitted changes on main. Clean up and retry.")
        );
        // A delivery push the remote doesn't have: the ticket stays in Review, where Retry push pushes again (Send
        // back is the review bar's).
        stopped.status = Status::WaitingForYou;
        stopped.auto_merge_stop = Some(otaq_core::domain::AutoMergeStop::PushNotLanded);
        stopped.signal = Some(otaq_core::board::tickets::AgentSignal {
            text: Some("The branch isn\u{2019}t on the remote as expected".into()),
            ..Default::default()
        });
        let view = view_of(&facts(&stopped, std::slice::from_ref(&stopped), &[]));
        assert_eq!(labels(&view), ["Retry push"]);
        assert_eq!(view.pill.label, "Delivery stopped");
        assert_eq!(
            view.stop_message.as_deref(),
            Some("The branch isn\u{2019}t on the remote as expected.")
        );

        // To deliver: Approve with its ▾ (in your check they are the review bar's).
        let deliver = fixture("SHOP-18", "Checkout API", Status::ToDeliver);
        let view = view_of(&facts(&deliver, std::slice::from_ref(&deliver), &[]));
        assert_eq!(view.approve_more, Some("Approve choices"));
    }

    /// T-0345 (AGT-05), d0108 second gap review rule 7: a refused model's callout above the bar, in place of the
    /// start callout, with Open agent… first; No result's too.
    #[test]
    fn a_refused_model_has_its_callout_and_open_agent() {
        use otaq_view::view_models::card_fixtures::agent_variants;
        let fixture = |name: &str| agent_variants().into_iter().find(|v| v.name == name).unwrap().ticket;
        let t = fixture("Start failed on the model");
        let view = view_of(&facts(&t, std::slice::from_ref(&t), &[]));
        assert_eq!(labels(&view), ["Open agent\u{2026}", "Retry start"]);
        assert_eq!(view.start_problem, None);
        let r = view.refusal.unwrap();
        assert_eq!(r.heading, "Claude Code couldn\u{2019}t run the model", "no sessions in these facts");
        assert!(r.message.starts_with('\u{201c}') && r.error);
        let t = fixture("No result \u{b7} the checker\u{2019}s model");
        let view = view_of(&facts(&t, std::slice::from_ref(&t), &[]));
        assert_eq!(labels(&view), ["Open agent\u{2026}", "Retry validation"], "Validate not skippable");
        let view = view_of(&Facts {
            skippable_check: true,
            ..facts(&t, std::slice::from_ref(&t), &[])
        });
        assert_eq!(
            labels(&view),
            ["Open agent\u{2026}", "Retry validation", "Move on anyway"]
        );
        assert!(!view.refusal.unwrap().error, "turquoise, not red");
    }

    /// AGM-03 (`s03-54`): no callout mid-flow; a held session's callout with Open agent…, which opens the held agent.
    #[test]
    fn the_panel_shows_the_holds() {
        use otaq_core::agent_tools::Tool;
        use otaq_core::agents::{AgentHold, HoldReason, SessionRole};
        use otaq_view::view_models::stage_choices::fixtures::{shop_29, tools};
        let tools = tools();
        let mid = shop_29();
        let t = fixture("SHOP-29", "Coupon field", Status::Working);
        let view = view_of(&Facts {
            agents: Some(&mid),
            tools: &tools,
            ..facts(&t, std::slice::from_ref(&t), &[])
        });
        assert!(view.holds.is_empty());
        assert_eq!(view.open_agent, None);
        let mut held = shop_29();
        held.runs.clear();
        held.holds = vec![AgentHold {
            id: "h44".into(),
            ticket_id: Some("t44".into()),
            project_id: None,
            role: SessionRole::Developer,
            reason: HoldReason::ToolUnavailable,
            tool: Tool::Claude,
            model: Some("sonnet".into()),
            agent_id: Some("spec-writer".into()),
            skill: None,
            since: 0,
        }];
        let t = fixture("SHOP-44", "Saved carts", Status::Queued);
        let view = view_of(&Facts {
            agents: Some(&held),
            tools: &tools,
            ..facts(&t, std::slice::from_ref(&t), &[])
        });
        let labels: Vec<&str> = view.holds[0].buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["Open agent\u{2026}", "Change agent\u{2026}"]);
        assert_eq!(view.open_agent.as_deref(), Some("spec-writer"));
    }

    #[test]
    fn start_interrupted_and_pr_open_have_their_extras() {
        let t = fixture("SHOP-28", "Cart badge count", Status::StartInterrupted);
        let view = view_of(&Facts {
            has_log: true,
            ..facts(&t, std::slice::from_ref(&t), &[])
        });
        assert_eq!(labels(&view), ["Retry start"]);
        let p = view.start_problem.unwrap();
        assert!(p.has_log && p.retry_does.len() == 3);
        assert_eq!(view.refusal, None);

        let mut pr = fixture("SHOP-26", "Order history", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        let view = view_of(&facts(&pr, std::slice::from_ref(&pr), &[]));
        // FLW-16 on every flow.
        assert_eq!(
            labels(&view),
            ["Open PR in browser", "Send back with PR comments", "Mark merged", "Mark closed"]
        );
        assert_eq!(view.mark_closed.unwrap().title, "Mark PR #481 as closed?");
    }

    #[test]
    fn the_close_confirmation_names_the_agent_and_the_dependents() {
        let t = fixture("SHOP-19", "Price table", Status::Working);
        let mut child = fixture("SHOP-20", "Checkout page", Status::Queued);
        child.depends_on = vec![t.key.clone()];
        let all = vec![t.clone(), child];
        let running = close_dialog_of(&t, &all, true);
        assert_eq!(running.title, "Close SHOP-19 Price table without merging?");
        assert_eq!(running.confirm, "Stop agent & close");
        assert_eq!(running.lines.len(), 2);
        assert!(running.lines[0].agent && !running.lines[1].agent);
        assert!(running.lines[1].text.starts_with("SHOP-20 Checkout page depends on it"));
        let idle = close_dialog_of(&t, &all, false);
        assert_eq!(idle.confirm, "Close ticket");
        assert!(idle.lines.iter().all(|l| !l.agent));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_panel_reads_and_changes_a_real_ticket() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 1);
        let project = match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p,
            other => panic!("{other:?}"),
        };
        let created = match h
            .app
            .service
            .send(Request::TicketCreate(NewTicketInput {
                project: project.key.clone(),
                title: "Price table".into(),
                body: String::new(),
                plan: String::new(),
                priority: None,
                base_branch: None,
                first_comment: None,
            }))
            .await
            .unwrap()
        {
            Reply::Ticket(t) => t,
            other => panic!("{other:?}"),
        };
        let key = created.key.to_string();

        let load = table(&h.app, "sheet.view", json!({ "key": key, "opened": true }))
            .await
            .unwrap();
        assert_eq!(load["view"]["title"], "Price table");
        // Not started: "Files" without a count (Q421).
        assert_eq!(load["view"]["sheet"]["tabs"][2]["label"], "Changes");
        assert_eq!(load["view"]["sheet"]["tabs"][2]["count"], Value::Null);
        assert_eq!(load["view"]["buttons"][0]["action"], "start");
        assert_eq!(load["view"]["sheet"]["tabs"][0]["tab"], "overview");
        assert_eq!(load["view"]["sheet"]["opens_on"], "overview");
        // Move… (SHT-01): the drag's columns, its own checked, Dev (the start) allowed.
        let moves = table(&h.app, "sheet.move_menu", json!({ "key": key })).await.unwrap();
        let moves = moves.as_array().unwrap();
        assert_eq!(moves[0]["id"], "backlog");
        assert_eq!(moves[0]["checked"], true);
        let dev = moves.iter().find(|m| m["id"] == "dev").unwrap();
        assert_eq!(dev["disabled"], false);

        let item = load["view"]["priority_menu"][0]["id"].as_str().unwrap().to_owned();
        table(&h.app, "sheet.priority", json!({ "key": key, "item": item }))
            .await
            .unwrap();
        table(&h.app, "sheet.menu", json!({ "key": key, "item": "pin" }))
            .await
            .unwrap();
        let load = table(&h.app, "sheet.view", json!({ "key": key })).await.unwrap();
        assert_eq!(load["view"]["priority"]["number"], 1);
        assert!(
            load["view"]["menu"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["id"] == "unpin")
        );

        let dialog = table(&h.app, "sheet.close_dialog", json!({ "key": key }))
            .await
            .unwrap();
        assert_eq!(dialog["confirm"], "Close ticket");
        table(&h.app, "sheet.close", json!({ "key": key })).await.unwrap();
        let load = table(&h.app, "sheet.view", json!({ "key": key })).await.unwrap();
        assert_eq!(load["view"]["buttons"][0]["action"], "reopen");
        table(&h.app, "sheet.action", json!({ "key": key, "action": "reopen" }))
            .await
            .unwrap();
        let load = table(&h.app, "sheet.view", json!({ "key": key })).await.unwrap();
        assert_eq!(load["view"]["buttons"][0]["action"], "start");

        assert!(
            table(&h.app, "sheet.action", json!({ "key": key, "action": "edit" }))
                .await
                .is_err(),
            "Edit is the page's"
        );

        // A card's Reopen runs through the sheet; an answer option stays the page's.
        table(&h.app, "sheet.close", json!({ "key": key })).await.unwrap();
        let sent = table(&h.app, "sheet.card", json!({ "key": key, "kind": "reopen" }))
            .await
            .unwrap();
        assert_eq!(sent["outcome"], "done");
        let load = table(&h.app, "sheet.view", json!({ "key": key })).await.unwrap();
        assert_eq!(load["view"]["buttons"][0]["action"], "start");
        assert!(
            table(
                &h.app,
                "sheet.card",
                json!({ "key": key, "kind": { "pick_option": 2 } })
            )
            .await
            .is_err(),
            "an answer option is the page's"
        );
        let gone = table(&h.app, "sheet.view", json!({ "key": "SHOP-99" }))
            .await
            .unwrap();
        assert_eq!(gone["view"], Value::Null);
        assert_eq!(gone["gone"], "SHOP-99 is gone.");
        h.service.stop_all().await;
    }

    /// d0133 (`s30-03`): a ticket whose build stage split its work shows Session as a list (the agent in charge, then a row per piece); without helpers, as today.
    #[test]
    fn a_ticket_with_helpers_lists_its_sessions() {
        use otaq_core::helpers::HelperStatus;
        use otaq_view::view_models::pieces::fixtures::mid_way;
        let mut t = otaq_view::view_models::card_fixtures::ticket(
            "SHOP-40",
            "Wishlist",
            otaq_core::domain::Status::Working,
        );
        t.plan = otaq_view::view_models::plan::fixtures::WISHLIST.to_owned();
        let all = vec![t.clone()];
        let mut session = agent(SessionState::Running);
        session.ticket = Some(t.key.clone());
        session.agent = Some(otaq_core::session::SessionAgent {
            id: "dev".into(),
            name: "Developer".into(),
        });
        let sessions = [session];
        let mut helpers = mid_way();
        helpers[2].status = HelperStatus::SentBack;
        let view = view_of(&Facts {
            helpers: &helpers,
            ..facts(&t, &all, &sessions)
        });
        let session_tab = view
            .sheet
            .tabs
            .iter()
            .find(|t| t.tab == SheetTab::Session)
            .unwrap();
        let list = session_tab.list.as_ref().expect("the Session list");
        assert_eq!(list.label, "SESSIONS · 1 IN CHARGE · 3 HELPERS");
        assert_eq!(list.in_charge.name, "Developer");
        assert_eq!(list.in_charge.status, "waiting for 40.2 and 40.3");
        assert_eq!(list.helpers.len(), 4);

        let alone = view_of(&facts(&t, &all, &sessions));
        assert!(alone.sheet.tabs.iter().all(|t| t.list.is_none()));
    }
}
