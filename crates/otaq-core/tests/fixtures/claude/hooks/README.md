# Claude Code hook payloads

One file per row of SDD §11.4, named `<event>-<case>.json` (`hook_event_name` is the event). The Claude Code
adapter's tests (`src/agent_tools/claude/hooks.rs`) and `otaq hook --tool claude <event>`
(`crates/otaq-cli/tests/suite/hook.rs`) read them (decision 0088, testing).

**Shaped from the hooks reference, 2026-10-10** (code.claude.com/docs/en/hooks; Claude Code 2.1.296), not captured
from a real session yet. The captured payloads replace these files as a small follow-up; record the Claude Code
version here when they do. The limit and failure payloads (`StopFailure-*`) can't be triggered at will and stay
real-shaped: their texts follow the error reference and the messages Claude Code shows.

| File | Row of §11.4 |
|---|---|
| `Notification-permission_prompt.json` | a permission prompt → Needs you |
| `Notification-elicitation_dialog.json` | an elicitation → Needs you |
| `Notification-helper_agent.json` | a permission prompt inside a helper agent → Needs you (bot icon) |
| `Notification-idle_prompt.json`, `Notification-auth_success.json` | other types → ignored |
| `Stop-idle.json` | nothing pending → Idle, or "turn ended" after a submit or an ask |
| `Stop-background.json` | a background shell and a helper agent (and a finished task, left out) → Working in background |
| `Stop-cron.json` | a scheduled wakeup → Working in background |
| `StopFailure-session_limit.json`, `StopFailure-weekly_limit.json` | the usage limit → Waiting for limit reset |
| `StopFailure-capacity.json` | a 429 that isn't the plan's limit → Idle with the tool's message (F10) |
| `StopFailure-other.json` | another failure → Idle with the tool's message (F10) |
| `UserPromptSubmit.json` | a new turn → Working |
| `SessionStart-startup.json` | the model that ran → *model known* (M22, AGT-04) |
| `StopFailure-model_not_found.json` | a model Claude Code can't use → *model refused* (M22, AGT-05) |

The two M22 files were written 2026-10-17 from the SDK schema in the installed Claude Code 2.1.302 binary (read
with `strings`, nothing run): `SessionStart` has `source` and `model`; `StopFailure`'s `error` kinds include
`model_not_found`, whose rendered message is "There's an issue with the selected model (…). It may not exist or you
may not have access to it." plus a `/model` hint. `error_details` is a guess. The close-out (T-0346) swaps in a
`SessionStart` and a model-error `StopFailure` captured from the debug log.
