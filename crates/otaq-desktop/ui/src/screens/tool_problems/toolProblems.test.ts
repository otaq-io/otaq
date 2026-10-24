// Where a tool problem's buttons go (T-0367, T-0411): Use fallback calls its request, Open agent… opens Settings →
// Agents on the agent, Sign in… opens the tool's login in the drawer, New agent on … opens the empty agent panel on that
// agent, Show in Agent tools opens Settings. Nothing starts by itself (d0018, d0087).
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { ToolProblemCallout } from "../../components";
import type { ToolProblemCallout as CalloutView } from "../../generated/ToolProblemCallout";
import { cardRoute } from "../sheet/actionRoutes";
import { runToolProblemAction, type ToolProblemRunner, toolProblemStep } from "./toolProblems";

function runner(answer: unknown = null) {
  return {
    request: vi.fn((): Promise<unknown> => Promise.resolve(answer)),
    openSettings: vi.fn(),
    showDrawerTab: vi.fn(),
    newAgentOn: vi.fn(),
  } satisfies ToolProblemRunner;
}

/** *05 `sFZiP`*: SHOP-18 on Standard with Codex hidden (Sonnet is Standard's fallback). */
const HIDDEN: CalloutView = {
  role: "developer",
  tone: "amber",
  avatar: null,
  heading: "Codex isn't available · hidden in Settings",
  text: "Standard runs on Codex. Use its fallback Sonnet, pick another agent, or show Codex again.",
  model: null,
  buttons: [
    { label: "Use fallback (Sonnet)", action: { kind: "use_fallback", role: "developer" }, primary: true },
    { label: "Open agent…", action: { kind: "open_agent", agent: "standard" }, primary: false },
    { label: "Show in Agent tools", action: { kind: "show_in_agent_tools" }, primary: false },
  ],
};

describe("Use fallback", () => {
  it("asks the service to move the developer row, as the card's button does", () => {
    const run = runner();
    runToolProblemAction({ kind: "use_fallback", role: "developer" }, { ticket: "SHOP-18" }, run);
    expect(run.request).toHaveBeenCalledWith("sheet.card", {
      key: "SHOP-18",
      kind: "use_fallback",
      clear: false,
    });
    expect(cardRoute("use_fallback")).toEqual({ kind: "service" });
  });

  it("the validator's moves the validator row", () => {
    const run = runner();
    runToolProblemAction({ kind: "use_fallback", role: "validator" }, { ticket: "SHOP-30" }, run);
    expect(run.request).toHaveBeenCalledWith("sheet.card", {
      key: "SHOP-30",
      kind: "use_validator_fallback",
      clear: false,
    });
    expect(cardRoute("use_validator_fallback")).toEqual({ kind: "service" });
  });

  it("does nothing without a ticket", () => {
    expect(toolProblemStep({ kind: "use_fallback", role: "developer" }, {})).toEqual({ kind: "none" });
  });
});

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

describe("the notice's other buttons", () => {
  it("Sign in… opens the tool's own login in the Terminals drawer", async () => {
    const run = runner({ session: "s9", tab: "t9", title: "Codex sign in" });
    runToolProblemAction({ kind: "sign_in", tool: "codex" }, { project: "SHOP" }, run);
    expect(run.request).toHaveBeenCalledWith("agent_tools.sign_in", { tool: "codex" });
    const pending = run.showDrawerTab.mock.calls[0]?.[0] as Promise<unknown>;
    expect(await pending).toBe("t9");
  });

  it("New agent on Sonnet opens the empty agent panel on Sonnet and starts nothing", () => {
    const run = runner();
    runToolProblemAction({ kind: "new_agent", agent: "sonnet" }, { project: "SHOP" }, run);
    expect(run.newAgentOn).toHaveBeenCalledWith("SHOP", "sonnet");
    expect(run.request).not.toHaveBeenCalled();
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
    for (const text of [
      "hidden in Settings",
      "Use fallback (Sonnet)",
      "Open agent…",
      "Show in Agent tools",
    ]) {
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
});
