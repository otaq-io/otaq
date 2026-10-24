import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { overlays } from "../../app/overlays";
import { createRegistries, registered } from "../../app/registry";
import type { ProcessesView } from "../../generated/ProcessesView";
import type { RunButtonView } from "../../generated/RunButtonView";
import type { RunCommandView } from "../../generated/RunCommandView";
import type { RunTabView } from "../../generated/RunTabView";
import type { ShellTabView } from "../../generated/ShellTabView";
import { INITIAL_STATE } from "../../terminal/state";
import { session } from ".";
import { afterStart, pressOf, suggestionFor } from "./logic";
import { ProcessesBody } from "./Processes";
import { RunCommandBody } from "./RunCommand";
import { RunTabBody } from "./RunTab";
import { ShellTabBody } from "./ShellTab";

afterEach(() => {
  overlays.store.set([]);
});

const button = (kind: RunButtonView["kind"], extra: Partial<RunButtonView> = {}): RunButtonView => ({
  kind,
  label: kind,
  icon: "play",
  primary: true,
  copy: null,
  ...extra,
});

/** `run_page::tab_view` while the Run serves. */
const RUNNING: RunTabView = {
  key: "SHOP-22",
  project: "SHOP",
  line: "pnpm dev · in SHOP-22’s worktree · :4022",
  state: { text: "Serving", tone: "success", icon: "dot" },
  message: null,
  open_url: { label: "Open http://localhost:4022", url: "http://localhost:4022" },
  buttons: [button("stop", { label: "Stop", primary: false, icon: "square" })],
  note: null,
  menu: [
    {
      id: "edit_command",
      label: "Edit command",
      icon: "pencil",
      shortcut: null,
      danger: false,
      disabled: false,
      hint: null,
      separated: false,
      checked: false,
      header: false,
    },
  ],
  menu_label: "Run options",
  session: null,
  idle: "(not running)",
};

const COMMAND: RunCommandView = {
  project: "SHOP",
  title: "What command runs shop-web?",
  intro: "Runs in the ticket’s worktree when you press Run.",
  command_placeholder: "e.g. pnpm dev",
  url_label: "Open URL",
  optional: "optional",
  url_placeholder: "e.g. http://localhost:$PORT",
  url_hint: "Leave empty for backends, CLIs and libraries — then there’s no Open button.",
  ask: {
    label: "Ask the agent",
    tooltip: "Asks Strong (the Project agent’s) to suggest a run command",
    note: "The agent looks at the repo and suggests a command.",
  },
  thinking: null,
  why: null,
  saved: null,
  error: null,
  save: "Save & start",
  can_save: false,
  cancel: "Cancel",
  done: "Done",
  fill: null,
};

const SHELL: ShellTabView = {
  key: "BILL-4",
  label: "Shell · not the agent",
  path: "~/.otaq/wt/billing-api/BILL-4",
  port: "Run port · $PORT=4004",
  session: null,
  live: false,
  problem: null,
  stopped: { title: "Stopped by restart · was open at 08:24", note: "A restart ended this shell." },
  reopen: "Reopen",
  hint: "Typing goes to the shell, not the agent",
  leave: { text: "leave terminal", keys: "⌃`" },
  terminal_label: "BILL-4 · shell",
};

const noop = () => undefined;

describe("the Run & Shell screen", () => {
  it("Session holds Run and Shell; its screen registers the run-command dialog and the processes menu", () => {
    const app = createRegistries();
    session(app);
    const r = registered(app);
    expect(r.sheetTabs).toEqual(["session"]);
    expect(r.overlays).toEqual(["processes", "run_command"]);
  });

  it("Session's Run is one line: what runs, its state, Open URL, Stop and ⋯; no state card (SHT-04, Q433)", () => {
    const html = renderToStaticMarkup(
      createElement(RunTabBody, {
        ticket: "SHOP-22",
        head: createElement("span", null, "the switch"),
        view: RUNNING,
      }),
    );
    expect(html).toMatch(
      /the switch.*pnpm dev · in SHOP-22’s worktree · :4022.*Serving.*Open http:\/\/localhost:4022.*Stop.*Run options/,
    );
    expect(html).toContain('data-tone="success"');
    expect(html).not.toContain("run-card");
    expect(html).not.toContain("OUTPUT");
  });

  it("says each state in the line's words: Run when stopped, the reason under the line, the backend's note", () => {
    const stopped = renderToStaticMarkup(
      createElement(RunTabBody, {
        ticket: "SHOP-22",
        view: {
          ...RUNNING,
          state: { text: "Stopped by you at 11:48", tone: "neutral", icon: "square" },
          open_url: null,
          buttons: [button("run", { label: "Run" })],
        },
      }),
    );
    expect(stopped).toContain("Stopped by you at 11:48");
    expect(stopped).toContain(">Run<");
    expect(stopped).not.toContain("Stop<");
    const failed = renderToStaticMarkup(
      createElement(RunTabBody, {
        ticket: "SHOP-22",
        view: {
          ...RUNNING,
          state: { text: "Failed to start · exit 1", tone: "danger", icon: "triangle-alert" },
          message: "pnpm exited after 3s. The command may be wrong for this project, or setup didn’t finish.",
          open_url: null,
          buttons: [button("retry", { label: "Retry", primary: false })],
        },
      }),
    );
    expect(failed).toMatch(/Failed to start · exit 1.*Retry.*pnpm exited after 3s/);
    expect(failed).toContain('class="run-message" data-tone="danger"');
    const unset = renderToStaticMarkup(
      createElement(RunTabBody, {
        ticket: "SHOP-22",
        view: {
          ...RUNNING,
          line: null,
          state: { text: "Not running", tone: "neutral", icon: "play" },
          message: "No run command yet. Run asks for the command that starts this project.",
          open_url: null,
          buttons: [button("run", { label: "Run" })],
        },
      }),
    );
    expect(unset).toContain("No run command yet.");
    expect(unset).not.toContain("session-line-text");
  });

  it("draws the run-command dialog's steps", () => {
    const empty = renderToStaticMarkup(
      createElement(RunCommandBody, {
        view: COMMAND,
        command: "",
        openUrl: "",
        onCommand: noop,
        onOpenUrl: noop,
        onAsk: noop,
      }),
    );
    for (const text of [
      "Ask the agent",
      "The agent looks at the repo",
      "Open URL",
      "optional",
      "Leave empty for backends",
    ]) {
      expect(empty).toContain(text);
    }
    const thinking = renderToStaticMarkup(
      createElement(RunCommandBody, {
        view: {
          ...COMMAND,
          ask: null,
          thinking: { title: "The agent is reading the repo…", lines: ["· Read(package.json)"] },
        },
        command: "",
        openUrl: "",
        onCommand: noop,
        onOpenUrl: noop,
        onAsk: noop,
      }),
    );
    expect(thinking).toContain("The agent is reading the repo…");
    expect(thinking).toContain("· Read(package.json)");
    expect(thinking).not.toContain("Ask the agent");
    const saved = renderToStaticMarkup(
      createElement(RunCommandBody, {
        view: { ...COMMAND, ask: null, saved: "Saved for shop-web. Running SHOP-19 on :4020…" },
        command: "pnpm dev --port $PORT",
        openUrl: "",
        onCommand: noop,
        onOpenUrl: noop,
        onAsk: noop,
      }),
    );
    expect(saved).toContain("Saved for shop-web.");
    expect(saved).toContain("disabled");
  });

  it("draws the Shell strip and a shell a restart stopped", () => {
    const html = renderToStaticMarkup(
      createElement(ShellTabBody, { view: SHELL, state: INITIAL_STATE, onState: noop, onReopen: noop }),
    );
    for (const text of [
      "Shell · not the agent",
      "~/.otaq/wt/billing-api/BILL-4",
      "Run port · $PORT=4004",
      "Stopped by restart · was open at 08:24",
      "Reopen",
      "Typing goes to the shell, not the agent",
    ]) {
      expect(html).toContain(text);
    }
    expect(html).not.toContain("leave terminal");
  });

  it("lists every running process with Stop all", () => {
    const view: ProcessesView = {
      title: "Run processes",
      stop_all: "Stop all",
      stop: "Stop",
      empty: "Nothing runs.",
      rows: [
        {
          project_name: "billing-api",
          key: "BILL-4",
          title: "Invoice PDF export",
          port: ":4004",
          uptime: "12 min",
          url: null,
          open: null,
        },
        {
          project_name: "shop-web",
          key: "SHOP-19",
          title: "Price table",
          port: ":4020",
          uptime: "3 min",
          url: "http://localhost:4020",
          open: "Open :4020",
        },
      ],
    };
    const html = renderToStaticMarkup(createElement(ProcessesBody, { view, onOpenTicket: noop }));
    for (const text of ["Run processes", "Stop all", "billing-api", "BILL-4", ":4004", "Open :4020"]) {
      expect(html).toContain(text);
    }
    expect(html.match(/Open :4020/g)).toHaveLength(1);
  });
});

describe("Run's interaction rules", () => {
  it("routes each button and asks for the command when there is none", () => {
    expect(pressOf(button("run"))).toEqual({ kind: "request", method: "run.start" });
    expect(pressOf(button("retry"))).toEqual({ kind: "request", method: "run.start" });
    expect(pressOf(button("stop"))).toEqual({ kind: "request", method: "run.stop" });
    expect(pressOf(button("copy_lsof", { copy: "lsof -i :4020" }))).toEqual({
      kind: "copy",
      text: "lsof -i :4020",
    });
    expect(pressOf(button("copy_lsof"))).toEqual({ kind: "none" });

    afterStart({ outcome: "needs_command", project: "SHOP" }, "SHOP-19");
    expect(overlays.store.get()).toEqual([
      { id: "run_command", params: { project: "SHOP", ticket: "SHOP-19" } },
    ]);
  });

  it("takes only its project's Ask the agent events", () => {
    const event = {
      kind: "service" as const,
      name: "run_suggestion",
      event: {
        kind: "run_suggestion",
        project: "SHOP",
        update: { state: "progress", line: "Read(package.json)" },
      },
    };
    expect(suggestionFor(event, "SHOP")).toEqual({ state: "progress", line: "Read(package.json)" });
    expect(suggestionFor(event, "BILL")).toBeUndefined();
    expect(suggestionFor({ kind: "resync" }, "SHOP")).toBeUndefined();
  });
});
