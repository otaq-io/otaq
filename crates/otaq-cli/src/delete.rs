//! `otaq delete <KEY> [--reason "…"]` (d0060, d0129): deletes a ticket that never started. The command is the
//! confirmation. The user deletes any never-started ticket; a ticket's agent only the never-started tickets it
//! created, and its reason goes into its own ticket's History ("Deleted SHOP-43 · merged into SHOP-42").
//!
//! `--json`: the `DeletePreview` of what went (`key`, `title`, `links`).

use otaq_core::data_dir::DataDir;
use otaq_core::protocol::{Reply, Request, TicketDeleteParams};

use crate::cli::GlobalArgs;
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

pub fn run(
    key: String,
    reason: Option<String>,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), otaq_core::protocol::ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, Some(key), global).await?;
        let preview = match client
            .request(Request::TicketDelete(TicketDeleteParams {
                key: key.to_string(),
                reason,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::DeletePreview(p) => p,
            other => return Err(unexpected(&other)),
        };
        let mut text = format!("Deleted {} {}", preview.key, preview.title);
        for line in preview.link_lines() {
            text.push_str(&format!("\n{line}"));
        }
        out.print(&text, &preview, Some(&preview.key.to_string()));
        Ok(())
    })
}
