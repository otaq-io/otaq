import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { NOTHING_SELECTED, selection } from "../../app/selection";
import type { FilesChangeRow } from "../../generated/FilesChangeRow";
import type { FilesTreeRow } from "../../generated/FilesTreeRow";
import type { FilesViewChanges } from "../../generated/FilesViewChanges";
import type { FilesViewFile } from "../../generated/FilesViewFile";
import type { FilesViewOwner } from "../../generated/FilesViewOwner";
import type { FilesViewPage } from "../../generated/FilesViewPage";
import type { FilesViewPicker } from "../../generated/FilesViewPicker";
import { openFileIn } from "../files/FilesTab";
import { searchFocus } from "../files/logic";
import { AllChangesView } from "./AllChangesView";
import { FileViewer, type FileViewerProps } from "./FileViewer";
import { FilesViewBody, PickerBody } from "./FilesView";
import {
  FIRST,
  diffOwner,
  filesViewWanted,
  goToFile,
  openProjectFile,
  ownerMenu,
  pickFile,
  pickerSections,
  shownFileMode,
  viewSearchFocus,
  viewStates,
  withFile,
  withFilter,
  withPick,
} from "./logic";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => ({ ready: new Promise(() => undefined), stop: () => undefined })),
}));

afterEach(() => {
  selection.set(NOTHING_SELECTED);
  viewStates.set(new Map());
});

const ignore = () => undefined;

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

const PAGE: FilesViewPage = {
  project: "SHOP",
  base_branch: "main",
  source: { title: "main · every ticket’s changes", subtitle: "the base branch · 8 tickets, 11 files" },
  search: { placeholder: "Search files by name", keys: "⌥⌘P" },
  filters: [
    { filter: "all", label: "All", tooltip: null, detail: null },
    { filter: "changed", label: "Changed 11", tooltip: null, detail: null },
    {
      filter: "needs_you",
      label: "Needs you 6",
      tooltip: "Needs you · 6 files",
      detail: "changed by the 4 tickets that need you",
    },
    { filter: "same_file", label: "Same file 1", tooltip: null, detail: null },
  ],
  filter: "all",
  loading: null,
  rows: [],
  empty: null,
  pick: "Pick a file in the tree to read it, or search for one by name.",
  checking: "on main · checking which tickets change it…",
  menu: [],
  working: true,
};

const ROWS = [
  row("src", { kind: "folder", quiet: false, changed_below: "8 changed" }),
  row("src/lib/legacy-price.ts", {
    depth: 2,
    quiet: false,
    struck: true,
    mark: { letter: "D", status: "deleted", counts: "−31", uncommitted: null },
    chips: [{ key: "SHOP-19", colour: "ansi-4" }],
  }),
  row("src/lib/pricing.ts", {
    depth: 2,
    quiet: false,
    mark: { letter: "M", status: "modified", counts: null, uncommitted: null },
    chips: [
      { key: "SHOP-29", colour: "ansi-4" },
      { key: "SHOP-19", colour: "ansi-4" },
    ],
  }),
  row("src/lib/vat.ts", {
    depth: 2,
    quiet: false,
    mark: { letter: "M", status: "modified", counts: "+3 −1", uncommitted: null },
    chips: [{ key: "SHOP-36", colour: "ansi-4" }],
    branch: "release/2.3",
  }),
  row("README.md"),
];

function body(page: FilesViewPage, rows: readonly FilesTreeRow[] = ROWS): string {
  return renderToStaticMarkup(
    createElement(FilesViewBody, {
      project: "SHOP",
      page,
      rows,
      open: new Set(["src"]),
      file: "src/lib/pricing.ts",
      found: null,
      query: "",
      onQuery: ignore,
      selectedMatch: -1,
      onOpenMatch: ignore,
      onRow: ignore,
      onFilter: ignore,
    }),
  );
}

describe("the project Files view (FIL-03, FIL-04, s28-03)", () => {
  it("draws the tree with chips, marks, folder counts and the other base's tag", () => {
    const out = body(PAGE);
    expect(out).toContain('<span class="files-changed-below">8 changed</span>');
    expect(out).toMatch(/data-struck="true".*legacy-price\.ts/);
    expect(out).toMatch(/pricing\.ts.*files-chip">.*ansi-4.*SHOP-29.*files-chip">.*SHOP-19/);
    expect(out).toContain('<span class="files-branch">release/2.3</span>');
    expect(out).toMatch(/aria-current="true".*pricing\.ts/);
    expect(out).toContain('data-quiet="true"');
  });

  it("draws the filters with their counts and the search box with Go to file's key", () => {
    const out = body(PAGE);
    for (const label of ["All", "Changed 11", "Needs you 6", "Same file 1"])
      expect(out).toContain(`>${label}</button>`);
    expect(out).toMatch(/role="radio" class="segment" aria-checked="true" tabindex="0">All</);
    expect(out).toContain('placeholder="Search files by name"');
    expect(out).toContain("⌥⌘P");
  });

  it("says when it is loading and when nothing is changing (s28-09)", () => {
    const loading = body({ ...PAGE, loading: "Loading the tickets’ changes…" });
    expect(loading).toContain("Loading the tickets’ changes…");
    const empty = body(
      {
        ...PAGE,
        source: { ...PAGE.source, subtitle: "the base branch · no ticket is changing files" },
        empty: {
          title: "No ticket is changing files right now.",
          text: "Start one from the board, or ask the Project agent.",
        },
      },
      [],
    );
    expect(empty).toContain("No ticket is changing files right now.");
    expect(empty).toContain("Start one from the board, or ask the Project agent.");
  });
});

const PICKER: FilesViewPicker = {
  placeholder: "Find a ticket or epic",
  sections: [
    {
      title: "BASE BRANCH",
      rows: [
        { pick: { kind: "base" }, key: null, title: "main · every ticket’s changes", detail: "the default" },
      ],
    },
    {
      title: "EPICS · THE EPIC’S BRANCH",
      rows: [
        {
          pick: { kind: "ticket", key: "SHOP-40" },
          key: null,
          title: "Wishlist · SHOP-40",
          detail: "40.1 merged · 40.2 on top",
        },
      ],
    },
    {
      title: "TICKETS · THEIR WORKTREE",
      rows: [
        {
          pick: { kind: "ticket", key: "SHOP-22" },
          key: "SHOP-22",
          title: "Fix flaky login redirect",
          detail: "Dev · 3 files",
        },
        {
          pick: { kind: "ticket", key: "SHOP-29" },
          key: "SHOP-29",
          title: "Coupon field",
          detail: "Dev · 2 files",
        },
      ],
    },
  ],
  no_worktree: "No worktree yet: SHOP-12 (Grill). A ticket gets its worktree when it reaches Dev.",
};

describe("the picker (FIL-07, s28-04)", () => {
  it("lists the base branch, the epics and the tickets, then the ones with no worktree", () => {
    const out = renderToStaticMarkup(
      createElement(PickerBody, { picker: PICKER, query: "", onQuery: ignore, onPick: ignore }),
    );
    expect(out).toMatch(/BASE BRANCH.*EPICS · THE EPIC’S BRANCH.*TICKETS · THEIR WORKTREE/);
    expect(out).toContain("40.1 merged · 40.2 on top");
    expect(out).toContain("No worktree yet: SHOP-12 (Grill).");
    expect(out).toContain('placeholder="Find a ticket or epic"');
  });

  it("filters by key or title as it is typed, dropping empty sections", () => {
    expect(pickerSections(PICKER, "").length).toBe(3);
    const coupon = pickerSections(PICKER, "coupon");
    expect(coupon.map((s) => s.title)).toEqual(["TICKETS · THEIR WORKTREE"]);
    expect(coupon[0]?.rows.map((r) => r.key)).toEqual(["SHOP-29"]);
    expect(pickerSections(PICKER, "shop-40")[0]?.rows[0]?.title).toBe("Wishlist · SHOP-40");
  });
});

describe("your own worktrees (#43)", () => {
  it("picking one shows its tree afresh; the base and a ticket clear it", () => {
    const onBase = { ...FIRST, filter: "changed" as const, file: "src/a.ts", open: new Set(["src"]) };
    const mine = withPick(onBase, { kind: "worktree", path: "/work/habitat-experiment" });
    expect([mine.worktree, mine.picked, mine.file, mine.filter, mine.open.size]).toEqual([
      "/work/habitat-experiment",
      null,
      null,
      "all",
      0,
    ]);
    const same = withPick(
      { ...mine, file: "accent.css" },
      { kind: "worktree", path: "/work/habitat-experiment" },
    );
    expect(same.file).toBe("accent.css");
    expect(withPick(mine, { kind: "base" }).worktree).toBeNull();
    const ticket = withPick(mine, { kind: "ticket", key: "SHOP-29" });
    expect([ticket.worktree, ticket.picked]).toEqual([null, "SHOP-29"]);
  });
});

describe("the view's state and keys (FIL-11)", () => {
  it("picking a file opens the folders above it on All", () => {
    const next = pickFile({ ...FIRST, filter: "needs_you" }, "src/lib/pricing.ts");
    expect(next.filter).toBe("all");
    expect(next.file).toBe("src/lib/pricing.ts");
    expect([...next.open]).toEqual(["src", "src/lib"]);
  });

  it("⌥⌘P asks for the view's search box, the picked tab's when one is picked, and leaves an open sheet to its tab", () => {
    const view = viewSearchFocus.get();
    expect(goToFile("SHOP", false)).toBe(true);
    expect(viewSearchFocus.get()).toBe(view + 1);
    viewStates.set(new Map([["SHOP", { ...FIRST, picked: "SHOP-29" }]]));
    const tab = searchFocus.get();
    expect(goToFile("SHOP", false)).toBe(true);
    expect(searchFocus.get()).toBe(tab + 1);
    expect(goToFile("SHOP", true)).toBe(false);
  });

  it("openFileIn opens the ticket's sheet on Files with the file (FIL-05's Open on Files)", () => {
    selection.set({ ...NOTHING_SELECTED, project: "SHOP" });
    openFileIn("SHOP-29", { path: "src/lib/pricing.ts", child: null, mode: "diff" });
    expect(selection.get()).toMatchObject({ ticket: "SHOP-29", tab: "files" });
  });
});

const owner = (key: string, extra: Partial<FilesViewOwner> = {}): FilesViewOwner => ({
  key,
  title: "Coupon field",
  avatar: null,
  colour: "ansi-4",
  status: "Dev · Working · back from Security",
  palette: "working",
  branch: `otaq/${key} · from main`,
  lines: "lines 41–52",
  counts: "+7 −2",
  diff_subtitle: `${key}’s diff against main · +7 −2`,
  menu_label: `${key} · Coupon field`,
  menu_hint: "Dev · lines 41–52 · +7 −2",
  ...extra,
});

const span = (text: string) => [{ text, token: "plain" as const }];

const PRICING: FilesViewFile = {
  path: "src/lib/pricing.ts",
  short_path: null,
  struck: false,
  subtitle: "on main · 2 tickets change it · read-only",
  narrow_subtitle: "on main · read-only",
  modes: ["File", "Diff"],
  opens_in: "file",
  can_file: true,
  can_diff: true,
  new_ticket: "New ticket from this file",
  new_ticket_body: "`src/lib/pricing.ts`\n\n",
  copy_path: "Copy path",
  same_file: [
    "SHOP-19 and SHOP-29 both edit applyDiscount (lines 41–58). Whichever merges second will rebase on the other.",
  ],
  other_base: null,
  huge: null,
  bars: [
    { key: "SHOP-29", colour: "ansi-4" },
    { key: "SHOP-19", colour: "ansi-5" },
  ],
  rows: [
    { kind: "line", number: 40, spans: span("/** Applies a coupon */"), bars: ["none", "none"] },
    { kind: "line", number: 47, spans: span("? total.times(coupon.value / 100)"), bars: ["line", "line"] },
    { kind: "fold", label: "⋯ 2,300 lines unchanged" },
  ],
  note: null,
  binary: null,
  no_preview: null,
  diff: null,
  diff_of: null,
  owners: [
    owner("SHOP-29"),
    owner("SHOP-19", {
      title: "Price table",
      colour: "ansi-5",
      status: "Security · Checking · round 1 of 3",
      lines: "lines 47–58",
      counts: "+5 −2",
      menu_label: "SHOP-19 · Price table",
      menu_hint: "Security · lines 47–58 · +5 −2",
    }),
  ],
  changed_by: "Changed by",
  changed_by_hint:
    "Click a ticket to open its sheet on the Files tab. Diff shows one ticket’s change at a time.",
  open_on_files: "Open on Files",
  open: "Open",
  diff_default: "SHOP-29",
  of: "of",
  diff_title: "Diff against main",
};

function viewer(view: FilesViewFile, extra: Partial<FileViewerProps> = {}): string {
  return renderToStaticMarkup(
    createElement(FileViewer, {
      view,
      mode: "file",
      owner: "SHOP-29",
      diff: undefined,
      onMode: ignore,
      onOwner: ignore,
      onNewTicket: ignore,
      onCopyPath: ignore,
      onOpen: ignore,
      ...extra,
    }),
  );
}

describe("a picked file (FIL-05, FIL-06, FIL-09, s28-03, s28-08)", () => {
  it("draws the file with a bar per ticket, the same-file note and Changed by, without line comments", () => {
    const out = viewer(PRICING);
    expect(out).toContain("<wbr/>pricing.ts</span>");
    expect(out).toContain('<span class="files-view-wide">on main · 2 tickets change it · read-only</span>');
    expect(out).toContain('<span class="files-view-narrow">on main · read-only</span>');
    expect(out).toContain("SHOP-19 and SHOP-29 both edit applyDiscount (lines 41–58).");
    expect(out).toMatch(
      /data-bar="line" style="color:var\(--ansi-4\)".*data-bar="line" style="color:var\(--ansi-5\)"/,
    );
    expect(out).toContain("⋯ 2,300 lines unchanged");
    expect(out).toContain('<span class="files-view-by-title">Changed by</span>');
    expect(out).toContain("Dev · Working · back from Security");
    expect(out).toContain("otaq/SHOP-29 · from main");
    expect(out).toContain("lines 47–58");
    expect(out.match(/Open on Files/g)?.length).toBe(2);
    expect(out).toContain("Click a ticket to open its sheet on the Files tab.");
    // New ticket from this file: the button, and the + of the narrow layout; Copy path.
    expect(out).toContain("<span>New ticket from this file</span>");
    expect(out).toContain('aria-label="New ticket from this file"');
    expect(out).toContain('aria-label="Copy path"');
    // The narrow line of chips: each ticket with its lines, then Open.
    expect(out).toMatch(/files-view-chips.*SHOP-29.*lines 41–52.*SHOP-19.*lines 47–58.*>Open</);
    expect(out).not.toContain("data-commentable");
  });

  it("names Diff's ticket with of SHOP-29 ▾ and its subtitle", () => {
    const out = viewer(PRICING, {
      mode: "diff",
      diff: { state: "loaded", view: { path: "src/lib/pricing.ts", hunks: [], truncated: null } },
    });
    expect(out).toContain("SHOP-29’s diff against main · +7 −2");
    expect(out).toMatch(/files-view-of.*<span>of<\/span>.*SHOP-29/);
    expect(out).not.toContain("⋯ 2,300 lines unchanged");
  });

  it("a file only on another base has File off and previews as its ticket's diff (s28-05)", () => {
    const view: FilesViewFile = {
      ...PRICING,
      path: "src/lib/vat-table.ts",
      subtitle: "not on main · only on release/2.3 · SHOP-36’s diff",
      narrow_subtitle: "not on main · only on release/2.3 · SHOP-36’s diff",
      opens_in: "diff",
      can_file: false,
      same_file: [],
      other_base: "SHOP-36 works on release/2.3, not main.",
      bars: [],
      rows: [],
      owners: [owner("SHOP-36")],
      diff_of: "SHOP-36",
      diff_default: "SHOP-36",
    };
    expect(shownFileMode(null, view)).toBe("diff");
    expect(shownFileMode("file", view)).toBe("diff");
    const out = viewer(view, { mode: "diff", owner: "SHOP-36" });
    expect(out).toContain("not on main · only on release/2.3 · SHOP-36’s diff");
    expect(out).toContain("SHOP-36 works on release/2.3, not main.");
    expect(out).not.toContain('role="radiogroup"');
    expect(out).not.toContain("files-view-of");
  });

  it("deleted, binary, huge and long paths (s28-09, s28-10)", () => {
    const deleted = viewer({
      ...PRICING,
      struck: true,
      subtitle: "deleted by SHOP-19 · shown as it is on main",
    });
    expect(deleted).toContain('data-struck="true"');
    expect(deleted).toContain("deleted by SHOP-19 · shown as it is on main");
    const binary = viewer({
      ...PRICING,
      rows: [],
      binary: "Binary file · 182 KB on main, 164 KB in SHOP-26",
      no_preview: "No preview",
    });
    expect(binary).toContain("Binary file · 182 KB on main, 164 KB in SHOP-26");
    expect(binary).toContain("No preview");
    const huge = viewer({
      ...PRICING,
      huge: "12,400 lines: only the parts tickets change are shown, with the lines between folded.",
    });
    expect(huge).toContain("12,400 lines: only the parts tickets change are shown");
    const long = viewer({
      ...PRICING,
      path: "src/app/(storefront)/[locale]/(shop)/checkout/[step]/AddressField.tsx",
      short_path: "src/app/(storefront)/…/checkout/[step]/AddressField.tsx",
    });
    expect(long).toContain("src/app/(storefront)/…/checkout/[step]/AddressField.tsx");
  });

  it("Diff starts on the file's first ticket and keeps a chosen one while it still changes the file", () => {
    expect(diffOwner(null, PRICING)).toBe("SHOP-29");
    expect(diffOwner("SHOP-19", PRICING)).toBe("SHOP-19");
    expect(diffOwner("SHOP-99", PRICING)).toBe("SHOP-29");
    const menu = ownerMenu(PRICING, "SHOP-19");
    expect(menu.map((m) => [m.label, m.hint, m.checked, m.header])).toEqual([
      ["Diff against main", null, false, true],
      ["SHOP-29 · Coupon field", "Dev · lines 41–52 · +7 −2", false, false],
      ["SHOP-19 · Price table", "Security · lines 47–58 · +5 −2", true, false],
    ]);
    expect(shownFileMode(null, PRICING)).toBe("file");
    expect(shownFileMode("diff", { ...PRICING, can_diff: false })).toBe("file");
  });

  it("picking another file starts it afresh; Changed drops the file for All changes (FIL-08)", () => {
    const state = { ...FIRST, file: "a.ts", mode: "diff" as const, diffOf: "SHOP-19" };
    expect(withFile(state, "a.ts")).toBe(state);
    expect(withFile(state, "b.ts")).toMatchObject({ file: "b.ts", mode: null, diffOf: null });
    expect(withFilter(state, "changed")).toMatchObject({ filter: "changed", file: null });
    expect(withFilter(state, "needs_you")).toMatchObject({ filter: "needs_you", file: "a.ts" });
  });

  it("the file pane shows the picked file in place of the pick line", () => {
    const out = renderToStaticMarkup(
      createElement(FilesViewBody, {
        project: "SHOP",
        page: PAGE,
        rows: ROWS,
        open: new Set(["src"]),
        file: "src/lib/pricing.ts",
        found: null,
        query: "",
        onQuery: ignore,
        selectedMatch: -1,
        onOpenMatch: ignore,
        onRow: ignore,
        onFilter: ignore,
        pane: createElement("p", null, "the viewer"),
      }),
    );
    expect(out).toContain("the viewer");
    expect(out).not.toContain(PAGE.pick);
  });
});

const change = (path: string, extra: Partial<FilesChangeRow> = {}): FilesChangeRow => ({
  id: path,
  path,
  label: path,
  child: null,
  mark: { letter: "M", status: "modified", counts: "+4 −1", uncommitted: null },
  note: null,
  collapsed: null,
  binary_label: null,
  rename_label: null,
  rows: 5,
  can_open: true,
  commentable: false,
  tags: [],
  ...extra,
});

const CHANGES: FilesViewChanges = {
  project: "SHOP",
  title: "All changes",
  summary: "8 tickets · 11 files against main · grouped by ticket, the ones that need you first",
  modes: ["Unified", "Split"],
  side_by_side: false,
  side_by_side_from: 900,
  groups: [
    {
      key: "SHOP-22",
      title: "Fix flaky login redirect",
      avatar: null,
      colour: "ansi-6",
      status: "Dev · Needs you: allow rm -rf .next?",
      palette: "attn",
      summary: "3 files · +13 −5",
      open: true,
      unfolded: "src/middleware.ts",
      files: [change("src/middleware.ts"), change("e2e/login.spec.ts")],
    },
    {
      key: "SHOP-29",
      title: "Coupon field",
      avatar: null,
      colour: "ansi-4",
      status: "Dev · Working · back from Security",
      palette: "working",
      summary: "1 file · +7 −2",
      open: false,
      unfolded: null,
      files: [change("src/lib/pricing.ts")],
    },
  ],
  empty: null,
  open: "Open",
  open_file: "Open file",
  load_diff: "Load diff",
};

describe("All changes, by ticket (FIL-08, s28-06)", () => {
  it("groups the tickets, the first open on its first file, the others folded, without line comments", () => {
    const out = renderToStaticMarkup(
      createElement(AllChangesView, {
        changes: CHANGES,
        pane: null,
        width: 900,
        onMode: ignore,
        onOpenFile: ignore,
        onOpen: ignore,
      }),
    );
    expect(out).toContain(
      "8 tickets · 11 files against main · grouped by ticket, the ones that need you first",
    );
    expect(out).toMatch(
      /SHOP-22.*Fix flaky login redirect.*· Dev · Needs you: allow rm -rf \.next\?.*3 files · \+13 −5/,
    );
    expect(out).toMatch(/SHOP-22.*aria-expanded="true".*SHOP-29.*aria-expanded="false"/s);
    // Its first file open (its card drawn), the second folded to its line; SHOP-29's files not drawn.
    expect(out.match(/class="files-card"/g)?.length).toBe(1);
    expect(out).toContain("e2e/login.spec.ts");
    expect(out).not.toContain("src/lib/pricing.ts");
    expect(out.match(/>Open</g)?.length).toBe(2);
    expect(out).toContain(">Unified</button>");
    expect(out).not.toContain("data-commentable");
  });
});

describe("a terminal's path link (d0117 point 8)", () => {
  it("opens the base's file as File at the line, closing an open sheet, and asks the board for the view", () => {
    selection.set({ ...NOTHING_SELECTED, project: "SHOP", ticket: "SHOP-3" });
    viewStates.set(new Map([["SHOP", { ...FIRST, picked: "SHOP-29", file: "a.ts", mode: "diff" as const }]]));
    const before = filesViewWanted.get()?.n ?? 0;
    openProjectFile("SHOP", "src/lib/pricing.ts", 47);
    const state = viewStates.get().get("SHOP");
    expect(state).toMatchObject({
      file: "src/lib/pricing.ts",
      mode: "file",
      picked: null,
      filter: "all",
      line: { line: 47 },
    });
    expect(state?.open.has("src/lib")).toBe(true);
    expect(selection.get().ticket).toBeNull();
    expect(filesViewWanted.get()).toEqual({ project: "SHOP", n: before + 1 });
    // Without a line: as it opens, nothing marked.
    openProjectFile("SHOP", "README.md", null);
    expect(viewStates.get().get("SHOP")).toMatchObject({ file: "README.md", mode: null, line: null });
  });

  it("marks the asked line in File, not in Diff", () => {
    expect(viewer(PRICING, { line: { line: 47 } })).toMatch(/data-line="47" data-marked="true"/);
    expect(viewer(PRICING, { line: { line: 47 } })).not.toMatch(/data-line="40" data-marked/);
    expect(viewer(PRICING, { line: { line: 47 }, mode: "diff" })).not.toContain("data-marked");
  });
});
