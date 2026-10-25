//! What an agent may do (CLI-02, CLI-03; d0031). Agents run `otaq` without permission prompts, so the
//! **service** limits them; the CLI stays a thin client.
//!
//! An agent caller may:
//! - **read anything** (`show`, `list`, `ready`, `blocked`, `dep tree`, `prime`, the board);
//! - **change only its own ticket** (`update`, `stage next` / `review submit`, `stage done`, `ask`, `answer`, and
//!   from M10 `pr propose`, `pr opened`, `branch rename`);
//! - **create tickets**, which always land in Backlog (including epic children with `--parent`);
//! - **add or remove dependencies** only from its own ticket, or between two tickets it created.
//!
//! Everything else is refused, for example "SHOP-18's agent can't change SHOP-12". The user (the app, their own
//! terminals, the Project terminal, Shell and Run) passes every check (CLI-03), except reporting a verdict.
//!
//! A **validator** caller (an agent check's session, d0063) reads anything and gives the verdict of its own ticket
//! (`check pass|fail`, AGM-10); everything else is refused: "SHOP-29's check can only give its verdict". Only an
//! agent check gives a verdict: "Only an agent check can give a verdict".
//!
//! **By stage kind** (STG-10, d0108): a ticket session's role is the kind of its stage run's stage
//! ([`crate::caller::by_stage`]): an Agent stage's session is a ticket agent (never a verdict), an agent check's is a
//! validator (the verdict only). On every flow the refusals name the stage ([`authorize_in_stage`], Q269): "NOTE-1's
//! Validate check can only give its verdict", "NOTE-1's Dev session can't give a check verdict".
//!
//! A **Project agent** caller (d0068, PAG-02) reads anything, creates tickets (always in Backlog, incl. epics,
//! children, External ID and priority), and edits, links and prioritises tickets **still in Backlog** (never
//! started); it asks a ticket's agent and the user with its own commands, and names itself. Everything else
//! (move, start, approve, merge, close, delete, edit a started ticket) is refused: "Noyan can only change Backlog
//! tickets".
//!
//! An **epic lead** caller (d0083, CLI-07, LEAD-08) reads anything; edits, links and prioritises its own epic's
//! **never-started** children ([`lead_may_change`]); creates children of its epic; and runs `note`, `answer`,
//! `suggest` and `decision mark` for its epic's children ([`is_child_of`]). Everything else (move, queue, take back,
//! park, approve, merge, close, delete, a started child, the epic itself, other tickets, `agent name`) is refused:
//! "Wishlist lead can only change SHOP-40's children that haven't started" ([`lead_refusal`]).
//!
//! **Stage choices** (`otaq create|update --stage`, AGM-11; d0108, d0087) are their own action,
//! [`Action::SetChoices`]: a Project agent on the tickets it may change (Backlog), an epic lead on its epic's
//! never-started children (not the epic), a ticket agent on its own ticket; a validator never. Which stage each may
//! choose for, and over whose choice, is [`crate::board::stage_choices::judge`]'s.
//!
//! **Ticket comments** (CMT-04, d0109) are [`Action::Comment`]: the user writes, edits and deletes any; a ticket's
//! agent comments on its own ticket and edits or deletes only its own comments; an epic lead comments on its epic and
//! its children (started or not: a comment is for their later stages) and edits or deletes only its own. A checker
//! posts only with its verdict (`check pass|fail`) and edits or deletes only its own (Q374); the Project agent and
//! other tickets' agents never.
//!
//! **Reading a ticket's terminal** (`otaq show <KEY> --terminal`, CLI-09, d0087) is its own narrower action, not
//! "read anything": a Project agent reads any ticket of its project, an epic lead its own epic's children; ticket
//! agents and validators are refused ("NOTE-5's agent can't read NOTE-6's terminal"). Nobody types into another
//! session through it: it writes nothing.
//!
//! [`authorize`] is the one policy function; the service maps every request to an [`Action`] before running it.

use crate::board::{BoardError, Refusal};
use crate::caller::Caller;
use crate::domain::{Actor, TicketKey};

/// The refusal of `check pass|fail` from anyone but an agent check's session (AGM-10).
pub const ONLY_A_VALIDATOR: &str = "Only an agent check can give a verdict";

/// Every refusal of a Project agent (PAG-02).
pub const BACKLOG_ONLY: &str = "Noyan can only change Backlog tickets";

/// Every refusal of an epic lead (CLI-07, d0083): "Wishlist lead can only change SHOP-40's children that haven't
/// started".
pub fn lead_refusal(lead: &str, epic: &TicketKey) -> String {
    format!("{lead} can only change {epic}'s children that haven't started")
}

/// Every refusal of a comment from a Project agent (CMT-04).
pub const PROJECT_AGENT_NO_COMMENTS: &str = "Noyan can read comments, not write them";

/// The refusal of an epic lead's comment outside its epic (CMT-04): "Wishlist lead can only comment on SHOP-40 and
/// its children".
pub fn lead_comment_refusal(lead: &str, epic: &TicketKey) -> String {
    format!("{lead} can only comment on {epic} and its children")
}

/// The refusal of editing or deleting another author's comment (CMT-04): "SHOP-18's agent can only edit or delete
/// its own comments".
pub fn not_own_comment(who: &str) -> String {
    format!("{who} can only edit or delete its own comments")
}

/// The refusal of `decision mark` from anyone but an epic's lead (DEC-04).
pub const ONLY_A_LEAD_MARKS: &str = "Only an epic's lead can mark a recorded decision";

/// The refusal of `suggest close|delete` from anyone but an epic's lead (LEAD-08).
pub const ONLY_A_LEAD_SUGGESTS: &str =
    "Only an epic's lead can suggest closing or deleting a child";

/// The refusal of a ticket's terminal to an epic lead outside its epic's children (CLI-09): "Wishlist lead can
/// only read SHOP-40's children's terminals".
pub fn lead_terminal_refusal(lead: &str, epic: &TicketKey) -> String {
    format!("{lead} can only read {epic}'s children's terminals")
}

/// The refusal of another project's ticket's terminal to a Project agent (CLI-09): "Checkout bug can only read
/// pocket-notes' tickets".
pub fn project_terminal_refusal(agent: &str, project: &str) -> String {
    let apostrophe = if project.ends_with('s') { "'" } else { "'s" };
    format!("{agent} can only read {project}{apostrophe} tickets")
}

/// How a caller and its project are named in refusals: an epic lead's "Wishlist lead", a Project agent's own name
/// and its project's name ("pocket-notes"). Without them the refusal falls back to "SHOP-40's lead", "Noyan" and
/// the project's key.
#[derive(Debug, Clone, Copy, Default)]
pub struct Names<'a> {
    pub caller: Option<&'a str>,
    pub project: Option<&'a str>,
}

/// The facts about a ticket the policy needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketFacts {
    pub id: String,
    /// How the ticket is named in a refusal: its key, or "the new ticket \"…\"" in a batch.
    pub label: String,
    pub created_by: Actor,
    /// In Backlog and never started: a Project agent may edit it (d0068).
    pub backlog: bool,
    /// Epic children only: the epic's id.
    pub epic_id: Option<String>,
    /// Never been to Dev, in Backlog or Ready for Dev ([`crate::board::epic_flight::is_never_started`]): an epic
    /// lead may edit it when it is a child of its epic (d0083).
    pub never_started: bool,
}

/// A child of the epic `epic_id` (any state): what a lead's `note`, `answer` and `decision mark` may name.
pub fn is_child_of(target: &TicketFacts, epic_id: &str) -> bool {
    target.epic_id.as_deref() == Some(epic_id)
}

/// A never-started child of the epic `epic_id`: what its lead may edit, link, prioritise and suggest closing or
/// deleting (CLI-07, LEAD-08).
pub fn lead_may_change(target: &TicketFacts, epic_id: &str) -> bool {
    is_child_of(target, epic_id) && target.never_started
}

/// What a request does, as far as permissions go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Reads anything: tickets, the board, history, settings, sessions' output.
    Read,
    /// Changes this ticket: its review submission, its questions and answers, its PR and branch.
    ChangeTicket(TicketFacts),
    /// Edits this ticket's fields (title, description, priority, External ID): for agents like
    /// [`Action::ChangeTicket`]; a Project agent edits Backlog tickets.
    EditTicket(TicketFacts),
    /// Creates a ticket or epic; new tickets always land in Backlog (or join a whole-queued epic's queue, EPC-13).
    /// `epic` is the new ticket's epic id, for a child.
    CreateTicket { epic: Option<String> },
    /// Adds or removes the link "`child` depends on `parent`".
    Dependency {
        child: TicketFacts,
        parent: TicketFacts,
    },
    /// Answers a question `asker` asked `asked` (`otaq answer`): for agents a change of `asked` (CLI-02); an epic
    /// lead answers when either is a child of its epic. `asker` is `None` when a Project agent asked.
    Answer {
        asker: Option<TicketFacts>,
        asked: TicketFacts,
    },
    /// An epic lead's own command on a child of its epic (`suggest`, `decision mark`, later `note`): `only` is the
    /// refusal for every other agent; `started_too` allows a child that has started (a mark, a note), else only a
    /// never-started child (a suggestion).
    LeadCommand {
        target: TicketFacts,
        only: &'static str,
        started_too: bool,
    },
    /// Reads this ticket's terminal (`otaq show <KEY> --terminal`, CLI-09); `project` is the ticket's project
    /// key.
    ReadTerminal {
        target: TicketFacts,
        project: String,
    },
    /// Sets this ticket's stage choices (`--stage`, AGM-11); each is then judged by
    /// [`crate::board::stage_choices::judge`].
    SetChoices(TicketFacts),
    /// Only the user may do this; `what` completes "SHOP-18's agent can't …" ("move tickets").
    UserOnly(&'static str),
    /// `check pass|fail` for this ticket: its agent check's session only.
    Verdict(TicketFacts),
    /// Several actions, all needed (a batch that creates tickets and links them).
    All(Vec<Action>),
    /// `otaq flow propose` for the project `project` (its key): the user, or a Project agent of that project (FLW-03).
    ProposeFlow { project: String },
    /// Posts a ticket comment on `target` (`own: None`), or edits or deletes one of its comments (`own`: whether the
    /// caller wrote it), CMT-04: the user any; a ticket's agent on its own ticket, editing only its own; an epic
    /// lead on its epic and its children, editing only its own. A checker posts only with its verdict and edits only
    /// its own (Q374), the Project agent never.
    Comment {
        target: TicketFacts,
        own: Option<bool>,
    },
}

/// Allows or refuses `action` for `caller`. Refusals are [`Refusal::NotAllowed`] with a message for people. An epic
/// lead is named "SHOP-40's lead" in its refusal; [`authorize_named`] gives its real name.
pub fn authorize(caller: &Caller, action: &Action) -> Result<(), BoardError> {
    authorize_named(caller, Names::default(), action)
}

/// [`authorize`], with the names the caller's refusals use ("Wishlist lead"; "Checkout bug" and "pocket-notes").
pub fn authorize_named(caller: &Caller, names: Names, action: &Action) -> Result<(), BoardError> {
    authorize_in_stage(caller, names, None, action)
}

/// A check session's refusal of anything but its verdict (STG-10): "NOTE-4's Security check can only give its
/// verdict".
pub fn check_only_verdict(ticket: &TicketKey, stage: &str) -> String {
    format!("{ticket}'s {stage} check can only give its verdict")
}

/// An Agent stage session's refusal of a verdict (STG-10): "NOTE-4's Grill session can't give a check verdict".
pub fn agent_stage_no_verdict(ticket: &TicketKey, stage: &str) -> String {
    format!("{ticket}'s {stage} session can't give a check verdict")
}

/// [`authorize_named`] for a ticket session whose stage run is in the stage named `stage` (STG-10, Q269): its
/// refusals name the stage ("NOTE-1's Validate check can only give its verdict"). `None` (no stage run) says "check".
pub fn authorize_in_stage(
    caller: &Caller,
    names: Names,
    stage: Option<&str>,
    action: &Action,
) -> Result<(), BoardError> {
    let refuse = |message: String| Err(BoardError::refused(Refusal::NotAllowed, message));
    let all = |actions: &[Action]| {
        actions
            .iter()
            .try_for_each(|a| authorize_in_stage(caller, names, stage, a))
    };
    if let Action::ProposeFlow { project } = action {
        return match caller {
            Caller::User => Ok(()),
            Caller::ProjectAgent { project: own, .. } if own == project => Ok(()),
            Caller::ProjectAgent { project: own, .. } => refuse(format!(
                "{} can only propose a flow for its own project, {}",
                names.caller.unwrap_or("Noyan"),
                names.project.unwrap_or(own)
            )),
            _ => refuse(crate::flow::propose::ONLY_A_PROJECT_AGENT_OR_YOU.to_owned()),
        };
    }
    if let Caller::Validator {
        ticket, ticket_id, ..
    } = caller
    {
        let who = match stage {
            Some(stage) => format!("{ticket}'s {stage} check"),
            None => format!("{ticket}'s check"),
        };
        return match action {
            Action::Read => Ok(()),
            Action::Verdict(target) if target.id == *ticket_id => Ok(()),
            Action::Verdict(target) => refuse(format!(
                "{who} can only report on {ticket}, not {}",
                target.label
            )),
            Action::ReadTerminal { target, .. } => {
                refuse(format!("{who} can't read {}'s terminal", target.label))
            }
            // Its own comments (its verdicts) on its ticket: edit or delete (CMT-04, Q374). A new one only with a verdict.
            Action::Comment {
                target,
                own: Some(true),
            } if target.id == *ticket_id => Ok(()),
            Action::Comment {
                target,
                own: Some(false),
            } if target.id == *ticket_id => refuse(not_own_comment(&who)),
            Action::All(actions) => all(actions),
            _ => match stage {
                Some(stage) => refuse(check_only_verdict(ticket, stage)),
                None => refuse(format!("{who} can only give its verdict")),
            },
        };
    }
    if let Action::Verdict(_) = action {
        return match (caller, stage) {
            (Caller::Agent { ticket, .. }, Some(stage)) => {
                refuse(agent_stage_no_verdict(ticket, stage))
            }
            _ => refuse(ONLY_A_VALIDATOR.to_owned()),
        };
    }
    if let Caller::EpicLead { epic, epic_id, .. } = caller {
        return match action {
            Action::Read => Ok(()),
            Action::CreateTicket { epic: Some(e) } if e == epic_id => Ok(()),
            Action::EditTicket(target) | Action::SetChoices(target)
                if lead_may_change(target, epic_id) =>
            {
                Ok(())
            }
            // "SHOP-40.5 waits for SHOP-40.4": both children, the waiting one never started.
            Action::Dependency { child, parent }
                if lead_may_change(child, epic_id) && is_child_of(parent, epic_id) =>
            {
                Ok(())
            }
            Action::Answer { asker, asked }
                if asker.as_ref().is_some_and(|a| is_child_of(a, epic_id))
                    || is_child_of(asked, epic_id) =>
            {
                Ok(())
            }
            Action::LeadCommand {
                target,
                started_too,
                ..
            } if is_child_of(target, epic_id) && (*started_too || target.never_started) => Ok(()),
            Action::ReadTerminal { target, .. } if is_child_of(target, epic_id) => Ok(()),
            Action::Comment { target, own }
                if target.id == *epic_id || is_child_of(target, epic_id) =>
            {
                if *own == Some(false) {
                    let fallback = format!("{epic}'s lead");
                    refuse(not_own_comment(names.caller.unwrap_or(&fallback)))
                } else {
                    Ok(())
                }
            }
            Action::All(actions) => all(actions),
            _ => {
                let fallback = format!("{epic}'s lead");
                let lead = names.caller.unwrap_or(&fallback);
                match action {
                    Action::ReadTerminal { .. } => refuse(lead_terminal_refusal(lead, epic)),
                    Action::Comment { .. } => refuse(lead_comment_refusal(lead, epic)),
                    _ => refuse(lead_refusal(lead, epic)),
                }
            }
        };
    }
    if let Action::LeadCommand { only, .. } = action
        && *caller != Caller::User
    {
        return refuse((*only).to_owned());
    }
    if let Caller::ProjectAgent { project, .. } = caller {
        return match action {
            Action::Read | Action::CreateTicket { .. } => Ok(()),
            Action::ReadTerminal { project: of, .. } if of == project => Ok(()),
            Action::ReadTerminal { .. } => refuse(project_terminal_refusal(
                names.caller.unwrap_or("Noyan"),
                names.project.unwrap_or(project),
            )),
            Action::EditTicket(target) | Action::SetChoices(target) if target.backlog => Ok(()),
            // A Backlog ticket may wait for any ticket ("SHOP-35 waits for SHOP-19").
            Action::Dependency { child, .. } if child.backlog => Ok(()),
            Action::All(actions) => all(actions),
            Action::Comment { .. } => refuse(PROJECT_AGENT_NO_COMMENTS.to_owned()),
            _ => refuse(BACKLOG_ONLY.to_owned()),
        };
    }
    let Caller::Agent {
        ticket, ticket_id, ..
    } = caller
    else {
        return Ok(());
    };
    match action {
        Action::Read | Action::CreateTicket { .. } => Ok(()),
        Action::ChangeTicket(target)
        | Action::EditTicket(target)
        | Action::SetChoices(target)
        | Action::Answer { asked: target, .. }
            if target.id == *ticket_id =>
        {
            Ok(())
        }
        Action::ChangeTicket(target)
        | Action::EditTicket(target)
        | Action::SetChoices(target)
        | Action::Answer { asked: target, .. } => {
            refuse(format!("{ticket}'s agent can't change {}", target.label))
        }
        Action::Dependency { child, parent } => {
            let own = Actor::Agent(ticket.clone());
            let from_own = child.id == *ticket_id;
            let both_created = child.created_by == own && parent.created_by == own;
            if from_own || both_created {
                Ok(())
            } else {
                refuse(format!(
                    "{ticket}'s agent can't change {}: it may link only its own ticket, or tickets it created",
                    child.label
                ))
            }
        }
        Action::ReadTerminal { target, .. } => refuse(format!(
            "{ticket}'s agent can't read {}'s terminal",
            target.label
        )),
        Action::Comment { target, own } if target.id == *ticket_id => {
            if *own == Some(false) {
                refuse(not_own_comment(&format!("{ticket}'s agent")))
            } else {
                Ok(())
            }
        }
        Action::Comment { target, .. } => refuse(format!(
            "{ticket}'s agent can't comment on {}",
            target.label
        )),
        Action::UserOnly(what) => refuse(format!("{ticket}'s agent can't {what}; ask the user")),
        Action::Verdict(_) | Action::LeadCommand { .. } | Action::ProposeFlow { .. } => {
            unreachable!("handled above")
        }
        Action::All(actions) => all(actions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(key: &str, id: &str) -> Caller {
        Caller::Agent {
            ticket: key.parse().unwrap(),
            ticket_id: id.into(),
            session: "s".into(),
        }
    }

    fn facts(key: &str, id: &str, created_by: &str) -> TicketFacts {
        TicketFacts {
            id: id.into(),
            label: key.into(),
            created_by: created_by.parse().unwrap(),
            backlog: false,
            epic_id: None,
            never_started: false,
        }
    }

    fn message(result: Result<(), BoardError>) -> Option<String> {
        match result {
            Ok(()) => None,
            Err(e) => {
                assert_eq!(e.refusal(), Some(Refusal::NotAllowed));
                Some(e.to_string())
            }
        }
    }

    /// Every command of CLI-02 for SHOP-18's agent, and the same for the user.
    #[test]
    fn the_cli_02_table() {
        let me = agent("SHOP-18", "t18");
        let own = facts("SHOP-18", "t18", "nevzat");
        let other = facts("SHOP-12", "t12", "nevzat");
        let mine_a = facts("SHOP-21", "t21", "agent:SHOP-18");
        let mine_b = facts("SHOP-22", "t22", "agent:SHOP-18");
        let theirs = facts("SHOP-23", "t23", "agent:SHOP-19");
        let dep = |child: &TicketFacts, parent: &TicketFacts| Action::Dependency {
            child: child.clone(),
            parent: parent.clone(),
        };
        let rows: Vec<(&str, Action, Option<&str>)> = vec![
            ("show / list / prime", Action::Read, None),
            ("update own", Action::ChangeTicket(own.clone()), None),
            ("review submit own", Action::ChangeTicket(own.clone()), None),
            ("ask human own", Action::ChangeTicket(own.clone()), None),
            (
                "update another",
                Action::ChangeTicket(other.clone()),
                Some("SHOP-18's agent can't change SHOP-12"),
            ),
            (
                "update a ticket it created",
                Action::ChangeTicket(mine_a.clone()),
                Some("SHOP-18's agent can't change SHOP-21"),
            ),
            (
                "create (Backlog, incl. --parent)",
                Action::CreateTicket { epic: None },
                None,
            ),
            ("dep add from own", dep(&own, &other), None),
            (
                "dep add between its own creations",
                dep(&mine_a, &mine_b),
                None,
            ),
            (
                "dep add between user tickets",
                dep(&other, &facts("SHOP-13", "t13", "nevzat")),
                Some(
                    "SHOP-18's agent can't change SHOP-12: it may link only its own ticket, or tickets it created",
                ),
            ),
            (
                "dep add from its creation to another agent's",
                dep(&mine_a, &theirs),
                Some(
                    "SHOP-18's agent can't change SHOP-21: it may link only its own ticket, or tickets it created",
                ),
            ),
            (
                "move",
                Action::UserOnly("move tickets"),
                Some("SHOP-18's agent can't move tickets; ask the user"),
            ),
            (
                "delete",
                Action::UserOnly("delete tickets"),
                Some("SHOP-18's agent can't delete tickets; ask the user"),
            ),
        ];
        for (name, action, expected) in &rows {
            assert_eq!(
                message(authorize(&me, action)).as_deref(),
                *expected,
                "agent: {name}"
            );
            assert_eq!(
                message(authorize(&Caller::User, action)),
                None,
                "user: {name}"
            );
        }
    }

    /// T-0200 (PAG-02, CLI-06): a Project agent creates, and edits, links and prioritises Backlog tickets only;
    /// everything else is refused with the one text.
    #[test]
    fn project_agents_change_backlog_tickets_only() {
        let me = Caller::ProjectAgent {
            project: "SHOP".into(),
            agent: "a1".into(),
            session: "s".into(),
        };
        let backlog = TicketFacts {
            backlog: true,
            ..facts("SHOP-35", "t35", "project-agent:a1")
        };
        let started = facts("SHOP-19", "t19", "nevzat");
        let rows: Vec<(&str, Action, bool)> = vec![
            ("show / list / prime", Action::Read, true),
            (
                "create (Backlog, epics, children)",
                Action::CreateTicket { epic: None },
                true,
            ),
            (
                "update a Backlog ticket",
                Action::EditTicket(backlog.clone()),
                true,
            ),
            (
                "update a started ticket",
                Action::EditTicket(started.clone()),
                false,
            ),
            (
                "dep add from a Backlog ticket to a started one",
                Action::Dependency {
                    child: backlog.clone(),
                    parent: started.clone(),
                },
                true,
            ),
            (
                "dep add from a started ticket",
                Action::Dependency {
                    child: started.clone(),
                    parent: backlog.clone(),
                },
                false,
            ),
            (
                "review submit",
                Action::ChangeTicket(backlog.clone()),
                false,
            ),
            ("move", Action::UserOnly("move tickets"), false),
            ("delete", Action::UserOnly("delete tickets"), false),
            ("approve / merge", Action::UserOnly("merge tickets"), false),
            ("close", Action::UserOnly("close tickets"), false),
        ];
        for (name, action, allowed) in rows {
            let got = message(authorize(&me, &action));
            if allowed {
                assert_eq!(got, None, "{name}");
            } else {
                assert_eq!(got.as_deref(), Some(BACKLOG_ONLY), "{name}");
            }
        }
        assert_eq!(
            message(authorize(&me, &Action::Verdict(backlog))).as_deref(),
            Some(ONLY_A_VALIDATOR)
        );
    }

    /// T-0280 (CLI-07, LEAD-08, DEC-04; d0083): Wishlist lead (epic SHOP-40) edits, links and prioritises its
    /// never-started children, creates children, and runs its own commands on its children; everything else is
    /// refused with the one text.
    #[test]
    fn the_cli_07_table() {
        let lead = Caller::EpicLead {
            project: "SHOP".into(),
            agent: "l1".into(),
            epic: "SHOP-40".parse().unwrap(),
            epic_id: "e40".into(),
            session: "s".into(),
        };
        let child = |key: &str, id: &str, never_started: bool, epic: &str| TicketFacts {
            epic_id: Some(epic.into()),
            never_started,
            backlog: never_started,
            ..facts(key, id, "nevzat")
        };
        let queued = child("SHOP-40.3", "c3", true, "e40");
        let waiting = child("SHOP-40.5", "c5", true, "e40");
        let running = child("SHOP-40.4", "c4", false, "e40");
        let other_epic = child("SHOP-45.1", "c451", true, "e45");
        let epic = TicketFacts {
            never_started: true,
            ..facts("SHOP-40", "e40", "nevzat")
        };
        let standalone = TicketFacts {
            never_started: true,
            backlog: true,
            ..facts("SHOP-23", "t23", "nevzat")
        };
        let dep = |child: &TicketFacts, parent: &TicketFacts| Action::Dependency {
            child: child.clone(),
            parent: parent.clone(),
        };
        let command = |target: &TicketFacts, started_too: bool| Action::LeadCommand {
            target: target.clone(),
            only: ONLY_A_LEAD_MARKS,
            started_too,
        };
        let rows: Vec<(&str, Action, bool)> = vec![
            ("show / list / prime --epic", Action::Read, true),
            (
                "create --parent SHOP-40",
                Action::CreateTicket {
                    epic: Some("e40".into()),
                },
                true,
            ),
            (
                "create outside the epic",
                Action::CreateTicket { epic: None },
                false,
            ),
            (
                "create in another epic",
                Action::CreateTicket {
                    epic: Some("e45".into()),
                },
                false,
            ),
            (
                "update a never-started child",
                Action::EditTicket(queued.clone()),
                true,
            ),
            (
                "update a started child",
                Action::EditTicket(running.clone()),
                false,
            ),
            ("update the epic", Action::EditTicket(epic.clone()), false),
            (
                "update another epic's child",
                Action::EditTicket(other_epic.clone()),
                false,
            ),
            (
                "update a standalone ticket",
                Action::EditTicket(standalone.clone()),
                false,
            ),
            (
                "dep add: 40.5 waits for running 40.4",
                dep(&waiting, &running),
                true,
            ),
            (
                "dep remove between never-started",
                dep(&waiting, &queued),
                true,
            ),
            (
                "dep add from a started child",
                dep(&running, &queued),
                false,
            ),
            (
                "dep add to another epic's child",
                dep(&waiting, &other_epic),
                false,
            ),
            (
                "dep add to a standalone ticket",
                dep(&waiting, &standalone),
                false,
            ),
            (
                "answer a child's question",
                Action::Answer {
                    asker: Some(running.clone()),
                    asked: epic.clone(),
                },
                true,
            ),
            (
                "answer another epic's question",
                Action::Answer {
                    asker: Some(other_epic.clone()),
                    asked: standalone.clone(),
                },
                false,
            ),
            (
                "decision mark on a started child",
                command(&running, true),
                true,
            ),
            (
                "suggest on a never-started child",
                command(&queued, false),
                true,
            ),
            (
                "suggest on a started child",
                command(&running, false),
                false,
            ),
            (
                "decision mark on another epic's child",
                command(&other_epic, true),
                false,
            ),
            (
                "review submit for a child",
                Action::ChangeTicket(queued.clone()),
                false,
            ),
            ("move", Action::UserOnly("move tickets"), false),
            ("queue", Action::UserOnly("queue epics"), false),
            ("take back", Action::UserOnly("take back epics"), false),
            ("park", Action::UserOnly("park tickets"), false),
            ("approve / merge", Action::UserOnly("merge tickets"), false),
            ("close", Action::UserOnly("close tickets"), false),
            ("delete", Action::UserOnly("delete tickets"), false),
            ("agent name", Action::UserOnly("name agents"), false),
        ];
        let text = "Wishlist lead can only change SHOP-40's children that haven't started";
        for (name, action, allowed) in rows {
            let names = Names {
                caller: Some("Wishlist lead"),
                project: None,
            };
            let got = message(authorize_named(&lead, names, &action));
            if allowed {
                assert_eq!(got, None, "{name}");
            } else {
                assert_eq!(got.as_deref(), Some(text), "{name}");
            }
        }
        assert_eq!(
            message(authorize(&lead, &Action::UserOnly("move tickets"))).as_deref(),
            Some("SHOP-40's lead can only change SHOP-40's children that haven't started")
        );
        // A batch needs every part.
        assert!(
            authorize(
                &lead,
                &Action::All(vec![
                    Action::CreateTicket {
                        epic: Some("e40".into())
                    },
                    dep(&waiting, &running),
                ])
            )
            .is_ok()
        );

        // The lead's own commands are refused for everyone else with their own text; the user is judged by the
        // handler (it needs a lead).
        let agent = agent("SHOP-40.4", "c4");
        let project_agent = Caller::ProjectAgent {
            project: "SHOP".into(),
            agent: "a1".into(),
            session: "s".into(),
        };
        for caller in [&agent, &project_agent] {
            assert_eq!(
                message(authorize(caller, &command(&running, true))).as_deref(),
                Some(ONLY_A_LEAD_MARKS)
            );
        }
        assert_eq!(
            message(authorize(&Caller::User, &command(&running, true))),
            None
        );
    }

    #[test]
    fn identity_is_by_id_so_aliases_count_as_the_same_ticket() {
        let me = agent("SHOP-50.2", "t51");
        // The ticket was SHOP-51 before Move to epic; the id is the same.
        assert!(
            authorize(
                &me,
                &Action::ChangeTicket(facts("SHOP-51", "t51", "nevzat"))
            )
            .is_ok()
        );
    }

    /// T-0184 (CLI-05, d0063): a validator reads and reports on its own ticket; it changes nothing, and nobody
    /// else reports a verdict. Without a stage run it is "SHOP-29's check".
    #[test]
    fn validators_read_and_report_only() {
        let validator = Caller::Validator {
            ticket: "SHOP-29".parse().unwrap(),
            ticket_id: "t29".into(),
            session: "v".into(),
        };
        let own = facts("SHOP-29", "t29", "nevzat");
        let other = facts("SHOP-12", "t12", "nevzat");
        assert_eq!(message(authorize(&validator, &Action::Read)), None);
        assert_eq!(
            message(authorize(&validator, &Action::Verdict(own.clone()))),
            None
        );
        assert_eq!(
            message(authorize(&validator, &Action::Verdict(other.clone()))).as_deref(),
            Some("SHOP-29's check can only report on SHOP-29, not SHOP-12")
        );
        // T-0195: it never creates tickets, also not from its own report's notes (the user's Create ticket).
        for action in [
            Action::ChangeTicket(own.clone()),
            Action::CreateTicket { epic: None },
            Action::UserOnly("create tickets from validation notes"),
            Action::UserOnly("move tickets"),
            Action::Dependency {
                child: own.clone(),
                parent: other.clone(),
            },
        ] {
            assert_eq!(
                message(authorize(&validator, &action)).as_deref(),
                Some("SHOP-29's check can only give its verdict"),
                "{action:?}"
            );
        }
        for caller in [Caller::User, agent("SHOP-29", "t29")] {
            assert_eq!(
                message(authorize(&caller, &Action::Verdict(own.clone()))).as_deref(),
                Some(ONLY_A_VALIDATOR)
            );
        }
    }

    /// T-0402 (STG-10, d0108): a session's rights are its stage's kind's, and its refusals name the stage. A
    /// Security check session gives its verdict only (no `stage next`); a Grill session never gives one.
    #[test]
    fn rights_by_stage_kind() {
        let security = Caller::Validator {
            ticket: "NOTE-4".parse().unwrap(),
            ticket_id: "t4".into(),
            session: "s".into(),
        };
        let grill = agent("NOTE-4", "t4");
        let own = facts("NOTE-4", "t4", "nevzat");
        let in_stage = |caller: &Caller, stage: &str, action: &Action| {
            message(authorize_in_stage(
                caller,
                Names::default(),
                Some(stage),
                action,
            ))
        };
        assert_eq!(
            in_stage(&security, "Security", &Action::ChangeTicket(own.clone())).as_deref(),
            Some("NOTE-4's Security check can only give its verdict")
        );
        assert_eq!(
            in_stage(&security, "Security", &Action::Verdict(own.clone())),
            None
        );
        assert_eq!(in_stage(&security, "Security", &Action::Read), None);
        assert_eq!(
            in_stage(&grill, "Grill", &Action::Verdict(own.clone())).as_deref(),
            Some("NOTE-4's Grill session can't give a check verdict")
        );
        assert_eq!(
            in_stage(&grill, "Grill", &Action::ChangeTicket(own.clone())),
            None
        );
        // The default flow's stages are named too (Q269).
        let validate = Caller::Validator {
            ticket: "NOTE-1".parse().unwrap(),
            ticket_id: "t1".into(),
            session: "s".into(),
        };
        let dev = agent("NOTE-1", "t1");
        let note1 = facts("NOTE-1", "t1", "nevzat");
        assert_eq!(
            in_stage(&validate, "Validate", &Action::ChangeTicket(note1.clone())).as_deref(),
            Some("NOTE-1's Validate check can only give its verdict")
        );
        assert_eq!(
            in_stage(&dev, "Dev", &Action::Verdict(note1)).as_deref(),
            Some("NOTE-1's Dev session can't give a check verdict")
        );
        // A session without a stage run.
        assert_eq!(
            message(authorize_in_stage(
                &grill,
                Names::default(),
                None,
                &Action::Verdict(own)
            ))
            .as_deref(),
            Some(ONLY_A_VALIDATOR)
        );
    }

    /// T-0317 (CLI-09, d0087): reading a ticket's terminal is its own action. The user always; a Project agent on
    /// its project's tickets; an epic lead on its epic's children; ticket agents and validators never.
    #[test]
    fn the_cli_09_terminal_rule() {
        let note6 = facts("NOTE-6", "t6", "nevzat");
        let read = |target: &TicketFacts, project: &str| Action::ReadTerminal {
            target: target.clone(),
            project: project.into(),
        };
        let child = TicketFacts {
            epic_id: Some("e10".into()),
            ..facts("NOTE-10.1", "c1", "nevzat")
        };
        let epic = facts("NOTE-10", "e10", "nevzat");
        let other = facts("API-3", "a3", "nevzat");

        assert_eq!(
            message(authorize(&Caller::User, &read(&note6, "NOTE"))),
            None
        );
        assert_eq!(
            message(authorize(&Caller::User, &read(&other, "API"))),
            None
        );

        let project_agent = Caller::ProjectAgent {
            project: "NOTE".into(),
            agent: "a1".into(),
            session: "s".into(),
        };
        let names = Names {
            caller: Some("Checkout bug"),
            project: Some("pocket-notes"),
        };
        for target in [&note6, &child, &epic] {
            assert_eq!(
                message(authorize_named(
                    &project_agent,
                    names,
                    &read(target, "NOTE")
                )),
                None
            );
        }
        assert_eq!(
            message(authorize_named(&project_agent, names, &read(&other, "API"))).as_deref(),
            Some("Checkout bug can only read pocket-notes' tickets")
        );
        assert_eq!(
            message(authorize(&project_agent, &read(&other, "API"))).as_deref(),
            Some("Noyan can only read NOTE's tickets")
        );

        let lead = Caller::EpicLead {
            project: "NOTE".into(),
            agent: "l1".into(),
            epic: "NOTE-10".parse().unwrap(),
            epic_id: "e10".into(),
            session: "s".into(),
        };
        let lead_names = Names {
            caller: Some("Sync lead"),
            project: Some("pocket-notes"),
        };
        assert_eq!(
            message(authorize_named(&lead, lead_names, &read(&child, "NOTE"))),
            None
        );
        for target in [&note6, &epic, &other] {
            assert_eq!(
                message(authorize_named(&lead, lead_names, &read(target, "NOTE"))).as_deref(),
                Some("Sync lead can only read NOTE-10's children's terminals"),
                "{}",
                target.label
            );
        }

        let agent = agent("NOTE-5", "t5");
        assert_eq!(
            message(authorize(&agent, &read(&note6, "NOTE"))).as_deref(),
            Some("NOTE-5's agent can't read NOTE-6's terminal")
        );
        assert_eq!(
            message(authorize(
                &agent,
                &read(&facts("NOTE-5", "t5", "nevzat"), "NOTE")
            ))
            .as_deref(),
            Some("NOTE-5's agent can't read NOTE-5's terminal")
        );
        let validator = Caller::Validator {
            ticket: "NOTE-6".parse().unwrap(),
            ticket_id: "t6".into(),
            session: "v".into(),
        };
        assert_eq!(
            message(authorize(&validator, &read(&note6, "NOTE"))).as_deref(),
            Some("NOTE-6's check can't read NOTE-6's terminal")
        );
        // A batch that reads a terminal needs the terminal rule too.
        assert!(
            authorize(
                &agent,
                &Action::All(vec![Action::Read, read(&note6, "NOTE")])
            )
            .is_err()
        );
    }
    /// AGM-11 (CLI-06, CLI-07): who may set stage choices at all. The user always; a Project agent on Backlog
    /// tickets; an epic lead on its never-started children (not the epic); a ticket agent on its own ticket only; a
    /// validator never.
    #[test]
    fn who_may_set_stage_choices() {
        let backlog = TicketFacts {
            backlog: true,
            never_started: true,
            ..facts("SHOP-13", "t13", "nevzat")
        };
        let ready = TicketFacts {
            never_started: true,
            ..facts("SHOP-20", "t20", "nevzat")
        };
        let child = |key: &str, id: &str, never_started: bool, epic: &str| TicketFacts {
            epic_id: Some(epic.into()),
            never_started,
            backlog: never_started,
            ..facts(key, id, "nevzat")
        };
        let unstarted = child("SHOP-40.3", "c3", true, "e40");
        let started = child("SHOP-40.2", "c2", false, "e40");
        let other_epic = child("SHOP-41.1", "c411", true, "e41");
        let epic = TicketFacts {
            never_started: true,
            backlog: true,
            ..facts("SHOP-40", "e40", "nevzat")
        };
        let project_agent = Caller::ProjectAgent {
            project: "SHOP".into(),
            agent: "a1".into(),
            session: "s".into(),
        };
        let lead = Caller::EpicLead {
            project: "SHOP".into(),
            agent: "l1".into(),
            epic: "SHOP-40".parse().unwrap(),
            epic_id: "e40".into(),
            session: "s".into(),
        };
        let validator = Caller::Validator {
            ticket: "SHOP-13".parse().unwrap(),
            ticket_id: "t13".into(),
            session: "v".into(),
        };
        let lead_text = "Wishlist lead can only change SHOP-40's children that haven't started";
        let names = Names {
            caller: Some("Wishlist lead"),
            project: None,
        };
        let rows: Vec<(&str, &Caller, &TicketFacts, Option<&str>)> = vec![
            ("user, a started ticket", &Caller::User, &started, None),
            ("Noyan, Backlog", &project_agent, &backlog, None),
            (
                "Noyan, Ready for Dev",
                &project_agent,
                &ready,
                Some(BACKLOG_ONLY),
            ),
            ("lead, unstarted child", &lead, &unstarted, None),
            ("lead, started child", &lead, &started, Some(lead_text)),
            (
                "lead, another epic's child",
                &lead,
                &other_epic,
                Some(lead_text),
            ),
            ("lead, its epic", &lead, &epic, Some(lead_text)),
            (
                "lead, a standalone ticket",
                &lead,
                &backlog,
                Some(lead_text),
            ),
        ];
        for (name, caller, target, expected) in rows {
            let action = Action::SetChoices(target.clone());
            assert_eq!(
                message(authorize_named(caller, names, &action)).as_deref(),
                expected,
                "{name}"
            );
        }
        let choose = |caller: &Caller, target: &TicketFacts| {
            message(authorize(caller, &Action::SetChoices(target.clone())))
        };
        // SHOP-13's own agent: its own ticket only.
        assert_eq!(choose(&agent("SHOP-13", "t13"), &backlog), None);
        assert_eq!(
            choose(&agent("SHOP-13", "t13"), &ready).as_deref(),
            Some("SHOP-13's agent can't change SHOP-20")
        );
        assert_eq!(
            choose(&validator, &backlog).as_deref(),
            Some("SHOP-13's check can only give its verdict")
        );
    }

    /// CMT-04's table: who may post, edit and delete ticket comments.
    #[test]
    fn the_comment_rights() {
        let wishlist = facts("SHOP-40", "t40", "nevzat");
        let child = TicketFacts {
            epic_id: Some("t40".into()),
            ..facts("SHOP-40.2", "t402", "nevzat")
        };
        let other_epic_child = TicketFacts {
            epic_id: Some("t41".into()),
            ..facts("SHOP-41.1", "t411", "nevzat")
        };
        let own = facts("SHOP-18", "t18", "nevzat");
        let other = facts("SHOP-12", "t12", "nevzat");
        let post = |target: &TicketFacts| Action::Comment {
            target: target.clone(),
            own: None,
        };
        let change = |target: &TicketFacts, own: bool| Action::Comment {
            target: target.clone(),
            own: Some(own),
        };
        let lead = Caller::EpicLead {
            project: "SHOP".into(),
            agent: "pa".into(),
            epic: "SHOP-40".parse().unwrap(),
            epic_id: "t40".into(),
            session: "s".into(),
        };
        let lead_names = Names {
            caller: Some("Wishlist lead"),
            project: None,
        };
        let validator = Caller::Validator {
            ticket: "SHOP-18".parse().unwrap(),
            ticket_id: "t18".into(),
            session: "s".into(),
        };
        let project_agent = Caller::ProjectAgent {
            project: "SHOP".into(),
            agent: "pa".into(),
            session: "s".into(),
        };
        let me = agent("SHOP-18", "t18");
        let rows: Vec<(&str, &Caller, Names, Action, Option<&str>)> = vec![
            (
                "user, any ticket",
                &Caller::User,
                Names::default(),
                post(&other),
                None,
            ),
            (
                "user edits an agent's",
                &Caller::User,
                Names::default(),
                change(&own, false),
                None,
            ),
            ("own agent posts", &me, Names::default(), post(&own), None),
            (
                "own agent edits its own",
                &me,
                Names::default(),
                change(&own, true),
                None,
            ),
            (
                "own agent edits another's",
                &me,
                Names::default(),
                change(&own, false),
                Some("SHOP-18's agent can only edit or delete its own comments"),
            ),
            (
                "another ticket's agent",
                &me,
                Names::default(),
                post(&other),
                Some("SHOP-18's agent can't comment on SHOP-12"),
            ),
            ("lead on a child", &lead, lead_names, post(&child), None),
            ("lead on its epic", &lead, lead_names, post(&wishlist), None),
            (
                "lead edits its own",
                &lead,
                lead_names,
                change(&child, true),
                None,
            ),
            (
                "lead edits another's",
                &lead,
                lead_names,
                change(&child, false),
                Some("Wishlist lead can only edit or delete its own comments"),
            ),
            (
                "lead on another epic's ticket",
                &lead,
                lead_names,
                post(&other_epic_child),
                Some("Wishlist lead can only comment on SHOP-40 and its children"),
            ),
            (
                "lead on a ticket outside epics",
                &lead,
                lead_names,
                post(&other),
                Some("Wishlist lead can only comment on SHOP-40 and its children"),
            ),
            (
                "Project agent",
                &project_agent,
                Names::default(),
                post(&own),
                Some(PROJECT_AGENT_NO_COMMENTS),
            ),
            (
                "checker, outside its verdict",
                &validator,
                Names::default(),
                post(&own),
                Some("SHOP-18's check can only give its verdict"),
            ),
            // Q374: a checker edits or deletes its own comments (its verdicts), never another's.
            (
                "checker, its own comment",
                &validator,
                Names::default(),
                change(&own, true),
                None,
            ),
            (
                "checker, another's comment",
                &validator,
                Names::default(),
                change(&own, false),
                Some("SHOP-18's check can only edit or delete its own comments"),
            ),
            (
                "checker, another ticket's comment",
                &validator,
                Names::default(),
                change(&other, true),
                Some("SHOP-18's check can only give its verdict"),
            ),
        ];
        for (name, caller, names, action, expected) in rows {
            assert_eq!(
                message(authorize_named(caller, names, &action)).as_deref(),
                expected,
                "{name}"
            );
        }
        // A started child too: a comment is for its later stages.
        assert_eq!(
            message(authorize_named(
                &lead,
                lead_names,
                &post(&TicketFacts {
                    never_started: false,
                    ..child.clone()
                })
            )),
            None
        );
    }
}
