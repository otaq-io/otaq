// Otaq's OpenCode plugin (d0088): passes each event Otaq reads to `otaq hook --tool opencode <type>` with the
// event's JSON on stdin, one at a time and in order. All of the logic is in Otaq's OpenCode adapter. It does
// nothing in an ordinary OpenCode session (OTAQ_TICKET and OTAQ_ROLE both unset) or when `otaq` isn't found.
import { spawn } from "node:child_process";

// The events Otaq's adapter maps; OpenCode sends many more.
const FORWARDED = new Set([
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
]);

function send(event) {
  return new Promise((resolve) => {
    const child = spawn("otaq", ["hook", "--tool", "opencode", event.type], {
      stdio: ["pipe", "ignore", "ignore"],
    });
    child.on("error", resolve);
    child.on("close", resolve);
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify(event));
  });
}

export default {
  id: "otaq",
  // setup returns at once: OpenCode 2.2 waits for every plugin's setup before its server answers (until then its API
  // answers 503 and the TUI never sends the launch's --prompt), so the subscription runs on its own.
  async setup(ctx) {
    if (!process.env.OTAQ_TICKET && !process.env.OTAQ_ROLE) return;
    let queue = Promise.resolve();
    (async () => {
      for await (const event of ctx.event.subscribe()) {
        if (!FORWARDED.has(event?.type)) continue;
        queue = queue.then(() => send(event));
      }
    })();
  },
};
