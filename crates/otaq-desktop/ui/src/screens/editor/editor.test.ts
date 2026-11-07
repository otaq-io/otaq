// @vitest-environment happy-dom
// Read mode and the form mount the description editor, which needs a DOM.
import type { Editor } from "@tiptap/core";
import { act, createElement, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { overlays, useOpenOverlays } from "../../app/overlays";
import { registered, registerScreens } from "../../app/registry";
import { NOTHING_SELECTED, requestTicketEdit, selection, ticketEditRequests } from "../../app/selection";
import type { EditorForm } from "../../generated/EditorForm";
import type { EditorReadView } from "../../generated/EditorReadView";
import type { EditorView } from "../../generated/EditorView";
import type { StageChoicesView } from "../../generated/StageChoicesView";
import { FullViewHost } from "../../markdown";
import { editor } from ".";
import { EditorFields, RefusalBox } from "./Fields";
import {
  canSave,
  changed,
  COPIED_FOR,
  copyMarkdown,
  formReport,
  isSaveKey,
  isStartKey,
  quickKey,
  refusalFor,
  shownPriority,
} from "./logic";
import { Compare, Description, DescriptionTools, LabelRow } from "./ReadMode";
import { FullForm, useFormKeys } from "./Modal";
import { takeEditRequest } from "./EditForm";
import { useEditor } from "./useEditor";
import { FORM, READ, REPORT_FIELD, TEXTS, VIEW } from "./editor_fixtures";
import { EMPTY_BOX, fileAttachment, withPasted } from "./commentRules";
import { NewTicketReport, useReportAttach } from "./NewTicketReport";
import { dropTargets } from "../../terminal/drops";

// The service, for the form's save: `ipc.reply` answers each request (an empty list by default, for the branch list).
type Reply = (method: string, params: unknown) => unknown;
const ipc = vi.hoisted((): { reply: Reply } => ({ reply: () => [] }));
vi.mock(import("../../tauri"), async (original) => ({
  ...(await original()),
  request: (method: string, params: unknown) => Promise.resolve(ipc.reply(method, params)),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  ipc.reply = () => [];
  selection.set(NOTHING_SELECTED);
  ticketEditRequests.set(null);
});

const html = (el: Parameters<typeof renderToStaticMarkup>[0]) => renderToStaticMarkup(el);
const noop = () => undefined;

/** The description's label row and its rendered text, as Overview draws them (T-0465). */
const readMode = (read: EditorReadView = READ, onExpand?: () => void) =>
  createElement(
    "div",
    null,
    createElement(LabelRow, {
      name: "What and why",
      field: "body",
      read,
      texts: TEXTS,
      onCompare: noop,
      tools: createElement(DescriptionTools, { stored: read.body_markdown, texts: TEXTS, onExpand }),
    }),
    createElement(Description, { read, texts: TEXTS, label: "What and why" }),
  );

let unmount = noop;

/** Renders into a page (the description editor draws its text after mounting). */
function mount(el: Parameters<typeof renderToStaticMarkup>[0]): HTMLElement {
  unmount();
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  act(() => {
    root.render(el);
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

describe("Copy as Markdown", () => {
  it("puts exactly the stored text on the clipboard, as plain text", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    for (const stored of [
      "## Why\nUsers mistype addresses; **12%** (see [report](https://example.com/report)).\n",
      "* star bullets\r\n1) paren list  \r\nno final newline",
      "Escaped \\* and \\_ <kbd>⌘</kbd>\n\n~~~ts\nconst a = 1;\n~~~\n\n",
      "",
    ]) {
      await copyMarkdown(stored, { writeText });
      expect(writeText).toHaveBeenLastCalledWith(stored);
    }
    expect(writeText).toHaveBeenCalledTimes(4);
  });
});

describe("the editor's form state", () => {
  it("quick-create: ↵ creates, ⇧↵ opens the full form, Esc cancels", () => {
    expect(quickKey({ key: "Enter", shiftKey: false })).toBe("create");
    expect(quickKey({ key: "Enter", shiftKey: true })).toBe("full");
    expect(quickKey({ key: "Escape", shiftKey: false })).toBe("cancel");
    expect(quickKey({ key: "Enter", shiftKey: false, isComposing: true })).toBeNull();
    expect(quickKey({ key: "a", shiftKey: false })).toBeNull();
  });

  it("⌘↵ saves from any field; save needs a title", () => {
    expect(isSaveKey({ key: "Enter", metaKey: true, ctrlKey: false })).toBe(true);
    expect(isSaveKey({ key: "Enter", metaKey: false, ctrlKey: true })).toBe(true);
    expect(isSaveKey({ key: "Enter", metaKey: false, ctrlKey: false })).toBe(false);
    expect(canSave({ ...FORM, title: "  " }, false)).toBe(false);
    expect(canSave({ ...FORM, title: "Checkout" }, true)).toBe(false);
    expect(canSave({ ...FORM, title: "Checkout" }, false)).toBe(true);
  });

  it("⇧⌘↵ is Create & start; ⌘↵ alone is not", () => {
    expect(isStartKey({ key: "Enter", shiftKey: true, metaKey: true, ctrlKey: false })).toBe(true);
    expect(isStartKey({ key: "Enter", shiftKey: true, metaKey: false, ctrlKey: true })).toBe(true);
    expect(isStartKey({ key: "Enter", shiftKey: false, metaKey: true, ctrlKey: false })).toBe(false);
    expect(isStartKey({ key: "Enter", shiftKey: true, metaKey: false, ctrlKey: false })).toBe(false);
  });

  it("priority shows the picked one, else the view's; a change is a change", () => {
    expect(shownPriority(VIEW, FORM)).toBe(3);
    expect(shownPriority(VIEW, { ...FORM, priority: 1 })).toBe(1);
    expect(changed(FORM, { ...FORM, title: "Checkout" })).toBe(true);
    expect(changed(FORM, { ...FORM })).toBe(false);
  });

  it("a refusal shows under its own field", () => {
    const refusal = {
      field: "base" as const,
      title: "Different base branches",
      message: "SHOP-31 starts from release/2.3",
      icon: "circle-x" as const,
    };
    expect(refusalFor(refusal, "base")).toBe(refusal);
    expect(refusalFor(refusal, "form")).toBeNull();
    const out = html(createElement(RefusalBox, { refusal }));
    expect(out).toContain("Different base branches");
    expect(out).toContain("SHOP-31 starts from release/2.3");
  });

  it("an edit request is taken once, by its own ticket's tab", () => {
    requestTicketEdit("SHOP-12");
    expect(selection.get().tab).toBe("overview");
    expect(takeEditRequest("SHOP-19")).toBe(false);
    expect(takeEditRequest("SHOP-12")).toBe(true);
    expect(takeEditRequest("SHOP-12")).toBe(false);
  });
});

describe("the New ticket form's report (RPT-03, T-0474)", () => {
  it("turns the box into the form's fields: a file by its path, pasted text, nothing without a report", () => {
    const file = fileAttachment({
      path: "/Users/me/test-run-login.md",
      name: "test-run-login.md",
      size: "3 KB",
      image: false,
    });
    expect(formReport({ attachments: [file], text: "The run." })).toEqual({
      report: { kind: "file", path: "/Users/me/test-run-login.md" },
      report_line: "The run.",
    });
    expect(formReport(withPasted(EMPTY_BOX, "log", true))).toEqual({
      report: { kind: "pasted", text: "log" },
      report_line: "",
    });
    expect(formReport({ ...EMPTY_BOX, text: "a line alone" })).toEqual({
      report: undefined,
      report_line: undefined,
    });
  });

  it("a dropped file becomes the chip waiting for its line; the line goes with it; ✕ takes it away", async () => {
    ipc.reply = (method, params) =>
      method === "overview.file"
        ? { path: (params as { path: string }).path, name: "test-run-login.md", size: "3 KB", image: false }
        : [];
    const seen: Partial<EditorForm>[] = [];
    function Harness() {
      const root = useRef<HTMLDivElement>(null);
      const attach = useReportAttach(root, true, (fields) => seen.push(fields));
      return createElement(
        "div",
        { ref: root },
        createElement(NewTicketReport, { field: REPORT_FIELD, project: "SHOP", attach, onSave: noop }),
      );
    }
    const page = mount(createElement(Harness));
    expect(page.textContent).toContain("Drop files here or paste");
    expect(page.querySelector("textarea")).toBeNull();
    await act(async () => {
      dropTargets.at(page.firstElementChild)?.takeFiles?.(["/Users/me/test-run-login.md"]);
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(page.querySelector(".comment-chip-label")?.textContent).toBe("test-run-login.md");
    expect(page.textContent).toContain("A comment always has its own line; the file goes with it.");
    expect(seen.at(-1)).toEqual({
      report: { kind: "file", path: "/Users/me/test-run-login.md" },
      report_line: "",
    });
    const line = page.querySelector("textarea");
    expect(line?.getAttribute("placeholder")).toBe("One line: what the report shows…");
    act(() => {
      Reflect.set(HTMLTextAreaElement.prototype, "value", "Fails 2 in 20 on Safari.", line);
      line?.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(seen.at(-1)?.report_line).toBe("Fails 2 in 20 on Safari.");
    act(() => {
      page.querySelector<HTMLButtonElement>(".comment-chip-remove")?.click();
    });
    expect(seen.at(-1)).toEqual({ report: undefined, report_line: undefined });
    expect(page.textContent).toContain("Drop files here or paste");
    unmount();
  });

  it("a file that can't be a report says why", () => {
    function Harness() {
      const root = useRef<HTMLDivElement>(null);
      const attach = useReportAttach(root, true, noop);
      return createElement(
        "div",
        { ref: root },
        createElement(NewTicketReport, {
          field: REPORT_FIELD,
          project: "SHOP",
          attach: { ...attach, error: "run.log is 212 MB; the limit is 20 MB." },
          onSave: noop,
        }),
      );
    }
    const page = mount(createElement(Harness));
    expect(page.querySelector("[role=alert]")?.textContent).toBe("run.log is 212 MB; the limit is 20 MB.");
    expect(page.textContent).not.toContain("agents read yours before they start");
    unmount();
  });
});

describe("the editor", () => {
  it("types the title and code as plain text: no autocorrect, capitals or spell check (#41)", () => {
    const host = mount(
      createElement(EditorFields, {
        view: VIEW,
        form: { ...FORM, body: "Run it:\n\n```\nnpm test --port 3000\n```" },
        onChange: noop,
        titleMissing: false,
        refusal: null,
        onSave: noop,
      }),
    );
    const title = host.querySelector(".editor-title");
    expect(title?.getAttribute("autocorrect")).toBe("off");
    expect(title?.getAttribute("autocapitalize")).toBe("off");
    expect(title?.getAttribute("spellcheck")).toBe("false");
    // The description is prose: no autocorrect or capitals, misspellings still underlined; its code blocks aren't.
    const prose = host.querySelector(".ProseMirror");
    expect(prose?.getAttribute("autocorrect")).toBe("off");
    expect(prose?.getAttribute("autocapitalize")).toBe("off");
    expect(prose?.getAttribute("spellcheck")).not.toBe("false");
    expect(host.querySelector(".md-code pre")?.getAttribute("spellcheck")).toBe("false");
    unmount();
  });

  it("the full form: every field of a new ticket, the title error once a save was tried", () => {
    const props = {
      view: VIEW,
      form: FORM,
      onChange: noop,
      titleMissing: true,
      refusal: null,
      onSave: noop,
    };
    const out = html(createElement(EditorFields, props));
    for (const text of [
      "Title",
      "Add a title",
      "P3 Normal",
      "What and why",
      "Waits for",
      "Start from and merge into",
    ]) {
      expect(out).toContain(text);
    }
    expect(out).not.toContain("External ID");
    // A link is added only from the waiting ticket: no "Blocks" here.
    expect(out).not.toContain("Blocks");
  });

  it("a new ticket's Waits for lists what it picked, each with its ✕; an existing ticket has none", () => {
    const props = {
      view: VIEW,
      form: { ...FORM, waits_for: ["SHOP-12"] },
      onChange: noop,
      titleMissing: false,
      refusal: null,
      onSave: noop,
    };
    const out = html(createElement(EditorFields, props));
    expect(out).toContain("SHOP-12");
    expect(out).toContain('aria-label="Remove SHOP-12"');
    const edit = html(
      createElement(EditorFields, { ...props, view: { ...VIEW, create: false, waits_for: null } }),
    );
    expect(edit).not.toContain("Waits for");
  });

  it("a locked base shows why", () => {
    const out = html(
      createElement(EditorFields, {
        view: { ...VIEW, create: false, base: { kind: "locked", text: "locked · the ticket has started" } },
        form: FORM,
        onChange: noop,
        titleMissing: false,
        refusal: null,
        onSave: noop,
      }),
    );
    expect(out).toContain("locked · the ticket has started");
    expect(out).toContain("disabled");
  });

  it("read mode: the rendered Markdown under its label, the agent's marker with Compare", () => {
    const page = mount(readMode());
    const out = page.innerHTML;
    for (const text of ["WHAT AND WHY", "Updated by agent", "Compare"]) {
      expect(out).toContain(text);
    }
    expect(out.match(/Updated by agent/g)).toHaveLength(1);
    expect(out).not.toContain("Dark mode");
    // Rendered by the read-only editor, not shown as source.
    const text = page.querySelector(".editor-read-body .ProseMirror");
    expect(text?.getAttribute("contenteditable")).toBe("false");
    expect(text?.querySelector("h2")?.textContent).toBe("Done when");
    expect(text?.querySelector("li")?.textContent).toBe("Toggle works");
    expect(text?.querySelector("pre")?.textContent).toBe("npm test");
    expect(out).not.toContain("## Done when");
    unmount();
  });

  it("the label row: marker and Compare, then Copy as Markdown and Expand, in that order", () => {
    const page = mount(readMode());
    const row = page.querySelector(".editor-section");
    const order = Array.from(row?.querySelectorAll(".editor-marker, .editor-link, .editor-tool") ?? []).map(
      (el) => el.textContent || el.getAttribute("aria-label"),
    );
    expect(order).toEqual(["Updated by agent", "Compare", "Copy as Markdown", "Expand"]);
    unmount();
  });

  it("the icons show without a marker too, and an empty description says nothing is written", () => {
    const page = mount(readMode({ ...READ, marked: [], body_markdown: "  \n", body_empty: true }));
    expect(page.querySelectorAll(".editor-section .editor-tool")).toHaveLength(2);
    expect(page.querySelector(".editor-read-body")).toBeNull();
    expect(page.textContent).toContain("Nothing written yet.");
    unmount();
  });

  it("Expand is a slot for the full-window view: disabled until it is given a handler", () => {
    const expand = (page: HTMLElement) => page.querySelector<HTMLButtonElement>('[aria-label="Expand"]');
    expect(expand(mount(readMode()))?.disabled).toBe(true);
    const onExpand = vi.fn();
    const page = mount(readMode(READ, onExpand));
    act(() => expand(page)?.click());
    expect(onExpand).toHaveBeenCalledOnce();
    unmount();
  });

  it("Copy as Markdown copies the stored text, then shows a check and a check-icon Copied tooltip for 1.5 s", async () => {
    vi.useFakeTimers();
    const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    const stored = "## Why\r\nUsers *mistype*  \r\n* [x] done\n\n| a |\n|:-|\n| \\* |";
    const page = mount(readMode({ ...READ, body_markdown: stored }));
    const copy = page.querySelector<HTMLButtonElement>('[aria-label="Copy as Markdown"]');
    const icon = () => copy?.querySelector<HTMLElement>(".icon")?.style.maskImage;
    const tooltip = () => page.querySelector<HTMLElement>('[role="tooltip"]');
    act(() => {
      copy?.focus();
      vi.advanceTimersByTime(0);
    });
    expect(icon()).toContain("lucide-clipboard-copy'");
    expect(tooltip()?.textContent).toBe("Copy as Markdown");
    expect(tooltip()?.querySelector(".icon")).toBeNull();
    await act(async () => {
      copy?.click();
      await Promise.resolve();
    });
    expect(writeText).toHaveBeenCalledWith(stored);
    expect(copy?.hasAttribute("data-copied")).toBe(true);
    expect(icon()).toContain("lucide-check'");
    expect(tooltip()?.textContent).toBe("Copied");
    expect(tooltip()?.querySelector<HTMLElement>(".icon")?.style.maskImage).toContain("lucide-check'");
    act(() => {
      vi.advanceTimersByTime(COPIED_FOR);
    });
    expect(copy?.hasAttribute("data-copied")).toBe(false);
    expect(icon()).toContain("lucide-clipboard-copy'");
    expect(tooltip()?.textContent).toBe("Copy as Markdown");
    unmount();
  });

  it("Compare shows other fields as plain text, without the toggle", () => {
    const page = mount(
      createElement(Compare, {
        entries: [
          {
            field: "title",
            heading: "“Title” · changed by agent:SHOP-19 · 2m ago",
            before: "Dark *mode*",
            after: "Light / dark",
          },
        ],
        texts: TEXTS,
      }),
    );
    for (const text of ["BEFORE", "AFTER", "Dark *mode*", "Light / dark"])
      expect(page.textContent).toContain(text);
    expect(page.querySelector(".ProseMirror")).toBeNull();
    expect(page.querySelector('[role="radiogroup"]')).toBeNull();
    unmount();
  });

  it("Compare renders the description on both sides, and the toggle shows its source", () => {
    const before = "Add a toggle.\n\n## Done when\n\n- [x] Toggle works";
    const after = "Add a toggle in **Settings**.\n\n## Done when\n\n- [x] Toggle works\n- [ ] Persists";
    const page = mount(
      createElement(Compare, {
        entries: [{ field: "body", heading: "“What and why” (the description)", before, after }],
        texts: TEXTS,
      }),
    );
    const side = (name: string) => page.querySelector(`.editor-compare-col[data-side="${name}"]`);
    const segments = () => Array.from(page.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
    // Rendered by default: read-only editors on both sides.
    expect(segments().map((b) => [b.textContent, b.getAttribute("aria-checked")])).toEqual([
      ["Rendered", "true"],
      ["Markdown", "false"],
    ]);
    for (const name of ["before", "after"]) {
      const text = side(name)?.querySelector(".editor-read-body .ProseMirror");
      expect(text?.getAttribute("contenteditable")).toBe("false");
      expect(text?.querySelector("h2")?.textContent).toBe("Done when");
    }
    expect(side("after")?.querySelector("strong")?.textContent).toBe("Settings");
    expect(side("after")?.querySelectorAll("li")).toHaveLength(2);
    expect(page.textContent).not.toContain("## Done when");
    // Markdown: the stored source, as is.
    act(() => segments()[1]?.click());
    expect(page.querySelector(".ProseMirror")).toBeNull();
    expect(side("before")?.querySelector(".editor-compare-source")?.textContent).toBe(before);
    expect(side("after")?.querySelector(".editor-compare-source")?.textContent).toBe(after);
    act(() => segments()[0]?.click());
    expect(side("after")?.querySelector(".ProseMirror h2")?.textContent).toBe("Done when");
    unmount();
  });

  /** The form as the Ticket tab and the modal hold it: `useEditor`'s state, the fields, ⌘↵ on the form's root. */
  function FormHarness({ view }: { view: EditorView }) {
    const editor = useEditor(view, { mode: "edit", key: "SHOP-47" });
    const root = useRef<HTMLDivElement>(null);
    const save = () => {
      editor.save(noop);
    };
    useFormKeys(root, save);
    return createElement(
      "div",
      { ref: root },
      createElement(EditorFields, {
        view,
        form: editor.form,
        onChange: editor.setForm,
        titleMissing: false,
        refusal: null,
        onSave: save,
      }),
      createElement("button", { className: "harness-save", onClick: save }, "Save"),
    );
  }

  /** Mounts the form on `body` and records every form sent to `editor.save`. */
  function openForm(body: string) {
    const saved: EditorForm[] = [];
    ipc.reply = (method, params) => {
      if (method !== "editor.save") return [];
      saved.push((params as { form: EditorForm }).form);
      return { outcome: "saved", key: "SHOP-47", note: null };
    };
    const view: EditorView = {
      ...VIEW,
      create: false,
      key: "SHOP-47",
      form: { ...FORM, title: "Checkout", body },
    };
    const page = mount(createElement(FormHarness, { view }));
    const text = page.querySelector<HTMLElement & { editor: Editor }>(".editor-description .ProseMirror");
    if (text === null) throw new Error("no description editor");
    const save = async () => {
      await act(async () => {
        page.querySelector<HTMLButtonElement>(".harness-save")?.click();
        await Promise.resolve();
      });
    };
    return { page, text, saved, save };
  }

  // A description the editor would spell differently (`*` bullets, `1)` list, no final newline).
  const STORED = "## Why\r\nUsers mistype **addresses**.\r\n\r\n* one\r\n* two\r\n\r\n1) first";

  it("the description is the Markdown editor with its toolbar, the hint and the placeholder", () => {
    const { page } = openForm("");
    const field = page.querySelector(".editor-description .md-editor");
    expect(field?.querySelector(".md-toolbar")).not.toBeNull();
    expect(field?.querySelector('.ProseMirror[contenteditable="true"]')?.getAttribute("aria-label")).toBe(
      "What and why",
    );
    expect(page.querySelector("textarea")).toBeNull();
    expect(page.textContent).toContain("Markdown · paste keeps formatting");
    unmount();
  });

  it("an untouched description saves byte for byte; an edited one saves as Markdown", async () => {
    const untouched = openForm(STORED);
    await untouched.save();
    expect(untouched.saved).toHaveLength(1);
    expect(untouched.saved[0]?.body).toBe(STORED);
    unmount();

    const edited = openForm(STORED);
    act(() => {
      edited.text.editor.commands.insertContentAt(1, "The ");
    });
    await edited.save();
    const body = edited.saved[0]?.body ?? "";
    expect(body).not.toBe(STORED);
    expect(body).toContain("## The Why");
    expect(body).toContain("**addresses**");
    expect(body).toContain("- one");
    unmount();
  });

  it("the toolbar gets its shadow once the text scrolls under it", () => {
    const { page } = openForm(STORED);
    const field = page.querySelector<HTMLElement>(".editor-description");
    const content = page.querySelector<HTMLElement>(".editor-description .md-content");
    const scrollTo = (top: number) => {
      act(() => {
        if (content) content.scrollTop = top;
        content?.dispatchEvent(new Event("scroll"));
      });
    };
    expect(field?.dataset.scrolled).toBeUndefined();
    scrollTo(40);
    expect(field?.dataset.scrolled).toBe("true");
    scrollTo(0);
    expect(field?.dataset.scrolled).toBeUndefined();
    unmount();
  });

  it("⌘↵ in the description saves the form instead of adding a line break", async () => {
    const { text, saved } = openForm(STORED);
    const key = new KeyboardEvent("keydown", {
      key: "Enter",
      metaKey: true,
      bubbles: true,
      cancelable: true,
    });
    await act(async () => {
      text.dispatchEvent(key);
      await Promise.resolve();
    });
    expect(key.defaultPrevented).toBe(true);
    expect(saved).toHaveLength(1);
    expect(saved[0]?.body).toBe(STORED);
    expect(JSON.stringify(text.editor.getJSON())).not.toContain("hardBreak");
    unmount();
  });

  /** The form with the overlay host's full-window box (the frame draws it in the app). */
  function openExpandable(body: string) {
    const form = openForm(body);
    const box = document.body.appendChild(document.createElement("div"));
    const boxRoot = createRoot(box);
    const Host = () => {
      const open = useOpenOverlays().find((o) => o.id === "full_view");
      return open?.id === "full_view"
        ? createElement(FullViewHost, { params: open.params, close: noop })
        : null;
    };
    act(() => {
      boxRoot.render(createElement(Host));
    });
    const formUnmount = unmount;
    unmount = () => {
      formUnmount();
      act(() => {
        boxRoot.unmount();
        overlays.store.set([]);
      });
      box.remove();
    };
    const view = () => document.querySelector<HTMLElement>(".md-fullview");
    const button = (label: string) =>
      [...(view()?.querySelectorAll<HTMLButtonElement>("button") ?? [])].find(
        (b) => b.textContent === label || b.getAttribute("aria-label") === label,
      );
    const expand = () => {
      act(() => {
        form.page.querySelector<HTMLButtonElement>('.md-toolbar [aria-label="Expand"]')?.click();
      });
    };
    return { ...form, view, button, expand };
  }

  it("Expand shows the form's description over the window with the ticket's header and Save / Cancel", () => {
    const { page, text, view, button, expand } = openExpandable(STORED);
    expand();
    expect(view()).not.toBeNull();
    expect(view()?.contains(text)).toBe(true);
    expect(page.querySelector(".editor-description .md-editor")).toBeNull();
    expect(view()?.querySelector(".md-fullview-header")?.textContent).toContain("SHOP-47·What and why");
    expect(view()?.querySelector(".md-fullview-header")?.textContent).toContain("Checkout");
    expect(view()?.querySelector(".md-fullview-header")?.textContent).toContain("Editing");
    // The toolbar and its Markdown toggle come along; its Expand is the header's Collapse.
    expect(view()?.querySelector(".md-fullview-band .md-toolbar .md-source-toggle")).not.toBeNull();
    expect(view()?.querySelector('.md-toolbar [aria-label="Expand"]')).toBeNull();
    expect(button("Collapse")).toBeDefined();
    expect(view()?.querySelector(".md-fullview-footer")?.textContent).toContain(
      "collapse (after closing a popover)",
    );
    act(() => button("Collapse")?.click());
    expect(view()).toBeNull();
    expect(page.querySelector(".editor-description .md-editor")?.contains(text)).toBe(true);
    unmount();
  });

  it("Cancel collapses and the form keeps the edits; Save collapses and saves the form", async () => {
    const { text, saved, view, button, expand } = openExpandable(STORED);
    expand();
    act(() => {
      text.editor.commands.insertContentAt(1, "The ");
    });
    act(() => button("Cancel")?.click());
    expect(view()).toBeNull();
    expect(saved).toHaveLength(0);
    expand();
    await act(async () => {
      button(TEXTS.primary)?.click();
      await Promise.resolve();
    });
    expect(view()).toBeNull();
    expect(saved).toHaveLength(1);
    expect(saved[0]?.body).toContain("## The Why");
    unmount();
  });

  it("expanding and collapsing an untouched description still saves it byte for byte", async () => {
    const { saved, save, button, expand } = openExpandable(STORED);
    expand();
    act(() => button("Collapse")?.click());
    await save();
    expect(saved[0]?.body).toBe(STORED);
    unmount();
  });

  it("registers quick-create, the edit modal, the full-window view, the report view and the image view from its folder", () => {
    expect(registered(registerScreens({ editor }))).toMatchObject({
      overlays: ["new_ticket", "edit_ticket", "full_view", "report", "image"],
    });
  });
});

describe("Create & start in the new-ticket form", () => {
  /** Mounts the full form on a new ticket, recording each request; `answer` replies to Create & start. */
  function openNew(answer: unknown, view: EditorView = VIEW) {
    const sent: { method: string; params: { target: unknown; form: EditorForm } }[] = [];
    ipc.reply = (method, params) => {
      if (method !== "editor.create_and_start" && method !== "editor.save") return [];
      sent.push({ method, params: params as { target: unknown; form: EditorForm } });
      return method === "editor.save" ? { outcome: "saved", key: "SHOP-13", note: null } : answer;
    };
    const close = vi.fn();
    const target = { mode: "create" as const, project: "SHOP" };
    const initial = { ...FORM, title: "Checkout page", waits_for: ["SHOP-12"] };
    mount(createElement(FullForm, { view, target, initial, close }));
    const button = (label: string) =>
      [...document.querySelectorAll<HTMLButtonElement>("button.button")].find((b) => b.textContent === label);
    const click = async (label: string) => {
      await act(async () => {
        button(label)?.click();
        await Promise.resolve();
      });
    };
    const key = async (shiftKey: boolean) => {
      const title = document.querySelector<HTMLInputElement>(".editor-title");
      await act(async () => {
        title?.dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "Enter",
            metaKey: true,
            shiftKey,
            bubbles: true,
            cancelable: true,
          }),
        );
        await Promise.resolve();
      });
    };
    return { sent, close, button, click, key };
  }

  it("sits next to Create; one request creates the ticket with its Waits for and starts it, then the form closes", async () => {
    const form = openNew({ outcome: "saved", key: "SHOP-13", note: null });
    expect(form.button("Create & start")).toBeDefined();
    expect(form.button("Create ticket")).toBeDefined();
    expect(document.body.textContent).toContain("create & start");
    await form.click("Create & start");
    expect(form.sent.map((s) => s.method)).toEqual(["editor.create_and_start"]);
    expect(form.sent[0]?.params.target).toEqual({ mode: "create", project: "SHOP" });
    expect(form.sent[0]?.params.form.waits_for).toEqual(["SHOP-12"]);
    expect(form.close).toHaveBeenCalledTimes(1);
    unmount();
  });

  it("⇧⌘↵ creates and starts, ⌘↵ only creates", async () => {
    const started = openNew({ outcome: "saved", key: "SHOP-13", note: null });
    await started.key(true);
    expect(started.sent.map((s) => s.method)).toEqual(["editor.create_and_start"]);
    unmount();
    const created = openNew({ outcome: "saved", key: "SHOP-13", note: null });
    await created.key(false);
    expect(created.sent.map((s) => s.method)).toEqual(["editor.save"]);
    unmount();
  });

  it("a refused start keeps the form open on the created ticket and says why", async () => {
    const form = openNew({
      outcome: "refused",
      refusal: {
        field: "form",
        title: "Created, but not started",
        message: "SHOP-13 is in Backlog. Dev's agent tool isn't set up.",
        icon: "circle-x",
      },
      created: "SHOP-13",
    });
    await form.click("Create & start");
    expect(form.close).not.toHaveBeenCalled();
    const refusal = document.querySelector(".editor-refusal");
    expect(refusal?.textContent).toContain("Created, but not started");
    expect(refusal?.textContent).toContain("SHOP-13 is in Backlog. Dev's agent tool isn't set up.");
    // Trying again saves and starts the ticket it created, not a second one.
    await form.click("Save & start");
    expect(form.sent[1]?.params.target).toEqual({ mode: "edit", key: "SHOP-13" });
    unmount();
  });

  it("an existing ticket's form has no Create & start", () => {
    openNew(null, { ...VIEW, create: false, texts: { ...TEXTS, create_and_start: null } });
    expect(document.body.textContent).not.toContain("Create & start");
    expect(document.body.textContent).not.toContain("create & start");
    unmount();
  });
});

describe("the Stage choices in the form (T-0411, AGM-09)", () => {
  const item = (id: string, checked: boolean) => ({
    id,
    label: id,
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint: null,
    separated: false,
    checked,
    header: false,
  });
  /** shop-web's flow (*08 → Row · Stage choices*): Product spec and Review skippable, Dev with Quick developer. */
  const CHOICES: StageChoicesView = {
    heading: "Stage choices",
    set_by: null,
    rows: [
      {
        stage_id: "product-spec",
        label: "Product spec:",
        value: "Spec writer",
        items: [item("Spec writer", true), item("skip", false)],
        lock: null,
        change: null,
      },
      {
        stage_id: "dev",
        label: "Dev:",
        value: "Rust developer",
        items: [item("Rust developer", true), item("Quick developer", false)],
        lock: null,
        change: null,
      },
      {
        stage_id: "review",
        label: "Review:",
        value: "required",
        items: [item("required", true), item("skip", false)],
        lock: null,
        change: null,
      },
    ],
    note: "Only stages with options in shop-web’s flow. You can change them until that stage starts.",
  };

  let shown: EditorForm = FORM;
  function Form({ view }: { view: EditorView }) {
    const [form, setForm] = useState(FORM);
    shown = form;
    return createElement(EditorFields, {
      view,
      form,
      onChange: setForm,
      titleMissing: false,
      refusal: null,
      onSave: noop,
    });
  }
  const press = (el: Element | null | undefined) => {
    act(() => {
      el?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
      el?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    });
  };

  it("⌘N: a picker per stage with options; a pick stays in the form; none without options", () => {
    const host = mount(createElement(Form, { view: { ...VIEW, choices: CHOICES } }));
    expect(host.textContent).toContain("Stage choices");
    expect([...host.querySelectorAll(".stage-choice-label")].map((l) => l.textContent)).toEqual([
      "Product spec:",
      "Dev:",
      "Review:",
    ]);
    expect(host.textContent).toContain("Only stages with options in shop-web’s flow.");
    press([...host.querySelectorAll(".stage-choice-select button")][1]);
    press(
      [...document.querySelectorAll('[role="menuitemradio"]')].find((i) =>
        i.textContent.includes("Quick developer"),
      ),
    );
    expect(shown.choices).toEqual([{ stage_id: "dev", choice: "Quick developer" }]);
    expect([...host.querySelectorAll(".stage-choice-select button")][1]?.textContent).toContain(
      "Quick developer",
    );
    const none = mount(createElement(Form, { view: VIEW }));
    expect(none.querySelector(".stage-choices")).toBeNull();
  });
});
