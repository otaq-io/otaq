//! `tools.*`: a checklist row's Install / Add extension / Sign in… (git, gh, az; onboarding and Settings → Tools).
//! The page sends the row's tool and action; the service picks the command and runs it in a terminal of no
//! project, which the page attaches to under the row. The page checks the tools again when that terminal ends.

use otaq_core::protocol::{GitIdentityParams, Reply, Request, ToolTerminalParams};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "tools.terminal" => reply(&terminal(app, params).await?),
        "tools.git_identity" => reply(&git_identity(app, params).await?),
        _ => Err(unknown(method)),
    }
}

/// **Save** on the name-and-email form (the Tools step, Settings → Git & pull requests, the merge dialog): the
/// service writes them into the user's own git settings. A refusal says what to fix, in plain words.
async fn git_identity(app: &App, params: Value) -> Result<(), RequestError> {
    let params: GitIdentityParams =
        serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))?;
    match app.service.send(Request::GitSetIdentity(params)).await? {
        Reply::Ok => Ok(()),
        other => Err(RequestError::plain(format!(
            "unexpected answer to tools.git_identity: {other:?}"
        ))),
    }
}

/// The terminal running the row's command: attach to `session`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolTerminalOpened")]
pub struct Opened {
    pub session: String,
}

async fn terminal(app: &App, params: Value) -> Result<Opened, RequestError> {
    let params: ToolTerminalParams =
        serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))?;
    match app.service.send(Request::ToolTerminalOpen(params)).await? {
        Reply::Session(session) => Ok(Opened { session: session.id }),
        other => Err(RequestError::plain(format!(
            "unexpected answer to tools.terminal: {other:?}"
        ))),
    }
}
