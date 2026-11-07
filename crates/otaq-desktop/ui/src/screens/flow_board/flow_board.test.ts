// Edit flow on the board: where the editor opens from each place, how a dragged stage lands, the pretend ticket's
// steps, and what Save, Import, Export, Ask for changes and Undo ask of Rust.
import { describe, expect, it } from "vitest";

import { createStore } from "../../app/store";
import type { FlowBoardView } from "../../generated/FlowBoardView";
import {
  askForChanges,
  clampStep,
  closeFlowBoard,
  columnOpen,
  dropTo,
  editing,
  exportFile,
  type FlowBoardOpen,
  importFile,
  keyMove,
  openFlowBoard,
  openStageSkill,
  restore,
  saveDraft,
  stepOf,
} from "./logic";

/** A fake door that records each call. */
function recorder() {
  const calls: [string, unknown][] = [];
  const ask = (method: string, params?: unknown) => {
    calls.push([method, params]);
    return Promise.resolve(null);
  };
  return { calls, ask };
}

/** The parts of a view the calls read. */
const VIEW = {
  project: "SHOP",
  proposal: 7,
  yaml: "flow: []\n",
  choices: { clashes: {}, models: {} },
  saved: { title: "Flow saved", text: "Applied now." },
  more: { export_title: "Export shop-web’s flow", export_file: "shop-web.flow.yaml", import_title: "Import" },
} as unknown as FlowBoardView;

describe("where Edit flow opens", () => {
  it("turns only that project's board into the editor, until Cancel or Save", () => {
    const store = createStore<FlowBoardOpen | null>(null);
    openFlowBoard({ project: "SHOP" }, store);
    expect(editing(store.get(), "SHOP")).toBe(true);
    expect(editing(store.get(), "BILL")).toBe(false);
    expect(editing(store.get(), null)).toBe(false);
    closeFlowBoard(store);
    expect(store.get()).toBeNull();
  });

  it("a column's ⋯: Edit <stage>… selects it; Add a stage after <stage> opens the palette there", () => {
    expect(columnOpen("SHOP", { open: "stage", stage: "validate" })).toEqual({
      project: "SHOP",
      select: "validate",
    });
    expect(columnOpen("SHOP", { open: "add_after", after: "dev" })).toEqual({
      project: "SHOP",
      addAfter: "dev",
    });
    // Backlog's: right after it.
    expect(columnOpen("SHOP", { open: "add_after", after: null })).toEqual({
      project: "SHOP",
      addAfter: null,
    });
    expect(columnOpen("SHOP", { open: "flow" })).toEqual({ project: "SHOP" });
  });

  it("use your own Open PR skill: Settings closes and the editor opens on the stage's Skill field", () => {
    const calls: string[] = [];
    const routes = {
      closeSettings: () => calls.push("close settings"),
      open: (o: FlowBoardOpen) => calls.push(JSON.stringify(o)),
    };
    openStageSkill("SHOP", "validate", routes);
    expect(calls).toEqual([
      "close settings",
      JSON.stringify({ project: "SHOP", select: "validate", focusSkill: true }),
    ]);
    // No stage named: How work leaves the flow.
    calls.length = 0;
    openStageSkill("SHOP", null, routes);
    expect(calls).toEqual(["close settings", JSON.stringify({ project: "SHOP", select: "done" })]);
  });
});

describe("moving a stage", () => {
  it("a stage dropped on another takes its place; dropped on itself it stays", () => {
    // Dev · Validate · Review: Dev dropped on Review goes last, Review dropped on Dev goes first.
    expect(dropTo(0, 2)).toBe(2);
    expect(dropTo(2, 0)).toBe(0);
    expect(dropTo(1, 1)).toBeNull();
  });

  it("Alt+← / Alt+→ move it one place, never past the ends", () => {
    expect(keyMove(0, 3, "ArrowLeft")).toBeNull();
    expect(keyMove(2, 3, "ArrowRight")).toBeNull();
    expect(keyMove(1, 3, "ArrowLeft")).toBe(0);
    expect(keyMove(1, 3, "ArrowRight")).toBe(2);
    expect(keyMove(1, 3, "Enter")).toBeNull();
  });
});

describe("Watch a ticket", () => {
  it("says which step of how many, and stays inside the walk when the draft changes", () => {
    expect(stepOf("Step {n} of {m}", 4, 7)).toBe("Step 5 of 7");
    const steps = [
      { column: "backlog", name: "Backlog", state: "Not started", text: "" },
      { column: "dev", name: "Dev", state: "Working", text: "" },
    ];
    expect(clampStep(5, steps)).toBe(1);
    expect(clampStep(-1, steps)).toBe(0);
  });
});

describe("the draft's requests", () => {
  it("Save flow applies the draft with its picks, then says what it did", async () => {
    const { calls, ask } = recorder();
    await saveDraft(VIEW, ask);
    expect(calls[0]).toEqual([
      "flow_editor.apply",
      { project: "SHOP", proposal: 7, choices: { clashes: {}, models: {} } },
    ]);
    expect(calls[1]?.[0]).toBe("notices.toast");
    expect(calls[1]?.[1]).toMatchObject({ level: "success", title: "Flow saved", text: "Applied now." });
  });

  it("Import reads the picked file into the draft; a cancelled dialog does nothing", async () => {
    const { calls, ask } = recorder();
    expect(await importFile("SHOP", () => Promise.resolve(null), ask)).toBe(false);
    expect(calls).toEqual([]);
    expect(await importFile("SHOP", () => Promise.resolve("/tmp/team.flow.yaml"), ask)).toBe(true);
    expect(calls).toEqual([["flow_editor.import", { project: "SHOP", path: "/tmp/team.flow.yaml" }]]);
  });

  it("Export offers the project's file name, then writes the draft there", async () => {
    const { calls, ask } = recorder();
    const offered: string[] = [];
    await exportFile(
      VIEW,
      (_title, name) => {
        offered.push(name);
        return Promise.resolve("/tmp/shop-web.flow.yaml");
      },
      ask,
    );
    expect(offered).toEqual(["shop-web.flow.yaml"]);
    expect(calls).toEqual([["flow_board.export", { project: "SHOP", path: "/tmp/shop-web.flow.yaml" }]]);
  });

  it("Ask for changes sends the user's words back to the agent; empty words send nothing", async () => {
    const { calls, ask } = recorder();
    expect(await askForChanges(VIEW, "   ", ask)).toBe(false);
    expect(await askForChanges(VIEW, " Keep Review. ", ask)).toBe(true);
    expect(calls).toEqual([
      ["flow_board.ask_changes", { project: "SHOP", proposal: 7, text: "Keep Review." }],
    ]);
  });

  it("Undo puts back the document before the last edit", async () => {
    const { calls, ask } = recorder();
    await restore("SHOP", "flow: []\n", ask);
    expect(calls).toEqual([["flow_board.restore", { project: "SHOP", yaml: "flow: []\n" }]]);
  });
});
