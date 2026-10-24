// @vitest-environment happy-dom
// Overview (SHT-03, RPT-02; T-0465): the description edited in place (Save ⌘S sends only the description, Esc
// discards, leaving with changes asks first), the tab row's bar, and the Attachments column with Add.
import type { Editor } from "@tiptap/core";
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { overlays } from "../../app/overlays";
import { closeTicket, NOTHING_SELECTED, openTicket, selection, showTab } from "../../app/selection";
import type { EditorView } from "../../generated/EditorView";
import type { OverviewView } from "../../generated/OverviewView";
import { attachRequests } from "../editor/CommentThread";
import { FORM, READ, VIEW } from "../editor/editor_fixtures";
import { DescriptionSection } from "./Description";
import { asksFirst, descriptionDraft, descriptionState, isSaveDescriptionKey } from "./draft";
import { Attachments } from "./Overview";
import { DescriptionSaveBar } from "./SaveBar";

type Reply = (method: string, params: unknown) => unknown;
const ipc = vi.hoisted((): { reply: Reply; calls: [string, unknown][] } => ({
  reply: () => null,
  calls: [],
}));
vi.mock(import("../../tauri"), async (original) => ({
  ...(await original()),
  request: (method: string, params: unknown) => {
    ipc.calls.push([method, params]);
    return Promise.resolve(ipc.reply(method, params));
  },
  pickReportFile: () => Promise.resolve("/Users/me/perf-notes.md"),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const noop = () => undefined;
let unmount: () => void = noop;

afterEach(() => {
  unmount();
  ipc.reply = () => null;
  ipc.calls = [];
  selection.set(NOTHING_SELECTED);
  overlays.store.set([]);
  descriptionDraft.set(null);
  attachRequests.set(null);
});

const KEY = "SHOP-12";
const EDITOR: EditorView = {
  ...VIEW,
  create: false,
  key: KEY,
  form: { ...FORM, title: "Dark mode", body: READ.body_markdown, priority: 3 },
  read: { ...READ, marked: [] },
};

/** `overview.view` for SHOP-12, as Rust words it (`view_models/overview.rs`). */
const OVERVIEW: OverviewView = {
  key: KEY,
  description: "Description",
  click_to_edit: "Click to edit",
  editing: "Editing · Esc to discard, ⌘S to save",
  placeholder: "Type / for blocks",
  bar: { changed: "Description changed", discard: "Discard", save: "Save", keys: "⌘S" },
  leave: {
    title: "Discard your description changes?",
    body: "SHOP-12’s description has changes you haven’t saved.",
    discard: "Discard changes",
    keep: "Keep editing",
  },
  usage_limit: null,
  manual_checks: null,
  attachments_heading: "Attachments · 2",
  attachments: [
    {
      n: 4,
      name: "Report",
      size: "3 KB",
      mark: { kind: "agent" },
      who: "Rust developer · Dev",
      mine: false,
      line: "Fails 2 in 20 on Safari: the middleware reads the cookie too early.",
    },
    {
      n: 1,
      name: "Report",
      size: "1 KB",
      mark: { kind: "you" },
      who: "You",
      mine: true,
      line: "Steps I used",
    },
  ],
  add: "Add",
  pick_title: "Attach a report",
  drop: "Drop files here or paste",
  note: "Markdown reports, yours and the agents’ (a test run summary, findings, notes), each with the comment it came with. Agents open reports when they need them; yours are marked from you and read first.",
};

const MAC = navigator.userAgent.includes("Mac");

function mount() {
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  act(() => {
    root.render(
      createElement(
        "div",
        null,
        createElement("div", { className: "sheet-tabs" }, createElement(DescriptionSaveBar, { ticket: KEY })),
        createElement(DescriptionSection, {
          ticket: KEY,
          view: EDITOR,
          overview: OVERVIEW,
          overviewTabs: ["overview"],
        }),
      ),
    );
  });
  unmount = () => {
    act(() => {
      root.unmount();
    });
    host.remove();
    unmount = noop;
  };
  return host;
}

/** Clicks the description as it reads: the editor opens. */
function clickToEdit(page: HTMLElement): Editor {
  act(() => {
    page.querySelector<HTMLElement>(".overview-read")?.click();
  });
  const dom = page.querySelector<HTMLElement & { editor?: Editor }>(".overview-edit .ProseMirror");
  if (!dom?.editor) throw new Error("the editor didn't open");
  return dom.editor;
}

function typeAtEnd(editor: Editor, text: string) {
  act(() => {
    editor.chain().focus("end").insertContent(text).run();
    editor.view.focus();
  });
}

function press(
  key: string,
  mods: KeyboardEventInit = {},
  on: EventTarget = document.activeElement ?? window,
) {
  act(() => {
    on.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...mods }));
  });
}

const saves = () => ipc.calls.filter(([method]) => method === "editor.save");

describe("the description's state (`overview.rs`)", () => {
  it("reads, edits unchanged, then changed; back to the saved text is unchanged again", () => {
    expect(descriptionState("a", null)).toBe("reading");
    expect(descriptionState("a", "a")).toBe("editing");
    expect(descriptionState("a", "ab")).toBe("changed");
  });

  it("asks first only with changes, and only when the move leaves this ticket's Overview", () => {
    const at = (ticket: string | null, tab: "agent" | "overview" | null) => ({ ticket, tab });
    expect(asksFirst("changed", KEY, at(KEY, "agent"), ["overview"])).toBe(true);
    expect(asksFirst("changed", KEY, at(null, null), ["overview"])).toBe(true);
    expect(asksFirst("changed", KEY, at("SHOP-13", null), ["overview"])).toBe(true);
    expect(asksFirst("changed", KEY, at(KEY, "overview"), ["overview"])).toBe(false);
    expect(asksFirst("changed", KEY, at(KEY, null), ["overview"])).toBe(false);
    expect(asksFirst("editing", KEY, at(null, null), ["overview"])).toBe(false);
  });

  it("⌘S on a Mac, Ctrl+S elsewhere", () => {
    const key = (mods: Partial<KeyboardEventInit>) => ({
      key: "s",
      metaKey: false,
      ctrlKey: false,
      shiftKey: false,
      altKey: false,
      ...mods,
    });
    expect(isSaveDescriptionKey(key({ metaKey: true }), true)).toBe(true);
    expect(isSaveDescriptionKey(key({ ctrlKey: true }), true)).toBe(false);
    expect(isSaveDescriptionKey(key({ ctrlKey: true }), false)).toBe(true);
    expect(isSaveDescriptionKey(key({ metaKey: true, shiftKey: true }), true)).toBe(false);
  });
});

describe("the description in place (s25-04)", () => {
  it("a click opens the editor; nothing shows on the tab row until the text differs", () => {
    const page = mount();
    expect(page.querySelector(".overview-hint")?.textContent).toBe("Click to edit");
    const editor = clickToEdit(page);
    expect(editor.isEditable).toBe(true);
    expect(page.querySelector(".overview-hint")?.textContent).toBe("Editing · Esc to discard, ⌘S to save");
    expect(page.querySelector(".overview-save-bar")).toBeNull();
    typeAtEnd(editor, " now");
    const bar = page.querySelector(".overview-save-bar");
    expect(bar?.textContent).toContain("Description changed");
    expect(bar?.textContent).toContain("Discard");
    expect(bar?.textContent).toContain("Save");
    expect(bar?.textContent).toContain("⌘S");
  });

  it("Save ⌘S sends only the description, to the in-place target", async () => {
    ipc.reply = (method) => (method === "editor.save" ? { outcome: "saved", key: KEY, note: null } : null);
    const page = mount();
    const editor = clickToEdit(page);
    typeAtEnd(editor, " now");
    await act(async () => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "s",
          metaKey: MAC,
          ctrlKey: !MAC,
          bubbles: true,
          cancelable: true,
        }),
      );
      await Promise.resolve();
    });
    expect(saves()).toHaveLength(1);
    const [, params] = saves()[0] ?? [];
    const sent = params as { target: unknown; form: typeof EDITOR.form };
    expect(sent.target).toEqual({ mode: "description", key: KEY });
    expect(sent.form.body).not.toBe(READ.body_markdown);
    expect(sent.form.body).toContain("now");
    // The rest of the form is the view's, untouched: Rust saves the description alone (`EditorModel::description`).
    expect({ ...sent.form, body: EDITOR.form.body }).toEqual(EDITOR.form);
    // Saved: back to the description as it reads.
    expect(page.querySelector(".overview-read")).not.toBeNull();
    expect(descriptionDraft.get()).toBeNull();
  });

  it("an untouched description is never saved: ⌘S just closes the editor", () => {
    const page = mount();
    clickToEdit(page);
    press("s", { metaKey: MAC, ctrlKey: !MAC }, window);
    expect(saves()).toHaveLength(0);
    expect(page.querySelector(".overview-read")).not.toBeNull();
  });

  it("Esc discards: the text gives up focus first, then the change goes", () => {
    const page = mount();
    const editor = clickToEdit(page);
    typeAtEnd(editor, " now");
    press("Escape");
    expect(page.querySelector(".overview-edit")).not.toBeNull();
    press("Escape");
    expect(page.querySelector(".overview-edit")).toBeNull();
    expect(page.querySelector(".overview-read")?.textContent).not.toContain("now");
    expect(saves()).toHaveLength(0);
    expect(descriptionDraft.get()).toBeNull();
  });

  it("closing the sheet or switching tabs with changes asks first; Discard changes goes on", () => {
    openTicket(KEY);
    const page = mount();
    const editor = clickToEdit(page);
    // Unchanged: nothing asks.
    showTab("overview");
    expect(overlays.store.get()).toEqual([]);
    typeAtEnd(editor, " now");
    showTab("overview");
    expect(overlays.store.get()).toEqual([]);
    showTab("agent");
    closeTicket();
    expect(selection.get()).toMatchObject({ ticket: KEY, tab: "overview" });
    const asked = overlays.store.get().at(-1);
    if (asked?.id !== "confirm") throw new Error("no question");
    expect(asked.params.title).toBe("Discard your description changes?");
    expect(asked.params.confirmLabel).toBe("Discard changes");
    expect(asked.params.cancelLabel).toBe("Keep editing");
    act(() => {
      asked.params.onConfirm();
    });
    expect(selection.get().ticket).toBeNull();
    expect(page.querySelector(".overview-edit")).toBeNull();
  });
});

describe("the Attachments column (RPT-02, s25-05)", () => {
  it("lists every report with its size, who · stage and its comment's line, then the drop zone and the note", () => {
    const out = renderToStaticMarkup(
      createElement(Attachments, { ticket: KEY, view: OVERVIEW, error: null, onAdd: () => undefined }),
    );
    for (const text of [
      "ATTACHMENTS · 2",
      "Add",
      "3 KB",
      "Rust developer · Dev",
      "Fails 2 in 20 on Safari: the middleware reads the cookie too early.",
      "Steps I used",
      "Drop files here or paste",
      "yours are marked from you and read first.",
    ]) {
      expect(out).toContain(text);
    }
    expect(out.match(/class="overview-attachment"/g)).toHaveLength(2);
    expect(out).toContain("data-mine");
  });
});
