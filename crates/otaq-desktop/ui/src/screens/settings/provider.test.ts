// @vitest-environment happy-dom
// The PR provider row (REV-11): on a remote no gh or az provider matches (GitLab), "use your own Open PR skill"
// closes Settings and opens the flow editor on the Delivery stage, its Skill field focused: the user's own Open PR
// skill goes there, not in an agent's skills.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { openOverlay, overlays } from "../../app/overlays";
import type { SettingsPage } from "../../generated/SettingsPage";
import type { SettingsProvider } from "../../generated/SettingsProvider";
import type { SettingsSection } from "../../generated/SettingsSection";
import { request } from "../../tauri";
import { SettingsContext } from "./context";
import { Section } from "./Rows";

vi.mock("../../tauri", async (original) => ({
  ...(await original<typeof import("../../tauri")>()),
  request: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const GITLAB: SettingsProvider = {
  description: "origin is gitlab.com/acme/shop-web · no gh or az provider matches.",
  status: "Not detected:",
  icon: "circle-question-mark",
  look: "not_detected",
  link: "use your own Open PR skill",
  stage: "done",
};

const SECTION: SettingsSection<string> = {
  id: "pr",
  title: "Pull requests",
  intro: null,
  rows: [
    {
      field: "pr_provider",
      label: "PR provider",
      description: "",
      control: { kind: "provider" },
      dimmed: false,
      reset: null,
      button: null,
      set_by: null,
    },
  ],
  note: null,
};

const PAGE = { page: "global", title: "Project", sections: [] } as unknown as SettingsPage;

let root: Root;
let box: HTMLElement;

beforeEach(() => {
  box = document.createElement("div");
  document.body.append(box);
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  box.remove();
  overlays.store.set([]);
  vi.mocked(request).mockReset();
});

describe("the PR provider row", () => {
  it("on GitLab, use your own Open PR skill opens the Delivery stage's Skill field in the flow editor", async () => {
    vi.mocked(request).mockResolvedValue(GITLAB);
    openOverlay("settings", { project: "SHOP" });
    await act(async () => {
      root.render(
        createElement(
          SettingsContext,
          { value: { page: PAGE, project: "SHOP", edit: () => undefined, act: () => undefined } },
          createElement(Section, { section: SECTION }),
        ),
      );
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith("settings.provider", { project: "SHOP" });
    const link = [...box.querySelectorAll("button")].find(
      (b) => b.textContent === "use your own Open PR skill",
    );
    expect(link).toBeDefined();
    expect(box.textContent).not.toMatch(/Settings → Agents|agent's skills/);
    act(() => {
      link?.click();
    });
    expect(overlays.store.get()).toEqual([
      {
        id: "flow_editor",
        params: { project: "SHOP", from: "project", select_stage: "done", focus_skill: true },
      },
    ]);
  });
});
