// Where a tool problem's buttons go (T-0367, T-0411, UX-65): Edit flow… opens the project's flow at the waiting stage,
// Sign in… opens the tool's login in its terminal's dialog, Show in Tools opens Settings. Nothing starts
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
    showSignIn: vi.fn(),
    editFlow: vi.fn(),
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
    { label: "Edit flow…", action: { kind: "edit_flow", stage: "dev" }, primary: true },
    { label: "Show in Tools", action: { kind: "show_in_tools" }, primary: false },
  ],
};

describe("Edit flow…", () => {
  it("opens the ticket's project's flow at the waiting stage (UX-65)", () => {
    const run = runner();
    runToolProblemAction({ kind: "edit_flow", stage: "validate" }, { ticket: "SHOP-30" }, run);
    expect(run.editFlow).toHaveBeenCalledWith({ project: "SHOP", select: "validate" });
    runToolProblemAction({ kind: "edit_flow", stage: null }, { project: "NOTE" }, run);
    expect(run.editFlow).toHaveBeenLastCalledWith({ project: "NOTE" });
    runToolProblemAction({ kind: "edit_flow", stage: null }, {}, run);
    expect(run.editFlow).toHaveBeenCalledTimes(2);
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
  // The drawer lists only the project's own terminals; a sign-in's belongs to no project, so it said "Nothing open".
  it("Sign in… shows the tool's own login terminal in its dialog, not the Terminals drawer", async () => {
    const signIn = { session: "s9", tab: "t9", title: "Claude Code sign in" };
    const run = runner(signIn);
    runToolProblemAction({ kind: "sign_in", tool: "claude" }, { ticket: "NOTE-11" }, run);
    expect(run.request).toHaveBeenCalledWith("agent_tools.sign_in", { tool: "claude" });
    await vi.waitFor(() => {
      expect(run.showSignIn).toHaveBeenCalledWith(signIn);
    });
  });

  it("Show in Tools opens Settings → Tools; Resume after reset asks the service", () => {
    const run = runner();
    runToolProblemAction({ kind: "show_in_tools" }, {}, run);
    expect(run.openSettings).toHaveBeenCalledWith({ section: "tools" });
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
    for (const text of ["hidden in Settings", "Edit flow…", "Show in Tools"]) {
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
