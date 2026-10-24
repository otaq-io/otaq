//! Agent activity as it happens (TKT-03, TKT-05, SES-08, UI-07; d0019, d0048): the "created by agent" origin
//! label on new Backlog cards, one success toast per batch of agent-created tickets (*11 · Banners, toasts & OS
//! notifications → Success toasts*), and the editor's note that the agent will be told what changed (*08 ·
//! Ticket editor → Editing while the agent works*).

use std::time::{Duration, Instant};

use otaq_core::board::change_note::change_parts;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status, TicketKey};
use otaq_core::signals::LIVE;

use crate::view_models::toasts::{NewToast, ToastLevel};

/// Tickets an agent creates within this quiet time make one toast (a `create -f` batch arrives as one burst).
pub const BATCH_QUIET: Duration = Duration::from_millis(800);

/// The title every agent-creation toast ends with; the toast host gives it the sparkles icon.
pub const CREATED_BY_AGENT: &str = "created by agent";

/// The card's origin label for a ticket a ticket's agent created, while it waits in Backlog. A Project agent's
/// tickets name the agent instead ([`project_agent_label`]).
pub fn origin_label(ticket: &Ticket) -> Option<&'static str> {
    (matches!(ticket.created_by, Actor::Agent(_)) && ticket.place.is_backlog())
        .then_some(CREATED_BY_AGENT)
}

/// "by Project agent “Checkout slowness”" on a Backlog card a Project agent created (d0068), with the agent's
/// current name; "by a Project agent" once it isn't listed any more.
pub fn project_agent_label(ticket: &Ticket, name: Option<&str>) -> Option<String> {
    if !matches!(ticket.created_by, Actor::ProjectAgent(_)) || !ticket.place.is_backlog() {
        return None;
    }
    Some(match name {
        Some(name) => format!("by Project agent \u{201c}{name}\u{201d}"),
        None => "by a Project agent".to_owned(),
    })
}

/// How long a ticket a Project agent created stays highlighted in Backlog.
pub const FRESH_MS: i64 = 8_000;

/// A Project agent created it moments ago: Backlog highlights it (*18 → live board feedback*).
pub fn fresh_from_project_agent(ticket: &Ticket, now_ms: i64) -> bool {
    matches!(ticket.created_by, Actor::ProjectAgent(_))
        && ticket.place.is_backlog()
        && (0..FRESH_MS).contains(&(now_ms - ticket.created_at))
}

/// A ticket an agent created, as the service told it (`created_by_agent`, #47).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    /// Who created it, as the toast names it: "DEMO-7" (a ticket's agent) or "Wishlist lead".
    pub by: String,
    pub key: TicketKey,
    /// Where it landed: Backlog, or Ready for Dev in its epic's queue.
    pub status: Status,
}

/// Collects agent-created tickets until a quiet moment, then gives one toast per creator and column.
#[derive(Debug, Default)]
pub struct CreationBatch {
    items: Vec<Created>,
    last: Option<Instant>,
}

impl CreationBatch {
    /// Notes a ticket an agent created (seen for the first time).
    pub fn add(&mut self, created: Created, now: Instant) {
        if !self.items.iter().any(|c| c.key == created.key) {
            self.items.push(created);
            self.last = Some(now);
        }
    }

    /// The toasts once nothing new arrived for [`BATCH_QUIET`], one per creator and column in the order they came;
    /// the batch is then empty again.
    pub fn take_if_quiet(&mut self, now: Instant) -> Vec<NewToast> {
        let Some(last) = self.last else {
            return Vec::new();
        };
        if now.duration_since(last) < BATCH_QUIET {
            return Vec::new();
        }
        self.last = None;
        let mut groups: Vec<(String, Status, Vec<TicketKey>)> = Vec::new();
        for c in std::mem::take(&mut self.items) {
            match groups
                .iter_mut()
                .find(|(by, status, _)| *by == c.by && *status == c.status)
            {
                Some((_, _, keys)) => keys.push(c.key),
                None => groups.push((c.by, c.status, vec![c.key])),
            }
        }
        groups
            .iter()
            .map(|(by, status, keys)| creation_toast(by, keys, *status))
            .collect()
    }
}

/// "Ticket created by agent" · "DEMO-7 created DEMO-12 → Backlog", or for a batch "3 tickets created by agent" ·
/// "DEMO-7 created DEMO-12, DEMO-13, DEMO-14 → Backlog". A click on a one-ticket toast opens that ticket.
pub fn creation_toast(by: &str, keys: &[TicketKey], status: Status) -> NewToast {
    let title = match keys {
        [_] => format!("Ticket {CREATED_BY_AGENT}"),
        _ => format!("{} tickets {CREATED_BY_AGENT}", keys.len()),
    };
    let list: Vec<String> = keys.iter().map(ToString::to_string).collect();
    NewToast {
        ticket: match keys {
            [key] => Some(key.to_string()),
            _ => None,
        },
        ..NewToast::new(
            ToastLevel::Success,
            title,
            Some(format!(
                "{by} created {} \u{2192} {}",
                list.join(", "),
                status.label()
            )),
        )
    }
}

/// Whether saving `ticket` sends its agent a change note: its agent is at work and has a live session.
pub fn agent_is_told(ticket: &Ticket, live_agent_session: bool) -> bool {
    live_agent_session && LIVE.contains(&ticket.status)
}

/// The line under Save while editing a working ticket: "SHOP-19’s agent will be told what changed."
pub fn save_hint(ticket: &Ticket, live_agent_session: bool) -> Option<String> {
    agent_is_told(ticket, live_agent_session)
        .then(|| format!("{}’s agent will be told what changed.", ticket.key))
}

/// After a save: "Saved. SHOP-19’s agent will be told what changed: The description changed: added 'works on
/// mobile widths'." `None` when no note goes out.
pub fn saved_note(before: &Ticket, after: &Ticket, live_agent_session: bool) -> Option<String> {
    if !agent_is_told(after, live_agent_session) {
        return None;
    }
    let parts = change_parts(before, after);
    (!parts.is_empty()).then(|| {
        format!(
            "Saved. {}’s agent will be told what changed: {}",
            after.key,
            parts.join(" ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::{Actor, Status};

    fn by_agent(key: &str, title: &str) -> Ticket {
        let mut t = ticket(key, title, Status::Backlog);
        t.created_by = Actor::Agent("SHOP-19".parse().unwrap());
        t
    }

    #[test]
    fn the_origin_label_is_for_agent_tickets_in_backlog() {
        assert_eq!(
            origin_label(&by_agent("SHOP-29", "Add cart analytics")),
            Some("created by agent")
        );
        assert_eq!(
            origin_label(&ticket("SHOP-24", "Footer", Status::Backlog)),
            None
        );
        let mut started = by_agent("SHOP-29", "Add cart analytics");
        started.status = Status::Working;
        started.place = otaq_core::domain::Place::stage_of("dev");
        assert_eq!(origin_label(&started), None);
    }

    #[test]
    fn the_card_carries_the_origin_label() {
        use crate::view_models::card::{CardExtras, CardViewModel};
        let vm = CardViewModel::from(
            &by_agent("SHOP-29", "Add cart analytics"),
            &CardExtras::default(),
            0,
        );
        assert_eq!(vm.origin.as_deref(), Some("Created by SHOP-19"));
        let mine = CardViewModel::from(
            &ticket("SHOP-24", "Footer", Status::Backlog),
            &CardExtras::default(),
            0,
        );
        assert_eq!(mine.origin, None);
    }

    #[test]
    fn a_project_agent_s_ticket_names_the_agent_and_glows_at_first() {
        use crate::view_models::card::{CardExtras, CardViewModel};
        let mut t = ticket("SHOP-36", "Cache product prices", Status::Backlog);
        t.created_by = Actor::ProjectAgent("a1".into());
        t.created_at = 1_000;
        let extras = CardExtras {
            agent_name: Some("Checkout slowness".into()),
            ..CardExtras::default()
        };
        let vm = CardViewModel::from(&t, &extras, 3_000);
        assert!(vm.fresh);
        assert_eq!(vm.origin.as_deref(), Some("Created by Checkout slowness"));
        assert!(!CardViewModel::from(&t, &extras, 1_000 + FRESH_MS).fresh);
        assert_eq!(
            project_agent_label(&t, None).as_deref(),
            Some("by a Project agent")
        );
        t.place = otaq_core::domain::Place::stage_of("dev");
        assert_eq!(project_agent_label(&t, Some("x")), None);
        assert!(!fresh_from_project_agent(&t, 3_000));
    }

    fn made(by: &str, key: &str, status: Status) -> Created {
        Created {
            by: by.into(),
            key: key.parse().unwrap(),
            status,
        }
    }

    #[test]
    fn one_toast_per_burst_naming_the_creator_and_the_tickets() {
        let t0 = Instant::now();
        let mut batch = CreationBatch::default();
        batch.add(made("DEMO-7", "DEMO-12", Status::Backlog), t0);
        assert_eq!(batch.take_if_quiet(t0 + Duration::from_millis(100)), []);
        let toasts = batch.take_if_quiet(t0 + BATCH_QUIET);
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts[0].level, ToastLevel::Success);
        assert_eq!(toasts[0].title, "Ticket created by agent");
        assert_eq!(
            toasts[0].text.as_deref(),
            Some("DEMO-7 created DEMO-12 \u{2192} Backlog")
        );
        assert_eq!(
            toasts[0].ticket.as_deref(),
            Some("DEMO-12"),
            "a click opens it"
        );
        assert_eq!(batch.take_if_quiet(t0 + BATCH_QUIET * 3), [], "emptied");

        // A burst: one toast per creator and column, seen twice counts once.
        for (n, key) in ["DEMO-13", "DEMO-14", "DEMO-15", "DEMO-13"]
            .iter()
            .enumerate()
        {
            batch.add(
                made("DEMO-7", key, Status::Backlog),
                t0 + Duration::from_millis(100 * n as u64),
            );
        }
        batch.add(made("Wishlist lead", "DEMO-40.5", Status::Queued), t0);
        let toasts = batch.take_if_quiet(t0 + Duration::from_secs(5));
        let shown: Vec<(&str, Option<&str>)> = toasts
            .iter()
            .map(|t| (t.title.as_str(), t.text.as_deref()))
            .collect();
        assert_eq!(
            shown,
            [
                (
                    "3 tickets created by agent",
                    Some("DEMO-7 created DEMO-13, DEMO-14, DEMO-15 \u{2192} Backlog")
                ),
                (
                    "Ticket created by agent",
                    Some("Wishlist lead created DEMO-40.5 \u{2192} Ready for Dev")
                ),
            ]
        );
        assert_eq!(toasts[0].ticket, None);
    }

    #[test]
    fn the_editor_note_only_for_a_working_ticket_with_a_live_agent() {
        let mut before = ticket("SHOP-19", "Price table", Status::Working);
        before.body = "- Monthly / yearly toggle".into();
        let mut after = before.clone();
        after.body.push_str("\n- Works on mobile widths");
        assert_eq!(
            save_hint(&before, true).as_deref(),
            Some("SHOP-19’s agent will be told what changed.")
        );
        assert_eq!(
            saved_note(&before, &after, true).as_deref(),
            Some(
                "Saved. SHOP-19’s agent will be told what changed: The description changed: added 'Works on mobile widths'."
            )
        );
        assert_eq!(saved_note(&before, &after, false), None, "no live session");
        assert_eq!(saved_note(&before, &before, true), None, "nothing changed");
        for status in [
            Status::Queued,
            Status::Parked,
            Status::Backlog,
            Status::WaitingForYou,
        ] {
            let mut t = after.clone();
            t.status = status;
            assert_eq!(saved_note(&before, &t, true), None, "{status}");
            assert_eq!(save_hint(&t, true), None, "{status}");
        }
    }
}
