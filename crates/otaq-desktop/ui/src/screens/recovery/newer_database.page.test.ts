// @vitest-environment happy-dom
// A board from a newer Otaq (*32 → Item 7*, UPD-08): the message with its buttons, the confirmation (Restore ·
// Cancel, Esc cancels), the three steps while it restores, and a restore that fails at each step, against stand-in
// actions instead of the service.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { NewerDatabaseMessage } from "../../generated/NewerDatabaseMessage";
import type { NewerDatabaseScreen } from "../../generated/NewerDatabaseScreen";
import type { RestoringView } from "../../generated/RestoringView";
import type { StepState } from "../../generated/StepState";
import { type NewerDatabaseActions, NewerDatabasePage } from "./NewerDatabase";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const MESSAGE: NewerDatabaseMessage = {
  icon: "database",
  title: "This board was last opened by Otaq 0.2.0",
  body: "You’re on 0.1.0, which can’t open it. Restore the copy Otaq made before 0.2.0 changed it, or reinstall 0.2.0.",
  restore: "Restore the copy from before 0.2.0",
  download: "Download 0.2.0",
  download_url: "https://example.invalid/releases/tag/v0.2.0",
  try_again: "Try again",
  terminal: { lead: "or in a terminal:", command: "otaq restore-database" },
  confirm: {
    title: "Restore the board from before 0.2.0?",
    what: "The board goes back to how it was before Otaq 0.2.0 changed it, on Oct 24 at 09:12.",
    untouched: "Worktrees, branches and your repos aren’t touched.",
    restore: "Restore",
    cancel: "Cancel",
  },
  failure: null,
};

const message = (extra: Partial<NewerDatabaseMessage> = {}): NewerDatabaseScreen => ({
  kind: "message",
  ...MESSAGE,
  ...extra,
});

function restoring(states: [StepState, StepState, StepState]): NewerDatabaseScreen {
  const view: RestoringView = {
    icon: "history",
    title: "Restoring the board…",
    note: "This takes a few seconds; the board opens when it’s done.",
    steps: [
      { label: "Background service stopped", state: states[0] },
      { label: "Putting back the copy from Oct 24, 09:12", state: states[1] },
      { label: "Starting the background service", state: states[2] },
    ],
  };
  return { kind: "restoring", ...view };
}

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.innerHTML = "";
});

function stand_in(fail = false): NewerDatabaseActions & { calls: string[] } {
  const calls: string[] = [];
  const answer = (name: string) => () => {
    calls.push(name);
    return fail ? Promise.reject(new Error(name)) : Promise.resolve(null);
  };
  return { calls, restore: answer("restore"), download: answer("download"), tryAgain: answer("try_again") };
}

function show(screen: NewerDatabaseScreen, actions: NewerDatabaseActions) {
  act(() => {
    root.render(createElement(NewerDatabasePage, { screen, actions }));
  });
}

function button(label: string): HTMLButtonElement {
  const found = [...document.querySelectorAll("button")].find((b) => b.textContent === label);
  if (!found) throw new Error(`no button ${label}: ${document.body.innerHTML}`);
  return found;
}

async function click(label: string) {
  await act(async () => {
    button(label).click();
    await Promise.resolve();
  });
}

describe("the message", () => {
  it("shows the title, the three buttons and the command", () => {
    show(message(), stand_in());
    const text = document.body.textContent;
    for (const line of [MESSAGE.title, MESSAGE.body, "Download 0.2.0", "Try again", "or in a terminal:"]) {
      expect(text).toContain(line);
    }
    expect(document.querySelector(".newer-db-terminal code")?.textContent).toBe("otaq restore-database");
    expect(button("Restore the copy from before 0.2.0").dataset.variant).toBe("primary");
    expect(document.querySelector("[role=alert]")).toBeNull();
  });

  it("without a copy has no Restore and no command", () => {
    show(
      message({ restore: null, terminal: null, confirm: null, body: "… so reinstall 0.2.0 to open it." }),
      stand_in(),
    );
    expect(document.body.textContent).not.toContain("Restore the copy");
    expect(document.querySelector(".newer-db-terminal")).toBeNull();
    expect(document.body.textContent).toContain("reinstall 0.2.0");
  });

  it("Download and Try again ask the service", async () => {
    const actions = stand_in();
    show(message(), actions);
    await click("Download 0.2.0");
    await click("Try again");
    expect(actions.calls).toEqual(["download", "try_again"]);
  });
});

describe("the confirmation", () => {
  it("opens on Restore, and Cancel or Esc closes it without restoring", async () => {
    const actions = stand_in();
    show(message(), actions);
    await click("Restore the copy from before 0.2.0");
    expect(document.querySelector(".dialog-title")?.textContent).toBe("Restore the board from before 0.2.0?");
    expect(document.body.textContent).toContain("on Oct 24 at 09:12");
    expect(document.body.textContent).toContain("aren’t touched");
    await click("Cancel");
    expect(document.querySelector(".dialog")).toBeNull();

    await click("Restore the copy from before 0.2.0");
    await act(async () => {
      document
        .querySelector(".dialog")
        ?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      await Promise.resolve();
    });
    expect(document.querySelector(".dialog")).toBeNull();
    expect(actions.calls).toEqual([]);
  });

  it("Restore restores, and a refused restore leaves the buttons usable", async () => {
    const actions = stand_in(true);
    show(message(), actions);
    await click("Restore the copy from before 0.2.0");
    await click("Restore");
    expect(actions.calls).toEqual(["restore"]);
    expect(document.querySelector(".dialog")).toBeNull();
    await act(async () => {
      await Promise.resolve();
    });
    expect(button("Try again").disabled).toBe(false);
  });
});

describe("the restore", () => {
  it("shows the three steps as they go", () => {
    show(restoring(["done", "current", "waiting"]), stand_in());
    expect(document.querySelector(".newer-db-title")?.textContent).toBe("Restoring the board…");
    const rows = [...document.querySelectorAll(".newer-db-steps li")];
    expect(rows.map((r) => r.getAttribute("data-state"))).toEqual(["done", "current", "waiting"]);
    expect(rows[1]?.hasAttribute("data-loading")).toBe(true);
    expect(rows[1]?.textContent).toBe("Putting back the copy from Oct 24, 09:12");
    expect(document.querySelectorAll("button")).toHaveLength(0);
  });

  // What Rust says when each step fails (`store::restore`): the message comes back with it, and Restore again.
  it.each([
    ["stopping", "The background service couldn't be stopped: it didn't stop in time. Nothing was changed."],
    [
      "putting back",
      "The copy /h/otaq.db.before-0.2.0 couldn't be put back: denied. The database it replaces was moved to " +
        "/h/otaq.db.replaced-20261027-091200 (move it back to /h/otaq.db to undo). The background service is stopped.",
    ],
    ["starting", "The background service didn't start: see the logs. The copy is back as /h/otaq.db."],
  ])("a restore that fails while %s says why and stays on the screen", (_step, why) => {
    const actions = stand_in();
    show(restoring(["current", "waiting", "waiting"]), actions);
    show(message({ failure: `Couldn’t restore the board: ${why}` }), actions);
    expect(document.querySelector("[role=alert]")?.textContent).toBe(`Couldn’t restore the board: ${why}`);
    expect(button("Restore the copy from before 0.2.0").disabled).toBe(false);
    expect(document.querySelector(".newer-db-steps")).toBeNull();
  });
});
