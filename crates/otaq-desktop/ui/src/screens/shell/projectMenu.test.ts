// The project menu (FED-09, `s26-08`): each choice goes where the frame says. Change flow… opens the flow editor over
// the board on the flow in use; Export flow… saves where the dialog said (a cancelled dialog asks nothing); Project
// settings… opens the project's Settings; Open in Finder and Open a terminal here ask the app.
import { describe, expect, it, vi } from "vitest";

import type { HeaderProject } from "../../generated/HeaderProject";
import type { FlowEditorOpen } from "../flow_editor/logic";
import { projectMenuChoice, type ProjectMenuRoutes, railMenuChoice } from "./projectMenu";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  saveFlowFile: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
}));

const SHOP: HeaderProject = {
  key: "SHOP",
  name: "shop-web",
  base_branch: "main",
  path: "~/code/shop-web",
  repo_path: "/code/shop-web",
  menu: {
    title: "shop-web",
    path: "~/code/shop-web",
    items: [],
    export_title: "Export shop-web’s flow",
    export_file: "shop-web-flow.yaml",
  },
  flow_setup: null,
};

function routes(saved: string | null = null) {
  const asked: [string, unknown][] = [];
  const editors: FlowEditorOpen[] = [];
  const settings: string[] = [];
  const terminals: Promise<unknown>[] = [];
  const r: ProjectMenuRoutes = {
    ask: (method, params) => {
      asked.push([method, params]);
      return Promise.resolve(method === "drawer.new" ? "terminal:s-1" : null);
    },
    openEditor: (open) => editors.push(open),
    saveFile: vi.fn(() => Promise.resolve(saved)),
    openSettings: (project) => settings.push(project),
    showTerminal: (pending) => terminals.push(pending),
  };
  return { r, asked, editors, settings, terminals };
}

describe("the project menu", () => {
  it("Change flow… opens the flow editor on the project's flow in use; nothing is asked", async () => {
    const t = routes();
    await projectMenuChoice(SHOP, "change_flow", t.r);
    expect(t.editors).toEqual([{ project: "SHOP", from: "project" }]);
    expect(t.asked).toEqual([]);
  });

  it("Export flow… saves where the dialog said; a cancelled dialog asks nothing", async () => {
    const cancelled = routes(null);
    await projectMenuChoice(SHOP, "export_flow", cancelled.r);
    expect(cancelled.r.saveFile).toHaveBeenCalledWith("Export shop-web’s flow", "shop-web-flow.yaml");
    expect(cancelled.asked).toEqual([]);
    const saved = routes("/tmp/shop-web-flow.yaml");
    await projectMenuChoice(SHOP, "export_flow", saved.r);
    expect(saved.asked).toEqual([
      ["settings.flow_export", { project: "SHOP", path: "/tmp/shop-web-flow.yaml" }],
    ]);
  });

  it("Project settings…, Open in Finder and Open a terminal here", async () => {
    const t = routes();
    await projectMenuChoice(SHOP, "project_settings", t.r);
    expect(t.settings).toEqual(["SHOP"]);
    await projectMenuChoice(SHOP, "open_folder", t.r);
    await projectMenuChoice(SHOP, "open_terminal", t.r);
    expect(t.asked).toEqual([
      ["shell.open_folder", { project: "SHOP" }],
      ["drawer.new", { project: "SHOP" }],
    ]);
    expect(await t.terminals[0]).toBe("terminal:s-1");
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
      { project: "SHOP" },
      { project: "SHOP", section: "general" },
      { project: "SHOP", section: "remove_project" },
    ]);
  });
});
