//! `otaq create` (alias `new`) and `otaq update` (SDD §11.2; PLG-05, CLI-02, CLI-04; d0019, d0032).
//!
//! New tickets land in **Backlog**. Long text goes through **stdin** (`--stdin`, PLG-05); `-d` is for a short
//! line. The service records the actor, so tickets an agent creates show "created by agent" and its edits are
//! kept as before/after (T-0023); an agent may only update its own ticket (T-0065).
//!
//! # `--json` shapes
//!
//! - `create "Title"`: the new `Ticket`; `create -f`: `[Ticket]` in file order.
//! - `update`: the `Ticket` as it is afterwards.
//!
//! Each `Ticket` also carries `"agents"`: the agent each stage runs, and `"choices"`: its stage choices, as `show
//! --json` has them (`crate::read::StageAgentJson`, `crate::read::ChoiceJson`; AGM-05, AGM-11).
//!
//! **Stage choices** (AGM-11, d0108, d0087): `--stage "<stage>=<agent|skip>"` (repeatable; `required` sets a
//! skippable stage back), `--reason` for an agent's choice, `--by-user` when the user named them. The service judges
//! who may set which (`otaq_core::board::stage_choices`).

use std::io::Read;
use std::path::PathBuf;

use otaq_core::board::batch::{BatchTicket, parse_markdown};
use otaq_core::board::stage_choices::ChoicePick;
use otaq_core::board::tickets::{Ticket, TicketUpdate};
use otaq_core::client::Client;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::Priority;
use otaq_core::protocol::{BatchParams, ErrorBody, ErrorKind, Reply, Request, TicketUpdateParams};

use crate::cli::{ChoiceFlags, GlobalArgs};
use crate::output::Output;
use crate::read::{self, unexpected};
use crate::service::{self, client_error, internal};
use crate::target;

pub struct CreateArgs {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<Priority>,
    pub deps: Vec<String>,
    pub stdin: bool,
    pub file: Option<PathBuf>,
    pub choices: ChoiceFlags,
}

pub struct UpdateArgs {
    pub key: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<Priority>,
    pub stdin: bool,
    pub choices: ChoiceFlags,
}

fn usage(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorKind::Usage, message)
}

/// The `--stage` flags as choices, each with `--reason`. `--reason` alone is a usage error, and so is `--by-user`
/// unless a file (`file`) brings the choices.
fn picks(flags: &ChoiceFlags, file: bool) -> Result<Vec<ChoicePick>, ErrorBody> {
    if flags.stages.is_empty() && flags.reason.is_some() {
        return Err(usage("--reason goes with --stage"));
    }
    if flags.stages.is_empty() && flags.by_user && !file {
        return Err(usage("--by-user goes with --stage (or -f)"));
    }
    flags
        .stages
        .iter()
        .map(|stage| ChoicePick::parse(stage, flags.reason.as_deref()).map_err(usage))
        .collect()
}

/// The body from `-d` or `--stdin` (not both).
fn body(description: Option<String>, stdin: bool) -> Result<Option<String>, ErrorBody> {
    match (description, stdin) {
        (Some(_), true) => Err(usage("give the description with -d or --stdin, not both")),
        (Some(d), false) => Ok(Some(d)),
        (None, true) => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| internal(format!("can't read stdin: {e}")))?;
            Ok(Some(text.trim_end().to_owned()))
        }
        (None, false) => Ok(None),
    }
}

pub fn create(args: CreateArgs, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    let choices = picks(&args.choices, args.file.is_some())?;
    let (tickets, single) = match &args.file {
        Some(file) => {
            // `--by-user` makes the file's choices the user's.
            if args.title.is_some()
                || !args.deps.is_empty()
                || args.priority.is_some()
                || args.description.is_some()
                || args.stdin
                || !choices.is_empty()
            {
                return Err(usage(
                    "-f takes everything from the file; give no title or other flags",
                ));
            }
            let text = std::fs::read_to_string(file)
                .map_err(|e| usage(format!("can't read {}: {e}", file.display())))?;
            (parse_markdown(&text).map_err(usage)?, false)
        }
        None => {
            let title = args
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| {
                    usage(format!(
                        "give the new ticket a title: {} create \"Title\"",
                        otaq_core::naming::COMMAND_NAME
                    ))
                })?;
            let body = body(args.description.clone(), args.stdin)?.unwrap_or_default();
            let ticket = BatchTicket {
                id: None,
                title,
                body,
                priority: args.priority,
                depends_on: args.deps.clone(),
                choices,
            };
            (vec![ticket], true)
        }
    };
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let (project, tickets) = resolve_refs(&client, &data, global, tickets).await?;
        let created = match client
            .request(Request::TicketCreateBatch(BatchParams {
                project,
                tickets,
                by_user: args.choices.by_user,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::Tickets(t) => t,
            other => return Err(unexpected(&other)),
        };
        let mut human = Vec::with_capacity(created.len());
        let mut values = Vec::with_capacity(created.len());
        for ticket in &created {
            let (line, value) = with_rows(&client, ticket).await?;
            human.push(format!("Created {line}"));
            values.push(value);
        }
        let keys: Vec<String> = created.iter().map(|t| t.key.to_string()).collect();
        if single {
            out.print(&human.join("\n"), &values[0], Some(&keys[0]));
        } else {
            out.print(&human.join("\n"), &values, Some(keys.join("\n").as_str()));
        }
        Ok(())
    })
}

/// A created or updated ticket as `create` and `update` print it: "SHOP-41 · Fix README typo"; and its `--json`
/// with the agent each of its stages runs and its stage choices.
async fn with_rows(
    client: &Client,
    ticket: &Ticket,
) -> Result<(String, serde_json::Value), ErrorBody> {
    let agents = read::ticket_agents(client, &ticket.key.to_string()).await?;
    let choices = read::ticket_choices(client, &ticket.key.to_string()).await?;
    let mut value = serde_json::to_value(ticket).map_err(|e| internal(e.to_string()))?;
    value["agents"] =
        serde_json::to_value(read::agents_json(&agents)).map_err(|e| internal(e.to_string()))?;
    value["choices"] =
        serde_json::to_value(read::choices_json(&choices)).map_err(|e| internal(e.to_string()))?;
    Ok((format!("{} · {}", ticket.key, ticket.title), value))
}

/// The project the batch goes to, and its references with bare numbers made full keys. Batch IDs stay.
async fn resolve_refs(
    client: &Client,
    data: &DataDir,
    global: &GlobalArgs,
    mut tickets: Vec<BatchTicket>,
) -> Result<(String, Vec<BatchTicket>), ErrorBody> {
    let ids: Vec<String> = tickets.iter().filter_map(|t| t.id.clone()).collect();
    let project = target::require_project(client, data, global).await?;
    let full = async |r: &String| -> Result<String, ErrorBody> {
        if ids.contains(r) || !r.trim().starts_with(|c: char| c.is_ascii_digit()) {
            return Ok(r.trim().to_owned());
        }
        Ok(target::resolve_key(client, data, Some(r.clone()), global)
            .await?
            .to_string())
    };
    for ticket in &mut tickets {
        let mut deps = Vec::new();
        for dep in &ticket.depends_on {
            deps.push(full(dep).await?);
        }
        ticket.depends_on = deps;
    }
    Ok((project.to_string(), tickets))
}

pub fn update(args: UpdateArgs, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    let choices = picks(&args.choices, false)?;
    let body = body(args.description, args.stdin)?;
    let update = TicketUpdate {
        title: args.title,
        body,
        priority: args.priority,
        choices,
        by_user: args.choices.by_user,
        ..Default::default()
    };
    let fields = TicketUpdate {
        by_user: false,
        ..update.clone()
    };
    if fields == TicketUpdate::default() {
        return Err(usage("nothing to change: give --title, -d, -p or --stdin"));
    }
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, args.key, global).await?;
        let ticket: Ticket = match client
            .request(Request::TicketUpdate(TicketUpdateParams {
                key: key.to_string(),
                update,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::Ticket(t) => *t,
            other => return Err(unexpected(&other)),
        };
        let (line, value) = with_rows(&client, &ticket).await?;
        out.print(
            &format!("Updated {line}"),
            &value,
            Some(&ticket.key.to_string()),
        );
        Ok(())
    })
}
