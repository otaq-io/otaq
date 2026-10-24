// @vitest-environment happy-dom
// The comment box's paste and limit rules (CMT-11, T-0446): a paste over the limit becomes the report chip, never
// text; with only the chip the button stays off; typing past the limit shows the counter and turns it off.
import { act, createElement, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { CommentBoxView } from "../../generated/CommentBoxView";
import type { CommentRow } from "../../generated/CommentRow";
import type { CommentThreadView } from "../../generated/CommentThreadView";
import type { CommentLimits } from "../../generated/CommentLimits";
import { CommentBox } from "./CommentBox";
import { CommentThread } from "./CommentThread";
import { type BoxValue, boxState, chars, EMPTY_BOX, pasteAction, withPasted } from "./commentRules";
import { type ReportTarget, setReportOpener } from "./reportOpener";

const ipc = vi.hoisted((): { calls: string[]; posts: unknown[]; thread: unknown } => ({
  calls: [],
  posts: [],
  thread: null,
}));
vi.mock(import("../../tauri"), async (original) => ({
  ...(await original()),
  request: (method: string, params?: unknown) => {
    ipc.calls.push(method);
    if (method === "ticket_comments.post") ipc.posts.push(params);
    return Promise.resolve(method === "ticket_comments.thread" ? ipc.thread : "· 48 KB");
  },
  subscribeEvents: () => Promise.resolve(),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(() => {
  ipc.calls = [];
  ipc.posts = [];
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
  needs_line: "Say what the next agent should look for",
  pasted: "Pasted text",
  file_line: "A comment always has its own line; the file goes with it.",
  remove_file: "Remove the file",
  open: "Open",
  remove: "Remove the pasted text",
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

  it("keeps one pasted report: a second paste replaces the first", () => {
    const once = withPasted(EMPTY_BOX, "x".repeat(3000));
    const twice = withPasted({ ...once, text: "Look at limit_rows" }, "é".repeat(2500));
    expect(twice).toEqual({
      text: "Look at limit_rows",
      pasted: { text: "é".repeat(2500), bytes: 5000 },
      file: null,
    });
  });
});

describe("the box's state", () => {
  it("shows 2,140 / 2,000 and turns Comment off when typing past the limit", () => {
    const state = boxState({ text: "x".repeat(2140), pasted: null, file: null }, LIMITS);
    expect(state).toEqual({
      count: 2140,
      over: true,
      counter: "2,140 / 2,000",
      needsLine: false,
      fileLine: false,
      canSubmit: false,
    });
    expect(boxState({ text: "x".repeat(2000), pasted: null, file: null }, LIMITS).canSubmit).toBe(true);
  });

  it("asks for a line while only a chip is there", () => {
    const chip = withPasted(EMPTY_BOX, "y".repeat(2001));
    expect(boxState(chip, LIMITS)).toMatchObject({ needsLine: true, canSubmit: false });
    expect(boxState({ ...chip, text: "  " }, LIMITS)).toMatchObject({ needsLine: true, canSubmit: false });
    expect(boxState({ ...chip, text: "Look at limit_rows" }, LIMITS)).toMatchObject({
      needsLine: false,
      canSubmit: true,
    });
  });

  it("needs text in the thread; a dialog's optional box may stay empty", () => {
    expect(boxState(EMPTY_BOX, LIMITS).canSubmit).toBe(false);
    expect(boxState(EMPTY_BOX, LIMITS, true).canSubmit).toBe(true);
    // Still never only a chip.
    expect(boxState(withPasted(EMPTY_BOX, "z".repeat(2001)), LIMITS, true).canSubmit).toBe(false);
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

/** A paste of `text` into the box; returns whether the box took it over (no default insert). */
async function paste(host: HTMLElement, text: string): Promise<boolean> {
  const area = host.querySelector("textarea");
  if (!area) throw new Error("no box");
  const event = new Event("paste", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", { value: { getData: () => text } });
  await act(async () => {
    area.dispatchEvent(event);
    await Promise.resolve();
  });
  return event.defaultPrevented;
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

  it("turns a 2,001-character paste into the chip, never text, with Comment off until a line is typed", async () => {
    const sent: BoxValue[] = [];
    const host = await mount((v) => sent.push(v));
    expect(await paste(host, "b".repeat(2001))).toBe(true);
    expect(host.querySelector("textarea")?.value).toBe("");
    expect(host.querySelector(".comment-chip-label")?.textContent).toBe("Pasted text");
    expect(host.querySelector(".comment-chip-size")?.textContent).toBe("· 48 KB");
    expect(ipc.calls).toContain("ticket_comments.pasted_size");
    expect(host.querySelector(".comment-box-hint")?.textContent).toBe(
      "Say what the next agent should look for",
    );
    expect(button(host).disabled).toBe(true);

    await type(host, "Look at limit_rows first.");
    expect(host.querySelector(".comment-box-hint")).toBeNull();
    expect(button(host).disabled).toBe(false);
    await act(async () => {
      button(host).click();
      await Promise.resolve();
    });
    expect(sent).toHaveLength(1);
    expect(sent[0]?.text).toBe("Look at limit_rows first.");
    expect(sent[0]?.pasted?.text).toBe("b".repeat(2001));

    // ✕ removes the chip.
    const remove = host.querySelector<HTMLButtonElement>(".comment-chip-remove");
    await act(async () => {
      remove?.click();
      await Promise.resolve();
    });
    expect(host.querySelector(".comment-chip")).toBeNull();
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
  report: null,
  ...over,
});

const THREAD: CommentThreadView = {
  key: "SHOP-34",
  heading: "Comments · 2",
  hint: "Every stage’s agent reads them before it starts",
  count: 2,
  comments: [
    row(2, {
      author: "You",
      mine: true,
      mark: { kind: "you" },
      meta: "2:20 pm",
      heading: "You · 2:20 pm",
      body: "Also keep the column order the same as the table on screen.",
    }),
    row(4, {
      author: "Validator",
      meta: "Validate · round 2 · 3:05 pm",
      edited: "· edited",
      body: "Fail: the 10,000 row limit isn’t enforced; 3 tests fail in export_tests.rs.",
      report: { label: "Report", size: "· 48 KB", open: "Open" },
    }),
  ],
  menu: [],
  menu_label: "Comment actions",
  edit: { cancel: "Cancel", save: "Save" },
  delete: { title: "Delete this comment?", note: "", cancel: "Cancel", confirm: "Delete" },
  box: BOX,
};

describe("CommentThread", () => {
  it("draws what the view says, and Open goes to the report view's door", async () => {
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
    expect(comments.map((c) => c.hasAttribute("data-mine"))).toEqual([true, false]);
    expect(comments[1]?.textContent).toContain("· edited");
    expect(host.querySelector(".comment-chip-size")?.textContent).toBe("· 48 KB");
    await act(async () => {
      host.querySelector<HTMLButtonElement>(".comment-report-open")?.click();
      await Promise.resolve();
    });
    expect(opened).toEqual([{ kind: "comment", ticket: "SHOP-34", n: 4 }]);
    setReportOpener(() => undefined);
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
