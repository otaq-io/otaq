//! The Open PR and Push skills' commands (REV-10, GIT-07; d0050): `otaq pr propose --stdin` (the proposal as
//! one JSON object on stdin, documented in the open-pr skill), `otaq pr message --stdin` (the delivery's message its
//! agent writes while the ticket waits in Review), `otaq pr opened <url>` and `otaq branch rename <name>`. All work on `$OTAQ_TICKET`; the service refuses an agent acting on another ticket.
//!
//! # `--json` shapes
//!
//! Each prints the ticket afterwards (`Ticket`); after `pr opened` its `flags.pr_number` and `work.pr_url` are set,
//! after `branch rename` its `work.branch`.

use std::io::Read;

use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_core::protocol::{
    BranchRenameParams, DeliveryMessageParams, ErrorBody, ErrorKind, PrOpenedParams,
    PrProposeParams, Reply, Request,
};

use crate::cli::{BranchCommand, GlobalArgs, PrCommand};
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

pub fn pr(command: PrCommand, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let cmd = naming::COMMAND_NAME;
    match command {
        PrCommand::Propose { stdin } => {
            if !stdin {
                return Err(ErrorBody::new(
                    ErrorKind::Usage,
                    format!("give the proposal on stdin: `{cmd} pr propose --stdin`"),
                ));
            }
            let mut proposal = String::new();
            std::io::stdin()
                .read_to_string(&mut proposal)
                .map_err(|e| internal(format!("can't read stdin: {e}")))?;
            if proposal.trim().is_empty() {
                return Err(ErrorBody::new(
                    ErrorKind::Usage,
                    "the proposal on stdin is empty",
                ));
            }
            send(
                global,
                out,
                |t| match t.flags.delivering_automatically {
                    // An automatic Delivery (FLW-15): Otaq confirmed the proposal itself, no dialog.
                    true => format!(
                        "{}'s proposal is confirmed: this flow opens the PR with no dialog. End your turn: you'll be told to push.",
                        t.key
                    ),
                    false => format!(
                        "{}'s proposal is with the user. End your turn: nothing is pushed until they confirm.",
                        t.key
                    ),
                },
                move |key| Request::TicketPrPropose(PrProposeParams { key, proposal }),
            )
        }
        PrCommand::Message { stdin } => {
            if !stdin {
                return Err(ErrorBody::new(
                    ErrorKind::Usage,
                    format!("give the message on stdin: `{cmd} pr message --stdin`"),
                ));
            }
            let mut message = String::new();
            std::io::stdin()
                .read_to_string(&mut message)
                .map_err(|e| internal(format!("can't read stdin: {e}")))?;
            if message.trim().is_empty() {
                return Err(ErrorBody::new(
                    ErrorKind::Usage,
                    "the message on stdin is empty",
                ));
            }
            send(
                global,
                out,
                |t| otaq_core::pr::message::stored_text(&t.key.to_string()),
                move |key| Request::TicketDeliveryMessage(DeliveryMessageParams { key, message }),
            )
        }
        PrCommand::Opened { url } => send(
            global,
            out,
            |t| match t.flags.pr_number {
                Some(n) => format!("{} has PR #{n}.", t.key),
                None => format!("{} has a PR.", t.key),
            },
            move |key| Request::TicketPrOpened(PrOpenedParams { key, url }),
        ),
    }
}

pub fn branch(command: BranchCommand, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let BranchCommand::Rename { name } = command;
    send(
        global,
        out,
        |t| {
            format!(
                "{}'s branch is now {}.",
                t.key,
                t.work
                    .as_ref()
                    .and_then(|w| w.branch.as_deref())
                    .unwrap_or("?")
            )
        },
        move |key| Request::TicketBranchRename(BranchRenameParams { key, name }),
    )
}

/// Sends a request about the current ticket and prints the ticket it answers with.
fn send(
    global: &GlobalArgs,
    out: &Output,
    done: fn(&otaq_core::board::tickets::Ticket) -> String,
    request: impl FnOnce(String) -> Request,
) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, None, global).await?;
        let ticket = match client
            .request(request(key.to_string()))
            .await
            .map_err(client_error)?
        {
            Reply::Ticket(t) => *t,
            other => return Err(unexpected(&other)),
        };
        out.print(&done(&ticket), &ticket, Some(&ticket.key.to_string()));
        Ok(())
    })
}
