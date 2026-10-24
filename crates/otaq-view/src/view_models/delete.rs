//! The delete confirmation (TKT-08; d0060, d0018; *05 · Ticket side panel → Delete ticket · confirm, Epic
//! dialogs → Delete epic · confirm*; *17 → Undo a created ticket*): it names the ticket, says it never
//! started (so nothing on disk is affected), names every link that goes, and for an epic the children that go
//! with it. Cancel is the default button; nothing is deleted without the confirm click.

use otaq_core::board::delete::DeletePreview;
use otaq_core::board::tickets::Ticket;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::menu::MenuItem;
use crate::view_models::ticket_comments::cut;

pub const DELETE: &str = "delete";

/// The card menu's Delete… item: enabled only for tickets that never went to Dev.
pub fn menu_item(ticket: &Ticket) -> MenuItem {
    let item = MenuItem::new(DELETE, "Delete…")
        .icon(IconName::Trash2)
        .danger()
        .separated();
    if ticket.has_started() {
        item.disabled(true)
            .hint("Started tickets can only be closed.")
    } else {
        item.hint("Never started · nothing on disk")
    }
}

/// The confirmation's texts; `DeleteDialogView` on the page, where the board's card menu, the panel's "…" menu and
/// the Project agent's summary all open the same `delete_ticket` dialog (T-0221).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DeleteDialogView")]
pub struct DeleteText {
    pub title: String,
    /// One paragraph, sentence by sentence.
    pub body: Vec<String>,
    pub confirm: String,
    pub confirm_icon: IconName,
    /// The service refused (the ticket started meanwhile): shown in the dialog, and the confirm button is off.
    pub error: Option<String>,
}

/// A ticket a Project agent created says so first (*18 → Delete a created ticket · confirm (0060)*): "Project agent
/// “Checkout slowness” created it and it never started."
pub fn created_by_agent(mut text: DeleteText, agent: &str) -> DeleteText {
    if let Some(first) = text.body.first_mut()
        && first.starts_with("It never started")
    {
        *first = format!("Project agent \u{201c}{agent}\u{201d} created it and it never started.");
    }
    text
}

/// The dialog when the service refuses the preview: "Delete SHOP-19?" with the service's own words.
pub fn refused(key: &str, message: impl Into<String>) -> DeleteText {
    DeleteText {
        title: format!("Delete {key}?"),
        body: Vec::new(),
        confirm: "Delete ticket".to_owned(),
        confirm_icon: IconName::Trash2,
        error: Some(message.into()),
    }
}

/// The toast after a delete: "Deleted SHOP-21", "Deleted SHOP-40 and 2 tickets".
pub fn deleted(preview: &DeletePreview) -> String {
    match preview.children.len() {
        0 => format!("Deleted {}", preview.key),
        n => format!(
            "Deleted {} and {n} ticket{}",
            preview.key,
            if n == 1 { "" } else { "s" }
        ),
    }
}

fn join_keys(keys: &[String]) -> String {
    match keys {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// How much of the ticket's title the confirmation's heading shows (a line or two, #6); the rest ends in "…".
const TITLE_CHARS: usize = 60;

pub fn text(preview: &DeletePreview) -> DeleteText {
    let key = &preview.key;
    let epic_with_children = preview.is_epic && !preview.children.is_empty();
    let title = if epic_with_children {
        let keys: Vec<String> = preview
            .children
            .iter()
            .map(|(k, _)| k.to_string())
            .collect();
        let n = keys.len();
        format!(
            "Delete {key} and its {n} ticket{} ({})?",
            if n == 1 { "" } else { "s" },
            keys.join(", ")
        )
    } else {
        format!("Delete {key} {}?", cut(&preview.title, TITLE_CHARS))
    };
    let mut body = vec![if epic_with_children {
        "None of them started.".to_owned()
    } else {
        "It never started, so nothing on disk is affected.".to_owned()
    }];
    // Links where the deleted ticket itself is the dependent: "Its link to SHOP-20 is removed too."
    let own: Vec<String> = preview
        .links
        .iter()
        .filter(|(dependent, _)| dependent == key)
        .map(|(_, dependency)| dependency.to_string())
        .collect();
    if !own.is_empty() {
        body.push(format!(
            "Its link{} to {} {} removed too.",
            if own.len() == 1 { "" } else { "s" },
            join_keys(&own),
            if own.len() == 1 { "is" } else { "are" }
        ));
    }
    // Tickets that wait on what goes.
    let mut by_dependency: Vec<(String, Vec<String>)> = Vec::new();
    for (dependent, dependency) in preview.links.iter().filter(|(d, _)| d != key) {
        let name = if dependency == key {
            "it".to_owned()
        } else {
            dependency.to_string()
        };
        match by_dependency.iter_mut().find(|(n, _)| *n == name) {
            Some((_, list)) => list.push(dependent.to_string()),
            None => by_dependency.push((name, vec![dependent.to_string()])),
        }
    }
    for (dependency, dependents) in by_dependency {
        let many = dependents.len() > 1;
        body.push(format!(
            "{} {} on {dependency}; {} will be removed.",
            join_keys(&dependents),
            if many { "depend" } else { "depends" },
            if many { "those links" } else { "that link" }
        ));
    }
    let confirm = if epic_with_children {
        let n = preview.children.len();
        format!("Delete epic & {n} ticket{}", if n == 1 { "" } else { "s" })
    } else if preview.is_epic {
        "Delete epic".to_owned()
    } else {
        "Delete ticket".to_owned()
    };
    DeleteText {
        title,
        body,
        confirm,
        confirm_icon: IconName::Trash2,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::{Status, TicketKey};

    fn key(k: &str) -> TicketKey {
        k.parse().unwrap()
    }

    fn preview(links: &[(&str, &str)]) -> DeletePreview {
        DeletePreview {
            key: key("SHOP-21"),
            title: "Order confirmation email".into(),
            is_epic: false,
            children: Vec::new(),
            links: links.iter().map(|(a, b)| (key(a), key(b))).collect(),
        }
    }

    #[test]
    fn menu_item_is_disabled_for_started_tickets() {
        let mut t = ticket("SHOP-21", "Order confirmation email", Status::Backlog);
        let item = menu_item(&t);
        assert!(!item.disabled && item.danger);
        assert_eq!(
            item.hint.as_deref(),
            Some("Never started · nothing on disk")
        );
        t.started_at = Some(1);
        let item = menu_item(&t);
        assert!(item.disabled);
        assert_eq!(
            item.hint.as_deref(),
            Some("Started tickets can only be closed.")
        );
    }

    #[test]
    fn no_dependents() {
        let t = text(&preview(&[]));
        assert_eq!(t.title, "Delete SHOP-21 Order confirmation email?");
        assert_eq!(
            t.body,
            ["It never started, so nothing on disk is affected."]
        );
        assert_eq!(t.confirm, "Delete ticket");
    }

    #[test]
    fn a_project_agent_s_ticket_names_the_agent() {
        let t = created_by_agent(
            text(&preview(&[("SHOP-21", "SHOP-20")])),
            "Checkout slowness",
        );
        assert_eq!(
            t.body,
            [
                "Project agent \u{201c}Checkout slowness\u{201d} created it and it never started.",
                "Its link to SHOP-20 is removed too.",
            ]
        );
    }

    #[test]
    fn one_dependent_and_its_own_link() {
        let t = text(&preview(&[("SHOP-21", "SHOP-20"), ("SHOP-22", "SHOP-21")]));
        assert_eq!(
            t.body,
            [
                "It never started, so nothing on disk is affected.",
                "Its link to SHOP-20 is removed too.",
                "SHOP-22 depends on it; that link will be removed.",
            ]
        );
    }

    #[test]
    fn three_dependents() {
        let t = text(&preview(&[
            ("SHOP-22", "SHOP-21"),
            ("SHOP-23", "SHOP-21"),
            ("SHOP-24", "SHOP-21"),
        ]));
        assert_eq!(
            t.body[1],
            "SHOP-22, SHOP-23 and SHOP-24 depend on it; those links will be removed."
        );
    }

    #[test]
    fn a_long_title_is_cut_to_about_sixty_characters() {
        let mut p = preview(&[]);
        p.title = "Make the weekly summary card show streaks, totals and the best day, with a tooltip per day and a \
                   link to the full history page for that week"
            .into();
        let t = text(&p);
        assert_eq!(
            t.title,
            "Delete SHOP-21 Make the weekly summary card show streaks, totals and the\u{2026}?"
        );
        // A title that fits stays whole (see `no_dependents`).
        p.title = "x".repeat(60);
        assert_eq!(
            text(&p).title,
            format!("Delete SHOP-21 {}?", "x".repeat(60))
        );
    }

    #[test]
    fn an_epic_with_two_children() {
        let p = DeletePreview {
            key: key("SHOP-40"),
            title: "Wishlist".into(),
            is_epic: true,
            children: vec![
                (key("SHOP-40.1"), "Wishlist API".into()),
                (key("SHOP-40.2"), "Wishlist button".into()),
            ],
            links: vec![(key("SHOP-41"), key("SHOP-40"))],
        };
        let t = text(&p);
        assert_eq!(
            t.title,
            "Delete SHOP-40 and its 2 tickets (SHOP-40.1, SHOP-40.2)?"
        );
        assert_eq!(
            t.body,
            [
                "None of them started.",
                "SHOP-41 depends on it; that link will be removed.",
            ]
        );
        assert_eq!(t.confirm, "Delete epic & 2 tickets");
        assert_eq!(deleted(&p), "Deleted SHOP-40 and 2 tickets");
    }

    #[test]
    fn a_refusal_and_the_toast_after_a_delete() {
        let r = refused("SHOP-19", "SHOP-19 has started; close it instead");
        assert_eq!(r.title, "Delete SHOP-19?");
        assert_eq!(
            r.error.as_deref(),
            Some("SHOP-19 has started; close it instead")
        );
        assert_eq!(deleted(&preview(&[])), "Deleted SHOP-21");
        let json = serde_json::to_value(text(&preview(&[]))).unwrap();
        assert_eq!(json["confirm_icon"], "trash-2");
        assert_eq!(json["error"], serde_json::Value::Null);
    }
}
