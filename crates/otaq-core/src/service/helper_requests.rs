//! Helper requests (d0133, T-0537): the agent in charge's `otaq helper start|list|diff|back|merge` and a helper's
//! `otaq helper done` and `otaq prime`. [`crate::helpers::Helpers`] checks who asks: the policy let the ticket's own
//! agent and its helpers through ([`super::permissions`]), and the user passes the policy unchecked, so the handlers
//! refuse everyone but the agent in charge (or the helper) themselves.

use super::Shared;
use super::board_requests::error_body;
use crate::caller::Caller;
use crate::protocol::{ErrorBody, ErrorKind, Reply, Request};

pub(super) async fn handle(
    request: Request,
    caller: &Caller,
    shared: &Shared,
) -> Result<Reply, ErrorBody> {
    let helpers = &shared.helpers;
    let one = |r: Result<crate::helpers::HelperReply, crate::board::BoardError>| {
        r.map(|h| Reply::Helper(Box::new(h)))
            .map_err(|e| error_body(&e))
    };
    match request {
        Request::HelperStart(p) => one(helpers
            .start(caller, &p.key, p.piece, &p.title, p.note.as_deref())
            .await),
        Request::HelperList(p) => helpers
            .list(caller, &p.key)
            .await
            .map(Reply::Helpers)
            .map_err(|e| error_body(&e)),
        Request::HelperDiff(p) => helpers
            .diff(caller, &p.key, p.piece)
            .await
            .map(|d| Reply::HelperDiff(Box::new(d)))
            .map_err(|e| error_body(&e)),
        Request::HelperBack(p) => one(helpers.back(caller, &p.key, p.piece, &p.text).await),
        Request::HelperMerge(p) => one(helpers.merge(caller, &p.key, p.piece).await),
        Request::HelperDone(p) => {
            let done = helpers.done(caller, &p.text).await;
            // A hand-back is a helper's own: its turn's Stop is no Idle (T-0538 reports a helper's idle).
            if done.is_ok()
                && let Caller::Helper { session, .. } = caller
            {
                shared.signals.mark_submitted_or_asked(session);
            }
            one(done)
        }
        Request::HelperPrime(_) => helpers
            .prime(caller)
            .await
            .map(|p| Reply::HelperPrime(Box::new(p)))
            .map_err(|e| error_body(&e)),
        other => Err(ErrorBody::new(
            ErrorKind::BadRequest,
            format!("{} isn't a helper request", other.method()),
        )),
    }
}
