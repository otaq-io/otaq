//! `apply_now.*`: the Apply now window on a waiting flow change (APN-01, APN-02, T-0439; `apply_now.rs` in `otaq-view`
//! says what it shows). `apply_now.view { project, picks }` reads the plan (`flow.apply_now_plan`) and the waiting
//! change's flow (`flow.waiting`, for the picker's icons) and builds the window with the user's picks; `null` once
//! nothing waits. `apply_now.apply { project, stamp, picks, basis }` sends `flow.apply_now`: applied, applied on its own
//! meanwhile or nothing waiting closes the window (the toast comes with the `flow_changed` event); a stale stamp or a
//! pick that isn't a choice redraws it from the fresh plan with a line naming what changed.

use otaq_core::flow::apply_now::{ApplyNowAnswer, ApplyNowParams, ApplyNowPlan, Pick};
use otaq_core::flow::{Flow, yaml};
use otaq_core::protocol::{ProjectParams, Reply, Request};
use otaq_view::view_models::apply_now::{self, Basis, Picks, View};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "apply_now.view" => {
            let p: ViewParams = parse(params)?;
            let view = match plan(app, &p.project).await? {
                Some(plan) => {
                    // The line a refresh wrote stays while the window shows that same plan.
                    let changed = p.changed.filter(|c| c.stamp == plan.stamp).map(|c| c.line);
                    let flow = flow(app, &p.project).await?;
                    Some(apply_now::view(&plan, flow.as_ref(), &p.picks, changed))
                }
                None => None,
            };
            reply(&Load { view })
        }
        "apply_now.apply" => reply(&apply(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

/// Exports the window's types (`generated.rs`' one line for it).
#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    ViewParams::export_all(cfg)?;
    Load::export_all(cfg)?;
    ApplyRequest::export_all(cfg)?;
    Answer::export_all(cfg)?;
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(format!("bad parameters: {e}")))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected reply to {what}: {other:?}"))
}

/// `{"project": "SHOP", "picks": {"SHOP-19": "stage:dev"}, "changed": {…}}`: the window with the user's picks so far.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ApplyNowParams")]
pub struct ViewParams {
    pub project: String,
    #[serde(default)]
    pub picks: Picks,
    /// The line the last refresh wrote, with the stamp of the plan it was written for.
    #[serde(default)]
    #[ts(optional)]
    pub changed: Option<ChangedLine>,
}

/// "The board changed: SHOP-41 entered Product spec." for the plan `stamp`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ApplyNowChanged")]
pub struct ChangedLine {
    pub stamp: String,
    pub line: String,
}

/// What `apply_now.view` answers: the window, or none once no change waits (the page closes it).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ApplyNowLoad")]
pub struct Load {
    pub view: Option<View>,
}

/// Apply now: the plan's stamp, the picks and what the window showed.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ApplyNowApplyParams")]
pub struct ApplyRequest {
    pub project: String,
    pub stamp: String,
    #[serde(default)]
    pub picks: Picks,
    #[serde(default)]
    pub basis: Basis,
}

/// What Apply now did, for the page: close the window, or redraw it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "ApplyNowAnswer")]
pub enum Answer {
    /// Applied, applied on its own meanwhile, or nothing waits any more.
    Closed,
    /// The board changed under the window (or a pick isn't a choice): the fresh window, with what changed.
    Refreshed { view: Box<View> },
}

async fn plan(app: &App, project: &str) -> Result<Option<ApplyNowPlan>, RequestError> {
    let request = Request::FlowApplyNowPlan(ProjectParams {
        project: project.to_owned(),
    });
    match app.service.send(request).await? {
        Reply::ApplyNowPlan(plan) => Ok(plan.map(|p| *p)),
        other => Err(unexpected("flow.apply_now_plan", &other)),
    }
}

/// The flow the waiting change applies, for the picker's icons; none when it can't be read.
async fn flow(app: &App, project: &str) -> Result<Option<Flow>, RequestError> {
    let request = Request::FlowWaiting(ProjectParams {
        project: project.to_owned(),
    });
    match app.service.send(request).await? {
        Reply::FlowWaiting(waiting) => {
            Ok(waiting.and_then(|w| yaml::read(&w.yaml).ok()).map(|f| f.flow))
        }
        other => Err(unexpected("flow.waiting", &other)),
    }
}

async fn apply(app: &App, p: ApplyRequest) -> Result<Answer, RequestError> {
    let mut picks = Vec::with_capacity(p.picks.len());
    for (ticket, id) in &p.picks {
        let ticket = ticket
            .parse()
            .map_err(|_| RequestError::plain(format!("not a ticket: {ticket}")))?;
        let to = apply_now::choice_of(id)
            .ok_or_else(|| RequestError::plain(format!("not a choice: {id}")))?;
        picks.push(Pick { ticket, to });
    }
    let request = Request::FlowApplyNow(ApplyNowParams {
        project: p.project.clone(),
        stamp: p.stamp,
        picks,
    });
    let answer = match app.service.send(request).await? {
        Reply::AppliedNow(answer) => *answer,
        other => return Err(unexpected("flow.apply_now", &other)),
    };
    Ok(match answer {
        ApplyNowAnswer::Applied { .. }
        | ApplyNowAnswer::AlreadyApplied { .. }
        | ApplyNowAnswer::NothingWaits => Answer::Closed,
        ApplyNowAnswer::Refreshed { plan, why } => {
            let changed = apply_now::changed_line(&p.basis, &plan, &why);
            let flow = flow(app, &p.project).await?;
            Answer::Refreshed {
                view: Box::new(apply_now::view(&plan, flow.as_ref(), &p.picks, Some(changed))),
            }
        }
    })
}

#[cfg(test)]
mod tests;
