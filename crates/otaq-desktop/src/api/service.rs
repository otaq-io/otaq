//! `service.*`: the link to the background service, for the connection line and the service banners (T-0231).

use otaq_core::protocol::{Reply, Request};
use serde_json::Value;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "service.connection" => connection(app, params).await,
        "service.status" => status(app, params).await,
        "service.restart" => restart(app, params).await,
        "service.restore_database" => restore_database(app, params).await,
        "service.download_release" => download_release(app, params).await,
        _ => Err(unknown(method)),
    }
}

/// The link's state now; changes arrive as `connection` events.
pub async fn connection(app: &App, _params: Value) -> Result<Value, RequestError> {
    reply(&app.service.connection())
}

/// The service's `service.status` (PID, build, agent counts).
pub async fn status(app: &App, _params: Value) -> Result<Value, RequestError> {
    match app.service.send(Request::ServiceStatus).await? {
        Reply::ServiceStatus(status) => reply(&status),
        other => Err(RequestError::plain(format!(
            "unexpected answer to service.status: {other:?}"
        ))),
    }
}

/// Restarts the service with this build, or starts it. Only sent on the user's click (d0022).
pub async fn restart(app: &App, _params: Value) -> Result<Value, RequestError> {
    app.service.restart().await.map_err(RequestError::plain)?;
    Ok(Value::Null)
}

/// **Restore the copy from before 0.2.0**, after its confirmation (UPD-08). The screen follows the steps through the
/// `connection` events; the answer is how it ended.
pub async fn restore_database(app: &App, _params: Value) -> Result<Value, RequestError> {
    app.service
        .restore_database()
        .await
        .map_err(RequestError::plain)?;
    Ok(Value::Null)
}

/// **Download 0.2.0**: the release page of the version that changed the board, in the browser.
pub async fn download_release(app: &App, _params: Value) -> Result<Value, RequestError> {
    let Some(version) = app.service.newer_database_version() else {
        return Err(RequestError::plain("the board doesn't need another version"));
    };
    open::that_detached(otaq_core::naming::release_page(&version))
        .map_err(|e| RequestError::plain(format!("couldn't open the release page: {e}")))?;
    Ok(Value::Null)
}
