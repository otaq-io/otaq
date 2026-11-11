// @vitest-environment happy-dom
// Sign in… on a ticket's sheet (Claude Code stopped · login expired) shows the login's terminal where the user can see
// it: its own dialog, as Settings → Agents and the restart dialog show theirs. The Terminals drawer lists only the
// project's own terminals, and a sign-in belongs to no project, so opening the drawer showed "Nothing open".
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { overlays } from "../../app/overlays";
import { RegistriesContext, registerScreens } from "../../app/registry";
import type { AgentToolSignIn } from "../../generated/AgentToolSignIn";
import { request } from "../../tauri";
import { settings } from "../settings";
import { toolProblemAction } from "./toolProblemRunner";

vi.mock("../../tauri", async (original) => ({
  ...(await original<typeof import("../../tauri")>()),
  request: vi.fn(),
}));

const SIGN_IN: AgentToolSignIn = { session: "sign-in-7", tab: "tab-7", title: "Claude Code sign in" };

afterEach(() => {
  overlays.store.set([]);
});

describe("Sign in… on a ticket's sheet", () => {
  it("opens a dialog showing the sign-in's own terminal, not the empty Terminals drawer", async () => {
    vi.mocked(request).mockResolvedValue(SIGN_IN);
    toolProblemAction({ kind: "sign_in", tool: "claude" }, { ticket: "NOTE-11" });
    expect(request).toHaveBeenCalledWith("agent_tools.sign_in", { tool: "claude" });
    await vi.waitFor(() => {
      expect(overlays.store.get()).toEqual([{ id: "sign_in", params: SIGN_IN }]);
    });

    const app = registerScreens({ settings });
    const entry = app.overlays.get("sign_in");
    expect(entry).toBeDefined();
    if (!entry) return;
    const html = renderToStaticMarkup(
      createElement(
        RegistriesContext.Provider,
        { value: app },
        createElement(entry.Component, { params: SIGN_IN, close: () => undefined }),
      ),
    );
    expect(html).toContain("Claude Code sign in");
    expect(html).toContain("agent-tool-sign-in");
    expect(html).not.toContain("Nothing open");
  });
});
