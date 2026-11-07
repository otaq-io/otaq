# OpenCode plugin events

One file per event Otaq's OpenCode plugin (`plugin/opencode/index.js`) forwards to `otaq hook --tool opencode
<type>`, named `<type>[-<case>].json`; each is the whole event (`id`, `created`, `type`, `location`, `data`). The
OpenCode adapter's tests (`src/agent_tools/opencode/hooks.rs`) and `crates/otaq-cli/tests/suite/hook.rs` read them
(decision 0088, testing).

**Written from `reports/Hands-on check of Codex, Copilot and OpenCode (2026-10-17).md`, row 3 (payload shapes), and
the event schemas in the OpenCode 2.0.17 binary** (read with `strings`; `session.step.started` carries `model`
`{id, providerID, variant}`, `session.step.ended` doesn't). The two `session.execution.failed-*` files follow the
OpenCode 2.2.5 binary's `Session.StructuredError` (`type`, `message`, `status`, `response.body`, `retryAfterMs`) and
its error types; the quota's body is the Gemini API's 429 (F10). Not captured from a real session: T-0369 replaces
them with captures (the adapter logs every event raw at debug level, "plugin event").

| File | What it gives |
|---|---|
| `session.inbox.enqueued.json`, `session.execution.started.json` | a new turn → Working |
| `session.step.started.json` | the model that runs the step → *model known* `opencode/big-pickle` |
| `permission.asked.json` | a permission prompt → Needs you ("shell: git status --short") |
| `permission.asked-helper_agent.json` | a permission prompt in a helper agent's child session → Needs you (helper) |
| `form.created.json` | a question → Asks you ("Which color?") |
| `session.usage.updated.json` | running token totals → *tokens* (the cost is ignored) |
| `session.execution.succeeded.json`, `session.execution.interrupted.json` | the turn ended → Idle |
| `session.execution.failed.json` | a failed turn (`provider.auth`) → Idle with the tool's message (F10) |
| `session.execution.failed-quota.json` | a provider's quota (`provider.quota`, Gemini's daily free tier) → Waiting for limit reset, reset at `created` plus the body's `retryDelay` |
| `session.execution.failed-rate_limit.json` | a rate limit OpenCode stopped retrying, with `retryAfterMs` → Waiting for limit reset, reset at `created` plus it |
| `session.retry.scheduled-rate_limit.json` | a rate-limit retry → Waiting for limit reset, reset at `at` |
| `session.retry.scheduled-transport.json` | another retry → nothing |
| `session.step.ended.json` | a type Otaq doesn't map → nothing |
