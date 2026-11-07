//! Otaq's OpenCode plugin's events (ADP-09, ADP-03; T-0354). The plugin (`plugin/opencode/index.js`) runs inside
//! the agent's own OpenCode server and passes each event it forwards, whole, to `otaq hook --tool opencode
//! <type>`: `{"id":"evt_…","created":…,"type":…,"location":{"directory"},"data":{"sessionID",…}}`. The shapes come
//! from the hands-on check of OpenCode 2.0.17 (row 3) and the event schemas in its binary.
//!
//! - `permission.asked` → *needs you* ("shell: git status --short"); `form.created` → *asks you* (its first
//!   field's description). Both also from a helper agent's own session (OpenCode's subagents run in child
//!   sessions of the same server), marked as a helper's.
//! - The agent's own session only: `session.execution.succeeded` / `.interrupted` → *turn ended*; `.failed` →
//!   *failed* with the error's `type` and `message` (OpenCode runs another model instead of refusing one, so no
//!   *model refused*), except a usage limit's error type ([`LIMIT_ERRORS`]) → *limit reached* (F10), reset at the
//!   event's time plus the error's `retryAfterMs`, else plus the `retryDelay` in the provider's response body (a
//!   Gemini quota), else from the message ("Please retry in 8h25m", [`super::usage::limit_reset_from_text`]);
//!   `session.retry.scheduled` on `provider.rate-limit` → *limit reached*, reset at the retry (OpenCode keeps
//!   retrying by itself); `session.inbox.enqueued` / `session.execution.started` → *new turn*;
//!   `session.usage.updated` → *tokens* (running totals; the cost is ignored, d0086); `session.step.started` →
//!   *model known* (`provider/id`, the model that runs each step).
//! - Any other type gives nothing: OpenCode sends many.
//!
//! Events are de-duplicated by their `id` (the last [`SEEN_PER_SESSION`] per session): OpenCode may run a plugin's
//! `setup` more than once in one server, and each copy forwards the same events. Every event is logged raw at debug
//! level (ADP-11). OpenCode's signals all come through the plugin: no watched file.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use crate::agent_tools::{SignalContext, Tool, ToolEvent};
use crate::usage::Tokens;
use crate::usage::limit::{LimitHit, LimitKind};

/// How many event ids are kept per session for de-duplication.
pub(super) const SEEN_PER_SESSION: usize = 500;

/// Every event type [`map_hook`] maps: the ones Otaq's plugin forwards (its `FORWARDED` list).
pub(super) const MAPPED: &[&str] = &[
    "permission.asked",
    "form.created",
    "session.inbox.enqueued",
    "session.execution.started",
    "session.execution.succeeded",
    "session.execution.failed",
    "session.execution.interrupted",
    "session.retry.scheduled",
    "session.usage.updated",
    "session.step.started",
];

/// The error type of a provider's rate limit.
const RATE_LIMIT: &str = "provider.rate-limit";

/// The error types of a failed execution that are a usage limit (F10), as OpenCode 2.2.5 names its provider errors
/// (`Session.StructuredError` `type`, read from its binary): a rate limit it stopped retrying, a provider's quota
/// (`QuotaExceeded`: Gemini's daily free-tier limit), ChatGPT's subscription limit, and OpenCode's own Free, Go and
/// workspace limits. A model's spend limit (`provider.model-budget-limit`, "Switch models to keep going") and every
/// other error are a failed turn.
pub(super) const LIMIT_ERRORS: &[&str] = &[
    RATE_LIMIT,
    "provider.quota",
    "provider.chatgpt-limit",
    "provider.free-limit",
    "provider.go-limit",
    "provider.budget-limit",
];

/// The event ids already handled, per OpenCode session.
#[derive(Debug, Default)]
pub struct Seen(Mutex<HashMap<String, VecDeque<String>>>);

impl Seen {
    /// Records `id` for `session`; `false` when it was already there.
    fn first_time(&self, session: &str, id: &str) -> bool {
        let mut seen = self.0.lock().expect("seen events");
        let ids = seen.entry(session.to_owned()).or_default();
        if ids.iter().any(|seen| seen == id) {
            return false;
        }
        if ids.len() == SEEN_PER_SESSION {
            ids.pop_front();
        }
        ids.push_back(id.to_owned());
        true
    }
}

pub(super) fn map_hook(
    seen: &Seen,
    event: &str,
    payload: &Value,
    context: SignalContext<'_>,
) -> Vec<ToolEvent> {
    tracing::debug!(tool = %Tool::OpenCode, event, payload = %payload, "plugin event");
    if !MAPPED.contains(&event) {
        return Vec::new();
    }
    let data = &payload["data"];
    let session = data["sessionID"].as_str().unwrap_or_default();
    if let Some(id) = payload["id"].as_str()
        && !seen.first_time(session, id)
    {
        return Vec::new();
    }
    // A helper agent's child session, or the agent's own (or one Otaq can't tell).
    let own = session.is_empty() || context.tool_id.is_none_or(|own| own == session);
    let text = |value: &Value| value.as_str().map(str::to_owned);
    let event = match event {
        "permission.asked" => ToolEvent::NeedsYou {
            prompt: permission_prompt(data),
            helper_agent: !own,
        },
        "form.created" => ToolEvent::AsksYou {
            prompt: form_prompt(&data["form"]),
        },
        _ if !own => return Vec::new(),
        "session.execution.succeeded" | "session.execution.interrupted" => ToolEvent::TurnEnded {
            background: Default::default(),
            last_message: None,
        },
        "session.execution.failed" => {
            let error = &data["error"];
            let kind = text(&error["type"]).unwrap_or_else(|| "unknown".to_owned());
            let message = text(&error["message"]).filter(|m| !m.trim().is_empty());
            if LIMIT_ERRORS.contains(&kind.as_str()) {
                ToolEvent::LimitReached {
                    resets_at: retry_at(payload["created"].as_i64(), error),
                    hit: LimitHit {
                        kind: LimitKind::Other,
                        text: message.unwrap_or(kind),
                    },
                }
            } else {
                ToolEvent::Failed {
                    error: kind,
                    message,
                }
            }
        }
        "session.retry.scheduled" if data["error"]["type"] == RATE_LIMIT => {
            ToolEvent::LimitReached {
                hit: LimitHit {
                    kind: LimitKind::Other,
                    text: text(&data["error"]["message"]).unwrap_or_else(|| RATE_LIMIT.to_owned()),
                },
                resets_at: data["at"].as_i64(),
            }
        }
        "session.inbox.enqueued" => ToolEvent::NewTurn {
            prompt: text(&data["item"]["payload"]["text"]),
        },
        "session.execution.started" => ToolEvent::NewTurn { prompt: None },
        "session.usage.updated" => match tokens(&data["tokens"]) {
            Some(totals) => ToolEvent::Tokens { totals },
            None => return Vec::new(),
        },
        "session.step.started" => match model(&data["model"]) {
            Some(model) => ToolEvent::ModelKnown { model },
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    vec![event]
}

pub(super) fn watched_file(_tool_id: &str, _cwd: &Path) -> Option<PathBuf> {
    None
}

pub(super) fn map_watched_lines(_lines: &str, _context: SignalContext<'_>) -> Vec<ToolEvent> {
    Vec::new()
}

/// When a limit's failed execution may be retried: the event's time (`created`, Unix milliseconds) plus the error's
/// `retryAfterMs` (a `Retry-After` OpenCode read), else plus the `retryDelay` ("30285s", Google's `RetryInfo`) in the
/// provider's response body.
fn retry_at(created: Option<i64>, error: &Value) -> Option<i64> {
    let created = created?;
    let after = error["retryAfterMs"].as_i64().or_else(|| {
        let body: Value = serde_json::from_str(error["response"]["body"].as_str()?).ok()?;
        retry_delay_ms(&body)
    })?;
    (after > 0).then_some(created + after)
}

/// The first `retryDelay` ("30285s", "41.7s") anywhere in a response body, in milliseconds.
fn retry_delay_ms(body: &Value) -> Option<i64> {
    match body {
        Value::Object(map) => map
            .get("retryDelay")
            .and_then(Value::as_str)
            .and_then(|d| d.trim().strip_suffix('s')?.parse::<f64>().ok())
            .filter(|secs| secs.is_finite() && *secs > 0.0)
            .map(|secs| (secs * 1000.0).round() as i64)
            .or_else(|| map.values().find_map(retry_delay_ms)),
        Value::Array(items) => items.iter().find_map(retry_delay_ms),
        _ => None,
    }
}

/// "shell: git status --short": the action and what it acts on.
fn permission_prompt(data: &Value) -> String {
    let action = data["action"].as_str().unwrap_or("permission");
    let resources: Vec<&str> = data["resources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    match resources.as_slice() {
        [] => action.to_owned(),
        resources => format!("{action}: {}", resources.join(", ")),
    }
}

/// The question: the first field's description, else the form's title.
fn form_prompt(form: &Value) -> String {
    [&form["fields"][0]["description"], &form["title"]]
        .into_iter()
        .find_map(|v| v.as_str().map(str::trim).filter(|t| !t.is_empty()))
        .unwrap_or("A question")
        .to_owned()
}

/// `{input, output, reasoning, cache: {read, write}}`: cache writes count as input, cache reads as cached.
fn tokens(tokens: &Value) -> Option<Tokens> {
    let n = |v: &Value| v.as_u64();
    Some(Tokens {
        input: n(&tokens["input"])? + n(&tokens["cache"]["write"]).unwrap_or(0),
        cached: n(&tokens["cache"]["read"]).unwrap_or(0),
        output: n(&tokens["output"])?,
        reasoning: n(&tokens["reasoning"]),
    })
}

/// `{"providerID": "opencode", "id": "big-pickle"}` → `opencode/big-pickle`, as OpenCode's list names it.
fn model(model: &Value) -> Option<String> {
    let provider = model["providerID"].as_str().filter(|p| !p.is_empty())?;
    let id = model["id"].as_str().filter(|i| !i.is_empty())?;
    Some(format!("{provider}/{id}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::signals::background::BackgroundWork;

    const OWN: &str = "ses_eb4534bbfffee5hwyyyzWkBMXI";

    /// A saved event (`tests/fixtures/opencode/events/`).
    fn fixture(name: &str) -> Value {
        let path = format!(
            "{}/tests/fixtures/opencode/events/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn map(seen: &Seen, name: &str) -> Vec<ToolEvent> {
        let event = fixture(name);
        let kind = event["type"].as_str().unwrap().to_owned();
        let context = SignalContext {
            tool_id: Some(OWN),
            cwd: None,
        };
        map_hook(seen, &kind, &event, context)
    }

    #[test]
    fn every_mapped_event() {
        let seen = Seen::default();
        assert_eq!(
            map(&seen, "session.inbox.enqueued"),
            [ToolEvent::NewTurn {
                prompt: Some("work-ticket".into())
            }]
        );
        assert_eq!(
            map(&seen, "session.execution.started"),
            [ToolEvent::NewTurn { prompt: None }]
        );
        assert_eq!(
            map(&seen, "session.step.started"),
            [ToolEvent::ModelKnown {
                model: "opencode/big-pickle".into()
            }]
        );
        assert_eq!(
            map(&seen, "permission.asked"),
            [ToolEvent::NeedsYou {
                prompt: "shell: git status --short".into(),
                helper_agent: false
            }]
        );
        assert_eq!(
            map(&seen, "permission.asked-helper_agent"),
            [ToolEvent::NeedsYou {
                prompt: "edit: src/totals.ts".into(),
                helper_agent: true
            }]
        );
        assert_eq!(
            map(&seen, "form.created"),
            [ToolEvent::AsksYou {
                prompt: "Which color?".into()
            }]
        );
        // Cache writes count as input, cache reads as cached; the cost is ignored (d0086).
        assert_eq!(
            map(&seen, "session.usage.updated"),
            [ToolEvent::Tokens {
                totals: Tokens {
                    input: 1350,
                    cached: 8000,
                    output: 340,
                    reasoning: Some(56),
                }
            }]
        );
        let turn_ended = ToolEvent::TurnEnded {
            background: BackgroundWork::default(),
            last_message: None,
        };
        assert_eq!(
            map(&seen, "session.execution.succeeded"),
            std::slice::from_ref(&turn_ended)
        );
        assert_eq!(map(&seen, "session.execution.interrupted"), [turn_ended]);
        assert_eq!(
            map(&seen, "session.execution.failed"),
            [ToolEvent::Failed {
                error: "provider.auth".into(),
                message: Some("Invalid API key".into())
            }]
        );
        assert_eq!(
            map(&seen, "session.retry.scheduled-rate_limit"),
            [ToolEvent::LimitReached {
                hit: LimitHit {
                    kind: LimitKind::Other,
                    text: "Rate limit exceeded".into()
                },
                resets_at: Some(1_792_270_980_000 + 10_000 + 882_000)
            }]
        );
        // A failed execution on a limit (F10): the quota's reset from the body's `retryDelay`, the rate limit's
        // from `retryAfterMs`, each after the event's time.
        let events = map(&seen, "session.execution.failed-quota");
        let [ToolEvent::LimitReached { hit, resets_at }] = events.as_slice() else {
            panic!("not a limit: {events:?}")
        };
        assert_eq!(hit.kind, LimitKind::Other);
        assert!(
            hit.text.starts_with("You exceeded your current quota")
                && hit.text.ends_with("Please retry in 30285.312s."),
            "{}",
            hit.text
        );
        assert_eq!(*resets_at, Some(1_792_270_992_000 + 30_285_000));
        assert_eq!(
            map(&seen, "session.execution.failed-rate_limit"),
            [ToolEvent::LimitReached {
                hit: LimitHit {
                    kind: LimitKind::Other,
                    text: "Rate limit exceeded".into()
                },
                resets_at: Some(1_792_270_994_000 + 120_000)
            }]
        );
        assert_eq!(map(&seen, "session.retry.scheduled-transport"), []);
        assert_eq!(map(&seen, "session.step.ended"), []);
    }

    /// The same event twice (a plugin set up twice in one server) is handled once; an old id falls out after
    /// [`SEEN_PER_SESSION`] newer ones.
    #[test]
    fn duplicate_ids_are_ignored() {
        let seen = Seen::default();
        assert_eq!(map(&seen, "permission.asked").len(), 1);
        assert_eq!(map(&seen, "permission.asked"), []);
        for n in 0..SEEN_PER_SESSION {
            let kind = "session.execution.started";
            let event = json!({"id": format!("evt_{n}"), "type": kind, "data": {"sessionID": OWN}});
            assert_eq!(
                map_hook(&seen, kind, &event, SignalContext::default()).len(),
                1
            );
        }
        assert_eq!(map(&seen, "permission.asked").len(), 1);
    }

    /// The plugin (d0088): plain JavaScript Node can parse, no dependency, nothing but forwarding the types the
    /// adapter maps to `otaq hook --tool opencode <type>`.
    #[test]
    fn the_plugin_forwards_exactly_the_mapped_events() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/opencode");
        let package: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).unwrap())
                .unwrap();
        assert_eq!(package["type"], "module");
        assert_eq!(package["main"], "index.js");
        assert!(package.get("dependencies").is_none());

        let index = dir.join("index.js");
        let node =
            crate::tools::find_on_path("node", &std::env::var_os("PATH").unwrap_or_default())
                .expect("node on PATH (the build needs it)");
        let checked = std::process::Command::new(node)
            .arg("--check")
            .arg(&index)
            .output()
            .unwrap();
        assert!(checked.status.success(), "{checked:?}");

        let text = std::fs::read_to_string(&index).unwrap();
        let imports: Vec<&str> = text.lines().filter(|l| l.starts_with("import ")).collect();
        assert_eq!(imports, [r#"import { spawn } from "node:child_process";"#]);
        let command = format!(
            r#"spawn("{}", ["hook", "--tool", "opencode", event.type]"#,
            crate::naming::COMMAND_NAME
        );
        assert!(text.contains(&command), "{text}");
        let list = &text[text.find("new Set([").unwrap()..];
        let list = &list[..list.find("]);").unwrap()];
        let forwarded: Vec<&str> = list
            .lines()
            .filter_map(|l| l.trim().strip_prefix('"')?.strip_suffix("\","))
            .collect();
        assert_eq!(forwarded, MAPPED);
    }

    /// A helper agent's child session gives only its prompts: its turns and tokens aren't the agent's.
    #[test]
    fn another_sessions_turns_are_not_the_agents() {
        let seen = Seen::default();
        let context = SignalContext {
            tool_id: Some("ses_other"),
            cwd: None,
        };
        for name in [
            "session.execution.succeeded",
            "session.usage.updated",
            "session.step.started",
        ] {
            let event = fixture(name);
            assert_eq!(map_hook(&seen, name, &event, context), [], "{name}");
        }
        let form = fixture("form.created");
        assert_eq!(map_hook(&seen, "form.created", &form, context).len(), 1);
    }
}
