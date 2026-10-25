//! `sections.*`: the ticket's sections and its Activity (UI-04, REC-03, MSG-01, MSG-03, MSG-06, EPC-01, EPC-04,
//! EPC-06, EPC-07, TKT-07; d0045, d0030, d0039, d0044, d0059; *05 · Ticket side sheet*). The Dependencies section is
//! `dependencies.rs`'s (DPV-04). The **Messages** between linked agents
//! with their states (`messages.rs`), answerable when one went to the user (Needs you); the sheet's **Activity** (SHT-05), sessions
//! with their tool session ids and events oldest first under All · Comments · Stages · Description (`history.rs`),
//! with one entry per validation round that opens
//! its report and **Create ticket** from a report's note (`validation_report.rs`, VAL-12). Each view is `null` when
//! its section has nothing to show for the ticket.

use std::time::{SystemTime, UNIX_EPOCH};

use otaq_core::board::validation::ValidationRecord;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::TicketKey;
use otaq_core::protocol::{
    AnswerParams, KeyParams, NoteParams, Reply, Request, SessionListParams,
};
use otaq_core::session::SessionInfo;
use otaq_view::icons::IconName;
use otaq_view::view_models::clock::local_hhmm;
use otaq_view::view_models::stage_choices;
use otaq_view::view_models::history::{self, HistoryRow, Opens};
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

/// "SHOP-40.2 → SHOP-40.1 · 10:42", the text and its status; a `null` end is the user ("you"), unless it has a name.
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

/// The section: "3 threads · linked tickets only"; `None` without threads.
pub fn section(threads: Vec<Thread>) -> Option<MessagesView> {
    (!threads.is_empty()).then(|| MessagesView {
        title: "Messages",
        heading: messages::heading(&threads),
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
    Ok(section(messages::all_threads(&list, &answers, &key, now_ms())))
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

/// **Create ticket** on a "Not covered by any ticket" note (VAL-12; the user's click): one prefilled Backlog ticket,
/// then the toast "Created SHOP-41 from a validation note". Answers the new ticket's key.
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

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    #[test]
    fn no_messages_no_section_and_activity_has_its_filters() {
        assert_eq!(section(Vec::new()), None);
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

    async fn create(h: &Harness, project: &str, title: &str) -> String {
        match h
            .app
            .service
            .send(Request::TicketCreate(NewTicketInput {
                project: project.to_owned(),
                title: title.into(),
                body: String::new(),
                priority: None,
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
        let lone = create(&h, &project, "Share link").await;

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

        let gone = table(&h.app, "sections.messages", json!({ "key": "SHOP-99" })).await.unwrap();
        assert_eq!(gone, Value::Null);
        h.service.stop_all().await;
    }
}
