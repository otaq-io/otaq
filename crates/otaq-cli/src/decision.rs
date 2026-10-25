//! `otaq decision record [KEY] --stdin` (DEC-02, d0084): an agent records a product choice it made instead of
//! stopping to ask. Stdin holds labelled lines (`Question:`, `Built:`, `Other option:` (repeatable), `Why:`; a
//! field may wrap onto following lines). It works on `$OTAQ_TICKET` unless a key is given; the service lets an
//! agent record on its own ticket only (CLI-02), and refuses a missing field with what's missing.
//!
//! # `--json` shape
//!
//! The recorded decision (`otaq_core::decisions::Decision`): `{"id": "SHOP-23#1", "question", "built",
//! "other_options": […], "why", "recorded_by", "recorded_at", "outcome": "open", …}`. `--silent` prints the ID.

use std::io::Read;

use otaq_core::data_dir::DataDir;
use otaq_core::decisions::{NewDecision, recorded_message};
use otaq_core::naming;
use otaq_core::protocol::{DecisionRecordParams, ErrorBody, ErrorKind, Reply, Request};

use crate::cli::{DecisionCommand, GlobalArgs};
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

pub fn run(command: DecisionCommand, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    match command {
        DecisionCommand::Record { key, stdin } => record(key, stdin, global, out),
    }
}

fn record(
    key: Option<String>,
    stdin: bool,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    if !stdin {
        return Err(ErrorBody::new(
            ErrorKind::Usage,
            format!(
                "give the decision on stdin: `{} decision record --stdin`",
                naming::COMMAND_NAME
            ),
        ));
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| internal(format!("can't read stdin: {e}")))?;
    let decision =
        NewDecision::parse(&text).map_err(|e| ErrorBody::new(ErrorKind::Usage, e.to_string()))?;
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, key, global).await?;
        let recorded = match client
            .request(Request::DecisionRecord(DecisionRecordParams {
                key: key.to_string(),
                decision,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::Decision(d) => *d,
            other => return Err(unexpected(&other)),
        };
        out.print(
            &recorded_message(&recorded),
            &recorded,
            Some(&recorded.id.to_string()),
        );
        Ok(())
    })
}
