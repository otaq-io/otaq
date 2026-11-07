// @vitest-environment happy-dom
// The project's Settings page: it draws its section list and one card per section from `project_settings.view`, an
// edit answers with the fresh view, Remove project asks first; opening it from elsewhere shows the project at the
// section asked for, and the section list follows the scroll.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { RegistriesContext, registerScreens } from "../../app/registry";
import type { ProjectSettingsView } from "../../generated/ProjectSettingsView";
import type { SettingsSection } from "../../generated/SettingsSection";
import { request } from "../../tauri";
import { projectSettings } from ".";
import { activeCard, openProjectSettings, projectSettingsWanted } from "./logic";
import { ProjectSettings } from "./ProjectSettings";

vi.mock("../../tauri", async (original) => ({
  ...(await original<typeof import("../../tauri")>()),
  request: vi.fn(),
}));

// The view loads at once, with no event stream.
vi.mock("../../service", async (original) => ({
  ...(await original<typeof import("../../service")>()),
  watchView: (load: () => Promise<unknown>, _on: unknown, onChange: (state: { view?: unknown }) => void) => {
    void load().then((view) => {
      onChange({ view });
    });
    return () => undefined;
  },
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const card = (id: SettingsSection<"repository">["id"], title: string): SettingsSection<"repository"> => ({
  id,
  title,
  intro: "Where agents start, how work leaves, and what runs before a merge.",
  rows: [
    {
      field: "delivery",
      label: "Delivery",
      description: "How work leaves the flow.",
      control: {
        kind: "delivery",
        delivers: [
          { id: "local", label: "Merge into main" },
          { id: "pr", label: "Pull request" },
          { id: "either", label: "Either" },
        ],
        delivers_value: "local",
        modes: [
          { id: "click", label: "On Approve" },
          { id: "automatic", label: "Automatically" },
        ],
        mode_value: "click",
        either: null,
        note: "This is the flow’s setting: changing it here changes the flow, and the flow file.",
      },
      dimmed: false,
      reset: null,
      button: null,
      set_by: null,
    },
  ],
  note: null,
});

function view(delivers = "local"): ProjectSettingsView {
  const repository = card("repository", "Repository");
  const row = repository.rows[0];
  if (row?.control.kind === "delivery") row.control.delivers_value = delivers;
  return {
    key: "SHOP",
    name: "shop-web",
    path: "~/code/shop-web",
    nav: [
      { id: "repository", label: "Repository" },
      { id: "remove_project", label: "Remove project" },
    ],
    app_settings: "App-wide settings (agents, notifications, appearance) are under the gear on the rail.",
    page: {
      page: "project",
      key: "SHOP",
      title: "shop-web",
      intro: "",
      sections: [
        repository,
        {
          id: "remove_project",
          title: "Remove project",
          intro: null,
          rows: [
            {
              field: "",
              label: "Remove project",
              description: "Removes shop-web from Otaq.",
              control: {
                kind: "button",
                label: "Remove shop-web…",
                icon: "trash-2",
                destructive: true,
                action: "remove_project",
              },
              dimmed: false,
              reset: null,
              button: null,
              set_by: null,
            },
          ],
          note: null,
        },
      ],
    },
  };
}

const app = registerScreens({ projectSettings });
let root: Root;
let host: HTMLElement;
const ask = vi.mocked(request);

beforeEach(() => {
  ask.mockReset();
  ask.mockImplementation((method: string, params: unknown = null) => {
    if (method === "project_settings.edit") {
      const p = params as { field: string; value: string };
      return Promise.resolve(view(p.field === "delivery.delivers" ? p.value : "local"));
    }
    if (method === "project_settings.remove_preview")
      return Promise.resolve({
        title: "Remove shop-web from Otaq?",
        deleted_heading: "Deleted from Otaq",
        deleted: ["2 tickets"],
        kept_heading: "Never touched",
        kept: ["The repository"],
        agents: null,
        confirm: "Remove project",
        cancel: "Cancel",
      });
    return Promise.resolve(view());
  });
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
  projectSettingsWanted.set(null);
});

async function draw() {
  act(() => {
    root.render(createElement(RegistriesContext, { value: app }, createElement(ProjectSettings, { project: "SHOP" })));
  });
  await act(async () => {
    await Promise.resolve();
  });
}

const button = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent === text) ?? null;

describe("the project's Settings page", () => {
  it("registers its cards in the page's order", () => {
    expect(app.settings.project.ids()).toEqual([
      "flow",
      "repository",
      "run",
      "workspace",
      "project_notes",
      "remove_project",
    ]);
  });

  it("draws the section list and the cards; Deliver as changes the flow through an edit", async () => {
    await draw();
    expect(ask).toHaveBeenCalledWith("project_settings.view", { project: "SHOP" });
    const nav = [...host.querySelectorAll(".project-settings-nav-item")].map((b) => b.textContent);
    expect(nav).toEqual(["Repository", "Remove project"]);
    expect(host.textContent).toContain("~/code/shop-web");
    expect(host.textContent).toContain("under the gear on the rail");
    expect(host.textContent).toContain("changing it here changes the flow");
    act(() => {
      button("Pull request")?.click();
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(ask).toHaveBeenCalledWith("project_settings.edit", {
      project: "SHOP",
      field: "delivery.delivers",
      value: "pr",
    });
    expect(button("Pull request")?.getAttribute("aria-checked")).toBe("true");
  });

  it("Remove project asks first, then removes", async () => {
    await draw();
    act(() => {
      button("Remove shop-web…")?.click();
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(document.body.textContent).toContain("Remove shop-web from Otaq?");
    const confirm = [...document.querySelectorAll(".dialog button")].find((b) => b.textContent === "Remove project");
    act(() => {
      (confirm as HTMLButtonElement | undefined)?.click();
    });
    expect(ask).toHaveBeenCalledWith("project_settings.remove", { project: "SHOP" });
  });
});

describe("opening the page from elsewhere", () => {
  it("closes Settings and the ticket, shows the project and asks for the section, counted", () => {
    const calls: string[] = [];
    const routes = {
      select: (p: string) => calls.push(`select ${p}`),
      closeSettings: () => calls.push("close settings"),
      closeTicket: () => calls.push("close ticket"),
    };
    openProjectSettings("SHOP", "repository", routes);
    openProjectSettings("SHOP", "repository", routes);
    expect(calls.slice(0, 3)).toEqual(["close settings", "close ticket", "select SHOP"]);
    expect(projectSettingsWanted.get()).toEqual({ project: "SHOP", section: "repository", n: 2 });
  });

  it("the section list marks the card whose top has reached the list's, the last at the bottom", () => {
    expect(activeCard([], 0, false)).toBe(-1);
    expect(activeCard([0, 400, 900], 0, false)).toBe(0);
    expect(activeCard([0, 400, 900], 390, false)).toBe(1);
    expect(activeCard([0, 400, 900], 500, true)).toBe(2);
  });
});
