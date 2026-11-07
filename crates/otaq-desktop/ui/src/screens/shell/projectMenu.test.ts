// The project switcher: the name only switches projects or adds one; and the rail's menu opens the project's Settings.
import { describe, expect, it, vi } from "vitest";

import { railMenuChoice, switcherChoice, type SwitcherRoutes } from "./projectMenu";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
}));

describe("the project switcher", () => {
  it("a project row shows that project; Add project… opens the add-project form", () => {
    const selected: string[] = [];
    let added = 0;
    const routes: SwitcherRoutes = {
      select: (project) => selected.push(project),
      addProject: () => {
        added += 1;
      },
    };
    switcherChoice("BILL", routes);
    switcherChoice(null, routes);
    expect(selected).toEqual(["BILL"]);
    expect(added).toBe(1);
  });
});

describe("the rail's project menu (#11)", () => {
  it("opens the project's Settings: at the top, at its Name field, at Remove project", () => {
    const opened: unknown[] = [];
    const open = (params: unknown) => {
      opened.push(params);
    };
    for (const id of ["project_settings", "rename_project", "remove_project", "unknown"])
      railMenuChoice("SHOP", id, open);
    expect(opened).toEqual([
      { project: "SHOP", section: null },
      { project: "SHOP", section: "repository" },
      { project: "SHOP", section: "remove_project" },
    ]);
  });
});
