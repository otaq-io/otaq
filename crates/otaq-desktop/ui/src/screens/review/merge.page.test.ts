// @vitest-environment happy-dom
// ⌘↵ in the Local merge dialog (dogfood #60, Q471), through the page's real key dispatcher: the message has the
// keyboard when the confirmation shows (opened directly, or after the choice), and ⌘↵ merges from it and from anywhere
// else in the dialog; ↵ in the message is a new line. With one way to deliver the choice is skipped (dogfood #18).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";

import { installKeys, keymap } from "../../app/keyboard";
import { overlays } from "../../app/overlays";
import { createRegistries } from "../../app/registry";
import type { ApproveView } from "../../generated/ApproveView";
import type { Keymap } from "../../generated/Keymap";
import type { KeyModifiers } from "../../generated/KeyModifiers";
import { request } from "../../tauri";
import { review } from ".";
import { LocalMergeDialog } from "./Dialogs";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const NONE: KeyModifiers = { control: false, alt: false, shift: false, platform: false, function: false };
const mac = /Mac|iP(hone|[oa]d)/.test(navigator.platform);
// The map's Approve, ⌘↵ / Ctrl+↵: what the dispatcher would run outside the dialog.
const SUBMIT: KeyboardEventInit = mac ? { metaKey: true } : { ctrlKey: true };

const KEYMAP: Keymap = {
  platform: mac ? "mac" : "win_linux",
  letters: true,
  log: false,
  typing_hint: "Typing? Click where you want to type",
  actions: [],
  terminal_keys: [],
  terminal_app_modifiers: [],
  bindings: [
    {
      action: { action: "approve" },
      slot: 0,
      text: "⌘↵",
      key: {
        kind: "stroke",
        stroke: {
          key: "Enter",
          code: "Enter",
          modifiers: mac ? { ...NONE, platform: true } : { ...NONE, control: true },
        },
      },
      place: "outside_terminals",
      custom: false,
    },
  ],
  sequences: [],
};

const LOCAL_MERGE = {
  choice: "local_merge",
  number: 1,
  icon: "git-merge",
  title: "Local merge",
  detail: "Squash into main",
  enabled: true,
} as const;

const VIEW: ApproveView = {
  key: "SHOP-18",
  menu: {
    key: "SHOP-18",
    title: "Approve SHOP-18 Checkout API",
    items: [
      LOCAL_MERGE,
      {
        choice: "open_pr",
        number: 2,
        icon: "git-pull-request",
        title: "Open PR on GitHub",
        detail: "into main",
        enabled: true,
      },
    ],
    note: "Both open a confirmation step before anything runs.",
    direct: null,
  },
  push_to_pr: false,
  confirm: {
    title: "Local merge SHOP-18 Checkout API",
    into: "Into (SHOP-18’s base branch)",
    target: "main",
    repo: "~/code/shop-web",
    locked: false,
    message_label: "Squash commit message",
    message: "SHOP-18: Checkout API",
    note: "The worktree for SHOP-18 is removed unless it has uncommitted changes.",
    conflict: null,
    // The link button sits above the message: it took the focus before (dogfood #60).
    merge_check: { text: "No merge check · ", link: "set one in project settings", project: "SHOP" },
    manual_checks: null,
    merge: "Squash & merge",
    cancel: "Cancel",
    hint: "cancel · ⌘↵ merge",
  },
  merging: {
    title: "Local merge SHOP-18 Checkout API",
    steps: [],
    label: "Merging…",
    note: "",
    close: "Close",
  },
  changed: null,
};

let root: Root | null = null;
let host: HTMLElement | null = null;
let stop: (() => void) | null = null;
let closed = 0;
const asked = vi.mocked(request);

/** The service's answers: the dialog's view, a merge that went through; the test sets the keyboard map itself. */
function answer(view: ApproveView) {
  asked.mockImplementation((method: string) => {
    if (method === "review.approve") return Promise.resolve(view);
    if (method === "review.local_merge") return Promise.resolve({ kind: "merged", toasts: [] });
    return new Promise(() => undefined);
  });
}

beforeEach(() => {
  answer(VIEW);
  const app = createRegistries();
  // The review screen's own ⌘↵ (Approve) is registered, as in the app.
  review(app);
  stop = installKeys(app);
  keymap.set(KEYMAP);
  closed = 0;
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
});

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  stop?.();
  overlays.store.set([]);
  asked.mockReset();
});

/** Opens the dialog over `view` as the app does (a modal overlay) and waits for its view. */
async function open(view: ApproveView, menu: boolean): Promise<void> {
  answer(view);
  overlays.open("local_merge", { ticket: "SHOP-18", menu });
  await act(async () => {
    root?.render(
      createElement(LocalMergeDialog, {
        params: { ticket: "SHOP-18", menu },
        close: () => {
          closed += 1;
        },
      }),
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

function press(key: string, mods: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key, code: key, bubbles: true, cancelable: true, ...mods });
  act(() => {
    (document.activeElement ?? document.body).dispatchEvent(event);
  });
  return event;
}

const merges = () => asked.mock.calls.filter(([method]) => method === "review.local_merge");
const message = () => host?.querySelector<HTMLTextAreaElement>("#review-message") ?? null;

test("opened directly, the message has the keyboard and ⌘↵ in it merges", async () => {
  await open(VIEW, false);
  expect(message()).not.toBeNull();
  expect(document.activeElement).toBe(message());
  // ↵ alone is a new line in the message: nothing is merged.
  expect(press("Enter").defaultPrevented).toBe(false);
  expect(merges()).toEqual([]);
  expect(press("Enter", SUBMIT).defaultPrevented).toBe(true);
  expect(merges()).toEqual([
    ["review.local_merge", { key: "SHOP-18", message: "SHOP-18: Checkout API", approve_anyway: false }],
  ]);
});

test("⌘↵ merges from anywhere in the dialog, the merge-check link too", async () => {
  await open(VIEW, false);
  const link = host?.querySelector<HTMLButtonElement>(".review-link");
  act(() => link?.focus());
  expect(document.activeElement).toBe(link);
  press("Enter", SUBMIT);
  expect(merges()).toHaveLength(1);
});

test("after the choice, the message has the keyboard again and ⌘↵ merges", async () => {
  await open(VIEW, true);
  expect(message()).toBeNull();
  press("1");
  expect(message()).not.toBeNull();
  expect(document.activeElement).toBe(message());
  press("Enter", SUBMIT);
  expect(merges()).toHaveLength(1);
});

test("an empty message doesn't merge", async () => {
  await open({ ...VIEW, confirm: { ...VIEW.confirm, message: "  " } }, false);
  press("Enter", SUBMIT);
  expect(merges()).toEqual([]);
  expect(closed).toBe(0);
});

test("with one way to deliver, Approve opens the confirmation without the choice (dogfood #18)", async () => {
  await open({ ...VIEW, menu: { ...VIEW.menu, items: [LOCAL_MERGE], direct: "local_merge" } }, true);
  expect(message()).not.toBeNull();
  expect(host?.textContent).not.toContain("Both open a confirmation step");
});
