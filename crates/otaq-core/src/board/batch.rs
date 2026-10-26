//! Creating several tickets at once (`otaq create -f plan.md`, d0032), and `create` with `--deps`.
//!
//! **All or nothing:** every reference is checked first; then the tickets are created in file order and linked
//! with the board's usual checks (cycles, same base, d0049, d0051). If anything fails, every ticket of the batch
//! is deleted again (their numbers stay used, as after any deletion, d0029) and the error is returned.
//!
//! # The `-f` format
//!
//! The Markdown shape of the v0.1 tickets file itself (a copy lives in `tests/fixtures/`), so that file can be imported:
//!
//! ```text
//! ## M4 · Agent signals                  ← level-2 headings group tickets; ignored
//!
//! ### T-0064 · Caller identity           ← one ticket per level-3 heading: "<ID> · <title>" or "<title>"
//! - **Depends on:** T-0007, SHOP-18      ← IDs of this file, or keys of existing tickets; "—" for none
//! - **Priority:** 2                      ← optional: 1 Urgent … 4 Low (default 3)
//! - **Stage:** Dev=Quick developer · typo fix ← optional, repeatable: a stage choice, " · " and the agent's reason
//! - **Size:** M                          ← any other "- **Name:** value" line at the top is ignored
//!
//! **What and why**                       ← the body: every other line of the section (this label removed)
//! The service must know who is calling…
//!
//! **Done when**                          ← a checklist: "- [ ] item", "- [x] item" or "- item" lines, appended
//! - [ ] the service maps a caller PID…      to the body as a "## Done when" list (d0067)
//!
//! **Notes:** macOS process ancestry…     ← after Done when: kept at the end of the body
//! ```
//!
//! An ID is a word with a digit before " · " in the heading (`T-0064`, `E1`); it only resolves links within the
//! file.

use serde::{Deserialize, Serialize};

use super::tickets::{NewTicketInput, Ticket};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, Priority, TicketKey};

/// One ticket of a batch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchTicket {
    /// The ticket's ID within the batch (`T-0064`), for links between its tickets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// The Plan section (d0133; `create --plan`); a file's tickets have none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub plan: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    /// What it depends on: batch IDs or keys.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Its stage choices (`--stage Dev=Quick developer`, `- **Stage:** Dev=Quick developer · typo fix`; AGM-11).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<super::stage_choices::ChoicePick>,
}

/// A reference in a batch, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ref {
    /// The n-th ticket of the batch.
    Batch(usize),
    /// A ticket that exists.
    Existing(Box<Ticket>),
}

impl Board {
    /// Resolves a batch reference: a batch ID first, then a key.
    pub async fn batch_ref(
        &self,
        tickets: &[BatchTicket],
        reference: &str,
    ) -> Result<Ref, BoardError> {
        let reference = reference.trim();
        if let Some(n) = tickets
            .iter()
            .position(|t| t.id.as_deref() == Some(reference))
        {
            return Ok(Ref::Batch(n));
        }
        match self.ticket(reference).await {
            Ok(t) => Ok(Ref::Existing(Box::new(t))),
            Err(BoardError::NotFound(_)) => Err(BoardError::NotFound(format!(
                "{reference} is neither a ticket of this batch nor an existing ticket"
            ))),
            Err(e) => Err(e),
        }
    }

    /// Creates every ticket of the batch in Backlog, in order, with their links; all or nothing.
    pub async fn create_batch(
        &self,
        project: &str,
        tickets: &[BatchTicket],
        actor: &Actor,
    ) -> Result<Vec<Ticket>, BoardError> {
        if tickets.is_empty() {
            return Err(BoardError::refused(
                Refusal::EmptyField,
                "There are no tickets to create",
            ));
        }
        // Check every reference before creating anything.
        for (n, ticket) in tickets.iter().enumerate() {
            if ticket.title.trim().is_empty() {
                return Err(BoardError::refused(
                    Refusal::EmptyField,
                    format!("Ticket {} of the batch has no title", n + 1),
                ));
            }
            for reference in &ticket.depends_on {
                self.batch_ref(tickets, reference).await?;
            }
        }

        let mut created: Vec<Ticket> = Vec::new();
        let result = self.create_all(project, tickets, actor, &mut created).await;
        match result {
            Ok(()) => {
                let mut fresh = Vec::new();
                for t in &created {
                    fresh.push(self.ticket_by_id(&t.id).await?);
                }
                Ok(fresh)
            }
            Err(e) => {
                for t in created.iter().rev() {
                    if let Err(undo) = self.delete_ticket(&t.key.to_string()).await {
                        tracing::error!(key = %t.key, error = %undo, "can't undo a batch ticket");
                    }
                }
                Err(e)
            }
        }
    }

    async fn create_all(
        &self,
        project: &str,
        tickets: &[BatchTicket],
        actor: &Actor,
        created: &mut Vec<Ticket>,
    ) -> Result<(), BoardError> {
        let key_of = |created: &[Ticket], r: Ref| -> Option<TicketKey> {
            match r {
                Ref::Batch(n) => created.get(n).map(|t| t.key.clone()),
                Ref::Existing(t) => Some(t.key.clone()),
            }
        };
        for ticket in tickets {
            let input = NewTicketInput {
                project: project.to_owned(),
                title: ticket.title.clone(),
                body: ticket.body.clone(),
                plan: ticket.plan.clone(),
                priority: ticket.priority,
                ..Default::default()
            };
            created.push(self.create_ticket(&input, actor).await?);
        }
        for (n, ticket) in tickets.iter().enumerate() {
            for dep in &ticket.depends_on {
                let r = self.batch_ref(tickets, dep).await?;
                let parent = key_of(created, r).expect("created in order");
                let child = created[n].key.to_string();
                self.add_dependency(&child, &parent.to_string(), actor)
                    .await?;
            }
        }
        Ok(())
    }
}

/// Reads the `-f` Markdown (see the module documentation).
pub fn parse_markdown(text: &str) -> Result<Vec<BatchTicket>, String> {
    let mut tickets = Vec::new();
    let mut current: Option<Section> = None;
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("### ") {
            tickets.extend(current.take().map(Section::finish));
            current = Some(Section::new(heading));
        } else if line.starts_with("## ") || line.starts_with("# ") {
            tickets.extend(current.take().map(Section::finish));
        } else if let Some(section) = &mut current {
            section.line(line)?;
        }
    }
    tickets.extend(current.map(Section::finish));
    if tickets.is_empty() {
        return Err("no tickets found: each ticket starts with a \"### Title\" heading".to_owned());
    }
    Ok(tickets)
}

#[derive(Debug, PartialEq, Eq)]
enum Part {
    /// The "- **Name:** value" lines right under the heading.
    Fields,
    Body,
    /// The "Done when" checklist; it becomes a list in the body.
    Criteria,
    /// After the Done when list.
    After,
}

struct Section {
    ticket: BatchTicket,
    part: Part,
    body: Vec<String>,
    criteria: Vec<String>,
    after: Vec<String>,
}

impl Section {
    fn new(heading: &str) -> Self {
        let heading = heading.trim();
        let (id, title) = match heading.split_once(" · ") {
            Some((id, title))
                if !id.trim().contains(' ') && id.bytes().any(|b| b.is_ascii_digit()) =>
            {
                (Some(id.trim().to_owned()), title.trim())
            }
            _ => (None, heading),
        };
        Self {
            ticket: BatchTicket {
                id,
                title: title.to_owned(),
                ..Default::default()
            },
            part: Part::Fields,
            body: Vec::new(),
            criteria: Vec::new(),
            after: Vec::new(),
        }
    }

    fn line(&mut self, line: &str) -> Result<(), String> {
        let trimmed = line.trim();
        if self.part == Part::Fields {
            if let Some((name, value)) = field(trimmed) {
                return self.field(&name, value);
            }
            if trimmed.is_empty() {
                return Ok(());
            }
            self.part = Part::Body;
        }
        if is_label(trimmed, "Done when") {
            self.part = Part::Criteria;
            return Ok(());
        }
        match self.part {
            Part::Fields | Part::Body => {
                if !is_label(trimmed, "What and why") {
                    self.body.push(line.to_owned());
                }
            }
            Part::Criteria => {
                if let Some(item) = checklist_item(trimmed) {
                    self.criteria.push(item.to_owned());
                } else if !trimmed.is_empty() {
                    self.part = Part::After;
                    self.after.push(line.to_owned());
                }
            }
            Part::After => self.after.push(line.to_owned()),
        }
        Ok(())
    }

    fn field(&mut self, name: &str, value: &str) -> Result<(), String> {
        let refs = || -> Vec<String> {
            value
                .split(',')
                .map(|v| v.trim().trim_matches('`').to_owned())
                .filter(|v| {
                    !v.is_empty() && v != "—" && v != "-" && !v.eq_ignore_ascii_case("none")
                })
                .collect()
        };
        match name.to_ascii_lowercase().as_str() {
            "depends on" => self.ticket.depends_on = refs(),
            "priority" => {
                self.ticket.priority = Some(
                    value
                        .parse()
                        .map_err(|e| format!("{}: {e}", self.ticket.title))?,
                );
            }
            "stage" => {
                let (pick, reason) = match value.split_once(" · ") {
                    Some((pick, reason)) => (pick, Some(reason)),
                    None => (value, None),
                };
                let pick = super::stage_choices::ChoicePick::parse(pick, reason)
                    .map_err(|e| format!("{}: {e}", self.ticket.title))?;
                self.ticket.choices.push(pick);
            }
            _ => {}
        }
        Ok(())
    }

    /// The body: the description, then the checklist as a "## Done when" list, then what came after it.
    fn finish(self) -> BatchTicket {
        let mut ticket = self.ticket;
        let criteria = self
            .criteria
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n");
        let parts = [
            self.body.join("\n").trim().to_owned(),
            if criteria.is_empty() {
                String::new()
            } else {
                format!("{CRITERIA_HEADING}\n\n{criteria}")
            },
            self.after.join("\n").trim().to_owned(),
        ];
        ticket.body = parts
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        ticket
    }
}

/// The heading a "Done when" checklist gets in the body; migration 0022 uses the same one.
pub const CRITERIA_HEADING: &str = "## Done when";

/// `- **Name:** value` → (Name, value).
fn field(line: &str) -> Option<(String, &str)> {
    let rest = line.strip_prefix("- **")?;
    let (name, value) = rest.split_once(":**")?;
    Some((name.trim().to_owned(), value.trim()))
}

/// `**Done when**`, `**Done when:**`, `Done when` as a label line.
fn is_label(line: &str, label: &str) -> bool {
    let bare = line.trim_matches('*').trim_end_matches(':').trim();
    bare.eq_ignore_ascii_case(label)
}

/// `- [ ] item`, `- [x] item`, `- item` → item.
fn checklist_item(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))?;
    let rest = rest
        .strip_prefix("[ ] ")
        .or_else(|| rest.strip_prefix("[x] "))
        .or_else(|| rest.strip_prefix("[X] "))
        .unwrap_or(rest);
    let item = rest.trim();
    (!item.is_empty()).then_some(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = "# Plan

Intro text is ignored.

## M4 · Agent signals

### T-0064 · Caller identity by process ancestry, `--actor`
- **Milestone:** M4 · Agent signals
- **Depends on:** T-0007, T-0050
- **Size:** M

**What and why**
The service must know **who** is calling.

It walks the ancestry.

**Done when**
- [ ] the service maps a caller PID
- [x] `--actor` defaults to `agent:SHOP-18`

**Notes:** macOS via libproc.

### T-0065 · Permission scope
- **Depends on:** T-0064
- **Priority:** 2

Agents run the CLI without prompts.

**Done when**
- one policy function

### E1 · Wishlist
- **Depends on:** —

### Wishlist API · button and page
- **Parent:** E1
";

    #[test]
    fn reads_the_tickets_file_shape() {
        let tickets = parse_markdown(PLAN).unwrap();
        assert_eq!(tickets.len(), 4);
        let first = &tickets[0];
        assert_eq!(first.id.as_deref(), Some("T-0064"));
        assert_eq!(
            first.title,
            "Caller identity by process ancestry, `--actor`"
        );
        assert_eq!(first.depends_on, ["T-0007", "T-0050"]);
        assert_eq!(
            first.body,
            "The service must know **who** is calling.\n\nIt walks the ancestry.\n\n## Done when\n\n\
             - the service maps a caller PID\n- `--actor` defaults to `agent:SHOP-18`\n\n\
             **Notes:** macOS via libproc."
        );
        let second = &tickets[1];
        assert_eq!(
            second.body,
            "Agents run the CLI without prompts.\n\n## Done when\n\n- one policy function"
        );
        assert_eq!(tickets[2].body, "", "no description, no checklist");
        assert_eq!(first.priority, None, "no line: the default");
        assert_eq!(second.priority, Some(Priority::High));
        assert!(parse_markdown("### X\n- **Priority:** 5\n").is_err());
        assert_eq!(tickets[2].id.as_deref(), Some("E1"));
        assert!(tickets[2].depends_on.is_empty());
        assert_eq!(tickets[3].id, None, "an ID has a digit");
        assert_eq!(tickets[3].title, "Wishlist API · button and page");
        assert!(tickets[3].depends_on.is_empty(), "a Parent line is ignored");
    }

    /// The format stays compatible with the tickets file itself (d0032): every ticket reads, its Done when list
    /// becomes part of a non-empty body, and every "Depends on" names a ticket of the file.
    #[test]
    fn the_v0_1_tickets_file_imports() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/v0.1-tickets.md"
        );
        let text = std::fs::read_to_string(path).unwrap();
        let tickets = parse_markdown(&text).unwrap();
        assert_eq!(tickets.len(), 247);
        let ids: Vec<&str> = tickets.iter().filter_map(|t| t.id.as_deref()).collect();
        assert_eq!(ids.len(), 247);
        for ticket in &tickets {
            assert!(ticket.id.as_deref().unwrap().starts_with("T-"));
            assert!(!ticket.body.is_empty(), "{}", ticket.title);
            assert!(ticket.body.contains(CRITERIA_HEADING), "{}", ticket.title);
            for dep in &ticket.depends_on {
                assert!(ids.contains(&dep.as_str()), "{}: {dep}", ticket.title);
            }
        }
        let t64 = tickets
            .iter()
            .find(|t| t.id.as_deref() == Some("T-0064"))
            .unwrap();
        assert_eq!(t64.title, "Caller identity by process ancestry, `--actor`");
        assert_eq!(t64.depends_on, ["T-0007", "T-0050"]);
    }

    /// AGM-11: "- **Stage:** Dev=Quick developer · typo fix" lines, each a choice with an optional reason.
    #[test]
    fn stage_lines_are_choices() {
        use crate::board::stage_choices::ChoicePick;
        let tickets = parse_markdown(
            "### Fix README typo\n- **Stage:** Dev=Quick developer · typo fix\n- **Stage:** Product spec=skip\n\nTypo.\n",
        )
        .unwrap();
        let pick = |stage: &str, choice: &str, reason: Option<&str>| ChoicePick {
            stage: stage.into(),
            choice: choice.into(),
            reason: reason.map(Into::into),
        };
        assert_eq!(
            tickets[0].choices,
            [
                pick("Dev", "Quick developer", Some("typo fix")),
                pick("Product spec", "skip", None)
            ]
        );
        assert_eq!(tickets[0].body, "Typo.");
        assert!(parse_markdown("### A\n- **Stage:** Dev\n").is_err());
    }

    #[test]
    fn a_file_without_tickets_is_an_error() {
        assert!(parse_markdown("# nothing here\n").is_err());
        assert!(parse_markdown("### X\n- **Priority:** 9\n").is_err());
    }
}
