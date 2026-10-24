import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { registered, registerScreens } from "../../app/registry";
import { NOTHING_SELECTED, selection } from "../../app/selection";
import type { DiffAnchor } from "../../generated/DiffAnchor";
import type { DiffHunkView } from "../../generated/DiffHunkView";
import type { FilesChangeRow } from "../../generated/FilesChangeRow";
import type { FilesChangesView } from "../../generated/FilesChangesView";
import type { FilesComments } from "../../generated/FilesComments";
import type { FilesFilePage } from "../../generated/FilesFilePage";
import type { FilesReviewBar } from "../../generated/FilesReviewBar";
import type { FilesSearchView } from "../../generated/FilesSearchView";
import type { FilesTabView } from "../../generated/FilesTabView";
import type { FilesTreeRow } from "../../generated/FilesTreeRow";
import { files } from ".";
import { AllChanges, FileBody, FilesTabBody, SearchResults } from "./FilesTab";
import { type CommentContext, Hunk } from "./Hunks";
import {
  canSave,
  chunks,
  composerKey,
  composerReducer,
  folders,
  NOTHING_OPEN,
  openFile,
  rowAnchors,
  saveRequest,
  searchFocus,
  showsLines,
  showsSideBySide,
  shownFilter,
  shownMode,
  steppedRow,
  threadsByAnchor,
  visibleRows,
  writesNew,
} from "./logic";
import { ReviewBarBody } from "./ReviewBar";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => ({ ready: new Promise(() => undefined), stop: () => undefined })),
}));

afterEach(() => {
  selection.set(NOTHING_SELECTED);
});

const anchor = (file: string, line: number, side: "old" | "new" = "new"): DiffAnchor => ({
  file,
  line,
  side,
  key: `${file}\u0000${side}\u0000${String(line)}`,
});

const row = (path: string, extra: Partial<FilesTreeRow> = {}): FilesTreeRow => ({
  name: path.split("/").pop() ?? path,
  path,
  kind: "file",
  depth: 0,
  mark: null,
  quiet: true,
  opens: true,
  tags: [],
  child: null,
  chips: [],
  changed_below: null,
  branch: null,
  struck: false,
  ...extra,
});

const M = (counts: string, uncommitted = false) => ({
  letter: "M",
  status: "modified" as const,
  counts,
  uncommitted: uncommitted ? "uncommitted" : null,
});

/** SHOP-22's worktree (`s25-07`): e2e/ and src/ on top, three unchanged files. */
const SHOP_22: FilesTabView = {
  key: "SHOP-22",
  source: { kind: "worktree", title: "SHOP-22’s worktree", subtitle: "otaq/SHOP-22 · from main", legend: [] },
  search: { placeholder: "Search files by name", keys: "⌥⌘P" },
  filters: ["All files", "Changed · 3"],
  changed: 3,
  opens_on: "all",
  rows: [
    row("e2e", { kind: "folder", quiet: false }),
    row("src", { kind: "folder", quiet: false }),
    row("next.config.ts"),
    row("README.md"),
  ],
  pick: "Pick a file in the tree to read it, or search for one by name.",
};

const change = (path: string, counts: string, extra: Partial<FilesChangeRow> = {}): FilesChangeRow => ({
  id: path,
  path,
  label: path,
  child: null,
  mark: M(counts),
  note: null,
  collapsed: null,
  binary_label: null,
  rename_label: null,
  rows: 5,
  can_open: true,
  commentable: true,
  tags: [],
  ...extra,
});

/** All changes of SHOP-22 (`s25-08`): three files, one not committed yet. */
const CHANGES: FilesChangesView = {
  key: "SHOP-22",
  revision: "r1",
  title: "All changes",
  summary: "3 files · +13 −5 against main",
  note: "· the PR will look like this",
  modes: ["Unified", "Split"],
  side_by_side: false,
  side_by_side_from: 900,
  files: [
    change("src/middleware.ts", "+4 −1"),
    change("e2e/login.spec.ts", "+6 −1"),
    change("src/app/login/page.tsx", "+3 −3", { mark: M("+3 −3", true) }),
  ],
  tree: [
    row("e2e", { kind: "folder", quiet: false }),
    row("e2e/login.spec.ts", { depth: 1, mark: M("+6 −1"), quiet: false }),
  ],
  empty: null,
  open_file: "Open file",
  load_diff: "Load diff",
};

/** SHOP-18's comments (`s25-09`): one on line 21, not sent yet. */
const COMMENTS: FilesComments = {
  key: "SHOP-18",
  can_comment: true,
  pending: 1,
  threads: [
    {
      anchor: anchor("src/app/api/checkout/coupon/route.ts", 21),
      comments: [
        {
          id: "c1",
          author: "You",
          meta: "on line 21 · not sent yet",
          pending: true,
          editable: true,
          body: "Expired codes should say “expired”, not “invalid”: the cart shows the message.",
        },
      ],
    },
  ],
  texts: {
    placeholder: "Leave a comment",
    hint: "Goes with Send back, to Dev",
    add: "Add comment",
    save: "Save",
    cancel: "Cancel",
    reply: "Reply",
    edit: "Edit",
    delete: "Delete",
    add_on_line: "Comment on this line",
  },
};

const ignore = () => undefined;

function body(view: FilesTabView, extra: Partial<Parameters<typeof FilesTabBody>[0]> = {}) {
  return renderToStaticMarkup(
    createElement(FilesTabBody, {
      ticket: view.key,
      view,
      comments: undefined,
      state: NOTHING_OPEN,
      onState: ignore,
      reload: ignore,
      ...extra,
    }),
  );
}

describe("the Files tab", () => {
  it("shows an epic's branch with the key to its tags, and a running child's file with whose change it is (FTB-06)", () => {
    const epic: FilesTabView = {
      ...SHOP_22,
      key: "SHOP-40",
      source: {
        kind: "branch",
        title: "Wishlist’s branch",
        subtitle: "otaq/PAY-123 · what its tickets merged",
        legend: [
          { label: "40.1 merged", writing: false },
          { label: "40.2 on top · its own worktree", writing: true },
        ],
      },
      rows: [
        row("src/components/WishlistButton.tsx", {
          quiet: false,
          tags: [{ label: "40.2 ✎", writing: true }],
          child: "SHOP-40.2",
        }),
      ],
    };
    const out = body(epic);
    for (const text of [
      "Wishlist’s branch",
      "otaq/PAY-123 · what its tickets merged",
      "40.1 merged",
      "40.2 ✎",
    ]) {
      expect(out).toContain(text);
    }
    expect(out).toContain('data-writing="true">40.2 on top · its own worktree');
    const page: FilesFilePage = {
      path: "src/components/WishlistButton.tsx",
      child: "SHOP-40.2",
      subtitle: "40.2’s change, on top of the epic’s branch · not merged yet",
      modes: ["File", "Diff"],
      opens_in: "file",
      split_modes: ["Unified", "Split"],
      side_by_side: false,
      side_by_side_from: 900,
      can_file: true,
      can_diff: true,
      new_ticket: "New ticket from this file",
      new_ticket_body: "",
      lines: [],
      note: null,
      writing: "40.2’s agent (Rust developer) is writing this file in 40.2’s own worktree.",
    };
    const file = renderToStaticMarkup(
      createElement(FileBody, {
        ticket: "SHOP-40",
        page,
        file: { path: page.path, child: "SHOP-40.2", mode: "file" },
        ctx: {} as CommentContext,
        width: 1200,
        onMode: ignore,
        onSplit: ignore,
      }),
    );
    expect(file).toContain('class="files-writing"');
    expect(file).toContain("40.2’s agent (Rust developer) is writing this file");
  });

  it("a picked file's Diff keeps Split and has the Unified · Split toggle (dogfood #61)", () => {
    const page: FilesFilePage = {
      path: "src/middleware.ts",
      child: null,
      subtitle: "in SHOP-22’s worktree · +4 −1 against main",
      modes: ["File", "Diff"],
      opens_in: "diff",
      split_modes: ["Unified", "Split"],
      side_by_side: true,
      side_by_side_from: 900,
      can_file: true,
      can_diff: true,
      new_ticket: "New ticket from this file",
      new_ticket_body: "",
      lines: [],
      note: null,
      writing: null,
    };
    const draw = (mode: "file" | "diff") =>
      renderToStaticMarkup(
        createElement(FileBody, {
          ticket: "SHOP-22",
          page,
          file: { path: page.path, child: null, mode },
          ctx: {} as CommentContext,
          width: 1200,
          onMode: ignore,
          onSplit: ignore,
        }),
      );
    const diff = draw("diff");
    expect(diff).toContain('aria-label="Unified / Split"');
    expect(/aria-checked="true"[^>]*>(?:<[^>]*>)*Split</.test(diff)).toBe(true);
    expect(draw("file")).not.toContain('aria-label="Unified / Split"');
  });

  it("registers itself as the sheet's Files tab, and ⌥⌘P shows it with the search box asked for", () => {
    const app = registerScreens({ files });
    expect(registered(app)).toMatchObject({ sheetTabs: ["files"] });
    expect(app.shortcuts.run({ action: "go_to_file" })).toBe(false);
    selection.set({ ...NOTHING_SELECTED, project: "SHOP", ticket: "SHOP-22" });
    const asked = searchFocus.get();
    expect(app.shortcuts.run({ action: "go_to_file" })).toBe(true);
    expect(selection.get().tab).toBe("files");
    expect(searchFocus.get()).toBe(asked + 1);
  });

  it("draws SHOP-22's tree like the frame: the source, the search, All files · Changed · 3", () => {
    const out = body(SHOP_22);
    expect(out).toContain("SHOP-22’s worktree");
    expect(out).toContain("otaq/SHOP-22 · from main");
    expect(out).toContain('placeholder="Search files by name"');
    expect(out).toContain("⌥⌘P");
    expect(out).toContain("All files");
    expect(out).toContain("Changed · 3");
    expect(out).toContain('aria-checked="true"');
    expect(out).toContain('data-quiet="true"');
    expect(out).toContain(SHOP_22.pick);
  });

  it("under Changed shows the changed tree beside All changes, the uncommitted file marked", () => {
    const out = body(SHOP_22, { state: { ...NOTHING_OPEN, filter: "changed" }, changes: CHANGES });
    expect(out).toContain("All changes");
    expect(out).toContain("3 files · +13 −5 against main · the PR will look like this");
    expect(out).toContain("Unified");
    expect(out).toContain("Split");
    expect(out.match(/Open file/g)).toHaveLength(3);
    expect(out).toContain('<span class="files-note">uncommitted</span>');
    expect(out).toContain("login.spec.ts");
  });

  it("shows search matches flat with their folder, the hit marked", () => {
    const found: FilesSearchView = {
      query: "log",
      rows: [
        {
          path: "e2e/login.spec.ts",
          name: [
            { text: "log", hit: true },
            { text: "in.spec.ts", hit: false },
          ],
          folder: [{ text: "e2e", hit: false }],
          mark: M("+6 −1"),
          quiet: false,
          tags: [],
          child: null,
          chips: [],
          branch: null,
        },
      ],
      empty: null,
      more: null,
    };
    const out = renderToStaticMarkup(createElement(SearchResults, { found, selected: 0, onOpen: ignore }));
    expect(out).toContain('<mark class="files-hit">log</mark>');
    expect(out).toContain('<span class="files-folder"><span>e2e</span></span>');
    expect(out).toContain('aria-selected="true"');
    expect(out).toContain("+6 −1");
  });

  it("opens a file as File or Diff with New ticket from this file", () => {
    const page: FilesFilePage = {
      path: "src/middleware.ts",
      child: null,
      subtitle: "in SHOP-22’s worktree · +4 −1 against main",
      modes: ["File", "Diff"],
      opens_in: "diff",
      split_modes: ["Unified", "Split"],
      side_by_side: false,
      side_by_side_from: 900,
      can_file: true,
      can_diff: true,
      new_ticket: "New ticket from this file",
      new_ticket_body: "`src/middleware.ts`\n\n",
      lines: [{ number: 1, spans: [{ text: "import { NextResponse } from 'next/server'", token: "plain" }] }],
      note: null,
      writing: null,
    };
    const ctx = {} as CommentContext;
    const file = { path: page.path, child: null, mode: "file" as const };
    const out = renderToStaticMarkup(
      createElement(FileBody, {
        ticket: "SHOP-22",
        page,
        file,
        ctx,
        width: 1200,
        onMode: ignore,
        onSplit: ignore,
      }),
    );
    expect(out).toContain("<span>src/</span><span><wbr/>middleware.ts</span>");
    expect(out).toContain("in SHOP-22’s worktree · +4 −1 against main");
    expect(out).toContain("New ticket from this file");
    expect(out).toContain("import { NextResponse } from &#x27;next/server&#x27;");
    // An unchanged file has no Diff to switch to.
    const plain = renderToStaticMarkup(
      createElement(FileBody, {
        ticket: "SHOP-22",
        page: { ...page, can_diff: false, opens_in: "file" },
        file: { ...file, mode: null },
        ctx,
        width: 1200,
        onMode: ignore,
        onSplit: ignore,
      }),
    );
    expect(plain).not.toContain('role="radiogroup"');
  });

  it("shows a line comment not sent yet under its line, with the box's words", () => {
    const hunk: DiffHunkView = {
      header: "@@ -18,6 +18,7 @@",
      lines: [
        {
          kind: "added",
          old: null,
          new: 21,
          sign: "+",
          anchor: anchor("src/app/api/checkout/coupon/route.ts", 21),
          spans: [{ text: "  if (!coupon) return", token: "plain" }],
        },
      ],
      rows: [[null, 0]],
    };
    const ctx: CommentContext = {
      ticket: "SHOP-18",
      comments: COMMENTS,
      threads: threadsByAnchor(COMMENTS),
      composer: null,
      dispatch: ignore,
      save: ignore,
    };
    const out = renderToStaticMarkup(createElement(Hunk, { hunk, sideBySide: false, ctx }));
    expect(out).toContain("on line 21 · not sent yet");
    expect(out).toContain("Expired codes should say");
    expect(out).toContain('aria-label="Comment on this line"');
    expect(out).toContain(">Edit<");
    expect(out).toContain(">Reply<");
    // Without comments allowed (a child's change on top), no + and no thread.
    const none = renderToStaticMarkup(createElement(Hunk, { hunk, sideBySide: false, ctx: null }));
    expect(none).not.toContain("Comment on this line");
  });

  it("draws All changes' empty state and Load diff for a collapsed file", () => {
    const out = renderToStaticMarkup(
      createElement(AllChanges, {
        ticket: "SHOP-22",
        changes: {
          ...CHANGES,
          files: [change("package-lock.json", "+900 −2", { collapsed: "Generated file: collapsed." })],
        },
        ctx: {} as CommentContext,
        width: 1000,
        pane: null,
        scrollTo: null,
        onOpenFile: ignore,
        onMode: ignore,
      }),
    );
    expect(out).toContain("Generated file: collapsed.");
    expect(out).toContain("Load diff");
    const empty = renderToStaticMarkup(
      createElement(AllChanges, {
        ticket: "SHOP-31",
        changes: {
          ...CHANGES,
          note: null,
          files: [],
          empty: { title: "No changes", text: "SHOP-31 hasn’t started: nothing has changed yet." },
        },
        ctx: {} as CommentContext,
        width: 1000,
        pane: null,
        scrollTo: null,
        onOpenFile: ignore,
        onMode: ignore,
      }),
    );
    expect(empty).toContain("SHOP-31 hasn’t started: nothing has changed yet.");
  });
});

describe("the review bar (SHT-06)", () => {
  it("reads like the frame for SHOP-18, then with a comment", () => {
    const bar: FilesReviewBar = {
      title: "Your check",
      line: "· Security passed · comment on any line to send it back",
      buttons: [
        { action: "send_back", label: "Send back…", primary: false, keys: null },
        { action: "approve", label: "Approve", primary: true, keys: "⌘↵" },
      ],
    };
    const out = renderToStaticMarkup(createElement(ReviewBarBody, { ticket: "SHOP-18", bar }));
    expect(out).toContain("Your check");
    expect(out).toContain("· Security passed · comment on any line to send it back");
    expect(out.indexOf("Send back…")).toBeLessThan(out.indexOf("Approve"));
    expect(out).toContain("⌘↵");
    const with_ = renderToStaticMarkup(
      createElement(ReviewBarBody, {
        ticket: "SHOP-18",
        bar: {
          title: "1 line comment, 1 being written",
          line: "· they go to Dev with Send back",
          buttons: [
            { action: "approve", label: "Approve", primary: false, keys: null },
            { action: "send_back", label: "Send back with comments", primary: true, keys: null },
          ],
        },
      }),
    );
    expect(with_).toContain("1 line comment, 1 being written");
    expect(with_.indexOf("Approve")).toBeLessThan(with_.indexOf("Send back with comments"));
  });
});

describe("the tab's interaction rules", () => {
  it("opens on Rust's filter until one is picked; opening a file opens its folders", () => {
    expect(shownFilter(NOTHING_OPEN, "changed")).toBe("changed");
    expect(shownFilter({ ...NOTHING_OPEN, filter: "all" }, "changed")).toBe("all");
    expect(folders("src/app/login/page.tsx")).toEqual(["src", "src/app", "src/app/login"]);
    const opened = openFile(
      { ...NOTHING_OPEN, filter: "changed" },
      { path: "src/a/b.ts", child: null, mode: "file" },
    );
    expect(opened.filter).toBe("all");
    expect([...opened.open]).toEqual(["src", "src/a"]);
  });

  it("shows the opened folders' rows under them", () => {
    const top = [row("src", { kind: "folder" }), row("README.md")];
    const inner = new Map([["src", [row("src/a.ts", { depth: 1 })]]]);
    const paths = (open: Set<string>) => visibleRows(top, open, inner).map((r) => r.path);
    expect(paths(new Set())).toEqual(["src", "README.md"]);
    expect(paths(new Set(["src"]))).toEqual(["src", "src/a.ts", "README.md"]);
  });

  it("keeps a picked mode the file has, else Rust's", () => {
    const page = { opens_in: "diff" as const, can_file: true, can_diff: true };
    expect(shownMode(null, page)).toBe("diff");
    expect(shownMode("file", page)).toBe("file");
    expect(shownMode("diff", { ...page, can_diff: false, opens_in: "file" })).toBe("file");
  });

  it("moves the search's row and stops at the ends", () => {
    expect(steppedRow(-1, 3, 1)).toBe(0);
    expect(steppedRow(2, 3, 1)).toBe(2);
    expect(steppedRow(0, 3, -1)).toBe(0);
    expect(steppedRow(0, 0, 1)).toBe(-1);
  });

  it("falls back to Unified below 900 px, shows lines of open text files, cuts long ones", () => {
    expect(showsSideBySide(true, 1000, 900)).toBe(true);
    expect(showsSideBySide(true, 800, 900)).toBe(false);
    const big = change("big.ts", "+900", { collapsed: "Large change: collapsed." });
    expect(showsLines(big, new Set())).toBe(false);
    expect(showsLines(big, new Set(["big.ts"]))).toBe(true);
    expect(
      showsLines(change("a.png", "binary", { binary_label: "Binary file added · 2 KB" }), new Set()),
    ).toBe(false);
    expect(chunks(250)).toEqual([
      [0, 120],
      [120, 240],
      [240, 250],
    ]);
    const a = anchor("a.ts", 3);
    expect(rowAnchors(a, a)).toEqual([a]);
  });
});

describe("the comment box", () => {
  const at = anchor("lib/cart.ts", 15);

  it("adds a new comment, replies in a thread, and edits a pending one", () => {
    const fresh = composerReducer(null, { kind: "open", anchor: at });
    if (!fresh) throw new Error("not open");
    expect(writesNew(fresh)).toBe(true);
    expect(canSave(fresh, "  ")).toBe(false);
    expect(saveRequest(fresh, " Use the shared formatter ", "SHOP-18")).toEqual({
      method: "files.comment_add",
      params: {
        key: "SHOP-18",
        file: "lib/cart.ts",
        line: 15,
        side: "new",
        body: "Use the shared formatter",
      },
    });
    const reply = composerReducer(null, { kind: "open", anchor: at, replyTo: "c1" });
    if (!reply) throw new Error("not open");
    expect(saveRequest(reply, "ok", "SHOP-18")).toEqual({
      method: "files.comment_reply",
      params: { id: "c1", body: "ok" },
    });
    const edit = composerReducer(reply, { kind: "edit", anchor: at, id: "c2", body: "old text" });
    if (!edit) throw new Error("not open");
    expect(writesNew(edit)).toBe(false);
    expect(edit.initial).toBe("old text");
    expect(saveRequest(edit, "new text", "SHOP-18")).toEqual({
      method: "files.comment_edit",
      params: { id: "c2", body: "new text" },
    });
  });

  it("saves once, shows a refusal, and closes when saved or cancelled", () => {
    const open = composerReducer(null, { kind: "open", anchor: at });
    const saving = composerReducer(open, { kind: "saving" });
    expect(saving?.saving).toBe(true);
    expect(canSave(saving, "text")).toBe(false);
    const failed = composerReducer(saving, { kind: "failed", message: "SHOP-18 isn't in your check" });
    expect(failed?.error).toBe("SHOP-18 isn't in your check");
    expect(canSave(failed, "text")).toBe(true);
    expect(composerReducer(failed, { kind: "saved" })).toBeNull();
    expect(composerReducer(open, { kind: "cancel" })).toBeNull();
  });

  it("saves with ⌘↵ or Ctrl+↵ and cancels with Esc", () => {
    expect(composerKey({ key: "Enter", metaKey: true, ctrlKey: false })).toBe("save");
    expect(composerKey({ key: "Enter", metaKey: false, ctrlKey: true })).toBe("save");
    expect(composerKey({ key: "Enter", metaKey: false, ctrlKey: false })).toBeNull();
    expect(composerKey({ key: "Escape", metaKey: false, ctrlKey: false })).toBe("cancel");
  });
});
