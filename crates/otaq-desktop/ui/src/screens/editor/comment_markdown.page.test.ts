// @vitest-environment happy-dom
// The Ticket tab's thread draws each comment as Markdown (tuq 110, plan comments-as-markdown piece 2) and folds a long
// one after about 15 lines with Show more / Show less (tuq 112); Copy text and Edit still take the comment exactly as
// written, and the Delete dialog quotes it drawn the same way. The test runner lays nothing out, so a comment body's
// height is given here: 19 px for each line the drawn text has (a paragraph, a list item, a line break, a code line),
// capped where editor.css caps a folded body (15 lines).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { CommentBoxView } from "../../generated/CommentBoxView";
import type { CommentRow } from "../../generated/CommentRow";
import type { CommentThreadView } from "../../generated/CommentThreadView";
import { CommentThread } from "./CommentThread";

const ipc = vi.hoisted((): { thread: unknown } => ({ thread: null }));
vi.mock(import("../../tauri"), async (original) => ({
  ...(await original()),
  request: (method: string) => Promise.resolve(method === "ticket_comments.thread" ? ipc.thread : null),
  subscribeEvents: () => Promise.resolve(),
}));

const LINE = 19;
const FOLDED = Math.round(15 * 13 * 1.45);

/** The drawn height of a comment body, as a browser would lay out its lines. */
function drawnHeight(el: HTMLElement): number {
  const lines =
    el.querySelectorAll("p, li, br, h1, h2, h3, h4, h5, h6").length +
    [...el.querySelectorAll("pre")].reduce((n, pre) => n + pre.textContent.split("\n").length, 0);
  return lines * LINE;
}

const heights = {
  scrollHeight: {
    configurable: true,
    get(this: HTMLElement) {
      return this.classList.contains("comment-body") ? drawnHeight(this) : 0;
    },
  },
  clientHeight: {
    configurable: true,
    get(this: HTMLElement) {
      if (!this.classList.contains("comment-body")) return 0;
      const full = drawnHeight(this);
      return this.closest(".comment-text[data-folded]") ? Math.min(full, FOLDED) : full;
    },
  },
} satisfies PropertyDescriptorMap;

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

let root: Root | null = null;
beforeEach(() => {
  Object.defineProperties(HTMLElement.prototype, heights);
});
afterEach(() => {
  act(() => root?.unmount());
  root = null;
  for (const name of Object.keys(heights)) Reflect.deleteProperty(HTMLElement.prototype, name);
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const BOX: CommentBoxView = {
  label: null,
  placeholder: "Add a comment for the next stage…",
  submit: "Comment",
  footer: null,
  footer_icon: "arrow-right",
  pasted: "Pasted text",
  remove_file: "Remove the file",
  open: "Open",
  remove: "Remove the pasted text",
  attach: "Attach",
  attach_title: "Attach files to the comment",
  optional: false,
  limits: {
    report_bytes: 20 * 1024 * 1024,
    report_label: "20 MB",
    message_bytes: 16 * 1024 * 1024 - 64 * 1024,
    too_long_to_send:
      "This comment is too long to send: one message carries at most 16 MB. Attach the text as a file instead.",
  },
  tell_agent: null,
};

const row = (n: number, body: string): CommentRow => ({
  n,
  author: "Rust developer",
  mine: false,
  mark: { kind: "agent" },
  meta: "Dev · 2:58 pm",
  editing_meta: "Dev · 2:58 pm · editing",
  edited: null,
  told: null,
  heading: `Rust developer · Dev · 2:58 pm · ${String(n)}`,
  body,
  thumbnails: [],
  attachments: [],
});

const item = (id: string, label: string): CommentThreadView["menu"][number] => ({
  id,
  label,
  icon: null,
  shortcut: null,
  danger: false,
  disabled: false,
  hint: null,
  separated: false,
  checked: false,
  header: false,
});

const thread = (comments: CommentRow[]): CommentThreadView => ({
  key: "SHOP-34",
  heading: `Comments · ${String(comments.length)}`,
  hint: "Every stage’s agent reads them before it starts",
  count: comments.length,
  comments,
  menu: [item("edit", "Edit"), item("copy", "Copy text"), item("delete", "Delete…")],
  menu_label: "Comment actions",
  edit: { cancel: "Cancel", save: "Save" },
  delete: {
    title: "Delete this comment?",
    note: "History keeps what it said.",
    cancel: "Cancel",
    confirm: "Delete",
  },
  fold: { more: "Show more", less: "Show less" },
  box: BOX,
});

/** An agent's hand-over, written in Markdown as agents write them. */
const HANDOVER = [
  "**Done.** Export writes `orders.csv`; Excel not added.",
  "Criteria:",
  "- [x] one row per order",
  "- [x] the column order of the table",
  "See [the issue](https://example.com/issues/12) and `Observable<number | null>`.",
].join("\n");

/** Thirty lines, one per criterion: far more than 15. */
const LONG = Array.from({ length: 30 }, (_, i) => `Criterion ${String(i + 1)} holds`).join("\n");

async function draw(view: CommentThreadView): Promise<HTMLElement> {
  ipc.thread = view;
  const host = document.createElement("div");
  document.body.append(host);
  await act(async () => {
    root = createRoot(host);
    root.render(createElement(CommentThread, { ticket: "SHOP-34" }));
    await new Promise((r) => setTimeout(r, 0));
  });
  return host;
}

/** The comment `n`'s article. */
function comment(host: HTMLElement, n: number): HTMLElement {
  const el = host.querySelector<HTMLElement>(`article[aria-label$="· ${String(n)}"]`);
  if (!el) throw new Error(`no comment ${String(n)}`);
  return el;
}

async function choose(host: HTMLElement, n: number, label: string) {
  await act(async () => {
    comment(host, n).querySelector<HTMLButtonElement>('button[aria-label="Comment actions"]')?.click();
    await Promise.resolve();
  });
  const choice = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(
    (r) => r.textContent === label,
  );
  await act(async () => {
    choice?.click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

const foldButton = (el: HTMLElement) => el.querySelector<HTMLButtonElement>(".comment-fold");

describe("the thread draws a comment as Markdown (tuq 110)", () => {
  it("draws bold, code, a task list and a link, keeps a single line break and shows a tag as text", async () => {
    const host = await draw(thread([row(3, HANDOVER)]));
    const body = comment(host, 3).querySelector(".comment-body .md-comment .ProseMirror");
    expect(body?.querySelector("strong")?.textContent).toBe("Done.");
    expect([...(body?.querySelectorAll("code") ?? [])].map((c) => c.textContent)).toEqual([
      "orders.csv",
      "Observable<number | null>",
    ]);
    expect(body?.querySelectorAll('ul[data-type="taskList"] > li')).toHaveLength(2);
    expect(body?.querySelector("a")?.getAttribute("href")).toBe("https://example.com/issues/12");
    // "Criteria:" sits on its own line under the first, as written.
    expect(body?.querySelector("p br")).not.toBeNull();
    // No raw marks are left, and nothing that looks like a tag became one.
    expect(body?.textContent).not.toContain("**");
    expect(body?.querySelector("number")).toBeNull();
    // A short comment doesn't fold.
    expect(foldButton(comment(host, 3))).toBeNull();
  });

  it("quotes the comment in the Delete dialog drawn the same way", async () => {
    const host = await draw(thread([row(3, HANDOVER)]));
    await choose(host, 3, "Delete…");
    const quoted = document.querySelector(".comment-quoted .comment-body.md-comment .ProseMirror");
    expect(quoted?.querySelector("strong")?.textContent).toBe("Done.");
    expect(quoted?.querySelectorAll('ul[data-type="taskList"] > li')).toHaveLength(2);
    // The dialog's box scrolls the whole comment: nothing folds there.
    expect(document.querySelector(".comment-quoted .comment-fold")).toBeNull();
  });
});

describe("a long comment folds after about 15 lines (tuq 112)", () => {
  it("shows Show more, which opens the rest in place, and Show less after it", async () => {
    const host = await draw(thread([row(5, LONG), row(3, HANDOVER)]));
    const long = comment(host, 5);
    const text = long.querySelector(".comment-text");
    expect(text?.hasAttribute("data-folded")).toBe(true);
    expect(foldButton(long)?.textContent).toBe("Show more");
    expect(foldButton(long)?.getAttribute("aria-expanded")).toBe("false");
    // The whole comment is drawn under the fold: the rest is there, only cut off.
    expect(long.querySelector(".comment-body")?.textContent).toContain("Criterion 30 holds");
    await act(async () => {
      foldButton(long)?.click();
      await Promise.resolve();
    });
    expect(text?.hasAttribute("data-folded")).toBe(false);
    expect(foldButton(long)?.textContent).toBe("Show less");
    expect(foldButton(long)?.getAttribute("aria-expanded")).toBe("true");
    await act(async () => {
      foldButton(long)?.click();
      await Promise.resolve();
    });
    expect(text?.hasAttribute("data-folded")).toBe(true);
    expect(foldButton(long)?.textContent).toBe("Show more");
    // The short comment next to it never folds.
    expect(foldButton(comment(host, 3))).toBeNull();
  });

  it("folds on the drawn height, not on the text's lines: a long code block folds too", async () => {
    const code = [
      "Repro:",
      "```rust",
      ...Array.from({ length: 24 }, (_, i) => `let x${String(i)} = ${String(i)};`),
      "```",
    ];
    const host = await draw(thread([row(6, code.join("\n"))]));
    expect(foldButton(comment(host, 6))?.textContent).toBe("Show more");
  });
});

describe("Copy text and Edit take the comment as written (tuq 110)", () => {
  it("copies the Markdown exactly, marks and line breaks included", async () => {
    const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    const host = await draw(thread([row(3, HANDOVER)]));
    await choose(host, 3, "Copy text");
    expect(writeText).toHaveBeenCalledWith(HANDOVER);
  });

  it("copies a folded comment in full", async () => {
    const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    const host = await draw(thread([row(5, LONG)]));
    await choose(host, 5, "Copy text");
    expect(writeText).toHaveBeenCalledWith(LONG);
  });

  it("opens Edit with the Markdown exactly as written", async () => {
    const host = await draw(thread([row(3, HANDOVER)]));
    await choose(host, 3, "Edit");
    expect(comment(host, 3).querySelector("textarea")?.value).toBe(HANDOVER);
  });
});
