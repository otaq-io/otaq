//! A helper's `otaq prime` (d0133): **its piece first** (the piece's entry in the Plan's Pieces list), the notes
//! between it and the agent in charge, then the ticket, the whole Plan, the ticket's other comments, and the rules of
//! a helper (hand back with `otaq helper done`; the agent in charge moves the ticket).

use serde::{Deserialize, Serialize};

use super::{Helper, Helpers};
use crate::board::BoardError;
use crate::board::ticket_comments::{CommentAuthor, TicketComment};
use crate::board::tickets::Ticket;
use crate::caller::Caller;
use crate::domain::plan::{Piece, piece};
use crate::naming;

/// Everything a helper's `prime` prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperPrime {
    pub ticket: Ticket,
    pub helper: Helper,
    /// The ticket's branch, which its work merges into: `otaq/SHOP-40`.
    pub merges_into: String,
    /// The piece's own entry in the Plan's Pieces list, when the Plan has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece: Option<Piece>,
    /// The notes between this helper and the agent in charge ("To 40.3: …", and the helper's hand-backs), oldest
    /// first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<TicketComment>,
    /// The ticket's other comments, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<TicketComment>,
}

impl Helpers {
    /// `otaq prime` from a helper.
    pub async fn prime(&self, caller: &Caller) -> Result<HelperPrime, BoardError> {
        let (ticket, helper) = self.helper_of(caller).await?;
        let board = &self.inner.board;
        let all = board.ticket_comments(&ticket.key.to_string()).await?;
        let label = helper.label();
        let (notes, comments) = all
            .comments
            .into_iter()
            .partition(|c| is_note_of(c, &label));
        Ok(HelperPrime {
            merges_into: super::ticket_branch(&ticket)?,
            piece: piece(&ticket.plan, helper.piece),
            notes,
            comments,
            ticket,
            helper,
        })
    }
}

/// Whether `comment` is between the helper `label` ("40.3") and the agent in charge: a note to it, or its own.
fn is_note_of(comment: &TicketComment, label: &str) -> bool {
    let to = comment.body.starts_with(&format!("To {label}:"));
    let by = matches!(&comment.author, CommentAuthor::Agent { name: Some(name), .. }
        if name == label || name.ends_with(&format!(" \u{b7} {label}")));
    to || by
}

/// The Markdown a helper reads.
pub fn render(p: &HelperPrime) -> String {
    let t = &p.ticket;
    let h = &p.helper;
    let cmd = naming::COMMAND_NAME;
    let label = h.label();
    let mut out = format!("# {label} \u{b7} {}\n\n", h.title);
    out.push_str(&format!(
        "You are the helper for piece {} of {} ({}). Work only on this piece, in this worktree, on the branch {}. The \
         agent in charge of {} reviews your work and merges it into {}.\n",
        h.piece, t.key, t.title, h.branch, t.key, p.merges_into
    ));
    out.push_str("\n## Your piece\n\n");
    match &p.piece {
        Some(piece) => {
            let waits = match piece.waits_for.as_slice() {
                [] => "waits for nothing".to_owned(),
                waits => format!(
                    "waits for {}",
                    waits
                        .iter()
                        .map(|n| crate::domain::plan::piece_name(&t.key, *n))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            out.push_str(&format!("{label} {} \u{2014} {waits}\n", piece.title));
            if !piece.text.is_empty() {
                out.push_str(&format!("\n{}\n", piece.text));
            }
        }
        None => out.push_str(&format!(
            "{label} {} (the Plan doesn't list it: the agent in charge's notes say what it is)\n",
            h.title
        )),
    }
    if !p.notes.is_empty() {
        out.push_str("\n## Notes between you and the agent in charge\n");
        for c in &p.notes {
            out.push_str(&format!("\n### {}\n\n{}\n", c.heading(), c.body.trim()));
        }
    }
    out.push_str(&format!(
        "\n## The ticket: {} \u{b7} {}\n\n",
        t.key, t.title
    ));
    let body = t.body.trim();
    out.push_str(if body.is_empty() {
        "(no description)"
    } else {
        body
    });
    out.push('\n');
    out.push_str(&crate::prime::render_plan(&t.plan));
    out.push_str(&crate::prime::render_comments(&t.key, &p.comments));
    out.push_str(&format!(
        "\n## Rules\n\n\
         - Commit your work in this worktree; never push, never merge into {merges}: the agent in charge does.\n\
         - When your piece is done, hand it back with a short summary: `{cmd} helper done --stdin`. It is refused while \
           files are uncommitted. Then end your turn: the agent in charge may send it back with a note.\n\
         - You never move the ticket: `{cmd} stage next`, `{cmd} stage done`, `{cmd} check` and `{cmd} helper start` \
           are the agent in charge's.\n",
        merges = p.merges_into
    ));
    out
}
