//! `editor.*`: the ticket editor and the Ticket tab (UI-05, TKT-03, TKT-05, TKT-09, DEP-01, DEP-02, DEP-04, EPC-07,
//! SES-08; d0019, d0051, d0059, d0065, d0067, d0048; *08 · Ticket editor*). One view per ticket (or new ticket):
//! the form's labels, hints and first values, which fields it shows (`editor_fields.rs`), and for an existing
//! ticket its read mode with the agent-edit markers (`agent_edits.rs`). The page keeps what is typed and sends the
//! whole form back on save; the rules (title required, only what changed, priority only once picked) are
//! `editor.rs`'s, and the service's refusals come back worded for the field they belong to. Dependencies are the
//! sheet's (DPV-04, `dependencies.rs`), not the form's.

use std::time::{SystemTime, UNIX_EPOCH};

use otaq_core::board::agent_edits::AgentEdits;
use otaq_core::board::settings::GlobalSettings;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::board::tickets::{Field, Ticket};
use otaq_core::domain::{Actor, Priority};
use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::models::ModelList;
use otaq_core::agents::facts::TicketAgents;
use otaq_core::board::stage_choices::{ChoicePick, TicketChoices};
use otaq_core::board::tickets::TicketUpdate;
use otaq_core::protocol::{
    KeyParams, ModelsParams, ProjectParams, Reply, Request, SessionListParams,
    TicketAgentsParams, TicketChoicesParams, TicketUpdateParams,
};
use otaq_core::session::{SessionInfo, SessionKind};
use otaq_view::icons::IconName;
use otaq_view::view_models::agent_activity;
use otaq_view::view_models::stage_choices::{self, ChoicesView};
use otaq_view::view_models::tool_problems::Tools;
use otaq_view::view_models::agent_edits::{changed, compare, origin_line};
use otaq_view::view_models::editor::{
    EditorModel, NewReport, REPORT_LINE_REQUIRED, ReportField, TITLE_REQUIRED, report_field,
};
use otaq_view::view_models::editor_fields::{
    BaseField, base_choices, refusal_icon, refusal_title, visibility,
};
use otaq_view::view_models::menu::{MenuItem, MenuSearch};
use otaq_view::view_models::priority::{self, FORM_HINT, PriorityView};
use otaq_view::view_models::validation_texts;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "editor.open" => reply(&open(app, parse(params)?).await?),
        "editor.save" => reply(&save(app, parse(params)?).await?),
        "editor.branches" => reply(&branches(app, parse(params)?).await?),
        "editor.compare" => reply(&compare_view(app, parse(params)?).await?),
        "editor.source_mode" => {
            set_source_mode(app, parse(params)?).await?;
            reply(&Value::Null)
        }
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


// ---------------------------------------------------------------------------------------------------------------
// What the page sends

/// Which editor: a new ticket in a project, or an existing ticket.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[ts(rename = "EditorTarget")]
pub enum Target {
    Create {
        project: Option<String>,
        /// The description it starts with: New ticket from this file puts the file's path there (FTB-03).
        #[serde(default)]
        #[ts(optional)]
        body: Option<String>,
    },
    Edit {
        key: String,
    },
    /// The description edited in place in Overview (SHT-03): only the form's `body` is saved, nothing else.
    Description {
        key: String,
    },
}

/// The form as typed: what the page keeps and sends back on save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "EditorForm")]
pub struct Form {
    pub title: String,
    pub body: String,
    /// 1–4 once picked; `null`: Normal for a new ticket.
    pub priority: Option<u8>,
    pub base_branch: Option<String>,
    pub external_id: String,
    /// The stage choices picked in the form (AGM-09): sent with the new ticket, or with the save.
    #[serde(default)]
    pub choices: Vec<FormChoice>,
    /// Create only: the report the form took (RPT-03), posted with `report_line` as the ticket's first comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub report: Option<FormReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub report_line: Option<String>,
}

/// The New ticket form's report (RPT-03): a file by its path (Add, a drop), or a pasted file's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "EditorReport")]
pub enum FormReport {
    File { path: String },
    Pasted { text: String },
}

/// One stage choice picked in the form: the stage's id and the word (`Quick developer`, `skip`, `required`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "EditorChoice")]
pub struct FormChoice {
    pub stage_id: String,
    pub choice: String,
}

// ---------------------------------------------------------------------------------------------------------------
// What the page gets

/// The "Start from and merge into" field: a dropdown, or fixed after start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "EditorBaseField")]
pub enum BaseView {
    Editable,
    Locked { text: String },
}

/// The form's words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EditorTexts")]
pub struct Texts {
    pub title: &'static str,
    pub title_placeholder: &'static str,
    pub title_required: &'static str,
    pub priority: &'static str,
    pub priority_hint: &'static str,
    pub body: &'static str,
    /// The description's label in the Ticket tab's read mode, over the comments (CMT-10, `s05-41`).
    pub plan: &'static str,
    pub body_hint: &'static str,
    pub body_placeholder: &'static str,
    pub base: &'static str,
    pub base_hint: &'static str,
    /// The base dropdown's search field.
    pub base_search: MenuSearch,
    pub external_id: &'static str,
    pub external_id_hint: &'static str,
    pub external_id_placeholder: &'static str,
    /// "⌘↵ create" / "save", "Esc discard" / "close".
    pub save_word: &'static str,
    pub discard_word: &'static str,
    pub close_word: &'static str,
    pub cancel: &'static str,
    pub close: &'static str,
    pub primary: &'static str,
    pub edit: &'static str,
    pub back: &'static str,
    pub in_word: &'static str,
    /// Quick-create: "↵ create · ⇧↵ full form".
    pub quick_create: &'static str,
    pub quick_full: &'static str,
    pub open_terminal: &'static str,
    pub marker: &'static str,
    pub compare: &'static str,
    pub before: &'static str,
    pub after: &'static str,
    pub nothing_written: &'static str,
    /// The "What and why" label row's icons (d0078 D7): their tooltips, and "Copied" after a copy.
    pub copy_markdown: &'static str,
    pub copied: &'static str,
    pub expand: &'static str,
    /// The full-window view (d0078 D11): its Collapse tooltip, the header's "Editing", and the footer's "Esc collapse
    /// (after closing a popover)".
    pub collapse: &'static str,
    pub editing: &'static str,
    pub collapse_word: &'static str,
    /// Compare's Rendered / Markdown toggle on the description (d0078): its name and its two choices.
    pub compare_view: &'static str,
    pub rendered: &'static str,
    pub markdown: &'static str,
}

/// The Ticket tab's read mode (*Ticket tab · read mode, with agent edits*).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "EditorReadView")]
pub struct ReadView {
    /// "Created by you  ·  edited by the agent · 4m ago".
    pub origin: String,
    pub origin_agent: bool,
    pub title: String,
    /// "What and why" exactly as stored: the page renders it (read-only editor) and Copy as Markdown copies it.
    pub body_markdown: String,
    /// Nothing but whitespace: the page shows `nothing_written` instead.
    pub body_empty: bool,
    pub priority: PriorityView,
    /// The Priority dropdown: `sheet.priority` items.
    pub priority_menu: Vec<MenuItem>,
    /// "main".
    pub base: String,
    pub external_id: Option<String>,
    /// The fields an agent changed since the user last looked ("Updated by agent").
    pub marked: Vec<EditorField>,
}

/// A field with an agent-edit marker, and the Compare filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EditorField {
    Title,
    Body,
    /// The Plan section (d0133).
    Plan,
    Priority,
    BaseBranch,
    ExternalId,
}

impl EditorField {
    fn of(field: Field) -> Self {
        match field {
            Field::Title => Self::Title,
            Field::Body => Self::Body,
            Field::Plan => Self::Plan,
            Field::Priority => Self::Priority,
            Field::BaseBranch => Self::BaseBranch,
            Field::ExternalId => Self::ExternalId,
        }
    }

    fn core(self) -> Field {
        match self {
            Self::Title => Field::Title,
            Self::Body => Field::Body,
            Self::Plan => Field::Plan,
            Self::Priority => Field::Priority,
            Self::BaseBranch => Field::BaseBranch,
            Self::ExternalId => Field::ExternalId,
        }
    }
}

/// One priority in the form's segmented control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EditorPriorityOption")]
pub struct PriorityOption {
    pub number: u8,
    pub view: PriorityView,
}

/// The editor for one ticket or a new one.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "EditorView")]
pub struct View {
    pub create: bool,
    /// The ticket's key; `null` while creating.
    pub key: Option<String>,
    pub project: String,
    pub project_name: String,
    /// "New ticket", "Edit SHOP-12".
    pub heading: String,
    pub texts: Texts,
    pub form: Form,
    pub priorities: Vec<PriorityOption>,
    /// The priority shown while none is picked: Normal.
    pub shown_priority: u8,
    pub base: BaseView,
    pub external_id_shown: bool,
    /// "SHOP-19’s agent will be told what changed." while its agent runs (SES-08).
    pub save_hint: Option<String>,
    /// While it validates (VAL-08): "SHOP-29’s validation will be cancelled and its agent told what changed."
    pub save_warning: Option<String>,
    /// While it validates, beside the description once it changed: "unsaved · saving cancels the validation".
    pub unsaved_warning: Option<&'static str>,
    pub read: Option<ReadView>,
    /// The description opens as its raw Markdown source, the mode last chosen with the "Markdown" toggle (d0078 D9).
    pub source_mode: bool,
    /// "Stage choices" under Priority (AGM-09, `s08-29`): a picker per stage with options, none on the default flow.
    pub choices: Option<ChoicesView>,
    /// Create only: the report place under the description (RPT-03).
    pub report: Option<ReportField>,
}

fn texts(create: bool) -> Texts {
    Texts {
        title: "Title",
        title_placeholder: "Short, imperative title",
        title_required: TITLE_REQUIRED,
        priority: "Priority",
        priority_hint: FORM_HINT,
        body: "What and why",
        plan: "Description \u{b7} the plan",
        body_hint: "Markdown \u{b7} paste keeps formatting",
        body_placeholder: "Context, constraints, links\u{2026}",
        base: "Start from and merge into",
        base_hint: "project default \u{b7} local and remote branches \u{b7} locked once the ticket starts",
        base_search: MenuSearch::branches(),
        external_id: "External ID",
        external_id_hint: "optional \u{b7} standalone tickets only \u{b7} used in the branch and PR title",
        external_id_placeholder: "e.g. PAY-123",
        save_word: if create { "create" } else { "save" },
        discard_word: "discard",
        close_word: "close",
        cancel: "Cancel",
        close: "Close",
        primary: if create { "Create ticket" } else { "Save" },
        edit: "Edit",
        back: "Back",
        in_word: "in",
        quick_create: "create",
        quick_full: "full form",
        open_terminal: "Open terminal",
        marker: "Updated by agent",
        compare: "Compare",
        before: "BEFORE",
        after: "AFTER",
        nothing_written: "Nothing written yet.",
        copy_markdown: "Copy as Markdown",
        copied: "Copied",
        expand: "Expand",
        collapse: "Collapse",
        editing: "Editing",
        collapse_word: "collapse (after closing a popover)",
        compare_view: "Show the description",
        rendered: "Rendered",
        markdown: "Markdown",
    }
}

fn priorities() -> Vec<PriorityOption> {
    Priority::ALL
        .into_iter()
        .map(|p| PriorityOption {
            number: p.number(),
            view: priority::view(p),
        })
        .collect()
}

fn base_view(field: BaseField) -> BaseView {
    match field {
        BaseField::Editable => BaseView::Editable,
        BaseField::Locked(text) => BaseView::Locked { text },
    }
}

fn form_of(model: &EditorModel) -> Form {
    Form {
        title: model.title.clone(),
        body: model.body.clone(),
        priority: model.priority.map(Priority::number),
        base_branch: model.base_branch.clone(),
        external_id: model.external_id.clone(),
        choices: Vec::new(),
        report: None,
        report_line: None,
    }
}

/// The editor for a new ticket in `project`.
pub fn create_view(project: &ProjectInfo) -> View {
    let model = EditorModel::create(&project.key, "", &project.base_branch);
    let shown = model.shown_priority();
    let fields = visibility(false);
    View {
        create: true,
        key: None,
        project: project.key.clone(),
        project_name: project.name.clone(),
        heading: "New ticket".to_owned(),
        texts: texts(true),
        form: form_of(&model),
        priorities: priorities(),
        shown_priority: shown.number(),
        base: base_view(fields.base),
        external_id_shown: fields.external_id,
        save_hint: None,
        save_warning: None,
        unsaved_warning: None,
        read: None,
        source_mode: false,
        choices: None,
        report: Some(report_field()),
    }
}

/// The editor of an existing ticket, with its read mode.
pub fn edit_view(
    ticket: &Ticket,
    project_name: &str,
    edits: &AgentEdits,
    live_agent: bool,
    now: i64,
) -> View {
    let model = EditorModel::edit(ticket);
    let shown = model.shown_priority();
    let fields = visibility(model.base_locked);
    let marked = [
        Field::Title,
        Field::Body,
        Field::Priority,
        Field::BaseBranch,
        Field::ExternalId,
    ]
    .into_iter()
    .filter(|&f| changed(edits, f))
    .map(EditorField::of)
    .collect();
    View {
        create: false,
        key: Some(ticket.key.to_string()),
        project: ticket.key.project.to_string(),
        project_name: project_name.to_owned(),
        heading: format!("Edit {}", ticket.key),
        texts: texts(false),
        form: form_of(&model),
        priorities: priorities(),
        shown_priority: shown.number(),
        base: base_view(fields.base),
        external_id_shown: fields.external_id,
        save_hint: agent_activity::save_hint(ticket, live_agent),
        save_warning: validation_texts::editor_warning(ticket),
        unsaved_warning: validation_texts::editor_warning(ticket)
            .map(|_| validation_texts::UNSAVED_CANCELS),
        read: Some(ReadView {
            origin: origin_line(ticket, edits, now),
            origin_agent: matches!(ticket.created_by, Actor::Agent(_)),
            title: ticket.title.clone(),
            body_markdown: ticket.body.clone(),
            body_empty: ticket.body.trim().is_empty(),
            priority: priority::view(shown),
            priority_menu: priority::menu_items(shown),
            base: ticket.base.branch().to_owned(),
            external_id: ticket.external_id.clone(),
            marked,
        }),
        source_mode: false,
        choices: None,
        report: None,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Service helpers

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

async fn found(app: &App, key: &str) -> Result<Ticket, RequestError> {
    ticket(app, key)
        .await?
        .ok_or_else(|| RequestError::plain(format!("{key} is gone.")))
}

async fn projects(app: &App) -> Result<Vec<ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => Ok(list),
        other => Err(unexpected("project.list", &other)),
    }
}

async fn project(app: &App, key: &str) -> Result<ProjectInfo, RequestError> {
    projects(app)
        .await?
        .into_iter()
        .find(|p| p.key == key)
        .ok_or_else(|| RequestError::plain(format!("project {key} is gone.")))
}

async fn live_agent(app: &App, key: &str) -> bool {
    match app
        .service
        .send(Request::SessionList(SessionListParams {
            key: Some(key.to_owned()),
        }))
        .await
    {
        Ok(Reply::Sessions(list)) => running(&list),
        _ => false,
    }
}

fn running(sessions: &[SessionInfo]) -> bool {
    sessions
        .iter()
        .any(|s| s.kind == SessionKind::Agent && s.is_running())
}

async fn agent_edits(app: &App, key: &str) -> AgentEdits {
    match app
        .service
        .send(Request::TicketAgentEdits(KeyParams { key: key.to_owned() }))
        .await
    {
        Ok(Reply::AgentEdits(edits)) => edits,
        _ => AgentEdits::default(),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// editor.open

pub async fn open(app: &App, target: Target) -> Result<View, RequestError> {
    let source_mode = global_settings(app).await?.description_source_mode;
    let view = match target {
        Target::Create {
            project: key,
            body,
        } => {
            let key = match key {
                Some(k) => k,
                None => projects(app)
                    .await?
                    .into_iter()
                    .next()
                    .map(|p| p.key)
                    .ok_or_else(|| RequestError::plain("Add a project first."))?,
            };
            let info = project(app, &key).await?;
            let choices = stage_choices_of(app, None, Some(&key)).await;
            let mut view = create_view(&info);
            if let Some(body) = body {
                view.form.body = body;
            }
            View {
                choices: choices.and_then(|c| stage_choices::form_choices(&c, &info.name)),
                ..view
            }
        }
        Target::Edit { key } | Target::Description { key } => {
            let ticket = found(app, &key).await?;
            let project_key = ticket.key.project.to_string();
            let info = project(app, &project_key).await?;
            let edits = agent_edits(app, &key).await;
            let live = live_agent(app, &key).await;
            let choices = stage_choices_of(app, Some(&key), None).await;
            View {
                choices: choices.and_then(|c| stage_choices::form_choices(&c, &info.name)),
                ..edit_view(&ticket, &info.name, &edits, live, now_ms())
            }
        }
    };
    Ok(View {
        source_mode,
        ..view
    })
}

async fn global_settings(app: &App) -> Result<GlobalSettings, RequestError> {
    match app.service.send(Request::SettingsGet).await? {
        Reply::GlobalSettings(r) => Ok(r.settings),
        other => Err(unexpected("settings.get", &other)),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// editor.source_mode

/// What `editor.source_mode` takes: the "Markdown" toggle's new state.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[ts(rename = "EditorSourceModeParams")]
pub struct SourceModeParams {
    pub on: bool,
}

/// Remembers the toggle for every editor opened after it (d0078 D9), in the global settings (d0023).
pub async fn set_source_mode(app: &App, params: SourceModeParams) -> Result<(), RequestError> {
    let mut settings = global_settings(app).await?;
    if settings.description_source_mode != params.on {
        settings.description_source_mode = params.on;
        app.service.send(Request::SettingsSet(settings)).await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// editor.save

/// What `editor.save` takes.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "EditorSaveParams")]
pub struct SaveParams {
    pub target: Target,
    pub form: Form,
}

/// Which field a refusal belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "EditorRefusalField")]
pub enum RefusalField {
    Base,
    Form,
}

/// A refusal as the editor shows it (*08 → Cycle prevention, Depends on · different base rejected*): a title by
/// its code, then the service's exact message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EditorRefusal")]
pub struct Refusal {
    pub field: RefusalField,
    pub title: &'static str,
    pub message: String,
    pub icon: IconName,
}

impl Refusal {
    pub fn of(error: &RequestError, field: RefusalField) -> Self {
        let code = error.refusal.as_deref();
        let field = match code {
            Some("cross_base" | "base_locked") if field == RefusalField::Form => {
                RefusalField::Base
            }
            _ => field,
        };
        Self {
            field,
            title: refusal_title(code),
            message: error.message.clone(),
            icon: refusal_icon(code),
        }
    }
}

/// How a save went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "EditorSaved")]
pub enum SaveOutcome {
    /// Created, or saved (an existing ticket goes back to read mode; `note` says what its agent will be told).
    Saved { key: String, note: Option<String> },
    /// No title: the field shows `TITLE_REQUIRED`.
    TitleMissing,
    /// Refused; `created` names a new ticket whose dependencies couldn't all be linked (the editor stays open on
    /// it).
    Refused {
        refusal: Refusal,
        created: Option<String>,
    },
}

/// The model the form stands for, with the page's values on top of the target's.
pub fn model_of(base: EditorModel, form: &Form) -> EditorModel {
    let mut model = base;
    model.title = form.title.clone();
    model.body = form.body.clone();
    model.priority = form
        .priority
        .and_then(|n| Priority::from_number(i64::from(n)));
    model.external_id = form.external_id.clone();
    if model.is_create() {
        model.report = form.report.clone().map(|r| match r {
            FormReport::File { path } => NewReport::File(path),
            FormReport::Pasted { text } => NewReport::Pasted(text),
        });
        model.report_line = form.report_line.clone().unwrap_or_default();
    }
    if !model.base_locked {
        model.base_branch = form.base_branch.clone().or(model.base_branch);
    }
    model.tried = true;
    model
}

pub async fn save(app: &App, params: SaveParams) -> Result<SaveOutcome, RequestError> {
    match params.target {
        Target::Create { project, .. } => {
            let Some(project) = project else {
                return Err(RequestError::plain("Pick a project first."));
            };
            let info = project_base(app, &project).await?;
            let form = params.form;
            let model = model_of(EditorModel::create(&project, "", &info), &form);
            if model.title_error().is_some() {
                return Ok(SaveOutcome::TitleMissing);
            }
            // A report needs its line (d0109): the box says so, and Create is refused here too.
            if model.report_error().is_some() {
                return Ok(SaveOutcome::Refused {
                    refusal: Refusal::of(
                        &RequestError::plain(REPORT_LINE_REQUIRED),
                        RefusalField::Form,
                    ),
                    created: None,
                });
            }
            let Some(input) = model.create_input() else {
                return Ok(SaveOutcome::TitleMissing);
            };
            let created = match app.service.send(Request::TicketCreate(input)).await {
                Ok(Reply::Ticket(t)) => *t,
                Ok(other) => return Err(unexpected("ticket.create", &other)),
                Err(e) => {
                    return Ok(SaveOutcome::Refused {
                        refusal: Refusal::of(&e, RefusalField::Form),
                        created: None,
                    });
                }
            };
            let key = created.key.to_string();
            // The stage choices picked in the form (AGM-09), the user's, checked before any is written (AGM-11).
            if let Some(choices) = stage_choices_of(app, None, Some(&project)).await
            {
                let picks = stage_choices::picks_to_send(&choices, &picked(&form.choices));
                if let Err(e) = set_choices(app, &key, picks).await {
                    return Ok(SaveOutcome::Refused {
                        refusal: Refusal::of(&e, RefusalField::Form),
                        created: Some(key),
                    });
                }
            }
            Ok(SaveOutcome::Saved { key, note: None })
        }
        Target::Description { key } => {
            let before = found(app, &key).await?;
            let Some(update) = EditorModel::description(&before, &params.form.body).update_input() else {
                // Untouched: nothing is saved (Q48).
                return Ok(SaveOutcome::Saved { key, note: None });
            };
            update_ticket(app, key, &before, update).await
        }
        Target::Edit { key } => {
            let before = found(app, &key).await?;
            let model = model_of(EditorModel::edit(&before), &params.form);
            if model.title_error().is_some() {
                return Ok(SaveOutcome::TitleMissing);
            }
            // The choices that changed and can still change, with the save (checked before any field changes).
            let choices = match stage_choices_of(app, Some(&key), None).await {
                Some(current) => changed_choices(&current, &picked(&params.form.choices)),
                None => Vec::new(),
            };
            let update = match model.update_input() {
                Some(update) => TicketUpdate { choices, ..update },
                None if !choices.is_empty() => TicketUpdate {
                    choices,
                    ..TicketUpdate::default()
                },
                None => return Ok(SaveOutcome::Saved { key, note: None }),
            };
            update_ticket(app, key, &before, update).await
        }
    }
}

/// Sends `update` for the ticket `key` (`before` as it was): saved, with what its agent will be told, or refused.
async fn update_ticket(
    app: &App,
    key: String,
    before: &Ticket,
    update: TicketUpdate,
) -> Result<SaveOutcome, RequestError> {
    let live = live_agent(app, &key).await;
    match app
        .service
        .send(Request::TicketUpdate(TicketUpdateParams {
            key: key.clone(),
            update,
        }))
        .await
    {
        Ok(Reply::Ticket(after)) => Ok(SaveOutcome::Saved {
            note: agent_activity::saved_note(before, &after, live),
            key,
        }),
        Ok(other) => Err(unexpected("ticket.update", &other)),
        Err(e) => Ok(SaveOutcome::Refused {
            refusal: Refusal::of(&e, RefusalField::Form),
            created: None,
        }),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The Agents block (AGT-04, AGM-05)

/// `ticket.agents` of an existing ticket.
pub(super) fn of_ticket(key: &str) -> TicketAgentsParams {
    TicketAgentsParams {
        key: Some(key.to_owned()),
        project: None,
    }
}

/// The facts behind a ticket's rows; `None` when the service can't say (the block is then left out).
pub(super) async fn agent_facts(app: &App, params: TicketAgentsParams) -> Option<TicketAgents> {
    match app.service.send(Request::TicketAgents(params)).await {
        Ok(Reply::TicketAgents(t)) => Some(*t),
        _ => None,
    }
}

/// `tool`'s cached model list (the display names of "Opus · always the newest", what a model resolves to).
async fn model_list(app: &App, tool: Tool) -> Option<ModelList> {
    match app
        .service
        .send(Request::AgentsModels(ModelsParams { tool }))
        .await
    {
        Ok(Reply::Models(copy)) => copy.map(|c| c.list),
        _ => None,
    }
}

/// The agent tools as the rows, the panel, the agent panel and recovery read them (M23): every tool's state
/// (`agent_tools.list`), every tool's active limit (`usage.activity`), each connected tool's cached model list, and
/// each tool's fallback and the default model (`agents.list`, AGM-03). When the service can't list the tools, the
/// first tool counts as the one connected, as before M23.
pub(super) async fn agent_tools(app: &App) -> Tools {
    let mut tools = tool_states(app).await;
    if let Ok(Reply::Agents(list)) = app.service.send(Request::AgentsList).await {
        tools.fallbacks = list
            .fallbacks
            .into_iter()
            .map(|f| (f.tool, f.fallback))
            .collect();
        tools.default_model = list.default_model;
    }
    // Every tool's active limit (USG-10), from Activity's per-tool data.
    if let Ok(Reply::Activity(activity)) = app.service.send(Request::UsageActivity).await {
        tools.limits = activity.tools.into_iter().filter_map(|t| t.limit).collect();
    }
    for &tool in &tools.connected.clone() {
        if let Some(list) = model_list(app, tool).await {
            tools.lists.push((tool, list));
        }
    }
    tools.now = now_ms();
    tools
}

/// Every tool's state alone (`agent_tools.list`), for what needs only which tools are connected.
pub(super) async fn tool_states(app: &App) -> Tools {
    match app.service.send(Request::AgentToolsList).await {
        Ok(Reply::AgentTools(list)) => Tools::from_statuses(list.tools),
        _ => Tools::one(None),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Stage choices (AGM-09)

/// `ticket.choices` of a ticket, or of a new one in `project`; `None` when the service can't say (the field is then
/// left out).
async fn stage_choices_of(
    app: &App,
    key: Option<&str>,
    project: Option<&str>,
) -> Option<TicketChoices> {
    match app
        .service
        .send(Request::TicketChoices(TicketChoicesParams {
            key: key.map(str::to_owned),
            project: project.map(str::to_owned),
        }))
        .await
    {
        Ok(Reply::TicketChoices(c)) => Some(*c),
        _ => None,
    }
}

fn picked(form: &[FormChoice]) -> Vec<(String, String)> {
    form.iter()
        .map(|c| (c.stage_id.clone(), c.choice.clone()))
        .collect()
}

/// The picks that change an existing ticket's choices: not locked, and not what the stage reads now.
pub fn changed_choices(current: &TicketChoices, picks: &[(String, String)]) -> Vec<ChoicePick> {
    picks
        .iter()
        .filter(|(stage, pick)| {
            current
                .stages
                .iter()
                .find(|s| &s.stage_id == stage)
                .is_some_and(|s| !s.locked && s.words().contains(pick) && *pick != s.label())
        })
        .map(|(stage, choice)| ChoicePick {
            stage: stage.clone(),
            choice: choice.clone(),
            reason: None,
        })
        .collect()
}

/// Writes a new ticket's picks in one `ticket.update` (all checked before any is written).
async fn set_choices(
    app: &App,
    key: &str,
    picks: Vec<(String, String)>,
) -> Result<(), RequestError> {
    if picks.is_empty() {
        return Ok(());
    }
    let choices = picks
        .into_iter()
        .map(|(stage, choice)| ChoicePick {
            stage,
            choice,
            reason: None,
        })
        .collect();
    app.service
        .send(Request::TicketUpdate(TicketUpdateParams {
            key: key.to_owned(),
            update: TicketUpdate {
                choices,
                ..TicketUpdate::default()
            },
        }))
        .await
        .map(|_| ())
}

async fn project_base(app: &App, project: &str) -> Result<String, RequestError> {
    Ok(self::project(app, project).await?.base_branch)
}

// ---------------------------------------------------------------------------------------------------------------
// editor.branches (DEP-04, d0051)

/// The base dropdown (*08 → Row · base branch per ticket*): LOCAL branches, the project default marked, then
/// REMOTE ones by their short name. An item's id is what the form stores ("release/2.3", "origin/release/2.2").
pub fn branches_of(local: &[String], remote: &[String], default: &str) -> Vec<MenuItem> {
    let (local, remote) = base_choices(local, remote);
    let mut items = vec![MenuItem::header("LOCAL")];
    items.extend(local.into_iter().map(|b| {
        let item = MenuItem::new(b.clone(), b.clone()).icon(IconName::GitBranch);
        if b == default {
            item.shortcut("project default")
        } else {
            item
        }
    }));
    if !remote.is_empty() {
        items.push(MenuItem::header("REMOTE"));
        items.extend(
            remote
                .into_iter()
                .map(|(full, name)| MenuItem::new(full, name).icon(IconName::GitBranch)),
        );
    }
    items
}

/// What `editor.branches` takes.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "EditorBranchesParams")]
pub struct BranchesParams {
    pub project: String,
}

pub async fn branches(app: &App, params: BranchesParams) -> Result<Vec<MenuItem>, RequestError> {
    let default = project_base(app, &params.project).await?;
    match app
        .service
        .send(Request::ProjectBranches(ProjectParams {
            project: params.project,
        }))
        .await?
    {
        Reply::Branches(b) => Ok(branches_of(&b.local, &b.remote, &default)),
        other => Err(unexpected("project.branches", &other)),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// editor.compare (TKT-05)

/// What `editor.compare` takes: the ticket and, from a field's marker, that field only.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "EditorCompareParams")]
pub struct CompareParams {
    pub key: String,
    pub field: Option<EditorField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EditorCompareEntry")]
pub struct CompareEntry {
    /// The field it changed: the description renders as Markdown, the others stay plain text (d0078).
    pub field: EditorField,
    pub heading: String,
    pub before: String,
    pub after: String,
}

/// The agent's edits before / after, newest first; reading them marks them seen (the history stays).
pub async fn compare_view(
    app: &App,
    params: CompareParams,
) -> Result<Vec<CompareEntry>, RequestError> {
    let edits = agent_edits(app, &params.key).await;
    let entries = compare_entries(&edits, params.field, now_ms());
    app.service
        .send(Request::TicketAgentEditsSeen(KeyParams { key: params.key }))
        .await?;
    Ok(entries)
}

fn compare_entries(edits: &AgentEdits, only: Option<EditorField>, now_ms: i64) -> Vec<CompareEntry> {
    compare(edits, only.map(EditorField::core), now_ms)
        .into_iter()
        .map(|e| CompareEntry {
            field: EditorField::of(e.field),
            heading: e.heading,
            before: e.before,
            after: e.after,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::domain::Status;
    use otaq_core::git::testing::TempRepos;
    use otaq_view::view_models::card_fixtures::ticket as fixture;
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    fn info() -> ProjectInfo {
        ProjectInfo {
            key: "SHOP".into(),
            name: "shop-web".into(),
            repo_path: "/tmp/shop-web".into(),
            base_branch: "main".into(),
            agents_working: 0,
            needs_you: 0,
            flow_setup_since: None,
            paused_since: None,
        }
    }

    fn form(title: &str) -> Form {
        Form {
            title: title.into(),
            body: String::new(),
            priority: None,
            base_branch: None,
            external_id: String::new(),
            choices: Vec::new(),
            report: None,
            report_line: None,
        }
    }

    #[test]
    fn a_new_ticket_starts_normal_on_the_project_base() {
        let view = create_view(&info());
        assert!(view.create);
        assert_eq!(view.texts.primary, "Create ticket");
        assert_eq!(view.texts.body_hint, "Markdown \u{b7} paste keeps formatting");
        assert_eq!(
            view.texts.collapse_word,
            "collapse (after closing a popover)"
        );
        assert_eq!(view.form.base_branch.as_deref(), Some("main"));
        assert_eq!(view.shown_priority, 3);
        assert_eq!(view.base, BaseView::Editable);
    }

    #[test]
    fn read_mode_marks_the_agents_edits_and_locks_the_base_after_start() {
        let mut t = fixture("SHOP-19", "Price table", Status::Working);
        t.base_locked = true;
        t.body = "Toggle.\n\n## Done when\n\n- Works".into();
        let view = edit_view(&t, "shop-web", &AgentEdits::default(), true, 0);
        let read = view.read.unwrap();
        assert_eq!(read.body_markdown, t.body, "the stored Markdown, untouched");
        assert!(!read.body_empty);
        assert!(read.marked.is_empty());
        assert_eq!(
            view.save_hint.as_deref(),
            Some("SHOP-19\u{2019}s agent will be told what changed.")
        );
        assert_eq!(
            view.base,
            BaseView::Locked {
                text: "locked \u{b7} the ticket has started".into()
            }
        );
    }

    /// *08 → 7 · Updated by agent · compare*: each entry names its field, so the page renders the description's
    /// before / after as Markdown and keeps the other fields plain; the Markdown arrives as stored.
    #[test]
    fn compare_entries_name_their_field() {
        use otaq_core::board::agent_edits::AgentChange;
        use otaq_core::board::tickets::FieldChange;
        let change = |field, before: &str, after: &str, at| AgentChange {
            actor: Actor::Agent("SHOP-12".parse().unwrap()),
            at,
            change: FieldChange {
                field,
                before: Some(before.into()),
                after: Some(after.into()),
            },
        };
        let edits = AgentEdits {
            updated_by_agent: true,
            last_agent: None,
            last_at: Some(2_000),
            changes: vec![
                change(Field::Title, "Dark mode", "Add dark mode toggle", 2_000),
                change(Field::Body, "Toggle.\n\n- [x] a", "Toggle.\n\n- [x] a\n- [ ] b", 1_000),
            ],
        };
        let all = compare_entries(&edits, None, 60_000);
        let fields: Vec<EditorField> = all.iter().map(|e| e.field).collect();
        assert_eq!(fields, [EditorField::Title, EditorField::Body]);
        assert_eq!(all[1].after, "Toggle.\n\n- [x] a\n- [ ] b");
        let body = compare_entries(&edits, Some(EditorField::Body), 60_000);
        assert_eq!(body.len(), 1);
        assert_eq!(serde_json::to_value(&body[0]).unwrap()["field"], "body");
    }

    /// *05 → Ticket tab · rich Markdown read mode*: the page gets the description as stored, byte for byte (Copy as
    /// Markdown copies it), and an empty one shows "Nothing written yet.".
    #[test]
    fn read_mode_sends_the_stored_markdown() {
        let mut t = fixture("SHOP-47", "Checkout address autocomplete", Status::Backlog);
        t.body = "## Why\r\nUsers mistype **addresses**  \r\n* [x] 3 chars\n\n| a |\n|:-|\n| \\* |".into();
        let read = edit_view(&t, "shop-web", &AgentEdits::default(), false, 0)
            .read
            .unwrap();
        assert_eq!(read.body_markdown, t.body);
        assert!(!read.body_empty);

        for empty in ["", " \n\n\t"] {
            t.body = empty.into();
            let read = edit_view(&t, "shop-web", &AgentEdits::default(), false, 0)
                .read
                .unwrap();
            assert!(read.body_empty, "{empty:?}");
            assert_eq!(read.body_markdown, empty);
        }
    }

    /// *08 → Editing while it validates (SHOP-29)*.
    #[test]
    fn editing_while_it_validates_warns_first() {
        let mut t = fixture("SHOP-29", "Coupon field", Status::Checking);
        t.base_locked = true;
        let view = edit_view(&t, "shop-web", &AgentEdits::default(), true, 0);
        assert_eq!(
            view.save_warning.as_deref(),
            Some("SHOP-29\u{2019}s validation will be cancelled and its agent told what changed.")
        );
        assert_eq!(
            view.unsaved_warning,
            Some("unsaved \u{b7} saving cancels the validation")
        );
        t.status = Status::Working;
        let view = edit_view(&t, "shop-web", &AgentEdits::default(), true, 0);
        assert_eq!((view.save_warning, view.unsaved_warning), (None, None));
    }

    #[test]
    fn the_form_becomes_the_model_and_refusals_find_their_field() {
        let mut f = form("  ");
        f.priority = Some(2);
        let model = model_of(EditorModel::create("SHOP", "", "main"), &f);
        assert_eq!(model.title_error(), Some(TITLE_REQUIRED));
        assert_eq!(model.priority, Some(Priority::High));

        let cross = RequestError {
            message: "SHOP-31 starts from release/2.3".into(),
            refusal: Some("cross_base".into()),
            not_found: false,
        };
        assert_eq!(Refusal::of(&cross, RefusalField::Form).field, RefusalField::Base);
        assert_eq!(
            Refusal::of(&cross, RefusalField::Form).title,
            "Different base branches"
        );
        let local = branches_of(
            &["main".into(), "release/2.3".into()],
            &["origin/main".into(), "origin/release/2.2".into()],
            "main",
        );
        let labels: Vec<_> = local.iter().map(|i| (i.id.as_str(), i.label.as_str())).collect();
        assert_eq!(
            labels,
            [
                ("", "LOCAL"),
                ("main", "main"),
                ("release/2.3", "release/2.3"),
                ("", "REMOTE"),
                ("origin/release/2.2", "release/2.2")
            ]
        );
        assert_eq!(local[1].shortcut.as_deref(), Some("project default"));
    }

    #[tokio::test]
    async fn the_editor_creates_edits_and_links_real_tickets() {
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
        let create = json!({ "mode": "create", "project": project });
        let view = table(&h.app, "editor.open", create.clone()).await.unwrap();
        assert_eq!(view["heading"], "New ticket");

        let missing = table(&h.app, "editor.save", json!({ "target": create, "form": form("") }))
            .await
            .unwrap();
        assert_eq!(missing["outcome"], "title_missing");

        let saved = table(
            &h.app,
            "editor.save",
            json!({ "target": create, "form": form("Checkout API") }),
        )
        .await
        .unwrap();
        assert_eq!(saved["outcome"], "saved");
        let parent = saved["key"].as_str().unwrap().to_owned();

        let saved = table(
            &h.app,
            "editor.save",
            json!({ "target": create, "form": form("Checkout page") }),
        )
        .await
        .unwrap();
        let child = saved["key"].as_str().unwrap().to_owned();

        let edit = json!({ "mode": "edit", "key": child });
        let view = table(&h.app, "editor.open", edit.clone()).await.unwrap();
        assert_eq!(view["deps"], Value::Null, "dependencies are the sheet's (DPV-04)");
        assert_eq!(view["read"]["origin"], "Created by you");

        let mut changed = view["form"].clone();
        changed["title"] = json!("Checkout page v2");
        let saved = table(&h.app, "editor.save", json!({ "target": edit, "form": changed }))
            .await
            .unwrap();
        assert_eq!(saved["outcome"], "saved");
        let view = table(&h.app, "editor.open", edit.clone()).await.unwrap();
        assert_eq!(view["read"]["title"], "Checkout page v2");

        // Overview's Save (SHT-03): only the description, even from a form that still holds the old title.
        let mut stale = changed.clone();
        stale["title"] = json!("Checkout page");
        stale["body"] = json!("Post the cart.\n\n- [ ] show 422 errors inline");
        let in_place = json!({ "mode": "description", "key": child });
        let saved = table(&h.app, "editor.save", json!({ "target": in_place, "form": stale }))
            .await
            .unwrap();
        assert_eq!(saved["outcome"], "saved");
        let view = table(&h.app, "editor.open", edit).await.unwrap();
        assert_eq!(view["read"]["title"], "Checkout page v2");
        assert_eq!(
            view["read"]["body_markdown"],
            "Post the cart.\n\n- [ ] show 422 errors inline"
        );

        let compare = table(&h.app, "editor.compare", json!({ "key": child, "field": null }))
            .await
            .unwrap();
        assert_eq!(compare, json!([]), "the user's own edits aren't the agent's");
        let branches = table(&h.app, "editor.branches", json!({ "project": project }))
            .await
            .unwrap();
        assert_eq!(branches[1]["shortcut"], "project default");

        // The Markdown toggle (d0078 D9): off at first, then every editor opens in the mode last chosen.
        assert_eq!(view["source_mode"], false);
        table(&h.app, "editor.source_mode", json!({ "on": true })).await.unwrap();
        let view = table(&h.app, "editor.open", create.clone()).await.unwrap();
        assert_eq!(view["source_mode"], true);
        let view = table(&h.app, "editor.open", json!({ "mode": "edit", "key": parent }))
            .await
            .unwrap();
        assert_eq!(view["source_mode"], true);
        table(&h.app, "editor.source_mode", json!({ "on": false })).await.unwrap();
        let view = table(&h.app, "editor.open", create).await.unwrap();
        assert_eq!(view["source_mode"], false);
        h.service.stop_all().await;
    }

    /// AGM-09 through a real service, on the default flow: no stage has options, so the form shows no Stage choices
    /// and the sheet only the ticket's path (no Choices); a stage choice the flow doesn't offer is refused on save.
    #[tokio::test]
    async fn the_default_flow_shows_only_the_path() {
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
        let create = json!({ "mode": "create", "project": project });
        let view = table(&h.app, "editor.open", create.clone()).await.unwrap();
        assert_eq!(view["choices"], Value::Null);

        let saved = table(&h.app, "editor.save", json!({ "target": create, "form": form("Price table") }))
            .await
            .unwrap();
        let key = saved["key"].as_str().unwrap().to_owned();
        let edit = json!({ "mode": "edit", "key": key });
        assert_eq!(table(&h.app, "editor.open", edit).await.unwrap()["choices"], Value::Null);
        let load = table(&h.app, "sheet.view", json!({ "key": key })).await.unwrap();
        assert_eq!(load["view"]["choices"], Value::Null);
        let summary = load["view"]["path"]["summary"].as_str().unwrap();
        assert!(summary.starts_with("Dev \u{b7} Developer \u{b7} "), "{summary}");
        assert!(summary.ends_with("\u{b7} Review \u{b7} Done"), "{summary}");
        let stages: Vec<&str> = load["view"]["path"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["stage"].as_str().unwrap())
            .collect();
        assert_eq!(stages, ["Dev", "Validate", "Review", "Done"]);
        assert_eq!(load["view"]["token_split"], Value::Null, "nothing ran yet");
        // The sheet's pick on a stage without options is refused, with nothing changed.
        let refused = table(
            &h.app,
            "sheet.set_choice",
            json!({ "key": key, "stage": "dev", "choice": "Developer" }),
        )
        .await;
        assert!(refused.is_err());
        h.service.stop_all().await;
    }
}
