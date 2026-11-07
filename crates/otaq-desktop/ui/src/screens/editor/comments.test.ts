// @vitest-environment happy-dom
// The comment box's paste and limit rules (CMT-11, T-0446): a paste over the limit becomes the report chip, never
// text; with only the chip the button stays off; typing past the limit shows the counter and turns it off.
import { act, createElement, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { CommentBoxView } from "../../generated/CommentBoxView";
import type { CommentRow } from "../../generated/CommentRow";
import type { CommentThreadView } from "../../generated/CommentThreadView";
import type { CommentThumbnail } from "../../generated/CommentThumbnail";
import { overlays } from "../../app/overlays";
import type { CommentLimits } from "../../generated/CommentLimits";
import { CommentBox } from "./CommentBox";
import { CommentThread, EditInPlace } from "./CommentThread";
import { ImageViewer } from "./ImageViewer";
import { dropTargets } from "../../terminal/drops";
import {
  type BoxValue,
  boxState,
  chars,
  editBox,
  editChanges,
  EMPTY_BOX,
  pasteAction,
  sentAttachments,
  withAdded,
  withoutAttachment,
  withPasted,
} from "./commentRules";
import { type ReportTarget, setReportOpener } from "./reportOpener";

const ipc = vi.hoisted(
  (): {
    calls: string[];
    posts: unknown[];
    edits: unknown[];
    thread: unknown;
    staged: string[];
    picked: string[];
  } => ({
    calls: [],
    posts: [],
    edits: [],
    thread: null,
    staged: [],
    picked: [],
  }),
);
/** Core's refusal of a file over 20 MB (`too_big`). */
const tooBig = (name: string) =>
  `${name} is 21 MB; the limit is 20 MB. Keep the part that matters, for example the failing tests.`;
const chip = (path: string) => {
  const name = path.split("/").at(-1) ?? path;
  return { path, name, size: "2 KB", image: name.endsWith(".png") };
};
vi.mock(import("../../tauri"), async (original) => ({
  ...(await original()),
  request: (method: string, params?: unknown) => {
    ipc.calls.push(method);
    if (method === "ticket_comments.post") ipc.posts.push(params);
    if (method === "ticket_comments.edit") ipc.edits.push(params);
    if (method === "ticket_comments.thread") return Promise.resolve(ipc.thread);
    if (method === "overview.file") {
      const { path } = params as { path: string };
      return path.includes("big")
        ? Promise.reject(new Error(tooBig("big.log")))
        : Promise.resolve(chip(path));
    }
    if (method === "attachments.refusal") {
      const { name, bytes } = params as { name: string; bytes: number };
      return Promise.resolve(bytes > 20 * 1024 * 1024 ? tooBig(name) : null);
    }
    return Promise.resolve("· 48 KB");
  },
  stageAttachment: (_file: Blob, name: string) => {
    ipc.staged.push(name);
    return Promise.resolve(chip(`/data/staged-attachments/1/${name}`));
  },
  boxImage: () => Promise.resolve(new Uint8Array([137, 80, 78, 71]).buffer),
  pickAttachmentFiles: () => Promise.resolve(ipc.picked),
  subscribeEvents: () => Promise.resolve(),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(() => {
  ipc.calls = [];
  ipc.posts = [];
  ipc.edits = [];
  ipc.staged = [];
  ipc.picked = [];
  document.body.innerHTML = "";
});

/** As the service sends them (`CommentLimits::current`). */
const LIMITS: CommentLimits = { chars: 2000, report_bytes: 20 * 1024 * 1024, report_label: "20 MB" };

const BOX: CommentBoxView = {
  label: null,
  placeholder: "Add a comment for the next stage…",
  submit: "Comment",
  footer: "Read by the next stage’s agent",
  footer_icon: "arrow-right",
  pasted: "Pasted text",
  file_line: null,
  remove_file: "Remove the file",
  open: "Open",
  remove: "Remove the pasted text",
  attach: "Attach",
  attach_title: "Attach files to the comment",
  optional: false,
  limits: LIMITS,
  tell_agent: null,
};

describe("the paste rule", () => {
  it("puts 1,999 characters in the text and turns 2,001 into the chip", () => {
    expect(pasteAction("a".repeat(1999), LIMITS)).toBe("text");
    expect(pasteAction("a".repeat(2000), LIMITS)).toBe("text");
    expect(pasteAction("a".repeat(2001), LIMITS)).toBe("chip");
  });

  it("counts characters as the service does (code points)", () => {
    expect(chars("é😀")).toBe(2);
    expect(pasteAction("😀".repeat(2000), LIMITS)).toBe("text");
  });

  it("keeps every paste as its own attachment, in order (tuq 001)", () => {
    const once = withPasted(EMPTY_BOX, "x".repeat(3000));
    const twice = withPasted({ ...once, text: "Look at limit_rows" }, "é".repeat(2500));
    expect(twice.text).toBe("Look at limit_rows");
    expect(twice.attachments.map((a) => (a.kind === "pasted" ? a.bytes : 0))).toEqual([3000, 5000]);
    expect(sentAttachments(twice)).toEqual([
      { kind: "pasted", text: "x".repeat(3000) },
      { kind: "pasted", text: "é".repeat(2500) },
    ]);
    // A box of one report (the New ticket form's) keeps the last.
    expect(withPasted(once, "y", true).attachments).toHaveLength(1);
  });
});

describe("the box's state", () => {
  it("shows 2,140 / 2,000 and turns Comment off when typing past the limit", () => {
    const state = boxState({ text: "x".repeat(2140), attachments: [] }, LIMITS);
    expect(state).toEqual({
      count: 2140,
      over: true,
      counter: "2,140 / 2,000",
      fileLine: false,
      canSubmit: false,
    });
    expect(boxState({ text: "x".repeat(2000), attachments: [] }, LIMITS).canSubmit).toBe(true);
  });

  it("sends a chip with no line, with a line, or with only spaces typed (tuq 007)", () => {
    const chip = withPasted(EMPTY_BOX, "y".repeat(2001));
    expect(boxState(chip, LIMITS).canSubmit).toBe(true);
    expect(boxState({ ...chip, text: "  " }, LIMITS).canSubmit).toBe(true);
    expect(boxState({ ...chip, text: "Look at limit_rows" }, LIMITS).canSubmit).toBe(true);
    // The limit still holds with a chip.
    expect(boxState({ ...chip, text: "x".repeat(2001) }, LIMITS).canSubmit).toBe(false);
  });

  it("needs text or an attachment in the thread; a dialog's optional box may stay empty", () => {
    expect(boxState(EMPTY_BOX, LIMITS).canSubmit).toBe(false);
    expect(boxState({ text: " \n ", attachments: [] }, LIMITS).canSubmit).toBe(false);
    expect(boxState(EMPTY_BOX, LIMITS, true).canSubmit).toBe(true);
    expect(boxState(withPasted(EMPTY_BOX, "z".repeat(2001)), LIMITS, true).canSubmit).toBe(true);
  });
});

/** The box with its own state, as the thread holds it. */
function Harness({ onSubmit }: { onSubmit: (value: BoxValue) => void }) {
  const [value, setValue] = useState<BoxValue>(EMPTY_BOX);
  return createElement(CommentBox, {
    ticket: "SHOP-34",
    view: BOX,
    value,
    onChange: setValue,
    onSubmit: () => {
      onSubmit(value);
    },
  });
}

async function mount(onSubmit: (value: BoxValue) => void = () => undefined) {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  await act(async () => {
    root.render(createElement(Harness, { onSubmit }));
    await Promise.resolve();
  });
  return host;
}

/** Lets the requests, the staging and the thumbnails' reads finish. */
async function settle() {
  await act(async () => {
    for (let i = 0; i < 5; i += 1) await new Promise((r) => setTimeout(r, 0));
  });
}

/** A paste of `text` (and `files`) into the box; returns whether the box took it over (no default insert). */
async function paste(host: HTMLElement, text: string, files: File[] = []): Promise<boolean> {
  const area = host.querySelector("textarea");
  if (!area) throw new Error("no box");
  const event = new Event("paste", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", { value: { getData: () => text, files } });
  await act(async () => {
    area.dispatchEvent(event);
    await Promise.resolve();
  });
  await settle();
  return event.defaultPrevented;
}

/** A file of `bytes` bytes, without allocating them (its size is what the box reads first). */
function fileOf(name: string, type: string, bytes: number): File {
  const file = new File([new Uint8Array([137, 80, 78, 71])], name, { type });
  Object.defineProperty(file, "size", { value: bytes });
  return file;
}

function chipLabels(host: HTMLElement): string[] {
  return [...host.querySelectorAll(".comment-chip-label")].map((l) => l.textContent);
}

async function type(host: HTMLElement, text: string) {
  const area = host.querySelector("textarea");
  if (!area) throw new Error("no box");
  await act(async () => {
    // React tracks the value it set; the prototype's setter is what typing goes through.
    Reflect.set(HTMLTextAreaElement.prototype, "value", text, area);
    area.dispatchEvent(new Event("input", { bubbles: true }));
    await Promise.resolve();
  });
}

function button(host: HTMLElement): HTMLButtonElement {
  const found = [...host.querySelectorAll("button")].find((b) => b.textContent === "Comment");
  if (!found) throw new Error("no Comment button");
  return found;
}

describe("CommentBox", () => {
  it("leaves a 1,999-character paste to the text", async () => {
    const host = await mount();
    expect(await paste(host, "a".repeat(1999))).toBe(false);
    expect(host.querySelector(".comment-chip")).toBeNull();
  });

  it("turns a 2,001-character paste into the chip, never text, with Comment on and no hint (tuq 007)", async () => {
    const sent: BoxValue[] = [];
    const host = await mount((v) => sent.push(v));
    expect(await paste(host, "b".repeat(2001))).toBe(true);
    expect(host.querySelector("textarea")?.value).toBe("");
    expect(host.querySelector(".comment-chip-label")?.textContent).toBe("Pasted text");
    expect(host.querySelector(".comment-chip-size")?.textContent).toBe("· 48 KB");
    expect(ipc.calls).toContain("ticket_comments.pasted_size");
    expect(host.querySelector(".comment-box-hint")).toBeNull();
    expect(button(host).disabled).toBe(false);

    await type(host, "Look at limit_rows first.");
    expect(host.querySelector(".comment-box-hint")).toBeNull();
    expect(button(host).disabled).toBe(false);
    await act(async () => {
      button(host).click();
      await Promise.resolve();
    });
    expect(sent).toHaveLength(1);
    expect(sent[0]?.text).toBe("Look at limit_rows first.");
    expect(sent[0] && sentAttachments(sent[0])).toEqual([{ kind: "pasted", text: "b".repeat(2001) }]);

    // ✕ removes the chip.
    const remove = host.querySelector<HTMLButtonElement>(".comment-chip-remove");
    await act(async () => {
      remove?.click();
      await Promise.resolve();
    });
    expect(host.querySelector(".comment-chip")).toBeNull();
  });

  it("sends a pasted screenshot on its own: Comment is on with only the image chip (tuq 007)", async () => {
    const sent: BoxValue[] = [];
    const host = await mount((v) => sent.push(v));
    expect(button(host).disabled).toBe(true);
    await paste(host, "", [fileOf("image.png", "image/png", 48_000)]);
    expect(chipLabels(host)).toEqual(["image.png"]);
    expect(host.querySelector(".comment-box-hint")).toBeNull();
    expect(button(host).disabled).toBe(false);
    await act(async () => {
      button(host).click();
      await Promise.resolve();
    });
    expect(sent).toHaveLength(1);
    expect(sent[0]?.text).toBe("");
    expect(sent[0] && sentAttachments(sent[0])).toEqual([
      { kind: "file", path: "/data/staged-attachments/1/image.png" },
    ]);
  });

  it("turns a pasted screenshot into an image chip with its thumbnail, never pasted text (tuq 002, 003)", async () => {
    const sent: BoxValue[] = [];
    const host = await mount((v) => sent.push(v));
    expect(await paste(host, "", [fileOf("image.png", "image/png", 48_000)])).toBe(true);
    expect(chipLabels(host)).toEqual(["image.png"]);
    expect(ipc.staged).toEqual(["image.png"]);
    expect(ipc.calls).not.toContain("ticket_comments.pasted_size");
    const thumb = host.querySelector<HTMLImageElement>(".comment-chip-thumb");
    expect(thumb?.getAttribute("src")).toMatch(/^data:image\/png;base64,/);
    // Files copied in Finder keep their names; the screenshot stays.
    await paste(host, "notes.md", [fileOf("notes.md", "text/markdown", 2048), fileOf("run.zip", "", 100)]);
    expect(chipLabels(host)).toEqual(["image.png", "notes.md", "run.zip"]);
    expect(host.querySelectorAll(".comment-chip-thumb")).toHaveLength(1);
    await type(host, "Before and after.");
    await act(async () => {
      button(host).click();
      await Promise.resolve();
    });
    expect(sent[0] && sentAttachments(sent[0])).toEqual([
      { kind: "file", path: "/data/staged-attachments/1/image.png" },
      { kind: "file", path: "/data/staged-attachments/1/notes.md" },
      { kind: "file", path: "/data/staged-attachments/1/run.zip" },
    ]);
  });

  it("adds two dropped files as two chips, and ✕ removes one", async () => {
    const host = await mount();
    const box = host.querySelector(".comment-box");
    await act(async () => {
      dropTargets.at(box)?.takeFiles?.(["/Users/me/before.png", "/Users/me/test-run.txt"]);
      await Promise.resolve();
    });
    await settle();
    expect(chipLabels(host)).toEqual(["before.png", "test-run.txt"]);
    expect(host.querySelector(".comment-chip-thumb")?.getAttribute("src")).toMatch(/^data:image\/png/);
    expect(host.querySelector(".comment-box-hint")).toBeNull();
    const remove = host.querySelector<HTMLButtonElement>('[aria-label="Remove the file before.png"]');
    await act(async () => {
      remove?.click();
      await Promise.resolve();
    });
    expect(chipLabels(host)).toEqual(["test-run.txt"]);
  });

  it("adds every file picked with Attach", async () => {
    ipc.picked = ["/Users/me/a.txt", "/Users/me/b.pdf"];
    const host = await mount();
    const attach = [...host.querySelectorAll("button")].find((b) => b.textContent === "Attach");
    await act(async () => {
      attach?.click();
      await Promise.resolve();
    });
    await settle();
    expect(chipLabels(host)).toEqual(["a.txt", "b.pdf"]);
  });

  it("refuses a 21 MB file by name when it is added; the text and the other files stay (tuq 005)", async () => {
    const host = await mount();
    await type(host, "The recording and the log.");
    await paste(host, "", [
      fileOf("perf.mov", "video/quicktime", 21 * 1024 * 1024),
      fileOf("log.txt", "", 10),
    ]);
    expect(ipc.staged).toEqual(["log.txt"]);
    expect(chipLabels(host)).toEqual(["log.txt"]);
    expect(host.querySelector(".comment-box-error")?.textContent).toBe(
      "perf.mov is 21 MB; the limit is 20 MB. Keep the part that matters, for example the failing tests.",
    );
    expect(host.querySelector("textarea")?.value).toBe("The recording and the log.");
    // A dropped one too, by `overview.file`'s refusal.
    await act(async () => {
      dropTargets.at(host.querySelector(".comment-box"))?.takeFiles?.(["/Users/me/big.log"]);
      await Promise.resolve();
    });
    await settle();
    expect(host.querySelector(".comment-box-error")?.textContent).toMatch(/^big\.log is 21 MB/);
    expect(chipLabels(host)).toEqual(["log.txt"]);
  });

  it("shows the counter and the red box when 2,140 characters are typed", async () => {
    const host = await mount();
    await type(host, "x".repeat(2140));
    expect(host.querySelector(".comment-box-counter")?.textContent).toBe("2,140 / 2,000");
    expect(host.querySelector(".comment-box")?.hasAttribute("data-over")).toBe(true);
    expect(host.querySelector("textarea")?.getAttribute("aria-invalid")).toBe("true");
    expect(button(host).disabled).toBe(true);
    expect(host.querySelector(".comment-chip")).toBeNull();
  });

  it("shows the quiet line from the view", async () => {
    const host = await mount();
    expect(host.querySelector(".comment-box-footer")?.textContent).toBe(BOX.footer);
  });
});

const SHOT: CommentThumbnail = {
  attachment: 1,
  name: "shot.png",
  size: "120 KB",
  open: "Open shot.png full size",
  close: "Close",
};

const row = (n: number, over: Partial<CommentRow>): CommentRow => ({
  n,
  author: "Rust developer",
  mine: false,
  mark: { kind: "agent" },
  meta: "Dev · 2:58 pm",
  editing_meta: "Dev · 2:58 pm · editing",
  edited: null,
  heading: "Rust developer · Dev · 2:58 pm",
  body: "Done. Excel not added; left a TODO where it would go.",
  thumbnails: [],
  attachments: [],
  ...over,
});

const THREAD: CommentThreadView = {
  key: "SHOP-34",
  heading: "Comments · 2",
  hint: "Every stage’s agent reads them before it starts",
  count: 2,
  // Newest first, as `ticket_comments.thread` gives them.
  comments: [
    row(4, {
      author: "Validator",
      meta: "Validate · round 2 · 3:05 pm",
      edited: "· edited",
      body: "Fail: the 10,000 row limit isn’t enforced; 3 tests fail in export_tests.rs.",
      thumbnails: [SHOT],
      attachments: [{ attachment: 2, name: "test-run.txt", kind: "text", size: "· 48 KB", open: "Open" }],
    }),
    row(2, {
      author: "You",
      mine: true,
      mark: { kind: "you" },
      meta: "2:20 pm",
      heading: "You · 2:20 pm",
      body: "Also keep the column order the same as the table on screen.",
    }),
  ],
  menu: [],
  menu_label: "Comment actions",
  edit: { cancel: "Cancel", save: "Save" },
  delete: { title: "Delete this comment?", note: "", cancel: "Cancel", confirm: "Delete" },
  box: BOX,
};

describe("CommentThread", () => {
  it("draws what the view says, the box above the newest comment, and Open goes to the report view's door", async () => {
    ipc.thread = THREAD;
    const opened: ReportTarget[] = [];
    setReportOpener((t) => opened.push(t));
    const host = document.createElement("div");
    document.body.append(host);
    await act(async () => {
      createRoot(host).render(createElement(CommentThread, { ticket: "SHOP-34" }));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(host.querySelector(".comments-head")?.textContent).toContain("COMMENTS · 2");
    const comments = [...host.querySelectorAll(".comment")];
    expect(comments.map((c) => c.hasAttribute("data-mine"))).toEqual([false, true]);
    expect(comments[0]?.textContent).toContain("· edited");
    // The box comes first, so the user writes where they read.
    const box = host.querySelector(".comments > .comment-box");
    expect(box).not.toBeNull();
    expect(box?.compareDocumentPosition(comments[0] as Node)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    // The image is a thumbnail after the text, loaded by its numbers (tuq 004); a click opens it full size.
    const thumbnail = (comments[0] as Element).querySelector<HTMLButtonElement>(".comment-thumbnail");
    expect(thumbnail?.getAttribute("aria-label")).toBe("Open shot.png full size");
    expect(thumbnail?.querySelector("img")?.getAttribute("src")).toBe(
      "otaq-attachment://localhost/SHOP-34%2F4%2F1",
    );
    expect(
      (comments[0] as Element).querySelector(".comment-body")?.compareDocumentPosition(thumbnail as Node),
    ).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    await act(async () => {
      thumbnail?.click();
      await Promise.resolve();
    });
    expect(overlays.store.get()).toEqual([
      { id: "image", params: { ticket: "SHOP-34", n: 4, thumbnail: SHOT } },
    ]);
    overlays.closeTop();
    // One chip per other attachment (tuq 001); only the text one opens, with its number.
    const sizes = [...(comments[0] as Element).querySelectorAll(".comment-chip-size")].map(
      (s) => s.textContent,
    );
    expect(sizes).toEqual(["· 48 KB"]);
    expect(host.querySelectorAll(".comment-report-open")).toHaveLength(1);
    await act(async () => {
      host.querySelector<HTMLButtonElement>(".comment-report-open")?.click();
      await Promise.resolve();
    });
    expect(opened).toEqual([{ kind: "comment", ticket: "SHOP-34", n: 4, attachment: 2 }]);
    setReportOpener(() => undefined);
  });
});

describe("editing a comment's attachments (tuq 008)", () => {
  const edited = THREAD.comments[0] as CommentRow;

  it("starts the box with the comment's text and its attachments, in order, and sends only what changed", () => {
    const value = editBox(edited);
    expect(value.text).toBe(edited.body);
    expect(
      value.attachments.map((a) => (a.kind === "posted" ? [a.attachment, a.name, a.size] : a.kind)),
    ).toEqual([
      [1, "shot.png", "120 KB"],
      [2, "test-run.txt", "48 KB"],
    ]);
    expect(sentAttachments(value)).toEqual([]);
    expect(editChanges(edited, value)).toEqual({ remove: [], attachments: [], changed: false });
    const shot = value.attachments[0];
    if (!shot) throw new Error("no chip");
    const next = withAdded(withoutAttachment(value, shot.id), [
      {
        kind: "file",
        id: -1,
        path: "/Users/me/fixed.png",
        name: "fixed.png",
        size: "2 KB",
        image: true,
        thumb: null,
      },
    ]);
    expect(editChanges(edited, next)).toEqual({
      remove: [1],
      attachments: [{ kind: "file", path: "/Users/me/fixed.png" }],
      changed: true,
    });
    // Its text may go while an attachment stays; with neither, Save is off.
    expect(boxState({ ...next, text: "" }, LIMITS).canSubmit).toBe(true);
    expect(boxState({ text: " ", attachments: [] }, LIMITS).canSubmit).toBe(false);
  });

  it("shows its attachments as chips with ✕, takes new ones by Attach, and Save sends them with the text", async () => {
    ipc.picked = ["/Users/me/fixed.png"];
    let done = 0;
    const host = document.createElement("div");
    document.body.append(host);
    await act(async () => {
      createRoot(host).render(
        createElement(EditInPlace, {
          view: THREAD,
          row: edited,
          onDone: () => {
            done += 1;
          },
        }),
      );
      await Promise.resolve();
    });
    expect(chipLabels(host)).toEqual(["shot.png", "test-run.txt"]);
    // A posted image shows its own thumbnail, loaded by its numbers.
    expect(host.querySelector("[data-posted] .comment-chip-thumb")?.getAttribute("src")).toBe(
      "otaq-attachment://localhost/SHOP-34%2F4%2F1",
    );
    await act(async () => {
      host.querySelector<HTMLButtonElement>('[aria-label="Remove the file shot.png"]')?.click();
      await Promise.resolve();
    });
    const attach = [...host.querySelectorAll("button")].find((b) => b.textContent === "Attach");
    await act(async () => {
      attach?.click();
      await Promise.resolve();
    });
    await settle();
    expect(chipLabels(host)).toEqual(["test-run.txt", "fixed.png"]);
    await type(host, "The fixed badge.");
    const save = [...host.querySelectorAll("button")].find((b) => b.textContent === "Save");
    await act(async () => {
      save?.click();
      await Promise.resolve();
    });
    await settle();
    expect(ipc.edits).toEqual([
      {
        key: "SHOP-34",
        n: 4,
        body: "The fixed badge.",
        remove: [1],
        attachments: [{ kind: "file", path: "/Users/me/fixed.png" }],
      },
    ]);
    expect(done).toBe(1);
  });

  it("turns Save off when the text and every attachment are gone (tuq 007)", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    await act(async () => {
      createRoot(host).render(
        createElement(EditInPlace, { view: THREAD, row: edited, onDone: () => undefined }),
      );
      await Promise.resolve();
    });
    await type(host, "");
    for (const name of ["shot.png", "test-run.txt"]) {
      await act(async () => {
        host.querySelector<HTMLButtonElement>(`[aria-label="Remove the file ${name}"]`)?.click();
        await Promise.resolve();
      });
    }
    const save = [...host.querySelectorAll("button")].find((b) => b.textContent === "Save");
    expect(save?.disabled).toBe(true);
    expect(ipc.edits).toEqual([]);
  });
});

describe("ImageViewer (tuq 004)", () => {
  it("shows the image full size with its name and size; Esc and its ✕ close it", async () => {
    const closed: string[] = [];
    const host = document.createElement("div");
    document.body.append(host);
    await act(async () => {
      createRoot(host).render(
        createElement(ImageViewer, {
          params: { ticket: "SHOP-34", n: 4, thumbnail: SHOT },
          close: () => closed.push("closed"),
        }),
      );
      await Promise.resolve();
    });
    const dialog = host.querySelector<HTMLElement>(".image-view");
    expect(dialog?.getAttribute("role")).toBe("dialog");
    expect(dialog?.textContent).toContain("shot.png");
    expect(dialog?.textContent).toContain("120 KB");
    const img = host.querySelector<HTMLImageElement>(".image-view-image");
    expect(img?.getAttribute("src")).toBe("otaq-attachment://localhost/SHOP-34%2F4%2F1");
    expect(img?.getAttribute("alt")).toBe("shot.png");
    const close = host.querySelector<HTMLButtonElement>('button[aria-label="Close"]');
    expect(document.activeElement).toBe(close);
    act(() => {
      dialog?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    act(() => {
      close?.click();
    });
    expect(closed).toEqual(["closed", "closed"]);
  });
});

describe("Also tell the agent now (d0123)", () => {
  async function thread(view: CommentThreadView) {
    ipc.thread = view;
    const host = document.createElement("div");
    document.body.append(host);
    await act(async () => {
      createRoot(host).render(createElement(CommentThread, { ticket: "SHOP-34" }));
      await new Promise((r) => setTimeout(r, 0));
    });
    return host;
  }
  const tickBox = (host: HTMLElement) =>
    host.querySelector<HTMLInputElement>(".comment-box input[type=checkbox]");
  async function post(host: HTMLElement, text: string) {
    await type(host, text);
    await act(async () => {
      button(host).click();
      await new Promise((r) => setTimeout(r, 0));
    });
  }

  it("shows the tick box, ticked, while a session runs, and the post tells the agent", async () => {
    const host = await thread({ ...THREAD, box: { ...BOX, tell_agent: "Also tell the agent now" } });
    const box = tickBox(host);
    expect(box?.checked).toBe(true);
    expect(box?.closest("label")?.textContent).toBe("Also tell the agent now");
    await post(host, "Use useToday() on the detail and Stats pages.");
    expect(ipc.posts).toMatchObject([{ key: "SHOP-34", tell_agent: true }]);
    await act(async () => {
      tickBox(host)?.click();
      await Promise.resolve();
    });
    await post(host, "Only for later stages.");
    expect(ipc.posts).toMatchObject([{ tell_agent: true }, { tell_agent: false }]);
  });

  it("has no tick box and never tells an agent when none runs", async () => {
    const host = await thread(THREAD);
    expect(tickBox(host)).toBeNull();
    await post(host, "For the next stage.");
    expect(ipc.posts).toMatchObject([{ tell_agent: false }]);
  });
});
