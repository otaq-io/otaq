//! The request table: the door's one request call lands here and is dispatched by the area before the dot of its
//! method name (`"service.status"` → `service`). Each area has its own `api/<area>.rs` with a
//! `pub async fn request(app, method, params)` that matches its full method names and ends with
//! `_ => Err(unknown(method))`, and one line in the [`areas!`] list below.

use serde::Serialize;
use serde_json::Value;

use crate::App;
pub use crate::service::RequestError;
#[cfg(test)]
pub use agent_tools::{
    DisconnectOutcome as AgentToolDisconnectOutcome, HiddenParams as AgentToolHiddenParams,
    MaybeToolParams as AgentToolsRecheckParams, OneTool as AgentToolParams,
    SignIn as AgentToolSignIn, ViewParams as AgentToolsViewParams,
};
#[cfg(test)]
pub use apply_now::export_types as export_apply_now_types;
#[cfg(test)]
pub use ask_bar::export_types as export_ask_bar_types;
#[cfg(test)]
pub use board::{
    DragBackParams as BoardDragBackParams, DropParams as BoardDropParams,
    FocusParams as BoardFocusParams, MenuParams as CardMenuParams, SearchParams,
    SetViewParams as BoardSetViewParams, TicketParams as BoardTicketParams,
};
#[cfg(test)]
pub use dependencies::export_types as export_dependency_types;
#[cfg(test)]
pub use drawer::{
    Gallery as DrawerGallery, Params as DrawerParams, TabParams as DrawerTabParams,
    View as DrawerView,
};
#[cfg(test)]
pub use editor::{
    BranchesParams as EditorBranchesParams, CompareEntry as EditorCompareEntry,
    CompareParams as EditorCompareParams, SaveOutcome as EditorSaved,
    SaveParams as EditorSaveParams, SourceModeParams as EditorSourceModeParams, View as EditorView,
};
#[cfg(test)]
pub use epic::export_types as export_epic_types;
#[cfg(test)]
pub use file_links::{FileLinkCheck, FileLinksFind};
#[cfg(test)]
pub use files::{
    AddParams as FilesCommentAdd, BarParams as FilesBarParams, FileParams as FilesFileParams,
    IdParams as FilesCommentId, Load as FilesLoad, ModeParams as FilesModeParams,
    Params as FilesParams, ReviewNotes as FilesReviewNotes, SearchParams as FilesSearchParams,
    TextParams as FilesCommentText, TreeParams as FilesTreeParams,
};
#[cfg(test)]
pub use files_view::export_types as export_files_view_types;
#[cfg(test)]
pub use flow_editor::export_types as export_flow_editor_types;
#[cfg(test)]
pub use keymap::Keymap;
#[cfg(test)]
pub use layout::AgentPanelWidthParams as LayoutAgentPanelWidthParams;
#[cfg(test)]
pub use lead::{Load as LeadTabLoad, Params as LeadTabParams};
#[cfg(test)]
pub use notices::{
    DismissBanner, HoldToast, NoticesProject, NotificationOpen, ToastId, ToastPress,
};
#[cfg(test)]
pub use onboarding::export_flow_setup_page_types;
#[cfg(test)]
pub use onboarding::{
    AddOutcome as AddProjectOutcome, FormParams as AddProjectParams, Start as OnboardingStart,
};
#[cfg(test)]
pub use overview::export_types as export_overview_types;
#[cfg(test)]
pub use palette::{Use as PaletteUse, ViewParams as PaletteParams};
#[cfg(test)]
pub use pane::{
    EndParams as PaneEndParams, Ended as PaneEnded, Params as PaneParams,
    ProjectOnly as PaneProjectParams, RenameParams as PaneRenameParams,
    ResumeParams as PaneResumeParams, SendParams as PaneSendParams,
};
#[cfg(test)]
pub use panel::AnswerAgentParams as PanelAnswerAgentParams;
#[cfg(test)]
pub use panel::NotesGallery as OptionNotesGallery;
#[cfg(test)]
pub use panel::{
    ActionParams as PanelActionParams, AnswerParams as PanelAnswerParams,
    CardParams as PanelCardParams, ChoiceParams as PanelChoiceParams,
    CloseDialog as PanelCloseDialog, Load as PanelLoad, MenuParams as PanelMenuParams,
    Params as PanelParams, ParentParams as PanelParentParams,
    PriorityParams as PanelPriorityParams, SendAgainParams as PanelSendAgainParams,
    Sent as PanelSent, SubmitDialog as PanelSubmitDialog, SubmitParams as PanelSubmitParams,
};
#[cfg(test)]
pub use pause::export_types as export_pause_types;
#[cfg(test)]
pub use pr::{
    Answer as PrAnswer, CheckParams as PrCheckParams, ConfirmParams as PrConfirmParams,
    EditParams as PrEditParams, OpenParams as PrOpenParams, Params as PrParams,
    StartParams as PrStartParams,
};
#[cfg(test)]
pub use recovery::{
    CleanupParams, GroupParams as CleanupGroupParams, ResumeParams as RecoveryResumeParams,
    SummaryParams as AfterRestartParams, ToggleParams as CleanupToggleParams,
    WarmParams as CleanupWarmClearParams,
};
pub use report::OpenReports;
#[cfg(test)]
pub use report::export_types as export_report_types;
#[cfg(test)]
pub use review::{
    Available as ReviewAvailable, ChangeParams as DecisionChangeParams,
    ChangeViewParams as DecisionChangeViewParams, MergeParams as LocalMergeParams,
    Params as ReviewParams, SendBackParams, export_check_types as export_review_check_types,
};
#[cfg(test)]
pub use run::{
    CommandOpenParams as RunCommandOpenParams, CommandOpened as RunCommandOpened,
    CommandParams as RunCommandParams, OpenParams as RunOpenParams, Params as RunParams,
    SaveParams as RunCommandSaveParams, ShellTabParams, Started as RunStarted,
    SuggestionParams as RunCommandSuggestionParams,
};
#[cfg(test)]
pub use sections::NoteRequest as CreateFromNoteParams;
#[cfg(test)]
pub use sections::{
    AnswerMessage as AnswerMessageParams, CloseEpicDialogView, EpicParams, HistoryView,
    MessagesView, MoveDialogView, MoveTo as MoveToEpicParams, Params as SectionParams,
};
#[cfg(test)]
pub use settings::{
    AgentsOpen, SettingsEdit, SettingsLink, SettingsOpen, SettingsProject, SettingsRevealParams,
    export_agent_types, export_flow_types,
};
#[cfg(test)]
pub use shell::{
    Broom as CleanupBroom, DigitParams, Header as HeaderView, Params as ShellParams,
    Quick as QuickSwitcherView, QuickParams, Rail as RailView, StepParams,
};
#[cfg(test)]
pub use strip::export_types as export_strip_types;
#[cfg(test)]
pub use terminal::{
    TerminalDetach, TerminalFocus, TerminalInput, TerminalLink, TerminalResize, TerminalSettings,
};
#[cfg(test)]
pub use terminal_tab::{
    DeliverParams as TerminalTabDeliverParams, Load as TerminalTabLoad, Params as TerminalTabParams,
};
#[cfg(test)]
pub use theme::{ActiveTheme, ThemeParams, ThemePick};
#[cfg(test)]
pub use ticket_comments::export_types as export_ticket_comment_types;
#[cfg(test)]
pub use tools::Opened as ToolTerminalOpened;
#[cfg(test)]
pub use usage::{Ask as UsageDataAsk, UsageChoice};
pub use window::{WindowFocus, follow_window_focus, window_focused};

/// Declares each area's module and its arm of [`request`]: one line per area.
macro_rules! areas {
    ($($area:ident,)*) => {
        $(mod $area;)*

        pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
            // Counted only when the user agreed to send usage data (d0085); the params are looked at, never kept.
            app.usage.record(method, &params);
            let answer = match method.split_once('.').map_or(method, |(area, _)| area) {
                $(stringify!($area) => $area::request(app, method, params).await,)*
                _ => Err(unknown(method)),
            };
            // The dialog's answer or Settings' switch: the sender follows at once.
            if answer.is_ok() && matches!(method, "usage.choose" | "settings.edit") {
                let _ = usage::sync(app).await;
            }
            answer
        }
    };
}

areas! {
    service,
    theme,
    window,
    keymap,
    keyboard,
    shortcuts,
    layout,
    terminal,
    file_links,
    gallery,
    shell,
    notices,
    panel,
    board,
    settings,
    drawer,
    onboarding,
    tools,
    sections,
    editor,
    files,
    files_view,
    terminal_tab,
    review,
    strip,
    pr,
    recovery,
    run,
    lead,
    usage,
    pane,
    ask_bar,
    agent_tools,
    limits,
    palette,
    pause,
    flow_editor,
    apply_now,
    ticket_comments,
    report,
    overview,
    epic,
    dependencies,
}

/// Reads the user's answer once the service answers, then sends the counts every [`crate::usage::FLUSH_EVERY`]
/// (nothing while the answer isn't yes).
pub async fn usage_loop(app: std::sync::Arc<App>) {
    while usage::sync(&app).await.is_err() {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    loop {
        tokio::time::sleep(crate::usage::FLUSH_EVERY).await;
        let _ = usage::sync(&app).await;
        app.usage.flush().await;
    }
}

/// The answer to a method no area knows.
pub fn unknown(method: &str) -> RequestError {
    RequestError::plain(format!("unknown request: {method}"))
}

/// A handler's answer as the page receives it.
pub fn reply(value: &impl Serialize) -> Result<Value, RequestError> {
    serde_json::to_value(value).map_err(|e| RequestError::plain(e.to_string()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn the_table_dispatches_by_method_name_to_the_real_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;

        let connection = request(&h.app, "service.connection", Value::Null)
            .await
            .unwrap();
        assert_eq!(connection["state"], "connected");
        assert_eq!(connection["compatibility"], "same");

        let status = request(&h.app, "service.status", Value::Null)
            .await
            .unwrap();
        assert_eq!(status["pid"], std::process::id());
        assert_eq!(status["build_id"], connection["build_id"]);

        for method in ["board.nothing", "service.nothing", "service", ""] {
            assert_eq!(
                request(&h.app, method, json!({})).await,
                Err(unknown(method)),
                "{method:?}"
            );
        }
        assert_eq!(
            unknown("board.nothing"),
            RequestError::plain("unknown request: board.nothing")
        );
        h.service.stop_all().await;
    }
}
