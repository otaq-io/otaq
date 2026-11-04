// @vitest-environment happy-dom
// The warm build switch's own logic (T-0301, T-0307): turning it off asks only when the row carries a confirmation,
// turning it on never asks, it works where the disk can't clone (the build is handed over), and a disabled switch
// can't be clicked.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import type { CleanupWarmConfirm } from "../../generated/CleanupWarmConfirm";
import type { SettingsRow } from "../../generated/SettingsRow";
import type { SettingsValue } from "../../generated/SettingsValue";
import { SettingsContext, type SettingsContextValue } from "./context";
import { WarmBuildRow } from "./Rows";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const confirm: CleanupWarmConfirm = {
  title: "Clear shop-web’s warm build (up to 4.2 GB)?",
  text: "Turning this off deletes shop-web’s warm build.",
  shared: "Tickets copied from it share part of this space, so it may free less.",
  line: { icon: "flame", text: "~/.data/warm/shop-web · up to 4.2 GB (delete)" },
  confirm: "Turn off and clear up to 4.2 GB",
};

// The shape `settings_page.rs` sends (`warm_builds_row`).
function warmRow(
  on: boolean,
  extra: { confirm?: CleanupWarmConfirm; reason?: string; clearing?: string; description?: string } = {},
): SettingsRow {
  return {
    field: "warm_builds",
    label: "Start new worktrees from a warm build",
    description: extra.description ?? "New tickets start from the last finished ticket’s build.",
    control: {
      kind: "warm_build",
      on,
      disabled: extra.reason !== undefined,
      reason: extra.reason ?? null,
      clearing: extra.clearing ?? null,
      confirm: extra.confirm ?? null,
    },
    dimmed: false,
    reset: null,
    button: null,
    set_by: null,
  };
}

let root: Root;
let box: HTMLElement;
let edits: [string, SettingsValue][];

function show(row: SettingsRow) {
  const value: SettingsContextValue = {
    page: { page: "project", key: "SHOP", title: "shop-web", intro: "", sections: [] },
    project: "SHOP",
    edit: (field, v) => {
      edits.push([field, v]);
    },
    act: () => undefined,
  };
  if (row.control.kind !== "warm_build") throw new Error("not the switch");
  const control = row.control;
  act(() => {
    root.render(
      createElement(SettingsContext.Provider, { value }, createElement(WarmBuildRow, { row, control })),
    );
  });
}

const toggle = () => box.querySelector<HTMLButtonElement>("[role=switch]");
const dialog = () => document.querySelector(".dialog");

function click(target: Element | null) {
  act(() => {
    (target as HTMLElement).click();
  });
}

function button(label: string) {
  return [...document.querySelectorAll<HTMLButtonElement>(".dialog button")].find((b) =>
    b.textContent.includes(label),
  );
}

beforeEach(() => {
  box = document.createElement("div");
  document.body.append(box);
  root = createRoot(box);
  edits = [];
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  box.remove();
});

describe("the warm build switch", () => {
  it("turns off at once when there's no warm build to delete", () => {
    show(warmRow(true));
    click(toggle());
    expect(dialog()).toBeNull();
    expect(edits).toEqual([["warm_builds", false]]);
  });

  it("asks first when turning it off deletes a warm build; Cancel leaves it on", () => {
    show(warmRow(true, { confirm }));
    click(toggle());
    expect(edits).toEqual([]);
    expect(dialog()?.textContent).toContain(confirm.title);
    expect(dialog()?.textContent).toContain(confirm.shared);
    click(button("Cancel") ?? null);
    expect(dialog()).toBeNull();
    expect(edits).toEqual([]);

    click(toggle());
    click(button(confirm.confirm) ?? null);
    expect(dialog()).toBeNull();
    expect(edits).toEqual([["warm_builds", false]]);
  });

  it("never asks when turning it on", () => {
    show(warmRow(false));
    click(toggle());
    expect(dialog()).toBeNull();
    expect(edits).toEqual([["warm_builds", true]]);
  });

  it("shows the clearing line while the Clear waits for a copy", () => {
    show(warmRow(false, { clearing: "Clearing… (waiting for SHOP-21’s copy)" }));
    expect(box.textContent).toContain("Clearing… (waiting for SHOP-21’s copy)");
  });

  it("works where the disk can't clone, saying the build is handed over", () => {
    const description = "This disk can’t copy-on-write; the build is handed over to the next ticket.";
    show(warmRow(true, { description }));
    expect(toggle()?.disabled).toBe(false);
    expect(box.textContent).toContain(description);
    click(toggle());
    expect(edits).toEqual([["warm_builds", false]]);
  });

  it("can't be clicked when disabled, with the reason under it", () => {
    const reason = "Unavailable on this disk.";
    show(warmRow(false, { reason }));
    expect(toggle()?.disabled).toBe(true);
    expect(toggle()?.getAttribute("aria-checked")).toBe("false");
    expect(box.textContent).toContain(reason);
    click(toggle());
    expect(edits).toEqual([]);
  });
});
