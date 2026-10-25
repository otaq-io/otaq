//! The ticket editor (UI-05, TKT-03; d0019; *08 · Ticket editor → Quick-create in Backlog, Full form · filled,
//! Validation · empty form*): Title, Priority and the description (Markdown). A
//! missing title is the only hard rule; a short or empty description is fine (d0067). Saving an existing ticket
//! sends only what changed. Priority is sent only once picked (TKT-09, d0065): a new ticket is Normal unless
//! picked. A new ticket can take a report (pick,
//! drop or paste) with its one line (RPT-03, d0109): Create posts them as the ticket's first comment, the user's, "at
//! creation" ([`AttachState`]).

use otaq_core::board::reports::ReportSource;
use otaq_core::board::ticket_comments::FirstComment;
use otaq_core::board::tickets::{NewTicketInput, Ticket, TicketUpdate};
use otaq_core::domain::{Priority, TicketKey};
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::overview;
use crate::view_models::ticket_comments::CommentBoxView;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorMode {
    /// A new ticket in this project.
    Create {
        project: String,
    },
    Edit {
        key: TicketKey,
    },
}

/// The report a new ticket takes (RPT-03): a file picked, dropped or copied in Finder, by its path; or a pasted file's
/// text (the comment box's "Pasted text").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewReport {
    File(String),
    Pasted(String),
}

/// Where the New ticket form's report stands (RPT-03): none; one waiting for its line ("A comment always has its own
/// line; the file goes with it."); or one with its line, sent on Create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachState {
    None,
    NeedsLine,
    Ready,
}

/// The New ticket form's report place (RPT-03): "Report", Add (the system picker), the drop line, and once a file is
/// there the comment box with its chip and the line (the box's own words, a dialog's: no button of its own).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EditorReportField")]
pub struct ReportField {
    pub label: String,
    pub hint: String,
    pub add: String,
    pub pick_title: String,
    pub drop: String,
    pub line_box: CommentBoxView,
}

pub const REPORT_LABEL: &str = "Report";
/// Q-A: the frames don't draw the New ticket form's report; these words follow the column's note.
pub const REPORT_HINT: &str =
    "Optional \u{b7} posted as your first comment; agents read yours before they start";
pub const REPORT_LINE_PLACEHOLDER: &str = "One line: what the report shows\u{2026}";
pub const REPORT_FOOTER: &str = "Posted as your first comment \u{b7} You \u{b7} at creation";

/// The New ticket form's report place.
pub fn report_field() -> ReportField {
    ReportField {
        label: REPORT_LABEL.to_owned(),
        hint: REPORT_HINT.to_owned(),
        add: overview::ADD.to_owned(),
        pick_title: overview::PICK_TITLE.to_owned(),
        drop: overview::DROP.to_owned(),
        line_box: CommentBoxView::dialog("", REPORT_FOOTER, true)
            .placeholder(REPORT_LINE_PLACEHOLDER)
            .without_label(),
    }
}

/// The refusal of a Create whose report has no line (d0109: a comment always has its own line).
pub const REPORT_LINE_REQUIRED: &str = crate::view_models::ticket_comments::FILE_LINE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorModel {
    pub mode: EditorMode,
    pub title: String,
    pub body: String,
    /// The ticket's own priority, picked or saved; `None` until picked (a new ticket is then Normal).
    pub priority: Option<Priority>,
    /// "Start from and merge into".
    pub base_branch: Option<String>,
    pub base_locked: bool,
    pub external_id: String,
    /// Create only: the report the form took and its one line (RPT-03).
    pub report: Option<NewReport>,
    pub report_line: String,
    /// A refused link, shown under Depends on: (code, the service's message).
    pub dep_error: Option<(Option<String>, String)>,
    /// A refused base change, shown under the base field.
    pub base_error: Option<String>,
    /// Save was tried: show the title error.
    pub tried: bool,
    pub saving: bool,
    pub error: Option<String>,
    original: Option<Ticket>,
}

/// The title error, under the field.
pub const TITLE_REQUIRED: &str = "Add a title";

impl EditorModel {
    /// A new ticket; `base` is the project's base branch (the default).
    pub fn create(project: &str, title: &str, base: &str) -> Self {
        Self {
            mode: EditorMode::Create {
                project: project.to_owned(),
            },
            title: title.to_owned(),
            body: String::new(),
            priority: None,
            base_branch: Some(base.to_owned()),
            base_locked: false,
            external_id: String::new(),
            report: None,
            report_line: String::new(),
            dep_error: None,
            base_error: None,
            tried: false,
            saving: false,
            error: None,
            original: None,
        }
    }

    pub fn edit(ticket: &Ticket) -> Self {
        Self {
            mode: EditorMode::Edit {
                key: ticket.key.clone(),
            },
            title: ticket.title.clone(),
            body: ticket.body.clone(),
            priority: Some(ticket.priority),
            base_branch: Some(ticket.base.branch().to_owned()),
            base_locked: ticket.base_locked,
            external_id: ticket.external_id.clone().unwrap_or_default(),
            report: None,
            report_line: String::new(),
            dep_error: None,
            base_error: None,
            tried: false,
            saving: false,
            error: None,
            original: Some(ticket.clone()),
        }
    }

    /// The description edited in place in Overview (SHT-03): the saved ticket with only `body` changed, so its
    /// [`Self::update_input`] holds the description alone (nothing at all when it is unchanged), whatever else the
    /// ticket's fields became meanwhile.
    pub fn description(ticket: &Ticket, body: &str) -> Self {
        Self {
            body: body.to_owned(),
            tried: true,
            ..Self::edit(ticket)
        }
    }

    pub fn is_create(&self) -> bool {
        matches!(self.mode, EditorMode::Create { .. })
    }

    pub fn title_error(&self) -> Option<&'static str> {
        (self.tried && self.title.trim().is_empty()).then_some(TITLE_REQUIRED)
    }

    /// The priority the form shows: the ticket's own, else the saved one, else Normal.
    pub fn shown_priority(&self) -> Priority {
        self.priority
            .or_else(|| self.original.as_ref().map(|t| t.priority))
            .unwrap_or_default()
    }

    /// Save/Create is enabled once there is a title.
    pub fn can_save(&self) -> bool {
        !self.title.trim().is_empty() && !self.saving
    }

    /// The report's state (RPT-03).
    pub fn attach_state(&self) -> AttachState {
        match &self.report {
            Some(_) if !self.takes_report() => AttachState::None,
            None => AttachState::None,
            Some(_) if self.report_line.trim().is_empty() => AttachState::NeedsLine,
            Some(_) => AttachState::Ready,
        }
    }

    /// The form shows the report's place: a new ticket.
    pub fn takes_report(&self) -> bool {
        self.is_create()
    }

    /// Create is refused while the report waits for its line (once tried).
    pub fn report_error(&self) -> Option<&'static str> {
        (self.tried && self.attach_state() == AttachState::NeedsLine)
            .then_some(REPORT_LINE_REQUIRED)
    }

    /// The first comment Create posts: the line with its report, once both are there.
    fn first_comment(&self) -> Option<FirstComment> {
        if self.attach_state() != AttachState::Ready {
            return None;
        }
        let report = match self.report.as_ref()? {
            NewReport::File(path) => ReportSource::Path(path.into()),
            NewReport::Pasted(text) => ReportSource::Text(text.clone()),
        };
        Some(FirstComment {
            body: self.report_line.trim().to_owned(),
            report: Some(report),
        })
    }

    /// The `ticket.create` input.
    pub fn create_input(&self) -> Option<NewTicketInput> {
        let EditorMode::Create { project } = &self.mode else {
            return None;
        };
        Some(NewTicketInput {
            project: project.clone(),
            title: self.title.trim().to_owned(),
            body: self.body.clone(),
            // Only when picked: the service makes the rest Normal.
            priority: self.priority,
            base_branch: self.base_branch.clone(),
            external_id: (!self.external_id.trim().is_empty())
                .then(|| self.external_id.trim().to_owned()),
            first_comment: self.first_comment(),
            // The editor's Plan comes with T-0540.
            plan: String::new(),
        })
    }

    /// The `ticket.update` changes against the saved ticket, or `None` when nothing changed.
    pub fn update_input(&self) -> Option<TicketUpdate> {
        let original = self.original.as_ref()?;
        let mut update = TicketUpdate::default();
        if self.title.trim() != original.title {
            update.title = Some(self.title.trim().to_owned());
        }
        if self.body != original.body {
            update.body = Some(self.body.clone());
        }
        if self.priority.is_some() && self.priority != Some(original.priority) {
            update.priority = self.priority;
        }
        if let Some(new) = &self.base_branch
            && new != original.base.branch()
        {
            update.base_branch = Some(new.clone());
        }
        if self.external_id.trim() != original.external_id.as_deref().unwrap_or_default() {
            update.external_id = Some(self.external_id.trim().to_owned());
        }
        (update != TicketUpdate::default()).then_some(update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::Status;

    fn saved_ticket() -> Ticket {
        let mut t = ticket("SHOP-20", "Checkout page", Status::Backlog);
        t.body = "Post the cart; show 422 errors inline.".into();
        t
    }

    /// Overview's Save (SHT-03): only the description goes, never a title or priority the ticket has since; an
    /// untouched description sends nothing.
    #[test]
    fn the_description_in_place_sends_only_the_description() {
        let mut t = saved_ticket();
        t.priority = otaq_core::domain::Priority::High;
        let m = EditorModel::description(
            &t,
            "Post the cart; show 422 errors inline.\n\n- [ ] keep the cart",
        );
        let update = m.update_input().unwrap();
        assert_eq!(
            update,
            TicketUpdate {
                body: Some("Post the cart; show 422 errors inline.\n\n- [ ] keep the cart".into()),
                ..TicketUpdate::default()
            }
        );
        assert_eq!(EditorModel::description(&t, &t.body).update_input(), None);
    }

    #[test]
    fn create_needs_a_title_only() {
        let mut m = EditorModel::create("SHOP", "", "main");
        assert!(!m.can_save());
        assert_eq!(m.title_error(), None, "no error before trying");
        m.tried = true;
        assert_eq!(m.title_error(), Some(TITLE_REQUIRED));
        m.title = "  Add order history page ".into();
        assert!(m.can_save(), "an empty description is fine");
        m.body = "Shows past orders".into();
        let input = m.create_input().unwrap();
        assert_eq!(input.title, "Add order history page");
        assert_eq!(input.body, "Shows past orders");
        assert_eq!(input.project, "SHOP");
        assert_eq!(input.base_branch.as_deref(), Some("main"));
        assert_eq!(input.external_id, None);
        assert!(m.update_input().is_none());
    }

    /// RPT-03: the New ticket form's report waits for its line; with it, Create sends both as the first comment.
    #[test]
    fn the_new_ticket_forms_report_needs_its_line() {
        let mut m = EditorModel::create("SHOP", "Fix flaky login redirect", "main");
        assert_eq!(m.attach_state(), AttachState::None);
        assert_eq!(m.create_input().unwrap().first_comment, None);

        m.report = Some(NewReport::File("/Users/me/test-run-login.md".into()));
        assert_eq!(m.attach_state(), AttachState::NeedsLine);
        assert_eq!(m.report_error(), None, "no error before trying");
        m.tried = true;
        assert_eq!(
            m.report_error(),
            Some("A comment always has its own line; the file goes with it.")
        );
        assert_eq!(m.create_input().unwrap().first_comment, None);

        m.report_line = "  Fails 2 in 20 on Safari; the run is attached. ".into();
        assert_eq!(m.attach_state(), AttachState::Ready);
        assert_eq!(m.report_error(), None);
        assert_eq!(
            m.create_input().unwrap().first_comment,
            Some(FirstComment {
                body: "Fails 2 in 20 on Safari; the run is attached.".into(),
                report: Some(ReportSource::Path("/Users/me/test-run-login.md".into())),
            })
        );

        m.report = Some(NewReport::Pasted("running 20 tests".into()));
        assert_eq!(
            m.create_input().unwrap().first_comment.unwrap().report,
            Some(ReportSource::Text("running 20 tests".into()))
        );

        // An edit has no report place.
        assert!(!EditorModel::edit(&saved_ticket()).takes_report());
    }

    /// The report place's words: Overview's Add and drop line, a dialog's box with no label or button of its own that
    /// may stay empty until a file is there.
    #[test]
    fn the_report_places_words() {
        let f = report_field();
        assert_eq!(
            (f.label.as_str(), f.add.as_str(), f.drop.as_str()),
            ("Report", "Add", "Drop files here or paste")
        );
        assert_eq!(f.pick_title, "Attach a report");
        assert_eq!(f.line_box.label, None);
        assert_eq!(f.line_box.submit, None);
        assert!(f.line_box.optional);
        assert_eq!(f.line_box.placeholder, REPORT_LINE_PLACEHOLDER);
        assert_eq!(f.line_box.file_line, REPORT_LINE_REQUIRED);
    }

    /// The editor's payloads, sent to a real background service (temporary data folder), come back with their
    /// title and description: create, then an edit that changes both.
    #[test]
    fn title_and_description_round_trip_through_the_service() {
        use otaq_core::client::{Client, ClientConfig};
        use otaq_core::protocol::{KeyParams, Reply, Request, TicketUpdateParams};
        use otaq_core::service::testing::TestService;

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let w = TestService::start().await;
            w.add_project("shop-web").await;
            let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
            let get = async |key: &str| match client
                .request(Request::TicketGet(KeyParams { key: key.into() }))
                .await
                .unwrap()
            {
                Reply::Ticket(t) => *t,
                other => panic!("{other:?}"),
            };

            let mut m = EditorModel::create("SHOP", "Checkout page", "main");
            m.body = "Post the cart.".into();
            let Reply::Ticket(created) = client
                .request(Request::TicketCreate(m.create_input().unwrap()))
                .await
                .unwrap()
            else {
                panic!()
            };
            let stored = get(&created.key.to_string()).await;
            assert_eq!(
                (stored.title.as_str(), stored.body.as_str()),
                ("Checkout page", "Post the cart.")
            );

            // The Ticket tab: Edit, change both, Save.
            let mut m = EditorModel::edit(&stored);
            m.title = "Checkout page v2".into();
            m.body = "Post the cart.\n\nShow 422 errors inline.".into();
            client
                .request(Request::TicketUpdate(TicketUpdateParams {
                    key: stored.key.to_string(),
                    update: m.update_input().unwrap(),
                }))
                .await
                .unwrap();
            let stored = get(&stored.key.to_string()).await;
            assert_eq!(
                (stored.title.as_str(), stored.body.as_str()),
                (
                    "Checkout page v2",
                    "Post the cart.\n\nShow 422 errors inline."
                )
            );
            assert_eq!(EditorModel::edit(&stored).update_input(), None);
        });
    }

    /// Quick-create and the full form: Normal unless picked.
    #[test]
    fn priority_is_sent_only_once_picked() {
        let mut m = EditorModel::create("SHOP", "Checkout page", "main");
        assert_eq!(m.shown_priority(), Priority::Normal);
        assert_eq!(
            m.create_input().unwrap().priority,
            None,
            "the service's Normal"
        );
        m.priority = Some(Priority::High);
        assert_eq!(m.create_input().unwrap().priority, Some(Priority::High));
        assert_eq!(m.shown_priority(), Priority::High);
    }

    /// The edit form and the Ticket tab: only a change is sent.
    #[test]
    fn priority_changes_are_sent_with_the_update() {
        let mut saved = saved_ticket();
        saved.priority = Priority::High;
        let mut m = EditorModel::edit(&saved);
        assert_eq!(m.priority, Some(Priority::High));
        m.priority = Some(Priority::High);
        assert_eq!(m.update_input(), None, "the same priority is no change");
        m.priority = Some(Priority::Urgent);
        assert_eq!(m.update_input().unwrap().priority, Some(Priority::Urgent));
    }

    /// Quick-create's and the full form's priority reach a real service: picked, it is the ticket's own; not
    /// picked, Normal.
    #[test]
    fn priority_round_trips_through_the_service() {
        use otaq_core::client::{Client, ClientConfig};
        use otaq_core::protocol::{Reply, Request, TicketUpdateParams};
        use otaq_core::service::testing::TestService;

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let w = TestService::start().await;
            w.add_project("shop-web").await;
            let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
            let create = async |m: &EditorModel| match client
                .request(Request::TicketCreate(m.create_input().unwrap()))
                .await
                .unwrap()
            {
                Reply::Ticket(t) => *t,
                other => panic!("{other:?}"),
            };
            let mut m = EditorModel::create("SHOP", "Wishlist button", "main");
            m.priority = Some(Priority::High);
            let child = create(&m).await;
            assert_eq!(child.priority, Priority::High);

            let mut m = EditorModel::edit(&child);
            m.priority = Some(Priority::Low);
            let Reply::Ticket(child) = client
                .request(Request::TicketUpdate(TicketUpdateParams {
                    key: child.key.to_string(),
                    update: m.update_input().unwrap(),
                }))
                .await
                .unwrap()
            else {
                panic!()
            };
            assert_eq!(child.priority, Priority::Low);
            let plain = create(&EditorModel::create("SHOP", "Footer", "main")).await;
            assert_eq!(plain.priority, Priority::Normal);
        });
    }

    #[test]
    fn typed_external_id_is_sent() {
        let mut m = EditorModel::create("SHOP", "Checkout page", "main");
        m.external_id = "PAY-123".into();
        assert_eq!(
            m.create_input().unwrap().external_id.as_deref(),
            Some("PAY-123")
        );
    }

    #[test]
    fn base_and_external_id_changes() {
        let mut m = EditorModel::edit(&saved_ticket());
        m.base_branch = Some("release/2.3".into());
        m.external_id = " PAY-123 ".into();
        let update = m.update_input().unwrap();
        assert_eq!(update.base_branch.as_deref(), Some("release/2.3"));
        assert_eq!(update.external_id.as_deref(), Some("PAY-123"));
    }

    #[test]
    fn saving_an_edit_sends_only_the_changes() {
        let saved = saved_ticket();
        let mut m = EditorModel::edit(&saved);
        assert_eq!(m.update_input(), None, "nothing changed");
        m.title = "Checkout page v2".into();
        let update = m.update_input().unwrap();
        assert_eq!(update.title.as_deref(), Some("Checkout page v2"));
        assert_eq!(update.body, None);
        m.body = "Post the cart.\n\n## Done when\n- Works with an empty cart".into();
        let update = m.update_input().unwrap();
        assert_eq!(update.body.as_deref(), Some(m.body.as_str()));
        assert_eq!((update.base_branch, update.external_id), (None, None));
    }
}
