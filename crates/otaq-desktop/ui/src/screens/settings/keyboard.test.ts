// @vitest-environment happy-dom
// Settings → Keyboard shortcuts (T-0395): a click makes the cell record, the next key is sent as pressed and saved,
// `g` waits for its letter, Esc cancels, Remove key takes the key away, a taken key asks in place and Use here
// moves it, Reset all asks first, the switch turns letters off, and a refusal keeps the cell recording.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import type { KeyEdit } from "../../generated/KeyEdit";
import type { KeyEditAnswer } from "../../generated/KeyEditAnswer";
import type { ShortcutsKey } from "../../generated/ShortcutsKey";
import type { ShortcutsPage } from "../../generated/ShortcutsPage";
import type { ShortcutsRecordAnswer } from "../../generated/ShortcutsRecordAnswer";
import type { ShortcutsRow } from "../../generated/ShortcutsRow";
import type { ShortcutsSlot } from "../../generated/ShortcutsSlot";
import { inKeyRecorder, KEY_RECORDER } from "../../app/keys";
import { type ShortcutsCalls, ShortcutsView } from "./Keyboard";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const slot = (action: ShortcutsSlot["action"], n: number): ShortcutsSlot => ({ action, slot: n });
const key = (s: ShortcutsSlot, chips: string[], changed = false): ShortcutsKey => ({
  slot: s,
  chips,
  changed,
  dimmed: false,
});
const row = (
  id: string,
  title: string,
  keys: ShortcutsKey[],
  extra: Partial<ShortcutsRow> = {},
): ShortcutsRow => ({
  id,
  title,
  default_line: "Default: …",
  changed: false,
  dimmed: false,
  cells: keys.map((k) => ({ kind: "keys", halves: [k] })),
  reset: null,
  terminals: null,
  ...extra,
});

// The shape `keyboard_page.rs` sends, a few rows of it.
function page(changed = 0): ShortcutsPage {
  return {
    title: "Keyboard shortcuts",
    intro: "Every action and its keys.",
    reset_all: {
      label: "Reset all",
      enabled: changed > 0,
      question: `Reset all ${String(changed)} changed shortcuts to their defaults?`,
      body: "Activity, New ticket and Next item that needs you go back to their default keys.",
      cancel: "Cancel",
    },
    letters: { label: "Single-letter shortcuts", description: "Keys like j, k, / and g then b.", on: true },
    terminals_note: null,
    terminals_column: false,
    groups: [
      {
        title: "Moving around",
        rows: [
          row("next_needs_you_item", "Next item that needs you", [
            key(slot({ action: "next_needs_you_item" }, 0), ["⌘J"]),
          ]),
        ],
      },
      {
        title: "Views",
        rows: [
          row("open_activity", "Activity", [
            key(slot({ action: "open_activity" }, 0), ["⇧⌘U"]),
            key(slot({ action: "open_activity" }, 1), ["g", "a"]),
          ]),
        ],
      },
      {
        title: "The ticket",
        rows: [
          row("new_ticket", "New ticket", [
            key(slot({ action: "new_ticket" }, 0), ["⌘N"]),
            key(slot({ action: "new_ticket" }, 1), ["c"]),
          ]),
          row("park", "Park", [key(slot({ action: "park" }, 0), [])]),
        ],
      },
    ],
    texts: {
      or: "or",
      then: "then",
      no_key: "no key",
      recording: "Press the new keys…",
      after_g: "g, then a letter…",
      recording_hint: "Esc cancels · Remove key takes this key away",
      recording_hint_no_key: "Esc cancels",
      remove_key: "Remove key",
      reset: "Reset to default",
      works_in_terminals: "Works inside terminals",
      use_here: "Use here",
      cancel: "Cancel",
      let_through: "Let it through",
    },
  };
}

let root: Root;
let box: HTMLElement;
let sent: unknown[];
let recordAnswers: ShortcutsRecordAnswer[];
let editAnswers: KeyEditAnswer[];

const done: KeyEditAnswer = { outcome: "done", changed: 1 };

const calls: ShortcutsCalls = {
  record: (s, press, afterG) => {
    sent.push({ record: s, key: press.key, meta: press.modifiers.platform, afterG });
    return Promise.resolve(recordAnswers.shift() ?? { outcome: "ignored" });
  },
  edit: (edit: KeyEdit) => {
    sent.push(edit);
    return Promise.resolve(editAnswers.shift() ?? done);
  },
  resetRow: (keys, confirmed) => {
    sent.push({ reset: keys, confirmed });
    return Promise.resolve(editAnswers.shift() ?? done);
  },
};

function show(p: ShortcutsPage = page()) {
  act(() => {
    root.render(createElement(ShortcutsView, { page: p, calls }));
  });
}

const rowOf = (id: string) => {
  const found = box.querySelector<HTMLElement>(`[data-row="${id}"]`);
  if (!found) throw new Error(`no row ${id}`);
  return found;
};
const cells = (id: string) => [...rowOf(id).querySelectorAll<HTMLButtonElement>(".shortcut-cell")];
const recorder = () => box.querySelector<HTMLButtonElement>(KEY_RECORDER);
const buttonNamed = (name: string) => {
  const found = [...document.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent === name || b.getAttribute("aria-label") === name,
  );
  if (!found) throw new Error(`no button ${name}`);
  return found;
};

async function click(button: HTMLElement) {
  await act(async () => {
    button.click();
    await Promise.resolve();
  });
}

async function pressKey(init: KeyboardEventInit) {
  const target = recorder();
  if (!target) throw new Error("nothing records");
  await act(async () => {
    target.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
    await Promise.resolve();
    await Promise.resolve();
  });
}

beforeEach(() => {
  sent = [];
  recordAnswers = [];
  editAnswers = [];
  box = document.createElement("div");
  document.body.append(box);
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  box.remove();
});

describe("recording a key", () => {
  it("records in the clicked cell only, sends the key as pressed and stops once saved", async () => {
    show();
    const [, letter] = cells("new_ticket");
    if (!letter) throw new Error("no letter cell");
    await click(letter);
    expect(recorder()?.textContent).toBe("Press the new keys…");
    expect(document.activeElement).toBe(recorder());
    // The dispatcher leaves every key to it.
    expect(inKeyRecorder(document.activeElement as HTMLElement)).toBe(true);
    // The other key stays drawn.
    expect(cells("new_ticket")[0]?.textContent).toBe("⌘N");
    expect(rowOf("new_ticket").textContent).toContain("Esc cancels · Remove key takes this key away");
    recordAnswers.push({ outcome: "edited", key: "n", answer: done });
    const event = new KeyboardEvent("keydown", { key: "n", code: "KeyN", bubbles: true, cancelable: true });
    await act(async () => {
      recorder()?.dispatchEvent(event);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(event.defaultPrevented).toBe(true);
    expect(sent).toEqual([
      { record: slot({ action: "new_ticket" }, 1), key: "n", meta: false, afterG: false },
    ]);
    expect(recorder()).toBeNull();
  });

  it("waits for the letter after g", async () => {
    show();
    await click(cells("park")[0] as HTMLElement);
    expect(rowOf("park").textContent).toContain("Esc cancels");
    expect(rowOf("park").textContent).not.toContain("Remove key");
    recordAnswers.push({ outcome: "then_letter" });
    await pressKey({ key: "g", code: "KeyG" });
    expect(recorder()?.textContent).toBe("g, then a letter…");
    recordAnswers.push({ outcome: "edited", key: "g p", answer: done });
    await pressKey({ key: "p", code: "KeyP" });
    expect(sent.at(-1)).toEqual({ record: slot({ action: "park" }, 0), key: "p", meta: false, afterG: true });
    expect(recorder()).toBeNull();
  });

  it("cancels on Esc, which doesn't reach Settings", async () => {
    show();
    let escaped = false;
    box.addEventListener("keydown", () => {
      escaped = true;
    });
    await click(cells("open_activity")[0] as HTMLElement);
    await pressKey({ key: "Escape", code: "Escape" });
    expect(recorder()).toBeNull();
    expect(sent).toEqual([]);
    expect(escaped).toBe(false);
  });

  it("Remove key takes the recording key away", async () => {
    show();
    await click(cells("next_needs_you_item")[0] as HTMLElement);
    await click(buttonNamed("Remove key"));
    expect(sent).toEqual([{ edit: "remove", action: { action: "next_needs_you_item" }, slot: 0 }]);
    expect(recorder()).toBeNull();
  });

  it("keeps recording after a refusal, with the reason under the row", async () => {
    show();
    await click(cells("open_activity")[0] as HTMLElement);
    recordAnswers.push({
      outcome: "edited",
      key: "⌘Q",
      answer: { outcome: "refused", reason: "⌘Q quits Otaq. Press another key." },
    });
    await pressKey({ key: "q", code: "KeyQ", metaKey: true });
    expect(rowOf("open_activity").querySelector('[data-tone="refused"]')?.textContent).toBe(
      "⌘Q quits Otaq. Press another key.",
    );
    expect(recorder()?.textContent).toBe("Press the new keys…");
  });
});

describe("a key another action has", () => {
  it("asks in place and Use here moves it", async () => {
    show();
    await click(cells("open_activity")[0] as HTMLElement);
    recordAnswers.push({
      outcome: "edited",
      key: "⌘J",
      answer: {
        outcome: "taken",
        key: "⌘J",
        by: { action: "next_needs_you_item" },
        question: "⌘J is used by Next item that needs you. Use it here instead?",
      },
    });
    await pressKey({ key: "j", code: "KeyJ", metaKey: true });
    const ask = rowOf("open_activity").querySelector('[data-tone="ask"]');
    expect(ask?.textContent).toContain("⌘J is used by Next item that needs you. Use it here instead?");
    // The new key waits in its cell, tinted, with the default under the name.
    expect(cells("open_activity")[0]?.textContent).toBe("⌘J");
    expect(cells("open_activity")[0]?.hasAttribute("data-changed")).toBe(true);
    expect(rowOf("open_activity").textContent).toContain("Default: …");
    await click(buttonNamed("Use here"));
    expect(sent.at(-1)).toEqual({
      edit: "record",
      action: { action: "open_activity" },
      slot: 0,
      key: "⌘J",
      confirmed: true,
    });
    expect(rowOf("open_activity").querySelector(".shortcut-answer")).toBeNull();
  });

  it("Cancel keeps both as they were", async () => {
    show();
    await click(cells("open_activity")[0] as HTMLElement);
    recordAnswers.push({
      outcome: "edited",
      key: "⌘J",
      answer: { outcome: "taken", key: "⌘J", by: { action: "next_needs_you_item" }, question: "⌘J is used…" },
    });
    await pressKey({ key: "j", code: "KeyJ", metaKey: true });
    await click(buttonNamed("Cancel"));
    expect(rowOf("open_activity").querySelector(".shortcut-answer")).toBeNull();
    expect(cells("open_activity")[0]?.textContent).toBe("⇧⌘U");
    expect(sent).toHaveLength(1);
  });
});

describe("the page", () => {
  it("Reset all is off while nothing changed, and asks before it resets", async () => {
    show();
    expect(buttonNamed("Reset all").disabled).toBe(true);
    show(page(3));
    await click(buttonNamed("Reset all"));
    expect(document.body.textContent).toContain("Reset all 3 changed shortcuts to their defaults?");
    expect(sent).toEqual([]);
    const confirm = [...document.querySelectorAll<HTMLButtonElement>(".dialog button")].find(
      (b) => b.textContent === "Reset all",
    );
    if (!confirm) throw new Error("no confirm button");
    await click(confirm);
    expect(sent).toEqual([{ edit: "reset_all" }]);
  });

  it("a row's reset resets its changed keys without asking", async () => {
    const p = page(1);
    const needsYou = p.groups[0]?.rows[0];
    if (!needsYou) throw new Error("no row");
    needsYou.changed = true;
    needsYou.reset = [slot({ action: "next_needs_you_item" }, 0)];
    show(p);
    await click(buttonNamed("Reset to default"));
    expect(sent).toEqual([{ reset: [slot({ action: "next_needs_you_item" }, 0)], confirmed: false }]);
  });

  it("the switch turns single letters off", async () => {
    show();
    await click(box.querySelector<HTMLElement>('[role="switch"]') as HTMLElement);
    expect(sent).toEqual([{ edit: "letters", on: false }]);
  });

  it("Works inside terminals warns first, then Let it through sends it again", async () => {
    const p = page();
    const activity = p.groups[1]?.rows[0];
    if (!activity) throw new Error("no row");
    activity.terminals = {
      slot: slot({ action: "open_activity" }, 0),
      on: false,
      fixed: false,
      in_terminals: "· in terminals Ctrl+Shift+R",
    };
    show(p);
    expect(rowOf("open_activity").textContent).toContain("· in terminals Ctrl+Shift+R");
    editAnswers.push({
      outcome: "warn",
      warning: "Ctrl+R: the shell’s history search won’t reach the terminal.",
    });
    await click(rowOf("open_activity").querySelector<HTMLElement>('input[type="checkbox"]') as HTMLElement);
    expect(rowOf("open_activity").querySelector('[data-tone="warn"]')?.textContent).toContain("Ctrl+R:");
    await click(buttonNamed("Let it through"));
    expect(sent.at(-1)).toEqual({
      edit: "works_in_terminals",
      action: { action: "open_activity" },
      slot: 0,
      on: true,
      let_through: true,
      confirmed: false,
    });
  });
});

describe("its classes", () => {
  // Every stylesheet is loaded at once in the app: the ? list's `.shortcuts { width: 920px }` once landed on this
  // page's section and made Settings scroll sideways. `shortcuts…` is the ? list's; this page's are `shortcut-…`.
  it("never use the ? list's shortcuts… classes", () => {
    const p = page();
    const activity = p.groups[1]?.rows[0];
    if (!activity) throw new Error("no row");
    activity.terminals = {
      slot: slot({ action: "open_activity" }, 0),
      on: true,
      fixed: false,
      in_terminals: null,
    };
    show({ ...p, terminals_note: "A shortcut that works inside terminals…", terminals_column: true });
    const classes = [...box.querySelectorAll("[class]")].flatMap((e) => [...e.classList]);
    expect(classes).toContain("shortcut-settings");
    expect(classes.filter((c) => c.startsWith("shortcuts"))).toEqual([]);
  });
});
