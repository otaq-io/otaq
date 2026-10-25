//! Agent edits in the editor's read mode (TKT-05, UI-05; d0019; *08 · Ticket editor → Ticket tab · read mode,
//! with agent edits; Compare · before / after*): a violet "Updated by agent" marker on each field an agent
//! changed since the user last looked, the origin line ("created by agent"), and the before/after list behind
//! Compare (title and description), newest first. Viewing the comparison marks the edits seen (the history
//! stays, T-0023).

use otaq_core::board::agent_edits::AgentEdits;
use otaq_core::board::tickets::{Field, Ticket};
use otaq_core::domain::Actor;

use crate::view_models::card::age_label;

/// One before/after pair, as Compare shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareEntry {
    /// The field it changed (the description renders as Markdown, d0078).
    pub field: Field,
    /// "“What and why” (the description) · changed by agent:SHOP-19 · 12m ago".
    pub heading: String,
    pub before: String,
    pub after: String,
}

/// A field's name as the editor shows it.
pub fn field_name(field: Field) -> &'static str {
    match field {
        Field::Title => "Title",
        Field::Body => "What and why",
        Field::Plan => "Plan",
        Field::Priority => "Priority",
        Field::BaseBranch => "Start from and merge into",
        Field::ExternalId => "External ID",
    }
}

/// Whether `field` carries the "Updated by agent" marker.
pub fn changed(edits: &AgentEdits, field: Field) -> bool {
    edits.changes.iter().any(|c| c.change.field == field)
}

/// "Created by you", or "Created by agent (SHOP-18)", plus "· edited by the agent · 4m ago" while edits are
/// unseen.
pub fn origin_line(ticket: &Ticket, edits: &AgentEdits, now_ms: i64) -> String {
    let created = match &ticket.created_by {
        Actor::Agent(key) => format!("Created by agent ({key})"),
        Actor::User(_) => "Created by you".to_owned(),
        // Its name comes with the Tauri UI (0074).
        Actor::ProjectAgent(_) => "Created by Noyan".to_owned(),
    };
    match edits.last_at {
        Some(at) if edits.updated_by_agent => {
            format!(
                "{created}  ·  edited by the agent · {} ago",
                age_label(now_ms - at)
            )
        }
        _ => created,
    }
}

/// The comparison, newest first, for every field or one.
pub fn compare(edits: &AgentEdits, only: Option<Field>, now_ms: i64) -> Vec<CompareEntry> {
    edits
        .changes
        .iter()
        .filter(|c| only.is_none_or(|f| c.change.field == f))
        .map(|c| CompareEntry {
            field: c.change.field,
            heading: format!(
                "“{}”{} · changed by {} · {} ago",
                field_name(c.change.field),
                // The description is the only place a ticket says what "done" means (d0067).
                if c.change.field == Field::Body {
                    " (the description)"
                } else {
                    ""
                },
                c.actor,
                age_label(now_ms - c.at)
            ),
            before: c.change.before.clone().unwrap_or_else(|| "—".into()),
            after: c.change.after.clone().unwrap_or_else(|| "(removed)".into()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::{NOW, ticket};
    use otaq_core::board::agent_edits::AgentChange;
    use otaq_core::board::tickets::FieldChange;
    use otaq_core::domain::Status;

    fn bot() -> Actor {
        Actor::Agent("SHOP-12".parse().unwrap())
    }

    fn edits() -> AgentEdits {
        let change = |field, before: Option<&str>, after: Option<&str>, at| AgentChange {
            actor: bot(),
            at,
            change: FieldChange {
                field,
                before: before.map(Into::into),
                after: after.map(Into::into),
            },
        };
        AgentEdits {
            updated_by_agent: true,
            last_agent: Some(bot()),
            last_at: Some(NOW - 4 * 60_000),
            changes: vec![
                change(
                    Field::Title,
                    Some("Dark mode"),
                    Some("Add dark mode toggle"),
                    NOW - 4 * 60_000,
                ),
                change(
                    Field::Body,
                    Some("Add a dark mode toggle to the settings page."),
                    Some("Add a light / dark toggle in Settings → Appearance."),
                    NOW - 10 * 60_000,
                ),
            ],
        }
    }

    #[test]
    fn markers_and_origin() {
        let mut t = ticket("SHOP-12", "Add dark mode toggle", Status::Working);
        let e = edits();
        assert!(changed(&e, Field::Body));
        assert!(changed(&e, Field::Title));
        assert!(!changed(&e, Field::BaseBranch));
        assert_eq!(
            origin_line(&t, &e, NOW),
            "Created by you  ·  edited by the agent · 4m ago"
        );
        t.created_by = bot();
        assert!(
            origin_line(&t, &AgentEdits::default(), NOW).starts_with("Created by agent (SHOP-12)")
        );
    }

    #[test]
    fn compare_is_newest_first_with_actor_and_time() {
        let all = compare(&edits(), None, NOW);
        assert_eq!(all.len(), 2);
        assert_eq!(
            all[0].heading,
            "“Title” · changed by agent:SHOP-12 · 4m ago"
        );
        assert_eq!(all[0].field, Field::Title);
        assert_eq!(all[0].before, "Dark mode");
        assert_eq!(
            all[1].heading,
            "“What and why” (the description) · changed by agent:SHOP-12 · 10m ago"
        );
        assert_eq!(
            all[1].before,
            "Add a dark mode toggle to the settings page."
        );
        let body = compare(&edits(), Some(Field::Body), NOW);
        assert_eq!(body.len(), 1);
        assert!(compare(&AgentEdits::default(), None, NOW).is_empty());
    }
}
