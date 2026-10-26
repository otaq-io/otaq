// The demo's sample data, as in the canvas's Demo3.dc.html script (decisions 0102, 0137): two projects with their
// flows, their tickets and Noyan's first conversation. Texts are copied exactly, built as drawn; the only change is the
// Terminals key, which shows the app's real shortcut (0137). Refresh them by hand when the canvas changes.

export type LineClass = "u" | "a" | "tool" | "o" | "d" | "ok" | "e" | "q";

/** One line of a terminal: its kind (the drawn `.tl` class) and its text. */
export interface Line {
  c: LineClass;
  t: string;
}

export const L = (c: LineClass, t: string): Line => ({ c, t });

export type ToolId = "CC" | "CX" | "CP" | "OC";

export const TOOL_NAMES: Record<ToolId, string> = {
  CC: "Claude Code",
  CX: "Codex",
  CP: "Copilot",
  OC: "OpenCode",
};

/** The model each agent runs, for the ticket sheet's Model line. */
export const MODELS: Readonly<Record<string, string>> = {
  "Rust developer": "opus",
  "Spec writer": "opus",
  "Security reviewer": "opus · effort high",
  Validator: "gpt-sol",
  "Docs updater": "haiku",
  Shipper: "default model",
  Developer: "opus",
};

// --- flows ---------------------------------------------------------------------------------------------------------

export type StageKind = "wait" | "agent" | "check" | "you" | "delivery";

/** A stage kind's colour on the board (`KC`). */
export const KIND_COLOR: Record<StageKind, string> = {
  wait: "var(--q)",
  agent: "var(--work)",
  check: "var(--inv)",
  you: "var(--rev)",
  delivery: "var(--ok)",
};

/** A stage kind's short label in a column head (`KIND`). */
export const KIND: Record<StageKind, string> = {
  wait: "waits",
  agent: "agent",
  check: "check",
  you: "you",
  delivery: "delivery",
};

/** A stage kind's long label in the flow editor's details (`KINDL`). */
export const KIND_LONG: Record<StageKind, string> = {
  wait: "Waits for an agent",
  agent: "Agent works",
  check: "Check · an agent",
  you: "Check · you",
  delivery: "Delivery",
};

export type StageId =
  "ready" | "dev" | "security" | "validate" | "review" | "ship" | "pspec" | "tspec" | "plan" | "sreview";

export interface Stage {
  id: StageId;
  name: string;
  kind: StageKind;
  who: string;
  skill: string;
  fail: string;
}

export interface Flow {
  name: string;
  desc: string;
  stages: readonly Stage[];
}

export type FlowId = "shop" | "default" | "solo" | "team" | "spec";

const S = (id: StageId, name: string, kind: StageKind, who: string, skill = "—", fail = "—"): Stage => ({
  id,
  name,
  kind,
  who,
  skill,
  fail,
});

export const FLOWS: Record<FlowId, Flow> = {
  shop: {
    name: "shop-web’s flow",
    desc: "In use · the board’s columns",
    stages: [
      S("ready", "Ready for Dev", "wait", "—"),
      S("dev", "Dev", "agent", "Rust developer", "/work-ticket"),
      S("security", "Security", "check", "Security reviewer", "/security-review", "Dev"),
      S("validate", "Validate", "check", "Validator", "/validate", "Dev"),
      S("review", "Review", "you", "You", "—", "Dev"),
      S("ship", "Ship", "delivery", "Shipper · opens a PR", "/open-pr"),
    ],
  },
  default: {
    name: "Otaq default",
    desc: "Agents build and check; you review",
    stages: [
      S("ready", "Ready for Dev", "wait", "—"),
      S("dev", "Dev", "agent", "Developer", "/work-ticket"),
      S("validate", "Validate", "check", "Validator", "/validate", "Dev"),
      S("review", "Review", "you", "You", "—", "Dev"),
      S("ship", "Ship", "delivery", "local merge, on your click"),
    ],
  },
  solo: {
    name: "Solo autopilot",
    desc: "No review; merges when checks pass",
    stages: [
      S("ready", "Ready for Dev", "wait", "—"),
      S("dev", "Dev", "agent", "Developer", "/work-ticket"),
      S("validate", "Validate", "check", "Validator", "/validate", "Dev"),
      S("ship", "Ship", "delivery", "merges on its own once Validate passes"),
    ],
  },
  team: {
    name: "Team PR",
    desc: "Your team reviews the PR",
    stages: [
      S("ready", "Ready for Dev", "wait", "—"),
      S("dev", "Dev", "agent", "Developer", "/work-ticket"),
      S("validate", "Validate", "check", "Validator", "/validate", "Dev"),
      S("ship", "Ship", "delivery", "opens a PR with gh", "/open-pr"),
    ],
  },
  spec: {
    name: "Spec first",
    desc: "Plan on the board, then build",
    stages: [
      S("pspec", "Product spec", "agent", "Spec writer", "/product-spec"),
      S("tspec", "Tech spec", "agent", "Architect", "/tech-spec"),
      S("plan", "Milestone plan", "agent", "Milestone planner", "/milestone-plan"),
      S("sreview", "Spec review", "you", "You", "—", "Product spec"),
      S("dev", "Dev", "agent", "Developer", "/work-ticket"),
      S("validate", "Validate", "check", "Validator", "/validate", "Dev"),
      S("review", "Code review", "you", "You", "—", "Dev"),
      S("ship", "Ship", "delivery", "local merge, on your click"),
    ],
  },
};

/** The presets in the flow editor's "Start from" list, in order; the project's own flow only on its project. */
export const PRESET_IDS: readonly FlowId[] = ["shop", "default", "solo", "team", "spec"];

// --- projects ------------------------------------------------------------------------------------------------------

export type ProjectKey = "SHOP" | "BILL";

export interface Project {
  key: ProjectKey;
  name: string;
  branch: string;
  flow: FlowId;
}

export const PROJECTS: readonly Project[] = [
  { key: "SHOP", name: "shop-web", branch: "main", flow: "shop" },
  { key: "BILL", name: "billing-api", branch: "main", flow: "default" },
];

export function project(key: ProjectKey): Project {
  const p = PROJECTS.find((x) => x.key === key);
  if (!p) throw new Error(`no project ${key}`);
  return p;
}

// --- tickets -------------------------------------------------------------------------------------------------------

/** Where a ticket is: Backlog, one of its flow's stages, or Done. */
export type Column = StageId | "backlog" | "done";

export type TicketStatus =
  | "queued"
  | "working"
  | "working2"
  | "attn"
  | "helpers"
  | "checking"
  | "checking2"
  | "failed"
  | "review"
  | "merging"
  | "pr"
  | "done";

/** What answering an option does (`act` in the canvas). */
export type Answer = "g1" | "p22" | "d22" | "h40" | "b7" | "r32" | "m32";

export interface Option {
  label: string;
  rec: string;
  act: Answer;
}

/** What a ticket waits for from you: a question, a permission prompt, or a decision after failed checks. */
export interface TicketAsk {
  kind: "ask" | "perm" | "decide";
  short: string;
  title: string;
  opts: readonly Option[];
}

/** A piece of a ticket's Plan; `cls` is its drawn status class. */
export interface Piece {
  n: string;
  t: string;
  s: string;
  cls: "" | "ok" | "a" | "r" | "w";
}

export interface CheckResult {
  t: string;
  s: string;
  color: string;
}

/** A diff line: its class (`hunk`, `add-l`, `del-l`), its line number and its text. */
export type DiffLine = readonly [c: "hunk" | "add-l" | "del-l", n: string, t: string];

export interface DiffFile {
  name: string;
  a: number;
  d: number;
  lines: readonly DiffLine[];
}

export interface HistoryEntry {
  at: string;
  t: string;
}

export type Priority = 1 | 2 | 3 | 4;

export interface Ticket {
  k: string;
  p: ProjectKey;
  title: string;
  col: Column;
  st: TicketStatus;
  prio: Priority;
  tool: ToolId;
  agent: string;
  tokens: string;
  ago: string;
  log: readonly Line[];
  acc: readonly string[];
  hist: readonly HistoryEntry[];
  waits: readonly string[];
  desc?: string;
  ask?: TicketAsk | null;
  pieces?: readonly Piece[];
  checks?: readonly CheckResult[];
  decisionT?: string;
  decision?: string;
  manual?: readonly string[];
  adds?: number;
  dels?: number;
  files?: readonly DiffFile[];
  /** Who filed it, when not you. */
  by?: string;
}

type Defaults = "log" | "acc" | "hist" | "waits" | "prio" | "tool" | "agent" | "tokens" | "ago";
type TicketInit = Omit<Ticket, Defaults> & Partial<Pick<Ticket, Defaults>>;

/** A ticket with the canvas's defaults (`T()` in its script). */
export function T(o: TicketInit): Ticket {
  return {
    log: [],
    acc: [],
    hist: [],
    waits: [],
    prio: 3,
    tool: "CC",
    agent: "Rust developer",
    tokens: "",
    ago: "",
    ...o,
  };
}

const passed = "var(--ok)";

export function initialTickets(): Ticket[] {
  return [
    T({
      k: "SHOP-31",
      p: "SHOP",
      title: "Hotfix: checkout crash",
      col: "backlog",
      st: "queued",
      prio: 1,
      desc: "Checkout throws when the cart has a deleted product. Noyan filed this from the error log.",
      acc: ["A deleted product is skipped with a notice", "Covered by a test"],
    }),
    T({
      k: "SHOP-41",
      p: "SHOP",
      title: "Fix README typo",
      col: "backlog",
      st: "queued",
      prio: 4,
      agent: "Docs updater",
      desc: "“recieve” in the install section.",
      acc: ["Spelled right"],
    }),
    T({
      k: "SHOP-20",
      p: "SHOP",
      title: "Checkout page",
      col: "ready",
      st: "queued",
      prio: 2,
      waits: ["SHOP-29"],
      desc: "The checkout page, with the coupon field from SHOP-29. Acceptance: uses the coupon endpoint; shows out-of-stock items by name.",
    }),
    T({
      k: "SHOP-21",
      p: "SHOP",
      title: "Saved cards at checkout",
      col: "ready",
      st: "queued",
      prio: 2,
      waits: ["SHOP-18"],
      desc: "Signed-in customers pick a saved card. Needs the Checkout API from SHOP-18. Acceptance: lists saved cards; a new card can still be added.",
    }),
    T({
      k: "SHOP-12",
      p: "SHOP",
      title: "Add dark mode toggle",
      col: "dev",
      st: "attn",
      prio: 3,
      tokens: "9k",
      ago: "8m",
      ask: {
        kind: "ask",
        short: "a question",
        title: "Rust developer asks you",
        opts: [
          { label: "Follow the system setting, with a manual override", rec: "Recommended", act: "g1" },
          { label: "Manual toggle only", rec: "", act: "g1" },
        ],
      },
      desc: "A toggle in the account menu that switches the whole app between light and dark.",
      acc: ["Remembered across sessions", "No flash of the wrong theme on load"],
      log: [
        L("d", "Claude Code · Rust developer · ~/.otaq/wt/shop-web/SHOP-12"),
        L("tool", "otaq show SHOP-12"),
        L("a", "The ticket doesn’t say what the toggle follows. Asking before I wire it."),
        L("q", "Should dark mode follow the system setting, or be a manual toggle only?"),
      ],
    }),
    T({
      k: "SHOP-44",
      p: "SHOP",
      title: "Saved carts",
      col: "ready",
      st: "queued",
      prio: 3,
      desc: "Keep a signed-in customer’s cart across devices.",
      acc: ["Cart survives sign-out and sign-in"],
    }),
    T({
      k: "SHOP-22",
      p: "SHOP",
      title: "Fix flaky login redirect",
      col: "dev",
      st: "attn",
      prio: 2,
      tokens: "31k",
      ago: "2m",
      ask: {
        kind: "perm",
        short: "allow a command",
        title: "Permission prompt · Claude Code",
        opts: [
          { label: "Allow rm -rf .next && pnpm build", rec: "once", act: "p22" },
          { label: "Deny", rec: "", act: "d22" },
        ],
      },
      desc: "About 1 in 20 sign-ins land on the home page instead of the page they came from.",
      acc: ["The return URL survives the OAuth round trip"],
      log: [
        L("d", "Claude Code · Rust developer · ~/.otaq/wt/shop-web/SHOP-22"),
        L("tool", "Read app/auth/callback/route.ts"),
        L(
          "a",
          "The state cookie expires before slow providers come back. Fixing the TTL, then a clean build to check.",
        ),
        L("q", "Allow rm -rf .next && pnpm build?"),
      ],
    }),
    T({
      k: "SHOP-33",
      p: "SHOP",
      title: "Shipping estimate",
      col: "done",
      st: "done",
      prio: 3,
      tokens: "18k",
      desc: "An estimate on the cart page from the carrier API.",
      acc: ["Shows a range, not a date"],
      log: [
        L("d", "Claude Code · Rust developer"),
        L("tool", "Edit lib/shipping.ts  +42 −3"),
        L("a", "Writing the estimate endpoint."),
      ],
    }),
    T({
      k: "SHOP-29",
      p: "SHOP",
      title: "Coupon field",
      col: "security",
      st: "checking2",
      prio: 3,
      agent: "Security reviewer",
      tokens: "27k",
      ago: "40s",
      desc: "A coupon field at checkout. One coupon per order.",
      acc: ["Invalid codes say why", "One coupon per order"],
      log: [
        L("d", "Claude Code · Rust developer"),
        L(
          "e",
          "Round 1 failed: the coupon code reached SQL unescaped. Fixed in Dev with a parameterized query.",
        ),
        L("d", "Security reviewer · round 2 of 3 · checking the fix"),
      ],
    }),
    T({
      k: "SHOP-40",
      p: "SHOP",
      title: "Wishlist",
      col: "dev",
      st: "helpers",
      prio: 2,
      tokens: "61k · helpers 120k",
      ago: "12m",
      pieces: [
        { n: "40.1", t: "Wishlist API", s: "merged", cls: "ok" },
        { n: "40.2", t: "Wishlist button", s: "asks you", cls: "a" },
        { n: "40.3", t: "Wishlist page", s: "being reviewed", cls: "r" },
        { n: "40.4", t: "Share link", s: "waits for 40.2, 40.3", cls: "" },
      ],
      ask: {
        kind: "ask",
        short: "40.2 asks",
        title: "40.2 asks you · passed on by the agent in charge",
        opts: [
          { label: "Keep the heart filled after sign-out", rec: "Recommended", act: "h40" },
          { label: "Clear it on sign-out", rec: "", act: "h40" },
        ],
      },
      desc: "Customers save products to a wishlist. Planned at Spec review: 4 pieces.",
      acc: ["Signed-in users only (decision 3)", "Share link works without sign-in"],
      log: [
        L("d", "Claude Code · Rust developer · in charge · 1 in charge · 3 helpers"),
        L("a", "40.1 merged. 40.2 and 40.3 are working in their own worktrees."),
        L(
          "o",
          "40.2 asked: keep the heart filled after sign-out? Not in the plan or DESIGN.md, passing it to you.",
        ),
      ],
    }),
    T({
      k: "SHOP-19",
      p: "SHOP",
      title: "Price table",
      col: "security",
      st: "checking",
      prio: 3,
      agent: "Security reviewer",
      tokens: "12k",
      ago: "40s",
      desc: "A price table component for the plans page.",
      acc: ["Prices from the API, never hard-coded"],
      log: [
        L("d", "Security reviewer · Claude Code · round 1 of 3"),
        L("tool", "/security-review"),
        L("a", "Reading the 6 changed files for anything that touches auth, SQL or secrets."),
      ],
    }),
    T({
      k: "SHOP-32",
      p: "SHOP",
      title: "Address autocomplete",
      col: "security",
      st: "failed",
      prio: 3,
      agent: "Security reviewer",
      tokens: "44k",
      ago: "20m",
      ask: {
        kind: "decide",
        short: "decide",
        title: "Security failed 3 times · unescaped SQL",
        opts: [
          { label: "Give it another round", rec: "", act: "r32" },
          { label: "Move on anyway", rec: "", act: "m32" },
        ],
      },
      desc: "Suggest addresses as the customer types.",
      acc: ["Debounced", "No request under 3 characters"],
      log: [
        L("d", "Security reviewer · round 3 of 3"),
        L(
          "e",
          "Failed: the query parameter is concatenated into SQL in lib/address.ts. Same finding as rounds 1 and 2.",
        ),
      ],
    }),
    T({
      k: "SHOP-18",
      p: "SHOP",
      title: "Checkout API",
      col: "review",
      st: "review",
      prio: 1,
      tokens: "182k",
      ago: "25m",
      adds: 412,
      dels: 38,
      checks: [
        { t: "Security · passed · round 1", s: "No raw SQL, secrets or auth changes.", color: passed },
        {
          t: "Validate · passed · round 2",
          s: "Round 1 found a missing 409 for out-of-stock items. Fixed in Dev, then passed.",
          color: passed,
        },
      ],
      decisionT: "Totals are recomputed on the server",
      decision:
        "The client total is ignored. The other option, trusting the cart and only checking it, lets a stale cart through.",
      manual: [
        "Pay with a card that gets declined, on the real gateway.",
        "Check the order email renders in Outlook.",
      ],
      desc: "POST /api/checkout takes a cart, checks stock, creates the order and returns a payment intent.",
      acc: [
        "Out-of-stock items fail with 409 and name the item",
        "Totals are computed on the server",
        "Integration tests",
      ],
      log: [
        L("a", "Submitted SHOP-18. Security and Validate passed; it’s in Review."),
        L("ok", "✓ Security · round 1 passed"),
        L("ok", "✓ Validate · round 2 passed"),
      ],
      files: [
        {
          name: "app/api/checkout/route.ts",
          a: 140,
          d: 12,
          lines: [
            ["hunk", "", "@@ -20,7 +20,11 @@"],
            ["del-l", "21", "  const total = body.total;"],
            ["add-l", "21", "  const total = priceCart(cart, prices);"],
            ["add-l", "22", "  const missing = cart.filter((i) => !inStock(i));"],
            ["add-l", "23", "  if (missing.length) return conflict(missing);"],
          ],
        },
        {
          name: "tests/checkout.test.ts",
          a: 212,
          d: 0,
          lines: [
            ["hunk", "", "@@ -0,0 +1,212 @@"],
            ["add-l", "1", "test('409 names the out-of-stock item', …"],
          ],
        },
      ],
      hist: [
        { at: "9:02 am", t: "Started by you · Ready → Dev" },
        { at: "9:41 am", t: "Submitted · Security round 1 passed" },
        { at: "9:50 am", t: "Validate round 1 failed: missing 409 · back to Dev" },
        { at: "10:05 am", t: "Validate round 2 passed · in Review" },
      ],
    }),
    T({
      k: "SHOP-23",
      p: "SHOP",
      title: "Split user full name",
      col: "review",
      st: "review",
      prio: 3,
      tokens: "21k",
      ago: "1h",
      adds: 38,
      dels: 20,
      checks: [
        { t: "Security · passed", s: "Nothing risky.", color: passed },
        { t: "Validate · passed · round 1", s: "Migration and tests check out.", color: passed },
      ],
      manual: ["Open a profile with a one-word name."],
      desc: "Store first and last name separately.",
      acc: ["Migration keeps existing names"],
      log: [L("ok", "✓ Both checks passed · in Review")],
      files: [
        {
          name: "prisma/schema.prisma",
          a: 2,
          d: 1,
          lines: [
            ["del-l", "14", "  fullName  String"],
            ["add-l", "14", "  firstName String"],
            ["add-l", "15", "  lastName  String"],
          ],
        },
      ],
    }),
    T({
      k: "SHOP-26",
      p: "SHOP",
      title: "Add sitemap",
      col: "ship",
      st: "pr",
      prio: 4,
      tool: "OC",
      agent: "Shipper",
      tokens: "6k",
      ago: "2h",
      desc: "sitemap.xml for the product pages.",
      acc: ["One entry per product"],
      log: [L("ok", "PR #481 opened with gh · your team reviews it there")],
    }),
    T({
      k: "SHOP-2",
      p: "SHOP",
      title: "Sign in with GitHub",
      col: "done",
      st: "done",
      prio: 2,
      log: [L("ok", "Merged through PR #448")],
    }),
    T({
      k: "SHOP-9",
      p: "SHOP",
      title: "Order confirmation email",
      col: "done",
      st: "done",
      prio: 3,
      log: [L("ok", "Merged · squash 7d3a910")],
    }),
    T({
      k: "SHOP-11",
      p: "SHOP",
      title: "Product search",
      col: "done",
      st: "done",
      prio: 2,
      log: [L("ok", "Merged through PR #462")],
    }),
    T({
      k: "SHOP-15",
      p: "SHOP",
      title: "Rate limiting on /api",
      col: "done",
      st: "done",
      prio: 2,
      log: [L("ok", "Merged · squash 1b8e3aa")],
    }),
    T({
      k: "BILL-7",
      p: "BILL",
      title: "Proration on plan change",
      col: "dev",
      st: "attn",
      prio: 2,
      tokens: "22k",
      ago: "5m",
      ask: {
        kind: "ask",
        short: "a question",
        title: "Rust developer asks you",
        opts: [
          { label: "Prorate to the day", rec: "Recommended", act: "b7" },
          { label: "Charge the full month", rec: "", act: "b7" },
        ],
      },
      desc: "Charge the difference when a customer upgrades mid-month.",
      acc: ["Invoice line says what was prorated"],
      log: [
        L("d", "Claude Code · Rust developer · ~/.otaq/wt/billing-api/BILL-7"),
        L("q", "Prorate to the day, or charge the full month on upgrade?"),
      ],
    }),
    T({
      k: "BILL-3",
      p: "BILL",
      title: "Invoice PDF",
      col: "review",
      st: "review",
      prio: 3,
      tokens: "58k",
      ago: "40m",
      adds: 220,
      dels: 4,
      checks: [{ t: "Validate · passed · round 1", s: "PDF snapshot tests pass.", color: passed }],
      manual: ["Print one and check the fold."],
      desc: "A PDF for every invoice.",
      acc: ["Matches the HTML invoice"],
      log: [L("ok", "✓ Validate passed · in Review")],
      files: [
        {
          name: "lib/invoice-pdf.ts",
          a: 210,
          d: 0,
          lines: [
            ["hunk", "", "@@ -0,0 +1,210 @@"],
            ["add-l", "1", "export function renderInvoice(inv: Invoice) {"],
          ],
        },
      ],
    }),
    T({
      k: "BILL-9",
      p: "BILL",
      title: "Dunning emails",
      col: "ready",
      st: "queued",
      prio: 3,
      desc: "Three reminders before a subscription is cancelled.",
      acc: ["Days 1, 7 and 14"],
    }),
  ];
}

/** The three tickets Noyan proposes, created in Backlog from keys `n`, `n + 1` and `n + 2`. */
export function noyanTickets(n: number): Ticket[] {
  const [a, b, c] = ["SHOP-" + String(n), "SHOP-" + String(n + 1), "SHOP-" + String(n + 2)];
  return [
    T({
      k: a,
      p: "SHOP",
      title: "Fetch carrier quotes in parallel",
      col: "backlog",
      st: "queued",
      prio: 2,
      by: "Noyan",
      desc: "The 3 carrier calls run one after another: 1.9 s together.",
      acc: ["Quotes fetched in parallel"],
    }),
    T({
      k: b,
      p: "SHOP",
      title: "Cache product prices for 60 s",
      col: "backlog",
      st: "queued",
      prio: 3,
      by: "Noyan",
      waits: [a],
      desc: "getPrices() runs once per cart row.",
      acc: ["One query per cart"],
    }),
    T({
      k: c,
      p: "SHOP",
      title: "Load the reviews widget after paint",
      col: "backlog",
      st: "queued",
      prio: 4,
      agent: "Docs updater",
      by: "Noyan",
      desc: "640 ms of hydration on checkout.",
      acc: ["Loads after first paint"],
    }),
  ];
}

// --- Noyan ---------------------------------------------------------------------------------------------------------

/** One message in Noyan's panel: yours (`u`) or Noyan's, with an optional ticket table, Create buttons or links. */
export interface NoyanMsg {
  cls: "u" | "";
  t: string;
  tbl?: readonly (readonly [n: string, title: string, agent: string])[];
  ask?: boolean;
  links?: readonly string[];
}

export const NOYAN0: readonly NoyanMsg[] = [
  { cls: "u", t: "Why is checkout slow? Make tickets for what you find." },
  {
    cls: "",
    t: "getPrices() runs once per cart row and prices are never cached. The shipping quote calls three carriers one after another. I’d make 3 tickets:",
    tbl: [
      ["1", "Fetch carrier quotes in parallel", "Rust developer"],
      ["2", "Cache product prices for 60 s", "Rust developer"],
      ["3", "Load the reviews widget after paint", "Docs updater"],
    ],
    ask: true,
  },
];

export const NOYAN_REPLY =
  "(Demo) In Otaq this is a real Claude Code session: it reads the code, answers, and proposes Backlog tickets you approve. It never edits code or moves tickets.";

export const NOYAN_NOT_YET = "Okay. Tell me what to change and I’ll update the list.";

export const FLOW_REPLY =
  "(Demo) In Otaq, Noyan explains a stage or changes the steps for you; its changes are tinted, with Undo. Nothing reaches the board until Apply.";

// --- tips ----------------------------------------------------------------------------------------------------------

/** What the buttons the demo doesn't build say in a toast (`tip…` in the canvas). */
export const TIPS = {
  new: "In the app this opens the ticket editor: title, Markdown description, priority, dependencies, and the agent per stage.",
  settings:
    "Settings: agent tools, your agents, project setup and run commands, themes, shortcuts. Try it in the app.",
  addProject:
    "Any folder that’s a git repository. Otaq suggests a key like SHOP and asks which flow to start from.",
  // The app's real key (decision 0137): the canvas says ⇧⌘T.
  terminals: "Plain terminals in the project, on the left rail (⌃`).",
  files: "Files: the project’s files and what each ticket changed. Try it in the app.",
  fold: "Empty columns can fold to shorten a long flow.",
  run: "Run starts the project’s run command for this ticket on its own $PORT. Try it in the app.",
  stop: "Stop holds this one agent. Pause holds them all.",
  helper: "Each helper has its own terminal. Type into it, or steer it through the agent in charge.",
  askTool:
    "In the app the tool reads its own history and your repo and proposes a flow, with a “why” line per stage. Nothing runs until you ask.",
  import: "Import a flow file another project exported.",
  addStage:
    "Start from Plan, Security check, Visual check, Docs or Blank. A new agent stage needs a skill before Apply.",
  move: "Move…: the board’s columns with the drag’s rules. Forward is the flow’s job; backward sends it anywhere earlier.",
  more: "Edit ticket…, Copy as Markdown, Copy branch name, Delete…",
  full: "Full page (e): the sheet fills the main area.",
  priority: "P1 Urgent · P2 High · P3 Normal · P4 Low",
  other: "Other… lets you type an answer; it goes into the agent’s session.",
  openPR: "Opens the pull request in your browser.",
  report: "Opens the report: the agent’s full output, with its comment.",
  attach: "Add a file as a report; it goes with a comment.",
} as const;

export type Tip = keyof typeof TIPS;
