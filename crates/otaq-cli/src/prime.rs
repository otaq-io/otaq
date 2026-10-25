//! `otaq prime` (PLG-02; d0032, d0040): the current ticket's context for its agent, as Markdown. Every skill
//! runs it first; a helper (`$OTAQ_PIECE` set) gets its piece's context (d0133). `--validate` prints the validator's context instead (d0063); `--project` a Project agent's (d0068).
//! `--json` prints the context itself (`otaq_core::prime::PrimeContext`).

use otaq_core::data_dir::DataDir;
use otaq_core::prime::{render, render_validate};
use otaq_core::protocol::{ErrorBody, KeyParams, Reply, Request};

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
        // A helper (`$OTAQ_PIECE` set, d0133): its piece first.
        if !validate && std::env::var_os(otaq_core::helpers::piece_env_var()).is_some() {
            let context = match client
                .request(Request::HelperPrime(KeyParams {
                    key: key.to_string(),
                }))
                .await
                .map_err(client_error)?
            {
                Reply::HelperPrime(context) => *context,
                other => return Err(unexpected(&other)),
            };
            out.print(
                otaq_core::helpers::prime::render(&context).trim_end(),
                &context,
                Some(&context.helper.label()),
            );
            return Ok(());
        }
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
