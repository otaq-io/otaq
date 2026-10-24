// @vitest-environment happy-dom
// The Environment variables row's own logic (T-0300, T-0506): what is sent when, and Esc that saves the row first and
// closes Settings only the second time.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import type { SettingsControl } from "../../generated/SettingsControl";
import type { SettingsRow } from "../../generated/SettingsRow";
import type { SettingsValue } from "../../generated/SettingsValue";
import { SettingsContext, type SettingsContextValue } from "./context";
import { EnvVarsRow } from "./EnvVars";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The shape `settings_page.rs` sends (`env_vars_row`).
function envRow(rows: { name: string; value: string; problem?: string }[]): SettingsRow {
  const control: SettingsControl = {
    kind: "env_vars",
    rows: rows.map((r) => ({
      name: r.name,
      value: r.value,
      problem: r.problem ?? null,
      remove_label: `Remove ${r.name}`,
    })),
    hint_example: "CI = true",
    hint: "e.g. so tools never wait for input or start watch mode",
    empty: "No variables. Sessions get your normal environment.",
    add_label: "Add variable",
    drop_label: "Remove variable",
    applies: "Changes apply to sessions, Runs and Shells started after this.",
    name_placeholder: "NAME",
    value_placeholder: "value",
  };
  return {
    field: "env_vars",
    label: "Environment variables",
    description: "Set in every agent session, Run and Shell of this project.",
    control,
    dimmed: false,
    reset: null,
    button: null,
  };
}

let root: Root;
let box: HTMLElement;
let edits: [string, SettingsValue][];
let closed: number;

function show(row: SettingsRow) {
  const value: SettingsContextValue = {
    page: { page: "project", key: "SHOP", title: "shop-web", intro: "", sections: [] },
    project: "SHOP",
    edit: (field, v) => {
      edits.push([field, v]);
    },
    act: () => undefined,
  };
  act(() => {
    root.render(createElement(SettingsContext.Provider, { value }, createElement(EnvVarsRow, { row })));
  });
}

const inputs = () => [...box.querySelectorAll("input")];

function type(input: HTMLInputElement, text: string) {
  act(() => {
    // The prototype's setter, so React's value tracker sees a change.
    Reflect.set(HTMLInputElement.prototype, "value", text, input);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function key(target: Element, k: string) {
  act(() => {
    target.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
  });
}

function addButton(): HTMLButtonElement {
  const found = [...box.querySelectorAll("button")].find((b) => b.textContent === "Add variable");
  if (!found) throw new Error("no Add variable");
  return found;
}

beforeEach(() => {
  edits = [];
  closed = 0;
  // Settings' own Esc listener on its box: it closes Settings unless a field took the key.
  box = document.body.appendChild(document.createElement("div"));
  box.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closed += 1;
  });
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
});

describe("the Environment variables row", () => {
  it("shows the empty box, the hint and the applies line", () => {
    show(envRow([]));
    expect(box.textContent).toContain("No variables. Sessions get your normal environment.");
    expect(box.querySelector("code")?.textContent).toBe("CI = true");
    expect(box.textContent).toContain("Changes apply to sessions, Runs and Shells started after this.");
  });

  it("adds a row with NAME focused and saves it with ↵", () => {
    show(envRow([]));
    act(() => {
      addButton().click();
    });
    const [name, value] = inputs();
    if (!name || !value) throw new Error("no new line");
    expect(document.activeElement).toBe(name);
    expect(box.textContent).not.toContain("No variables");
    type(name, "CI");
    type(value, "true");
    key(value, "Enter");
    expect(edits).toEqual([["env_vars.add", { name: "CI", value: "true" }]]);
    expect(inputs()).toHaveLength(0);
    expect(document.activeElement).toBe(addButton());
  });

  it("keeps a new line on name, value, Tab and Esc (the dogfood run's CI, true), and Esc again closes Settings", () => {
    show(envRow([]));
    act(() => {
      addButton().click();
    });
    const [name, value] = inputs();
    const drop = box.querySelector<HTMLButtonElement>('button[aria-label="Remove variable"]');
    if (!name || !value || !drop) throw new Error("no new line");
    type(name, "CI");
    act(() => {
      value.focus();
    });
    type(value, "true");
    // Tab: the focus moves to the line's ×, still inside the line, so nothing is saved yet.
    key(value, "Tab");
    act(() => {
      drop.focus();
    });
    expect(edits).toEqual([]);
    key(drop, "Escape");
    expect(edits).toEqual([["env_vars.add", { name: "CI", value: "true" }]]);
    expect(closed).toBe(0);
    expect(inputs()).toHaveLength(0);
    expect(document.activeElement).toBe(addButton());
    key(addButton(), "Escape");
    expect(closed).toBe(1);
  });

  it("drops only an empty new line on Esc; one with a value and no name is saved, for the row to mark it", () => {
    show(envRow([]));
    act(() => {
      addButton().click();
    });
    const [name] = inputs();
    if (!name) throw new Error("no new line");
    key(name, "Escape");
    expect(inputs()).toHaveLength(0);
    expect(closed).toBe(0);
    expect(edits).toEqual([]);
    for (const end of ["Enter", "Escape", "blur"]) {
      edits.length = 0;
      act(() => {
        addButton().click();
      });
      const [, value] = inputs();
      if (!value) throw new Error("no new line");
      act(() => {
        value.focus();
      });
      type(value, "1");
      if (end === "blur")
        act(() => {
          value.blur();
        });
      else key(value, end);
      expect(edits, end).toEqual([["env_vars.add", { name: "", value: "1" }]]);
      expect(closed, end).toBe(0);
    }
  });

  it("marks a saved row with a value and no name", () => {
    show(envRow([{ name: "", value: "1", problem: "Add a name, or remove the row" }]));
    const [name] = inputs();
    expect(name?.getAttribute("aria-invalid")).toBe("true");
    expect(box.textContent).toContain("Add a name, or remove the row");
  });

  it("saves an edited field on ↵ and on Esc, which closes Settings only the second time", () => {
    show(envRow([{ name: "NODE_OPTIONS", value: "--max-old-space-size=4096" }]));
    const [, value] = inputs();
    if (!value) throw new Error("no row");
    type(value, "--max-old-space-size=6144");
    key(value, "Enter");
    expect(edits).toEqual([["env_vars.0.value", "--max-old-space-size=6144"]]);
    edits.length = 0;
    show(envRow([{ name: "NODE_OPTIONS", value: "--max-old-space-size=6144" }]));
    type(value, "--inspect");
    key(value, "Escape");
    expect(edits).toEqual([["env_vars.0.value", "--inspect"]]);
    expect(closed).toBe(0);
    // Saved (the answer may still be on its way): Esc closes Settings as usual, and sends nothing again.
    key(value, "Escape");
    expect(closed).toBe(1);
    expect(edits).toHaveLength(1);
    act(() => {
      value.blur();
    });
    expect(edits).toHaveLength(1);
  });

  it("marks a row with a problem and removes a row with ×", () => {
    show(
      envRow([
        { name: "NODE_OPTIONS", value: "a" },
        { name: "MAX=JOBS", value: "2", problem: "Remove “=” from the name" },
      ]),
    );
    const [first, , second] = inputs();
    expect(first?.getAttribute("aria-invalid")).toBeNull();
    expect(second?.getAttribute("aria-invalid")).toBe("true");
    expect(box.textContent).toContain("Remove “=” from the name");
    const remove = box.querySelector<HTMLButtonElement>('button[aria-label="Remove NODE_OPTIONS"]');
    act(() => {
      remove?.click();
    });
    expect(edits).toEqual([["env_vars.remove", 0]]);
  });
});
