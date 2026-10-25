//! A ticket's agent fixing the tickets it filed (d0129): while they haven't started it edits or deletes them, sets
//! their stage choices and links them ([`crate::policy::may_fix`]). Each change is logged on the creating ticket too
//! ("Edited SHOP-42" · "title, priority"; "Deleted SHOP-43" · "merged into SHOP-42"), and `prime` lists what it may
//! still change ("Tickets you created, not started: SHOP-42, SHOP-43").

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::tickets::{EventKind, Field, Ticket, is_never_started, record_event};
use super::{Board, BoardError};
use crate::domain::{Actor, TicketKey};

/// What the creating ticket's agent changed on a ticket it filed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum Fix {
    /// Fields of `otaq update`.
    Edited { fields: Vec<Field> },
    /// "SHOP-42 depends on `parent`" was added.
    DependencyAdded { parent: TicketKey },
    /// "SHOP-42 depends on `parent`" was removed.
    DependencyRemoved { parent: TicketKey },
    /// Stage choices, as History reads them: "Dev: Quick developer".
    ChoicesSet { choices: Vec<String> },
    /// The ticket was deleted; the agent's reason, when it gave one.
    Deleted {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl Board {
    /// Logs on the creating ticket `creator_id` that its agent changed (or deleted) `target`
    /// ([`EventKind::FixedCreatedTicket`]).
    pub async fn record_fix(
        &self,
        creator_id: &str,
        target: &TicketKey,
        fix: &Fix,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let mut tx = self.store.begin_write().await?;
        record_event(
            &mut tx,
            creator_id,
            EventKind::FixedCreatedTicket,
            actor,
            fix_payload(target, fix),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// The History payload of a fix: `{"ticket": "SHOP-42", "change": "edited", "fields": ["title"]}`.
pub fn fix_payload(target: &TicketKey, fix: &Fix) -> Value {
    let mut payload = serde_json::to_value(fix).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut payload {
        map.insert("ticket".to_owned(), json!(target));
    }
    payload
}

/// The fields `otaq update` changed between `before` and `after`, for [`Fix::Edited`].
pub fn edited_fields(before: &Ticket, after: &Ticket) -> Vec<Field> {
    [
        (Field::Title, before.title != after.title),
        (Field::Body, before.body != after.body),
        (Field::Plan, before.plan != after.plan),
        (Field::Priority, before.priority != after.priority),
        (Field::BaseBranch, before.base != after.base),
        (Field::ExternalId, before.external_id != after.external_id),
    ]
    .into_iter()
    .filter_map(|(field, changed)| changed.then_some(field))
    .collect()
}

/// The tickets `creator`'s agent filed that never started, in key order: what it may still change (d0129).
pub fn created_not_started(creator: &TicketKey, tickets: &[Ticket]) -> Vec<TicketKey> {
    let own = Actor::Agent(creator.clone());
    let mut keys: Vec<TicketKey> = tickets
        .iter()
        .filter(|t| t.created_by == own && is_never_started(t))
        .map(|t| t.key.clone())
        .collect();
    keys.sort();
    keys
}

/// `prime`'s list of [`created_not_started`] and what the agent may do with them; empty when there are none.
pub fn render_created_not_started(keys: &[TicketKey]) -> String {
    if keys.is_empty() {
        return String::new();
    }
    let cmd = crate::naming::COMMAND_NAME;
    let list = keys
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\n## Tickets you filed\n\nTickets you created, not started: {list}. You may still fix them: `{cmd} update \
         <KEY>`, `{cmd} dep add|remove <KEY> <PARENT>`, the stage choices (`--stage`, never over one the user set), \
         and `{cmd} delete <KEY> --reason \"…\"`. Tickets you didn't create, and started ones, stay as they are.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: &str) -> TicketKey {
        k.parse().unwrap()
    }

    #[test]
    fn the_payload_names_the_ticket_and_the_change() {
        assert_eq!(
            fix_payload(
                &key("SHOP-42"),
                &Fix::Edited {
                    fields: vec![Field::Title, Field::Priority]
                }
            ),
            json!({"ticket": "SHOP-42", "change": "edited", "fields": ["title", "priority"]})
        );
        assert_eq!(
            fix_payload(
                &key("SHOP-43"),
                &Fix::Deleted {
                    title: "Order email".into(),
                    reason: Some("merged into SHOP-42".into())
                }
            ),
            json!({"ticket": "SHOP-43", "change": "deleted", "title": "Order email",
                   "reason": "merged into SHOP-42"})
        );
        assert_eq!(
            fix_payload(
                &key("SHOP-41"),
                &Fix::DependencyAdded {
                    parent: key("SHOP-40")
                }
            ),
            json!({"ticket": "SHOP-41", "change": "dependency_added", "parent": "SHOP-40"})
        );
    }

    #[test]
    fn prime_lists_what_it_may_change() {
        assert_eq!(render_created_not_started(&[]), "");
        let text = render_created_not_started(&[key("SHOP-42"), key("SHOP-43")]);
        assert!(
            text.contains("Tickets you created, not started: SHOP-42, SHOP-43."),
            "{text}"
        );
        assert!(text.contains("delete <KEY> --reason"), "{text}");
    }
}
