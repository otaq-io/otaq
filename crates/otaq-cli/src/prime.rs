//! `otaq prime` (PLG-02; d0032, d0040): the current ticket's context for its agent, as Markdown. Every skill
//! runs it first. `--validate` prints the validator's context instead (d0063); `--project` a Project agent's (d0068);
//! `--epic` an epic lead's (LEAD-03). `--json` prints the context itself (`otaq_core::prime::PrimeContext`).

use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_core::prime::{render, render_validate};
use otaq_core::protocol::{ErrorBody, ErrorKind, KeyParams, Reply, Request};

use crate::cli::GlobalArgs;
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

pub fn run(
    key: Option<String>,
    validate: bool,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, key, global).await?;
        // `--validate`: what the ticket's validator works from (VAL-10, d0063).
        if validate {
            let context = match client
                .request(Request::TicketPrimeValidate(KeyParams {
                    key: key.to_string(),
                }))
                .await
                .map_err(client_error)?
            {
                Reply::PrimeValidate(context) => *context,
                other => return Err(unexpected(&other)),
            };
            out.print(
                render_validate(&context).trim_end(),
                &context,
                Some(&context.ticket.key.to_string()),
            );
            return Ok(());
        }
        let context = match client
            .request(Request::TicketPrime(KeyParams {
                key: key.to_string(),
            }))
            .await
            .map_err(client_error)?
        {
            Reply::Prime(context) => *context,
            other => return Err(unexpected(&other)),
        };
        out.print(
            render(&context).trim_end(),
            &context,
            Some(&context.ticket.key.to_string()),
        );
        Ok(())
    })
}

/// `otaq prime --project` (PAG-03): the current project's tickets, worktrees and Noyan’s instructions.
pub fn project(global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let project = target::require_project(&client, &data, global).await?;
        let context = match client
            .request(Request::ProjectAgentPrime(
                otaq_core::protocol::ProjectParams {
                    project: project.to_string(),
                },
            ))
            .await
            .map_err(client_error)?
        {
            Reply::ProjectPrime(context) => *context,
            other => return Err(unexpected(&other)),
        };
        out.print(
            otaq_core::project_agents::prime::render(&context).trim_end(),
            &context,
            Some(&context.project),
        );
        Ok(())
    })
}

/// `otaq prime --epic [KEY]` (LEAD-03): an epic lead's context. A bare `--epic` reads `$OTAQ_EPIC`, which the lead's
/// session sets.
pub fn epic(key: String, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    let env = naming::epic_env_var();
    let key = Some(key)
        .filter(|k| !k.trim().is_empty())
        .or_else(|| std::env::var(&env).ok().filter(|v| !v.trim().is_empty()))
        .ok_or_else(|| {
            ErrorBody::new(
                ErrorKind::Usage,
                format!("name an epic (--epic SHOP-40), or run it where {env} is set"),
            )
        })?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, Some(key), global).await?;
        let context = match client
            .request(Request::LeadPrime(KeyParams {
                key: key.to_string(),
            }))
            .await
            .map_err(client_error)?
        {
            Reply::EpicPrime(context) => *context,
            other => return Err(unexpected(&other)),
        };
        out.print(
            otaq_core::epic_lead::prime::render(&context).trim_end(),
            &context,
            Some(&context.epic.key.to_string()),
        );
        Ok(())
    })
}
