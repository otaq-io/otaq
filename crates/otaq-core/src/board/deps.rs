//! Dependencies (DEP-01, DEP-02, DEP-04, PRJ-05; d0005 DAG part, d0024, d0046, d0049, d0051).
//!
//! Tickets form a DAG with any number of parents. A link is refused when it points at itself, already exists,
//! crosses projects, would close a loop (the error names the loop, "SHOP-21 → SHOP-20 → SHOP-21"), or joins
//! tickets with different bases. Bases compare resolved: a standalone ticket or epic has its own branch, an epic
//! child the epic's branch, so a ticket may depend on an epic only with the epic's base (d0046, d0051).
//! Readiness ("every parent merged") is the scheduler's (M5); this module only stores and validates links.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::tickets::{Base, EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, Kind, Status, TicketKey};

/// One line of a dependency tree: a ticket and, nested, its parents (in [`DepTree::parents`]) or its
/// dependents (in [`DepTree::children`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepNode {
    pub key: TicketKey,
    pub title: String,
    pub kind: Kind,
    pub status: Status,
    pub nodes: Vec<DepNode>,
}

/// A ticket with its parents (up, transitively) and the tickets depending on it (down, transitively), each ticket
/// once (#9): a ticket reached by several paths shows under the nearest one (fewest links away, then the first by
/// key), so a diamond doesn't repeat its tip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepTree {
    pub key: TicketKey,
    pub title: String,
    pub kind: Kind,
    pub status: Status,
    pub parents: Vec<DepNode>,
    pub children: Vec<DepNode>,
}

impl Board {
    /// Makes `child` depend on `parent`.
    pub async fn add_dependency(
        &self,
        child: &str,
        parent: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let child = self.ticket(child).await?;
        let parent = self.ticket(parent).await?;
        if child.id == parent.id {
            return Err(BoardError::refused(
                Refusal::SelfDependency,
                format!("{} can't depend on itself", child.key),
            ));
        }
        if child.key.project != parent.key.project {
            return Err(BoardError::refused(
                Refusal::OtherProject,
                format!(
                    "{} and {} are in different projects; dependencies stay within one project",
                    child.key, parent.key
                ),
            ));
        }
        if child.depends_on.contains(&parent.key) {
            return Err(BoardError::refused(
                Refusal::DuplicateDependency,
                format!("{} already depends on {}", child.key, parent.key),
            ));
        }
        if child.epic.as_ref() == Some(&parent.key) {
            return Err(BoardError::refused(
                Refusal::DependsOnOwnEpic,
                format!(
                    "{} is part of {} and can't depend on it",
                    child.key, parent.key
                ),
            ));
        }
        check_same_base(&child, &parent)?;
        let tickets = self.tickets(child.key.project.as_str()).await?;
        if let Some(path) = path_down(&tickets, &child.key, &parent.key) {
            let mut lap = vec![parent.key.to_string()];
            lap.extend(path.iter().map(ToString::to_string));
            return Err(BoardError::refused(
                Refusal::Cycle,
                format!(
                    "{} can't depend on {}: that would make a loop: {}",
                    child.key,
                    parent.key,
                    lap.join(" → ")
                ),
            ));
        }

        let mut tx = self.store.begin_write().await?;
        sqlx::query("INSERT INTO ticket_deps (ticket_id, depends_on_id) VALUES (?, ?)")
            .bind(&child.id)
            .bind(&parent.id)
            .execute(&mut *tx)
            .await?;
        record_event(
            &mut tx,
            &child.id,
            EventKind::DependencyAdded,
            actor,
            json!({"parent": parent.key}),
        )
        .await?;
        tx.commit().await?;
        self.ticket_by_id(&child.id).await
    }

    /// Removes the link "`child` depends on `parent`".
    pub async fn remove_dependency(
        &self,
        child: &str,
        parent: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let child = self.ticket(child).await?;
        let parent = self.ticket(parent).await?;
        let mut tx = self.store.begin_write().await?;
        let removed =
            sqlx::query("DELETE FROM ticket_deps WHERE ticket_id = ? AND depends_on_id = ?")
                .bind(&child.id)
                .bind(&parent.id)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if removed == 0 {
            return Err(BoardError::NotFound(format!(
                "{} doesn't depend on {}",
                child.key, parent.key
            )));
        }
        record_event(
            &mut tx,
            &child.id,
            EventKind::DependencyRemoved,
            actor,
            json!({"parent": parent.key}),
        )
        .await?;
        tx.commit().await?;
        self.ticket_by_id(&child.id).await
    }

    /// The ticket's parents (up) and dependents (down), with statuses: what the Stack list and `dep tree` show.
    pub async fn dependency_tree(&self, key: &str) -> Result<DepTree, BoardError> {
        let ticket = self.ticket(key).await?;
        let tickets = self.tickets(ticket.key.project.as_str()).await?;
        let graph = Graph::new(&tickets);
        Ok(DepTree {
            parents: graph.nodes(&ticket.key, Direction::Up),
            children: graph.nodes(&ticket.key, Direction::Down),
            key: ticket.key,
            title: ticket.title,
            kind: ticket.kind,
            status: ticket.status,
        })
    }

    /// Every dependency loop in a project, each as a path that starts and ends on the same ticket. Empty on a
    /// valid database: links that would close a loop are refused.
    pub async fn dependency_cycles(
        &self,
        project: &str,
    ) -> Result<Vec<Vec<TicketKey>>, BoardError> {
        let tickets = self.tickets(project).await?;
        Ok(find_cycles(&tickets))
    }

    /// Refuses a base change while the ticket has links to tickets on another base (DEP-02).
    pub(crate) async fn check_base_change(
        &self,
        ticket: &Ticket,
        new_base: &Base,
    ) -> Result<(), BoardError> {
        let tickets = self.tickets(ticket.key.project.as_str()).await?;
        let moved = Ticket {
            base: new_base.clone(),
            ..ticket.clone()
        };
        for other in &tickets {
            let linked =
                ticket.depends_on.contains(&other.key) || other.depends_on.contains(&ticket.key);
            if linked {
                check_same_base(&moved, other)?;
            }
        }
        Ok(())
    }
}

/// Refuses a link between tickets with different bases, naming both (d0051).
pub(crate) fn check_same_base(a: &Ticket, b: &Ticket) -> Result<(), BoardError> {
    if a.base == b.base {
        return Ok(());
    }
    Err(BoardError::refused(
        Refusal::CrossBase,
        format!(
            "{} merges into {}, {} into {}. Linked tickets must use the same base branch.",
            a.key,
            a.base.describe(),
            b.key,
            b.base.describe()
        ),
    ))
}

/// A path from `from` to `to` following "is depended on by" links, both ends included.
fn path_down(tickets: &[Ticket], from: &TicketKey, to: &TicketKey) -> Option<Vec<TicketKey>> {
    let graph = Graph::new(tickets);
    let mut previous: HashMap<&TicketKey, &TicketKey> = HashMap::new();
    let mut queue = std::collections::VecDeque::from([from]);
    let mut seen = HashSet::from([from]);
    while let Some(current) = queue.pop_front() {
        if current == to {
            let mut path = vec![current.clone()];
            let mut at = current;
            while let Some(prev) = previous.get(at) {
                path.push((*prev).clone());
                at = prev;
            }
            path.reverse();
            return Some(path);
        }
        for next in graph.children_of(current) {
            if seen.insert(next) {
                previous.insert(next, current);
                queue.push_back(next);
            }
        }
    }
    None
}

fn find_cycles(tickets: &[Ticket]) -> Vec<Vec<TicketKey>> {
    let graph = Graph::new(tickets);
    let mut cycles = Vec::new();
    let mut done: HashSet<&TicketKey> = HashSet::new();
    for start in tickets.iter().map(|t| &t.key) {
        let mut stack: Vec<&TicketKey> = Vec::new();
        visit(&graph, start, &mut stack, &mut done, &mut cycles);
    }
    cycles
}

fn visit<'a>(
    graph: &Graph<'a>,
    at: &'a TicketKey,
    stack: &mut Vec<&'a TicketKey>,
    done: &mut HashSet<&'a TicketKey>,
    cycles: &mut Vec<Vec<TicketKey>>,
) {
    if done.contains(at) {
        return;
    }
    if let Some(i) = stack.iter().position(|k| *k == at) {
        let mut lap: Vec<TicketKey> = stack[i..].iter().map(|k| (*k).clone()).collect();
        lap.push(at.clone());
        cycles.push(lap);
        return;
    }
    stack.push(at);
    for next in graph.children_of(at) {
        visit(graph, next, stack, done, cycles);
    }
    stack.pop();
    done.insert(at);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
}

/// Links of one project, both ways.
struct Graph<'a> {
    by_key: HashMap<&'a TicketKey, &'a Ticket>,
    children: HashMap<&'a TicketKey, Vec<&'a TicketKey>>,
}

impl<'a> Graph<'a> {
    fn new(tickets: &'a [Ticket]) -> Self {
        let by_key = tickets.iter().map(|t| (&t.key, t)).collect();
        let mut children: HashMap<&TicketKey, Vec<&TicketKey>> = HashMap::new();
        for t in tickets {
            for parent in &t.depends_on {
                children.entry(parent).or_default().push(&t.key);
            }
        }
        for list in children.values_mut() {
            list.sort();
        }
        Self { by_key, children }
    }

    fn children_of(&self, key: &TicketKey) -> impl Iterator<Item = &'a TicketKey> + '_ {
        self.children.get(key).into_iter().flatten().copied()
    }

    fn parents_of(&self, key: &TicketKey) -> Vec<&'a TicketKey> {
        self.by_key
            .get(key)
            .map(|t| t.depends_on.iter().collect())
            .unwrap_or_default()
    }

    /// The nested parents or dependents of `key`, each ticket once: a breadth-first walk puts each under the
    /// first ticket that reaches it on the nearest level; the visited set also guards against loops in a damaged
    /// database.
    fn nodes(&self, key: &TicketKey, direction: Direction) -> Vec<DepNode> {
        let next = |k: &TicketKey| -> Vec<&'a TicketKey> {
            let mut next = match direction {
                Direction::Up => self.parents_of(k),
                Direction::Down => self.children_of(k).collect(),
            };
            next.sort();
            next
        };
        let mut seen: HashSet<&TicketKey> = HashSet::from([key]);
        let mut under: HashMap<&TicketKey, Vec<&'a TicketKey>> = HashMap::new();
        let mut level = vec![key];
        while !level.is_empty() {
            let mut below = Vec::new();
            for at in level {
                for k in next(at) {
                    if self.by_key.contains_key(k) && seen.insert(k) {
                        under.entry(at).or_default().push(k);
                        below.push(k);
                    }
                }
            }
            level = below;
        }
        self.built(key, &under)
    }

    fn built(
        &self,
        key: &TicketKey,
        under: &HashMap<&TicketKey, Vec<&'a TicketKey>>,
    ) -> Vec<DepNode> {
        under
            .get(key)
            .into_iter()
            .flatten()
            .filter_map(|k| {
                let ticket = self.by_key.get(k)?;
                Some(DepNode {
                    key: ticket.key.clone(),
                    title: ticket.title.clone(),
                    kind: ticket.kind,
                    status: ticket.status,
                    nodes: self.built(k, under),
                })
            })
            .collect()
    }
}
