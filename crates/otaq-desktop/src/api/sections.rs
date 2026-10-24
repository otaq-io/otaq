//! `sections.*`: the ticket's sections and its Activity (UI-04, REC-03, MSG-01, MSG-03, MSG-06, EPC-01, EPC-04,
//! EPC-06, EPC-07, TKT-07; d0045, d0030, d0039, d0044, d0059; *05 · Ticket side sheet*). The Dependencies section is
//! `dependencies.rs`'s (DPV-04). The **Messages** between linked agents
//! with their states (`messages.rs`), answerable when one went to the user (Needs you); the sheet's **Activity** (SHT-05), sessions
//! with their tool session ids and events oldest first under All · Comments · Stages · Description (`history.rs`),
//! with one entry per validation round that opens
//! its report and **Create ticket** from a report's note (`validation_report.rs`, VAL-12); and the epic dialogs, Move
//! to epic… and Close epic… (`epic_actions.rs`). The epic's children are its sheet's (`epic.*`, SHT-07). Each view is
//! `null` when its section has nothing to show for the ticket.

use std::time::{SystemTime, UNIX_EPOCH};

use otaq_core::board::validation::ValidationRecord;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::TicketKey;
use otaq_core::protocol::{
    AnswerParams, CloseParams, KeyParams, MoveToEpicParams, NoteParams, ProjectParams, Reply,
    Request, SessionListParams,
};
use otaq_core::session::SessionInfo;
use otaq_view::icons::IconName;
use otaq_view::view_models::clock::local_hhmm;
use otaq_view::view_models::epic_actions::{self, EpicChoice};
use otaq_view::view_models::stage_choices;
use otaq_view::view_models::history::{self, HistoryRow, Opens};
use otaq_core::epic_lead::log::LeadLog;
use otaq_view::view_models::messages::{self, Line, Thread, Tone};
use otaq_view::view_models::ticket_comments::Mark;
use otaq_view::view_models::tool_avatar;
use otaq_view::view_models::validation_report;
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "sections.messages" => reply(&messages_view(app, parse(params)?).await?),
        "sections.answer_message" => reply(&answer_message(app, parse(params)?).await?),
        "sections.history" => reply(&history_view(app, parse(params)?).await?),
        "sections.create_from_note" => reply(&create_from_note(app, parse(params)?).await?),
        "sections.move_dialog" => reply(&move_dialog(app, parse(params)?).await?),
        "sections.move_to_epic" => reply(&move_to_epic(app, parse(params)?).await?),
        "sections.close_epic_dialog" => reply(&close_epic_dialog(app, parse(params)?).await?),
        "sections.close_epic" => reply(&close_epic(app, parse(params)?).await?),
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

fn toast(app: &App, level: ToastLevel, title: String, text: Option<String>) {
    app.notices.toast(NewToast::new(level, title, text));
}

/// What every `sections.*` view takes: the ticket the sheet shows.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SectionParams")]
pub struct Params {
    pub key: String,
}

async fn send_key(app: &App, request: Request, what: &str) -> Result<Reply, RequestError> {
    app.service
        .send(request)
        .await
        .map_err(|e| RequestError::plain(format!("{what}: {}", e.message)))
}

/// The ticket, or `None` when it is gone.
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

async fn tickets(app: &App, project: &str) -> Result<Vec<Ticket>, RequestError> {
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

/// The ticket with the rest of its project; `None` when it is gone.
async fn with_project(app: &App, key: &str) -> Result<Option<(Ticket, Vec<Ticket>)>, RequestError> {
    let Some(ticket) = ticket(app, key).await? else {
        return Ok(None);
    };
    let all = tickets(app, &ticket.key.project.to_string()).await?;
    Ok(Some((ticket, all)))
}

async fn found(app: &App, key: &str) -> Result<(Ticket, Vec<Ticket>), RequestError> {
    with_project(app, key)
        .await?
        .ok_or_else(|| RequestError::plain(format!("{key} is gone.")))
}

// ---------------------------------------------------------------------------------------------------------------
// Messages (MSG-01, MSG-03, MSG-06; d0030)

/// How a message's status line looks (*05 → Message states*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "MessageTone")]
pub enum MessageTone {
    Muted,
    Delivered,
    Done,
    Attention,
}

impl From<Tone> for MessageTone {
    fn from(tone: Tone) -> Self {
        match tone {
            Tone::Muted => Self::Muted,
            Tone::Delivered => Self::Delivered,
            Tone::Done => Self::Done,
            Tone::Attention => Self::Attention,
        }
    }
}

/// "SHOP-40.2 → SHOP-40.1 · 10:42", the text and its status; a `null` end is the user ("you"), unless it has a name
/// (the epic's lead, "Wishlist lead").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "MessageLineView")]
pub struct LineView {
    pub from: Option<String>,
    pub to: Option<String>,
    pub from_name: Option<String>,
    pub to_name: Option<String>,
    pub time: String,
    pub text: String,
    pub status: String,
    pub icon: IconName,
    pub tone: MessageTone,
}

impl From<Line> for LineView {
    fn from(l: Line) -> Self {
        Self {
            from: l.from.map(|k| k.to_string()),
            to: l.to.map(|k| k.to_string()),
            from_name: l.from_name,
            to_name: l.to_name,
            time: l.time,
            text: l.text,
            status: l.status,
            icon: l.icon,
            tone: l.tone.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "MessageThreadView")]
pub struct ThreadView {
    pub id: String,
    pub question: LineView,
    pub answer: Option<LineView>,
    /// Sent to the user (Needs you): answered here.
    pub answerable: bool,
}

impl From<Thread> for ThreadView {
    fn from(t: Thread) -> Self {
        Self {
            id: t.id,
            question: t.question.into(),
            answer: t.answer.map(Into::into),
            answerable: t.answerable,
        }
    }
}

/// The Messages section; `null` when the ticket has no messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "MessagesView")]
pub struct MessagesView {
    pub title: &'static str,
    /// "1 thread · linked tickets only".
    pub heading: String,
    pub threads: Vec<ThreadView>,
    pub you: &'static str,
    pub answer: &'static str,
    pub placeholder: &'static str,
    pub send: &'static str,
}

/// The section with the epic lead's threads among them (T-0290): "3 threads · with Wishlist lead".
pub fn with_lead(threads: Vec<Thread>, lead: Option<&str>) -> Option<MessagesView> {
    (!threads.is_empty()).then(|| MessagesView {
        title: "Messages",
        heading: match lead {
            Some(lead) => messages::lead_heading(threads.len(), lead),
            None => messages::heading(&threads),
        },
        threads: threads.into_iter().map(Into::into).collect(),
        you: "you",
        answer: "Answer",
        placeholder: "Type your answer",
        send: "Send",
    })
}

pub async fn messages_view(app: &App, params: Params) -> Result<Option<MessagesView>, RequestError> {
    let Ok(key) = params.key.parse::<TicketKey>() else {
        return Ok(None);
    };
    let k = || KeyParams {
        key: params.key.clone(),
    };
    let list = match app.service.send(Request::TicketMessages(k())).await {
        Ok(Reply::Messages(list)) => list,
        Ok(other) => return Err(unexpected("ticket.messages", &other)),
        Err(e) if e.not_found => return Ok(None),
        Err(e) => return Err(e),
    };
    let answers = match send_key(app, Request::TicketHumanAnswers(k()), "ticket.human_answers").await? {
        Reply::HumanAnswers(list) => list,
        other => return Err(unexpected("ticket.human_answers", &other)),
    };
    let log = match app
        .service
        .send(Request::TicketLeadLog(k()))
        .await
    {
        Ok(Reply::LeadLog(log)) => *log,
        // An older service keeps no lead log.
        _ => LeadLog::default(),
    };
    let (threads, lead) = threads_of(&list, &answers, &log, &key, now_ms());
    Ok(with_lead(threads, lead.as_deref()))
}

/// The section's threads, oldest first, with the lead's name when the lead's log has any. A question the lead passed
/// on or handed back is also one of the ticket's answered `ask human` questions; it is drawn once, as the lead's
/// thread (the same `human_questions` row: same text, same time).
pub fn threads_of(
    list: &[otaq_core::messages::Message],
    answers: &[otaq_core::board::questions::HumanAnswer],
    log: &LeadLog,
    key: &TicketKey,
    now: i64,
) -> (Vec<Thread>, Option<String>) {
    let own: Vec<_> = answers
        .iter()
        .filter(|a| {
            !log
                .questions
                .iter()
                .any(|q| q.question == a.question && q.asked_at == a.asked_at)
        })
        .cloned()
        .collect();
    let mut threads = messages::all_threads(list, &own, key, now);
    let lead = lead_entries(log);
    if let Some((name, entries)) = &lead {
        threads.extend(messages::lead_threads(entries, name));
        threads.sort_by_key(|t| t.asked_at);
    }
    (threads, lead.map(|(name, _)| name))
}

/// The epic lead's log (`ticket.lead_log`: T-0284's routed questions, T-0281's notes) as the Messages section's
/// entries, with the lead's name; `None` when nothing is logged.
pub fn lead_entries(log: &LeadLog) -> Option<(String, Vec<messages::LeadEntry>)> {
    use otaq_core::epic_lead::questions::LeadQuestionState as State;
    use otaq_view::view_models::messages::{LeadEntry, LeadOutcome, UserAnswer};
    let name = log
        .questions
        .first()
        .map(|q| q.lead.clone())
        .or_else(|| log.notes.first().map(|n| n.lead.clone()))?;
    let yours = |q: &otaq_core::epic_lead::questions::LeadQuestion| {
        q.answer.as_ref().map(|text| UserAnswer {
            text: text.clone(),
            at: q.answered_at.unwrap_or(q.asked_at),
            // The answer is typed into the child as an automatic message when its agent is idle (d0048).
            delivered: true,
        })
    };
    let questions = log.questions.iter().map(|q| LeadEntry::Question {
        id: q.id.clone(),
        child: q.child.clone(),
        text: q.question.clone(),
        asked_at: q.asked_at,
        outcome: match q.state {
            State::WithLead => LeadOutcome::Waiting,
            State::Answered => LeadOutcome::Answered {
                text: q.answer.clone().unwrap_or_default(),
                at: q.answered_at.or(q.lead_at).unwrap_or(q.asked_at),
                delivered: true,
            },
            State::PassedOn => LeadOutcome::PassedOn {
                recommended: q.recommended_label().map(str::to_owned),
                reason: q.reason.clone().unwrap_or_default(),
                answer: yours(q),
            },
            State::HandedBack => LeadOutcome::HandedBack { answer: yours(q) },
        },
    });
    let notes = log.notes.iter().map(|n| LeadEntry::Note {
        id: n.id.clone(),
        child: n.child.clone(),
        text: n.text.clone(),
        at: n.created_at,
        // Which stop a note answers is T-0285's (the stuck child's note).
        after: None,
        read: n.read_at.is_some(),
    });
    Some((name, questions.chain(notes).collect()))
}

/// What `sections.answer_message` takes: a question that went to the user (Needs you), and the typed answer.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AnswerMessageParams")]
pub struct AnswerMessage {
    pub id: String,
    pub answer: String,
}

/// The answer to a question sent to the user (Needs you) (MSG-06); a refusal toasts and rejects, so the box stays open.
pub async fn answer_message(app: &App, params: AnswerMessage) -> Result<(), RequestError> {
    let answer = params.answer.trim().to_owned();
    if answer.is_empty() {
        return Err(RequestError::plain("Type an answer first."));
    }
    match app
        .service
        .send(Request::MessageAnswer(AnswerParams {
            id: params.id,
            answer,
        }))
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            toast(
                app,
                ToastLevel::Error,
                "The answer wasn\u{2019}t sent".to_owned(),
                Some(e.message.clone()),
            );
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// History (REC-03; d0045)

/// One row of Activity's timeline (Q432, `s25-07`): the time, who, a plain sentence and what it opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HistoryRowView")]
pub struct HistoryRowView {
    /// "You created the ticket", "Grill started · Griller".
    pub title: String,
    /// "Claude Code · sonnet", "“Don’t touch the auth middleware…”".
    pub detail: Option<String>,
    /// "09:42".
    pub when: String,
    /// "· 18 min · 64k tokens" after the sentence, only where an agent ran (never a dollar figure, d0086).
    pub cost: Option<String>,
    /// The full tool session id, for the copy button (`claude --resume <id>`).
    pub tool_session_id: Option<String>,
    /// Who: you, the agent (its tool avatar), Otaq.
    pub mark: Mark,
    /// What the row opens: a before / after, or what a comment said and its report (CMT-14).
    pub opens: Option<Opens>,
    /// Activity's filter it shows under (SHT-05).
    pub kind: history::Kind,
}

/// One of Activity's filters: "All", "Comments", "Stages", "Description".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityFilterView")]
pub struct FilterView {
    pub filter: history::Filter,
    pub label: &'static str,
    /// The row kind it shows; `None` for All.
    pub kind: Option<history::Kind>,
}

impl From<HistoryRow> for HistoryRowView {
    fn from(r: HistoryRow) -> Self {
        Self {
            cost: r.cost(),
            title: r.title,
            detail: r.detail,
            when: local_hhmm(r.at),
            tool_session_id: r.tool_session_id,
            mark: r.mark,
            opens: r.opens,
            kind: r.kind,
        }
    }
}

/// "Validation total · 4 entries" and its tokens under the History table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HistoryTotal")]
pub struct HistoryTotal {
    pub label: String,
    pub tokens: String,
}

/// The sheet's Activity (SHT-05; History, UI-04): sessions and events, oldest first, under its filters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HistoryView")]
pub struct HistoryView {
    /// All · Comments · Stages · Description.
    pub filters: Vec<FilterView>,
    /// "oldest first".
    pub order: &'static str,
    pub rows: Vec<HistoryRowView>,
    pub copy_hint: &'static str,
    pub copied: &'static str,
    pub empty: &'static str,
    /// Under the rows once the ticket had validation rounds.
    pub total: Option<HistoryTotal>,
    /// The ticket, for a report's Create ticket.
    pub key: String,
}

/// Activity from History's rows (newest first): oldest first, with the filters.
pub fn history_of(rows: Vec<HistoryRow>) -> HistoryView {
    HistoryView {
        filters: history::Filter::ALL
            .into_iter()
            .map(|filter| FilterView {
                filter,
                label: filter.label(),
                kind: filter.kind(),
            })
            .collect(),
        order: history::OLDEST_FIRST,
        rows: history::oldest_first(rows)
            .into_iter()
            .map(Into::into)
            .collect(),
        copy_hint: "Copy session ID",
        copied: "Copied",
        empty: "Nothing has happened to this ticket yet.",
        total: None,
        key: String::new(),
    }
}

/// The ticket's validation rounds, newest first; none when the service can't say (History still shows).
pub(super) async fn validations(app: &App, key: &str) -> Vec<ValidationRecord> {
    match app
        .service
        .send(Request::TicketValidations(KeyParams {
            key: key.to_owned(),
        }))
        .await
    {
        Ok(Reply::Validations(records)) => records,
        _ => Vec::new(),
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

pub async fn history_view(app: &App, params: Params) -> Result<Option<HistoryView>, RequestError> {
    let k = || KeyParams {
        key: params.key.clone(),
    };
    let events = match app.service.send(Request::TicketHistory(k())).await {
        Ok(Reply::History(events)) => events,
        Ok(other) => return Err(unexpected("ticket.history", &other)),
        Err(e) if e.not_found => return Ok(None),
        Err(e) => return Err(e),
    };
    let sessions = sessions(app, &params.key).await?;
    // Tokens are a nicety: the table still shows without them (USG-01's "—").
    let usage = match app.service.send(Request::UsageSessions(k())).await {
        Ok(Reply::SessionUsage(usage)) => usage,
        _ => Vec::new(),
    };
    let records = validations(app, &params.key).await;
    let ticket = ticket(app, &params.key).await?;
    let rounds = match &ticket {
        Some(t) if !records.is_empty() => super::sheet::check_rounds(app, t).await,
        _ => otaq_core::flow::check::DEFAULT_VALIDATION_ROUNDS,
    };
    // A comment's author names its stage by id; History shows its name in the flow now (CMT-14).
    let stages = match &ticket {
        Some(t) => super::board::flow(app, t.key.project.as_str())
            .await
            .stages
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect(),
        None => Default::default(),
    };
    let facts = history::Facts {
        stages,
        avatars: tool_avatar::shown(),
    };
    let rows = history::rows_with_rounds(&events, &sessions, &usage, &records, rounds, &facts);
    // The named agents each session ran on (AGT-05): left out when the rows can't be read.
    let rows = match super::editor::agent_facts(app, super::editor::of_ticket(&params.key)).await {
        Some(agents) => {
            let tools = super::editor::agent_tools(app).await;
            history::with_agents(rows, &stage_choices::history_lines(&agents, &tools))
        }
        None => rows,
    };
    let mut view = history_of(rows);
    view.total =
        validation_report::total(&records).map(|(label, tokens)| HistoryTotal { label, tokens });
    view.key = params.key;
    Ok(Some(view))
}

/// What `sections.create_from_note` takes: the ticket and its report's note, exactly as the report gave it.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CreateFromNoteParams")]
pub struct NoteRequest {
    pub key: String,
    pub note: String,
}

/// **Create ticket** on a "Not covered by any ticket" note (VAL-12; the user's click): one prefilled Backlog ticket
/// in the same epic, then the toast "Created SHOP-41 from a validation note". Answers the new ticket's key.
pub async fn create_from_note(app: &App, params: NoteRequest) -> Result<String, RequestError> {
    let request = Request::TicketCreateFromNote(NoteParams {
        key: params.key.clone(),
        note: params.note.clone(),
    });
    match app.service.send(request).await {
        Ok(Reply::Ticket(t)) => {
            let (title, text) = validation_report::created_toast(&t.key, &params.note);
            let mut toast = NewToast::new(ToastLevel::Success, title, Some(text));
            toast.ticket = Some(t.key.to_string());
            app.notices.toast(toast);
            Ok(t.key.to_string())
        }
        Ok(other) => Err(unexpected("ticket.create_from_note", &other)),
        Err(e) => {
            app.notices.toast(NewToast::new(
                ToastLevel::Error,
                "The ticket wasn\u{2019}t created",
                Some(e.message.clone()),
            ));
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Move to epic… (EPC-07, TKT-07; d0044) and Close epic… (EPC-06; d0033)

/// An epic the ticket can move into, with the dialog's texts for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "MoveChoiceView")]
pub struct MoveChoiceView {
    pub key: String,
    pub title: String,
    pub progress: String,
    pub new_key: String,
    /// "Move SHOP-51 Wishlist share link to an epic".
    pub heading: String,
    /// "· the old key SHOP-51 still works".
    pub alias_note: String,
    /// The base change, when the ticket had another base (*08 → Move to epic · base changes*).
    pub base_note: Option<String>,
    /// "Move to SHOP-40".
    pub confirm: String,
}

/// The Move to epic… dialog: the open epics to pick from (the first picked), or why it can't move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "MoveDialogView")]
pub struct MoveDialogView {
    pub old_key: String,
    pub choices: Vec<MoveChoiceView>,
    pub disk_note: &'static str,
    /// Set when the ticket can't move (it has been to Dev, it is in an epic, or no epic is open).
    pub refusal: Option<String>,
    pub title: String,
}

pub fn move_dialog_of(ticket: &Ticket, tickets: &[Ticket]) -> MoveDialogView {
    let choices: Vec<EpicChoice> = epic_actions::epic_choices(tickets);
    let refusal = if !epic_actions::can_move_to_epic(ticket) {
        Some(format!(
            "{} can\u{2019}t move: only a ticket that has never been to Dev, and isn\u{2019}t in an epic, moves to one.",
            ticket.key
        ))
    } else if choices.is_empty() {
        Some(format!("{} has no open epic to move into.", ticket.key.project))
    } else {
        None
    };
    MoveDialogView {
        old_key: ticket.key.to_string(),
        title: format!("Move {} {} to an epic", ticket.key, ticket.title),
        choices: choices
            .iter()
            .map(|c| {
                let preview = epic_actions::move_preview(ticket, c, tickets);
                MoveChoiceView {
                    key: c.key.to_string(),
                    title: c.title.clone(),
                    progress: c.progress.clone(),
                    new_key: c.new_key.to_string(),
                    heading: preview.title,
                    alias_note: preview.alias_note,
                    base_note: preview.base_note,
                    confirm: preview.confirm,
                }
            })
            .collect(),
        disk_note: "No branch or worktree exists yet, so nothing on disk is renamed.",
        refusal,
    }
}

pub async fn move_dialog(app: &App, params: Params) -> Result<MoveDialogView, RequestError> {
    let (ticket, all) = found(app, &params.key).await?;
    Ok(move_dialog_of(&ticket, &all))
}

/// What `sections.move_to_epic` takes.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "MoveToEpicParams")]
pub struct MoveTo {
    pub key: String,
    pub epic: String,
}

/// Moves the ticket; answers its new key ("SHOP-40.4"). A refusal rejects and stays in the dialog.
pub async fn move_to_epic(app: &App, params: MoveTo) -> Result<String, RequestError> {
    let old = params.key.clone();
    match app
        .service
        .send(Request::EpicMoveTo(MoveToEpicParams {
            key: params.key,
            epic: params.epic,
        }))
        .await?
    {
        Reply::Ticket(t) => {
            let new = t.key.to_string();
            toast(
                app,
                ToastLevel::Success,
                format!("Moved {old} \u{2192} {new}"),
                None,
            );
            Ok(new)
        }
        other => Err(unexpected("epic.move_to", &other)),
    }
}

/// What the Close epic… overlay takes.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "EpicParams")]
pub struct EpicParams {
    pub epic: String,
}

/// The Close epic confirmation (*Epic dialogs → Close epic · confirm*): the agents it stops, the tickets it also
/// closes, the ones that stay merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CloseEpicDialogView")]
pub struct CloseEpicDialogView {
    pub title: String,
    pub agents: Vec<String>,
    pub also: Option<String>,
    pub note: &'static str,
    pub confirm: String,
}

pub async fn close_epic_dialog(
    app: &App,
    params: EpicParams,
) -> Result<CloseEpicDialogView, RequestError> {
    let (epic, all) = found(app, &params.epic).await?;
    let plan = match app
        .service
        .send(Request::EpicClosePlan(KeyParams {
            key: params.epic.clone(),
        }))
        .await?
    {
        Reply::ClosePlan(plan) => plan,
        other => return Err(unexpected("epic.close_plan", &other)),
    };
    let text = epic_actions::close_dialog(&epic, &plan, &all);
    Ok(CloseEpicDialogView {
        title: text.title,
        agents: text.agents,
        also: text.also,
        note: text.note,
        confirm: text.confirm,
    })
}

/// Closes the epic and its unfinished children; a refusal rejects and stays in the dialog.
pub async fn close_epic(app: &App, params: EpicParams) -> Result<(), RequestError> {
    let key = params.epic;
    app.service
        .send(Request::EpicClose(CloseParams {
            key: key.clone(),
            confirm: true,
        }))
        .await?;
    toast(
        app,
        ToastLevel::Success,
        format!("Closed epic {key}"),
        None,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::domain::{Kind, Status};
    use otaq_core::git::testing::TempRepos;
    use otaq_view::view_models::card_fixtures::ticket as fixture;
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    fn key(text: &str) -> TicketKey {
        text.parse().unwrap()
    }

    #[test]
    fn no_messages_no_section_and_activity_has_its_filters() {
        assert_eq!(with_lead(Vec::new(), None), None);
        assert_eq!(with_lead(Vec::new(), Some("Wishlist lead")), None);
        // The epic lead's threads (T-0290): named ends and the "with Wishlist lead" heading.
        let log = [messages::LeadEntry::Question {
            id: "q7".into(),
            child: key("SHOP-40.2"),
            text: "Filled heart once saved?".into(),
            asked_at: 1,
            outcome: messages::LeadOutcome::Waiting,
        }];
        let view = with_lead(messages::lead_threads(&log, "Wishlist lead"), Some("Wishlist lead")).unwrap();
        assert_eq!(view.heading, "1 thread \u{b7} with Wishlist lead");
        assert_eq!(view.threads[0].question.from.as_deref(), Some("SHOP-40.2"));
        assert_eq!(view.threads[0].question.to_name.as_deref(), Some("Wishlist lead"));
        let history = history_of(Vec::new());
        let filters: Vec<_> = history.filters.iter().map(|f| (f.label, f.kind)).collect();
        assert_eq!(
            filters,
            [
                ("All", None),
                ("Comments", Some(history::Kind::Comment)),
                ("Stages", Some(history::Kind::Stage)),
                ("Description", Some(history::Kind::Description))
            ]
        );
        assert_eq!(history.order, "oldest first");
        assert_eq!(history.copy_hint, "Copy session ID");
    }

    /// A question the lead passed on, and one it handed back, are also answered `ask human` questions: each shows
    /// once, as the lead's thread, on the epic and on the child (T-0290 fix). An unrelated answer of the user's stays.
    #[test]
    fn passed_on_and_handed_back_questions_show_once() {
        use otaq_core::board::questions::HumanAnswer;
        use otaq_core::epic_lead::questions::{LeadQuestion, LeadQuestionState};
        let question = |id: &str, text: &str, state, asked_at| LeadQuestion {
            id: id.into(),
            child: key("SHOP-40.2"),
            child_title: "Wishlist button".into(),
            epic: key("SHOP-40"),
            lead: "Wishlist lead".into(),
            lead_agent: Some("lead-1".into()),
            question: text.into(),
            options: vec!["Yes".into(), "No".into()],
            recommended: Some(1),
            notes: Vec::new(),
            state,
            reason: Some("PAY-123 only mentions signed-in users.".into()),
            answer: Some("Answer to your question: Yes (option 1).".into()),
            asked_at,
            lead_at: Some(asked_at + 1),
            answered_at: Some(asked_at + 2),
        };
        let log = LeadLog {
            questions: vec![
                question("8", "Can guests save items?", LeadQuestionState::PassedOn, 100),
                question("9", "Should links expire?", LeadQuestionState::HandedBack, 200),
            ],
            notes: Vec::new(),
        };
        let answer = |text: &str, asked_at| HumanAnswer {
            question: text.into(),
            answer: "Answer to your question: Yes (option 1).".into(),
            asked_at,
            answered_at: asked_at + 2,
        };
        let answers = [
            answer("Can guests save items?", 100),
            answer("Should links expire?", 200),
            answer("Which colour?", 300),
        ];
        for ticket in ["SHOP-40", "SHOP-40.2"] {
            let (threads, lead) = threads_of(&[], &answers, &log, &key(ticket), 1_000);
            assert_eq!(lead.as_deref(), Some("Wishlist lead"));
            let texts: Vec<&str> = threads.iter().map(|t| t.question.text.as_str()).collect();
            assert_eq!(
                texts,
                ["Can guests save items?", "Should links expire?", "Which colour?"],
                "{ticket}: each question once"
            );
            assert_eq!(threads[0].question.to_name.as_deref(), Some("Wishlist lead"));
            assert_eq!(threads[1].question.to_name.as_deref(), Some("Wishlist lead"));
            assert_eq!(threads[2].question.to_name, None, "the user's own question");
        }
        let (threads, lead) = threads_of(&[], &answers, &LeadLog::default(), &key("SHOP-40.2"), 1_000);
        assert_eq!((threads.len(), lead), (3, None), "no lead log: every answer stays");
    }

    /// The lead's log (`ticket.lead_log`) as Messages entries (T-0290): answered by the lead, passed on with the
    /// user's answer, and a note; nothing logged, no lead threads.
    #[test]
    fn the_lead_log_becomes_the_leads_threads() {
        use otaq_core::epic_lead::questions::{LeadQuestion, LeadQuestionState};
        let question = |id: &str, state, answer: Option<&str>| LeadQuestion {
            id: id.into(),
            child: key("SHOP-40.2"),
            child_title: "Wishlist button".into(),
            epic: key("SHOP-40"),
            lead: "Wishlist lead".into(),
            lead_agent: Some("lead-1".into()),
            question: "Can guests save items without logging in?".into(),
            options: vec!["Logged-in users only".into(), "Guests too".into()],
            recommended: Some(1),
            notes: Vec::new(),
            state,
            reason: Some("PAY-123 only mentions signed-in users.".into()),
            answer: answer.map(str::to_owned),
            asked_at: 1,
            lead_at: Some(2),
            answered_at: answer.map(|_| 3),
        };
        assert_eq!(lead_entries(&LeadLog::default()), None);
        let log = LeadLog {
            questions: vec![
                question("7", LeadQuestionState::Answered, Some("Yes: filled heart.")),
                question("8", LeadQuestionState::PassedOn, Some("Logged-in users only (option 1).")),
            ],
            notes: Vec::new(),
        };
        let (name, entries) = lead_entries(&log).unwrap();
        assert_eq!(name, "Wishlist lead");
        let threads = messages::lead_threads(&entries, &name);
        assert_eq!(
            threads[0].answer.as_ref().unwrap().status,
            "Answered by Wishlist lead \u{b7} typed into SHOP-40.2\u{2019}s session"
        );
        assert_eq!(
            threads[1].question.status,
            "Passed on to you \u{b7} the lead recommended \u{201c}Logged-in users only\u{201d}: PAY-123 only \
             mentions signed-in users."
        );
        assert_eq!(
            threads[1].answer.as_ref().unwrap().text,
            "Logged-in users only (option 1)."
        );
    }

    #[test]
    fn move_is_for_never_started_tickets() {
        let mut epic = fixture("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        epic.next_child_number = Some(3);
        let mut api = fixture("SHOP-40.1", "Wishlist API", Status::Merged);
        api.epic = Some(key("SHOP-40"));
        let mut button = fixture("SHOP-40.2", "Wishlist button", Status::Working);
        button.epic = Some(key("SHOP-40"));
        button.depends_on = vec![key("SHOP-40.1")];
        let share = fixture("SHOP-51", "Wishlist share link", Status::Backlog);
        let mut working = fixture("SHOP-19", "Price table", Status::Working);
        working.started_at = Some(1);
        let all = vec![epic.clone(), api, button, share.clone(), working.clone()];

        let dialog = move_dialog_of(&share, &all);
        assert_eq!(dialog.refusal, None);
        assert_eq!(dialog.choices[0].new_key, "SHOP-40.3");
        assert_eq!(dialog.choices[0].confirm, "Move to SHOP-40");
        assert_eq!(dialog.choices[0].alias_note, "\u{b7} the old key SHOP-51 still works");
        assert!(move_dialog_of(&working, &all).refusal.is_some(), "it has been to Dev");
    }

    async fn create(h: &Harness, project: &str, title: &str, kind: Kind, epic: Option<&str>) -> String {
        match h
            .app
            .service
            .send(Request::TicketCreate(NewTicketInput {
                project: project.to_owned(),
                kind,
                title: title.into(),
                body: String::new(),
                priority: None,
                epic: epic.map(str::to_owned),
                base_branch: None,
                external_id: None,
                first_comment: None,
            }))
            .await
            .unwrap()
        {
            Reply::Ticket(t) => t.key.to_string(),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn the_sections_read_and_change_real_tickets() {
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
                flow_setup: false,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p.key,
            other => panic!("{other:?}"),
        };
        let epic = create(&h, &project, "Wishlist", Kind::Epic, None).await;
        let child = create(&h, &project, "Wishlist API", Kind::Ticket, Some(&epic)).await;
        let lone = create(&h, &project, "Share link", Kind::Ticket, None).await;

        // The epic's sheet (SHT-07): its Tickets tab and Overview's grid list its children.
        let view = table(&h.app, "epic.tickets", json!({ "key": epic })).await.unwrap();
        assert_eq!(view["rows"][0]["card"]["key"], child.as_str());
        assert_eq!(view["add"], "Add ticket to epic");
        let overview = table(&h.app, "epic.overview", json!({ "key": epic })).await.unwrap();
        assert_eq!(overview["grid"]["heading"], "TICKETS \u{b7} 1");
        assert_eq!(overview["grid"]["rows"][0]["key"], child.as_str());

        let messages = table(&h.app, "sections.messages", json!({ "key": lone })).await.unwrap();
        assert_eq!(messages, Value::Null);
        let history = table(&h.app, "sections.history", json!({ "key": lone })).await.unwrap();
        assert_eq!(history["rows"][0]["title"], "You created the ticket");
        assert_eq!(history["rows"][0]["mark"]["kind"], "you");
        assert_eq!(history["rows"][0]["cost"], Value::Null);
        assert_eq!(history["total"], Value::Null, "no validation rounds");
        assert_eq!(history["key"], lone.to_string());
        // Create ticket takes only a "Not covered by any ticket" note of the ticket's validation.
        let refused = table(
            &h.app,
            "sections.create_from_note",
            json!({ "key": lone, "note": "Not covered by any ticket: refunds endpoint" }),
        )
        .await
        .unwrap_err();
        assert!(refused.not_found, "{}", refused.message);

        let dialog = table(&h.app, "sections.move_dialog", json!({ "key": lone })).await.unwrap();
        assert_eq!(dialog["refusal"], Value::Null);
        let new_key = dialog["choices"][0]["new_key"].as_str().unwrap().to_owned();
        let moved = table(
            &h.app,
            "sections.move_to_epic",
            json!({ "key": lone, "epic": epic }),
        )
        .await
        .unwrap();
        assert_eq!(moved, new_key.as_str());
        let view = table(&h.app, "epic.tickets", json!({ "key": epic })).await.unwrap();
        assert_eq!(view["rows"].as_array().unwrap().len(), 2);

        let close = table(&h.app, "sections.close_epic_dialog", json!({ "epic": epic }))
            .await
            .unwrap();
        assert_eq!(close["confirm"], "Close epic & 2 tickets");
        table(&h.app, "sections.close_epic", json!({ "epic": epic }))
            .await
            .unwrap();
        let gone = table(&h.app, "sections.messages", json!({ "key": "SHOP-99" })).await.unwrap();
        assert_eq!(gone, Value::Null);
        h.service.stop_all().await;
    }
}
