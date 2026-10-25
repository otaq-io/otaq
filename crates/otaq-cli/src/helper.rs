//! `otaq helper …` (d0133, T-0537): the agent in charge's helpers. `start <n> --title "<piece>" [--stdin]` starts the
//! helper of piece `n` (the note on stdin is its first comment), `list` prints each helper's piece, title, status,
//! branch and last activity, `diff <n>` its branch against the ticket's, `back <n> --stdin` sends it back with a note,
//! `merge <n>` merges its piece into the ticket's branch, `answer <n> --stdin` answers its question and `pass-on <id>
//! [--note …]` passes it on to the user (T-0538). A helper (`$OTAQ_PIECE` set) runs `done --stdin` to hand its
//! piece back. They work on `$OTAQ_TICKET`; the service refuses anyone but the agent in charge (or the helper).
//!
//! `--json` prints the helper (`otaq_core::helpers::Helper`) with the text as `{"helper", "text"}`, `list --json` the
//! helpers, `diff --json` `{"helper", "against", "files", "patch"}`.

use std::io::Read;

use clap::Subcommand;
use otaq_core::data_dir::DataDir;
use otaq_core::helpers::{Helper, piece_env_var};
use otaq_core::naming;
use otaq_core::protocol::{
    ErrorBody, ErrorKind, HelperNoteParams, HelperPassOnParams, HelperPieceParams,
    HelperStartParams, KeyParams, Reply, Request,
};

use crate::cli::GlobalArgs;
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum HelperCommand {
    /// Start the helper of piece N in its own worktree (the agent in charge): `start 3 --title "Wishlist page"`;
    /// a note for it on stdin with --stdin
    Start {
        /// The piece's number in the Plan's Pieces list
        piece: u32,
        /// The piece's title, as the Plan names it
        #[arg(long)]
        title: String,
        /// Read a note for the helper from stdin (posted as its first comment)
        #[arg(long)]
        stdin: bool,
    },
    /// Each helper's piece, title, status, branch and last activity
    List,
    /// The helper's branch against the ticket's: what merging it would bring
    Diff {
        /// The piece's number
        piece: u32,
    },
    /// Send the helper back with a note on stdin, typed into its session when it is idle
    Back {
        /// The piece's number
        piece: u32,
        /// Read the note from stdin
        #[arg(long)]
        stdin: bool,
    },
    /// Merge the helper's piece into the ticket's branch: refused with uncommitted files or a conflict
    Merge {
        /// The piece's number
        piece: u32,
    },
    /// Answer the helper's question with the answer on stdin, typed into its session when it is idle
    Answer {
        /// The piece's number
        piece: u32,
        /// Read the answer from stdin
        #[arg(long)]
        stdin: bool,
    },
    /// Pass the helper's question on to the user (its id is in the note that told you about it), with why
    #[command(name = "pass-on")]
    PassOn {
        /// The question's id
        id: String,
        /// Why it is the user's: "it's a product question the plan doesn't answer"
        #[arg(long)]
        note: Option<String>,
    },
    /// Hand your piece back to the agent in charge with a summary on stdin (helpers only); refused while files are
    /// uncommitted
    Done {
        /// Read the summary from stdin
        #[arg(long)]
        stdin: bool,
    },
}

fn usage(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorKind::Usage, message)
}

fn read_stdin() -> Result<String, ErrorBody> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| internal(format!("can't read stdin: {e}")))?;
    Ok(text.trim().to_owned())
}

/// Runs one `otaq helper` command.
pub fn run(command: HelperCommand, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let cmd = naming::COMMAND_NAME;
    // What stdin carries is read before connecting, as the other `--stdin` commands do.
    let text = match &command {
        HelperCommand::Start { stdin: true, .. } => Some(read_stdin()?),
        HelperCommand::Back {
            stdin: false,
            piece,
        } => {
            return Err(usage(format!(
                "give the note on stdin: `{cmd} helper back {piece} --stdin`"
            )));
        }
        HelperCommand::Answer {
            stdin: false,
            piece,
        } => {
            return Err(usage(format!(
                "give the answer on stdin: `{cmd} helper answer {piece} --stdin`"
            )));
        }
        HelperCommand::Done { stdin: false } => {
            return Err(usage(format!(
                "give your summary on stdin: `{cmd} helper done --stdin`"
            )));
        }
        HelperCommand::Back { .. } | HelperCommand::Answer { .. } | HelperCommand::Done { .. } => {
            let text = read_stdin()?;
            if text.is_empty() {
                return Err(usage("the text on stdin is empty"));
            }
            Some(text)
        }
        _ => None,
    };
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, None, global)
            .await?
            .to_string();
        let request = match command {
            HelperCommand::Start { piece, title, .. } => Request::HelperStart(HelperStartParams {
                key,
                piece,
                title,
                note: text.filter(|t| !t.is_empty()),
            }),
            HelperCommand::List => Request::HelperList(KeyParams { key }),
            HelperCommand::Diff { piece } => Request::HelperDiff(HelperPieceParams { key, piece }),
            HelperCommand::Merge { piece } => {
                Request::HelperMerge(HelperPieceParams { key, piece })
            }
            HelperCommand::Back { piece, .. } => Request::HelperBack(HelperNoteParams {
                key,
                piece,
                text: text.unwrap_or_default(),
            }),
            HelperCommand::Answer { piece, .. } => Request::HelperAnswer(HelperNoteParams {
                key,
                piece,
                text: text.unwrap_or_default(),
            }),
            HelperCommand::PassOn { id, note } => {
                Request::HelperPassOn(HelperPassOnParams { key, id, note })
            }
            HelperCommand::Done { .. } => Request::HelperDone(HelperNoteParams {
                key,
                piece: std::env::var(piece_env_var())
                    .ok()
                    .and_then(|p| p.trim().parse().ok())
                    .unwrap_or_default(),
                text: text.unwrap_or_default(),
            }),
        };
        match client.request(request).await.map_err(client_error)? {
            Reply::Helper(reply) => {
                out.print(&reply.text, &reply, Some(&reply.helper.label()));
            }
            Reply::Helpers(helpers) => out.print(&render_list(&helpers), &helpers, None),
            Reply::HelperDiff(diff) => {
                let human = if diff.files.is_empty() {
                    format!(
                        "{} has no changes against {}.",
                        diff.helper.label(),
                        diff.against
                    )
                } else {
                    diff.patch.trim_end().to_owned()
                };
                out.print(&human, &diff, Some(&diff.helper.label()));
            }
            other => return Err(unexpected(&other)),
        }
        Ok(())
    })
}

/// `otaq helper list`:
///
/// ```text
/// 40.1  Wishlist API   merged           otaq/SHOP-40.1  12 min ago
/// 40.3  Wishlist page  working          otaq/SHOP-40.3  just now
/// ```
fn render_list(helpers: &[Helper]) -> String {
    if helpers.is_empty() {
        return format!(
            "No helpers yet: start one with `{} helper start <n> --title \"<piece>\"`.",
            naming::COMMAND_NAME
        );
    }
    let now = otaq_core::store::now_ms();
    let rows: Vec<[String; 5]> = helpers
        .iter()
        .map(|h| {
            [
                h.label(),
                h.title.clone(),
                match &h.held_reason {
                    Some(reason) if h.status == otaq_core::helpers::HelperStatus::Held => {
                        format!("{} ({reason})", h.status.label())
                    }
                    _ => h.row_status(),
                },
                h.branch.clone(),
                ago(now - h.updated_at),
            ]
        })
        .collect();
    let width = |i: usize| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0);
    let widths = [width(0), width(1), width(2), width(3)];
    rows.iter()
        .map(|r| {
            format!(
                "{:w0$}  {:w1$}  {:w2$}  {:w3$}  {}",
                r[0],
                r[1],
                r[2],
                r[3],
                r[4],
                w0 = widths[0],
                w1 = widths[1],
                w2 = widths[2],
                w3 = widths[3],
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// "just now", "12 min ago", "3 h ago", "2 d ago".
fn ago(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        0 => "just now".to_owned(),
        m if m < 60 => format!("{m} min ago"),
        m if m < 24 * 60 => format!("{} h ago", m / 60),
        m => format!("{} d ago", m / (24 * 60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(ago(30_000), "just now");
        assert_eq!(ago(12 * 60_000), "12 min ago");
        assert_eq!(ago(3 * 3_600_000), "3 h ago");
        assert_eq!(ago(50 * 3_600_000), "2 d ago");
    }
}
