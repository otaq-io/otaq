// Where a tool problem's buttons go (T-0367, T-0411): Open agent… opens Settings →
// Agents on the agent, Sign in… opens the tool's login in the drawer, Show in Agent tools opens Settings. Nothing starts
// by itself, and no other agent is offered in a waiting one's place (d0018, d0087).
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { ToolProblemCallout } from "../../components";
import type { ToolProblemCallout as CalloutView } from "../../generated/ToolProblemCallout";
import { runToolProblemAction, type ToolProblemRunner } from "./toolProblems";

function runner(answer: unknown = null) {
  return {
    request: vi.fn((): Promise<unknown> => Promise.resolve(answer)),
    openSettings: vi.fn(),
    showDrawerTab: vi.fn(),
    changeAgent: vi.fn(),
  } satisfies ToolProblemRunner;
}

/** *05 `sFZiP`*: SHOP-18 on Standard with Codex hidden: it waits for Codex. */
const HIDDEN: CalloutView = {
  role: "developer",
  tone: "amber",
  avatar: null,
  heading: "Codex isn't available · hidden in Settings",
  text: "Nothing started and nothing is locked. Standard starts once Codex is available again.",
  model: null,
  buttons: [
    { label: "Open agent…", action: { kind: "open_agent", agent: "standard" }, primary: true },
    { label: "Show in Agent tools", action: { kind: "show_in_agent_tools" }, primary: false },
  ],
};

describe("Open agent…", () => {
  it("opens Settings → Agents on the held agent's page (AGM-03)", () => {
    const run = runner();
    runToolProblemAction({ kind: "open_agent", agent: "verifier" }, { ticket: "SHOP-30" }, run);
    expect(run.openSettings).toHaveBeenCalledWith({ section: "agents", agent: "verifier" });
    runToolProblemAction({ kind: "open_agent", agent: null }, { ticket: "SHOP-40" }, run);
    expect(run.openSettings).toHaveBeenLastCalledWith({ section: "agents" });
    expect(run.request).not.toHaveBeenCalled();
  });

  it("for a held first message opens Settings → Agents of the project (Q-O)", () => {
    const run = runner();
    runToolProblemAction(
      { kind: "agent_settings", project: "SHOP", agent: "old-gpt" },
      { project: "SHOP" },
      run,
    );
    expect(run.openSettings).toHaveBeenCalledWith({ section: "agents", agent: "old-gpt" });
  });
});

describe("Change agent…", () => {
  it("opens the ticket's Change agent… dialog, and nothing outside a ticket", () => {
    const run = runner();
    runToolProblemAction({ kind: "change_agent" }, { ticket: "SHOP-30" }, run);
    expect(run.changeAgent).toHaveBeenCalledWith("SHOP-30");
    runToolProblemAction({ kind: "change_agent" }, { project: "SHOP" }, run);
    expect(run.changeAgent).toHaveBeenCalledTimes(1);
    expect(run.request).not.toHaveBeenCalled();
  });
});

describe("Check again", () => {
  it("asks the service to check the ticket's held stage again, and nothing outside a ticket", () => {
    const run = runner();
    runToolProblemAction({ kind: "check_again" }, { ticket: "SHOP-30" }, run);
    expect(run.request).toHaveBeenCalledWith("sheet.check_again", { key: "SHOP-30" });
    runToolProblemAction({ kind: "check_again" }, { project: "SHOP" }, run);
    expect(run.request).toHaveBeenCalledTimes(1);
  });
});

describe("the notice's other buttons", () => {
  it("Sign in… opens the tool's own login in the Terminals drawer", async () => {
    const run = runner({ session: "s9", tab: "t9", title: "Codex sign in" });
    runToolProblemAction({ kind: "sign_in", tool: "codex" }, { project: "SHOP" }, run);
    expect(run.request).toHaveBeenCalledWith("agent_tools.sign_in", { tool: "codex" });
    const pending = run.showDrawerTab.mock.calls[0]?.[0] as Promise<unknown>;
    expect(await pending).toBe("t9");
  });

  it("Show in Agent tools opens Settings → Agent tools; Resume after reset asks the service", () => {
    const run = runner();
    runToolProblemAction({ kind: "show_in_agent_tools" }, {}, run);
    expect(run.openSettings).toHaveBeenCalledWith({ section: "agent_tools" });
    runToolProblemAction({ kind: "resume_tool", tool: "codex" }, { project: "SHOP" }, run);
    expect(run.request).toHaveBeenCalledWith("limits.resume_tool", { tool: "codex" });
  });
});

describe("the callout", () => {
  it("draws the heading, the line and the buttons, amber and never turquoise", () => {
    const html = renderToStaticMarkup(
      createElement(ToolProblemCallout, { callout: HIDDEN, look: "edge", onAction: () => undefined }),
    );
    expect(html).toContain('data-tone="amber"');
    for (const text of ["hidden in Settings", "Open agent…", "Show in Agent tools"]) {
      expect(html).toContain(text);
    }
    expect(html).not.toContain("attn");
  });

  it("a limit is neutral", () => {
    const limit: CalloutView = {
      ...HIDDEN,
      tone: "neutral",
      heading: "Claude Code limit reached · resets 3:45 pm",
      text: null,
      buttons: [],
    };
    const html = renderToStaticMarkup(
      createElement(ToolProblemCallout, { callout: limit, look: "band", onAction: () => undefined }),
    );
    expect(html).toContain('data-tone="neutral"');
    expect(html).not.toContain("tool-problem-buttons");
  });

  it("an agent whose tool stopped on an error is red, with the next step and Sign in…", () => {
    const stopped: CalloutView = {
      ...HIDDEN,
      role: null,
      tone: "error",
      heading: "Claude Code stopped · login expired",
      text: "Sign in to Claude Code, then Resume: the agent goes on where it stopped.",
      buttons: [{ label: "Sign in…", action: { kind: "sign_in", tool: "claude" }, primary: true }],
    };
    const html = renderToStaticMarkup(
      createElement(ToolProblemCallout, { callout: stopped, look: "edge", onAction: () => undefined }),
    );
    expect(html).toContain('data-tone="error"');
    for (const text of ["stopped · login expired", "then Resume", "Sign in…"]) {
      expect(html).toContain(text);
    }
  });
});
