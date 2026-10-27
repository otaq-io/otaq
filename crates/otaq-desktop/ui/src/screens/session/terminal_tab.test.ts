// Session's terminal (SHT-04): the agent's and the check's sides, their line, strip and hint, without a terminal.
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { createRegistries, registered, registerScreens, RegistriesContext } from "../../app/registry";
import type { TerminalPhase } from "../../generated/TerminalPhase";
import type { TerminalTabStrip } from "../../generated/TerminalTabStrip";
import type { TerminalTabView } from "../../generated/TerminalTabView";
import { INITIAL_STATE, type TerminalState } from "../../terminal/state";
import { session } from ".";
import {
  answers,
  bodyOf,
  checkOf,
  isNarrow,
  partShown,
  scrolledLabel,
  sessionOf,
  shownPhase,
  type Side,
  stripFor,
  toneColor,
} from "./terminalLogic";
import { SessionList } from "./Session";
import { TerminalTabBody } from "./TerminalTab";

const PHASES: TerminalPhase[] = ["no_session", "attaching", "live", "reconnecting", "ended"];

/** Strips worded like Rust's, enough to tell them apart. */
function strips(interrupted = false): TerminalTabStrip[] {
  return PHASES.flatMap((phase) =>
    [false, true].map((focused) => ({
      phase,
      focused,
      state: interrupted
        ? { label: "Interrupted", tone: "error" as const }
        : { label: `${phase}${focused ? " focused" : ""}`, tone: "working" as const },
      hint:
        phase === "live"
          ? {
              icon: "keyboard" as const,
              text: focused ? "Typing goes to the agent" : "Click the terminal to type to the agent",
              tone: focused ? ("focus" as const) : ("muted" as const),
            }
          : null,
    })),
  );
}

const VIEW: TerminalTabView = {
  key: "SHOP-19",
  session: "s1",
  label: "SHOP-19 · agent terminal",
  no_session: "SHOP-19 has no agent session yet. It starts when the ticket goes to Dev.",
  strips: strips(),
  path: "…/wt/shop-web/SHOP-19",
  port: "agent :4019",
  attach_command: "otaq attach SHOP-19",
  attach_short: "attach",
  copy_attach: "Copy attach command",
  waiting: null,
  instructions: { label: "Instructions: shop-web", project: "SHOP" },
  leave: { text: "leave terminal", keys: "⌃`" },
  smaller: "Smaller text",
  bigger: "Larger text",
  find: { text: "Find in scrollback", keys: "⌘F" },
  background: null,
  narrow_below: 560,
  ended: null,
  recovery: null,
  line: "Rust developer · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-19",
  typing: {
    focused: "Typing goes to Rust developer · SHOP-19",
    unfocused: "Click to type to Rust developer · SHOP-19",
  },
  checks: [],
  helpers: [],
  resume: null,
};

/** In a check: the Security reviewer's own strips, worded so the tests tell them from the agent's. */
const CHECKING: TerminalTabView = {
  ...VIEW,
  checks: [
    {
      session: "v1",
      label: "Security reviewer",
      agent: null,
      terminal_label: "SHOP-19 · Security reviewer terminal",
      line: "Security reviewer · Claude Code · sonnet · ~/.otaq/wt/shop-web/SHOP-19",
      typing: {
        focused: "Typing goes to Security reviewer · SHOP-19",
        unfocused: "Click to type to Security reviewer · SHOP-19",
      },
      read_only: false,
      strips: strips().map((s) => ({ ...s, state: { ...s.state, label: `check ${s.state.label}` } })),
      attach_command: "otaq attach SHOP-19 --stage security",
    },
  ],
};

const LIVE: TerminalState = { ...INITIAL_STATE, phase: "live" };

function render(
  view: TerminalTabView,
  state: TerminalState = LIVE,
  narrow = false,
  side: Side = "agent",
  agent: string | null = null,
) {
  return renderToStaticMarkup(
    createElement(
      RegistriesContext,
      { value: createRegistries() },
      createElement(TerminalTabBody, { view, side, agent, state, narrow }),
    ),
  );
}

describe("the agent's and the check's sides (SHT-04)", () => {
  it("leads with the line: who, on what, where, and who typing goes to, with the ticket's key", () => {
    const focused = { ...LIVE, focused: true };
    const html = render(VIEW, focused);
    expect(html).toContain("Rust developer · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-19");
    expect(html).toContain("Typing goes to Rust developer · SHOP-19");
    const check = render(CHECKING, focused, false, "check");
    expect(check).toContain("Security reviewer · Claude Code · sonnet");
    expect(check).toContain("Typing goes to Security reviewer · SHOP-19");
    expect(render({ ...VIEW, typing: null }, focused)).not.toContain("Typing goes to Rust developer");
  });

  it("says Typing goes to only while the terminal has the keyboard, else Click to type to (finding 55)", () => {
    const html = render(VIEW);
    expect(html).toContain("Click to type to Rust developer · SHOP-19");
    expect(html).not.toContain("Typing goes to Rust developer");
  });

  it("attaches the check's session with its own strip and attach command", () => {
    expect(sessionOf(CHECKING, "check")).toBe("v1");
    expect(sessionOf(CHECKING, "agent")).toBe("s1");
    expect(sessionOf(VIEW, "check")).toBe("s1");
    expect(stripFor(CHECKING, "live", false, "check")?.state.label).toBe("check live");
    expect(bodyOf({ ...CHECKING, recovery: { kind: "loading", text: "…" } }, "check")).toBe("terminal");
    const html = render(CHECKING, LIVE, false, "check");
    expect(html).toContain("otaq attach SHOP-19 --stage security");
    expect(html).not.toContain("agent :4019");
    expect(html).not.toContain("Instructions: shop-web");
  });
});

describe("one terminal per check agent (d0130)", () => {
  const [security] = CHECKING.checks;
  if (security === undefined) throw new Error("a check");
  const two: TerminalTabView = {
    ...CHECKING,
    checks: [
      { ...security, agent: "Security reviewer", label: "Security reviewer ✓" },
      {
        ...security,
        session: "v2",
        agent: "Performance reviewer",
        label: "Performance reviewer",
        attach_command: 'otaq attach SHOP-33 --stage validate --agent "Performance reviewer"',
      },
    ],
  };
  it("shows the chosen check agent's terminal, else the first", () => {
    expect(checkOf(two, "Performance reviewer")?.session).toBe("v2");
    expect(sessionOf(two, "check", "Performance reviewer")).toBe("v2");
    expect(sessionOf(two, "check", null)).toBe("v1");
    expect(sessionOf(two, "check", "Gone reviewer")).toBe("v1");
    expect(checkOf(VIEW)).toBeNull();
  });
  it("checks the switch's part of the agent shown", () => {
    const parts = [
      { content: "agent", agent: null },
      { content: "check", agent: "Security reviewer" },
      { content: "check", agent: "Performance reviewer" },
    ];
    const shown = (agent: string | null) => parts.map((p) => partShown(p, "check", agent, parts));
    expect(shown("Performance reviewer")).toEqual([false, false, true]);
    expect(shown(null)).toEqual([false, true, false]);
  });
});

describe("the terminal's state switching", () => {
  it("shows the terminal, the recovery view or the no-session line", () => {
    expect(bodyOf(VIEW)).toBe("terminal");
    expect(bodyOf({ ...VIEW, session: null })).toBe("no_session");
    expect(bodyOf({ ...VIEW, recovery: { kind: "loading", text: "Reading…" } })).toBe("recovery");
    expect(shownPhase({ ...VIEW, session: null }, "live")).toBe("no_session");
    expect(shownPhase(VIEW, "reconnecting")).toBe("reconnecting");
  });

  it("picks the strip of the terminal's phase and focus", () => {
    expect(stripFor(VIEW, "live", false)?.state.label).toBe("live");
    expect(stripFor(VIEW, "live", true)?.hint?.text).toBe("Typing goes to the agent");
    expect(stripFor(VIEW, "ended", false)?.hint).toBeNull();
    expect(stripFor({ ...VIEW, session: null }, "live", false)?.state.label).toBe("no_session");
    expect(toneColor("error")).toBe("var(--st-error)");
  });

  it("is narrow under its width", () => {
    expect(isNarrow(480, VIEW)).toBe(true);
    expect(isNarrow(660, VIEW)).toBe(false);
    expect(isNarrow(0, VIEW)).toBe(false);
  });

  it("answers each focus request for its ticket once", () => {
    expect(answers({ ticket: "SHOP-19", seq: 3 }, "SHOP-19", 2)).toBe(true);
    expect(answers({ ticket: "SHOP-19", seq: 3 }, "SHOP-19", 3)).toBe(false);
    expect(answers({ ticket: "SHOP-22", seq: 4 }, "SHOP-19", 3)).toBe(false);
    expect(answers(null, "SHOP-19", 0)).toBe(false);
  });

  it("counts scrolled-up lines", () => {
    expect(scrolledLabel(1)).toBe("Scrolled up 1 line");
    expect(scrolledLabel(1320)).toBe("Scrolled up 1,320 lines");
  });
});

describe("the terminal's markup", () => {
  it("Session registers its tab, and Run's dialog and the running processes", () => {
    expect(registered(registerScreens({ session }))).toMatchObject({
      sheetTabs: ["session"],
      overlays: ["processes", "run_command"],
    });
  });

  it("draws the strip and the hint of a live terminal", () => {
    const html = render(VIEW);
    expect(html).toContain("agent :4019");
    expect(html).toContain("otaq attach SHOP-19");
    expect(html).toContain("Click the terminal to type to the agent");
    expect(html).toContain("Instructions: shop-web");
    expect(html).not.toContain("leave terminal");
  });

  it("says how to leave a focused terminal and draws its focus ring", () => {
    const html = render(VIEW, { ...LIVE, focused: true });
    expect(html).toContain("Typing goes to the agent");
    expect(html).toContain("leave terminal");
    expect(html).toContain("data-focused");
    expect(html).not.toContain("Instructions: shop-web");
  });

  it("drops the port when narrow", () => {
    const html = render(VIEW, LIVE, true);
    expect(html).not.toContain("agent :4019");
    expect(html).toContain(">attach<");
  });

  it("offers Deliver now while messages wait", () => {
    const html = render({ ...VIEW, waiting: { label: "1 message waiting", deliver: "Deliver now" } });
    expect(html).toContain("1 message waiting");
    expect(html).toContain("Deliver now");
    expect(html).not.toContain("otaq attach SHOP-19");
  });

  it("marks an ended session and shows how far it is scrolled", () => {
    const ended = {
      ...VIEW,
      ended: { title: "Session ended · 11:02 · 41 min", text: "The agent’s session finished." },
    };
    expect(render(ended, { ...LIVE, phase: "ended" })).toContain("Session ended · 11:02 · 41 min");
    expect(render(ended, LIVE)).not.toContain("Session ended");
    expect(render(VIEW, { ...LIVE, scrolledUp: 1320 })).toContain("Scrolled up 1,320 lines");
  });

  it("shows the no-session line and Before the restart instead of a terminal", () => {
    expect(render({ ...VIEW, session: null })).toContain("has no agent session yet");
    const html = render({
      ...VIEW,
      strips: strips(true),
      recovery: {
        kind: "before_restart",
        title: "Before the restart",
        source: "from Claude’s saved conversation · read-only",
        session: "session 3be94c10",
        lines: [{ time: "08:09", who: "Bash", text: "pnpm build", action: true }],
        ended: "Session ended by a restart at 08:24.",
        resume: "Resume",
      },
    });
    expect(html).toContain("Interrupted");
    expect(html).toContain("Before the restart");
    expect(html).toContain("pnpm build");
    expect(html).toContain("Resume");
    expect(html).not.toContain("terminal-well");
  });
});

describe("a helper's terminal (d0133, `s30-03`)", () => {
  const [security] = CHECKING.checks;
  if (security === undefined) throw new Error("a check");
  const helper = {
    ...security,
    session: "h3",
    agent: "40.3",
    label: "40.3",
    terminal_label: "SHOP-40 · 40.3’s helper terminal",
    line: "Helper for 40.3 · Claude Code · sonnet · its own worktree, otaq/SHOP-40.3",
    typing: {
      focused: "Typing goes to 40.3’s helper · SHOP-40",
      unfocused: "Click to type to 40.3’s helper · SHOP-40",
    },
    read_only: false,
    attach_command: "",
  };
  const split: TerminalTabView = { ...VIEW, helpers: [helper] };
  it("attaches the piece's helper, and nothing for a piece without a session", () => {
    expect(sessionOf(split, "helper", "40.3")).toBe("h3");
    expect(bodyOf(split, "helper", "40.3")).toBe("terminal");
    expect(sessionOf(split, "helper", "40.4")).toBeNull();
    expect(bodyOf(split, "helper", "40.4")).toBe("no_session");
    expect(sessionOf(split, "agent")).toBe("s1");
  });
  it("shows the helper's line and no attach command", () => {
    const html = render(split, LIVE, false, "helper", "40.3");
    expect(html).toContain("Helper for 40.3 · Claude Code · sonnet · its own worktree, otaq/SHOP-40.3");
    expect(html).toContain("Click to type to 40.3’s helper · SHOP-40");
    expect(html).not.toContain("otaq attach");
  });
});

describe("Session's list keeps the terminal's height (T-0573)", () => {
  const row = (n: number) => ({
    piece: n,
    name: `40.${String(n)}`,
    title: `Piece ${String(n)}`,
    status: "Working",
    tone: "working" as const,
    opens: true,
  });
  it("puts every helper in its own scrolled box, bounded, with the list at its own height", async () => {
    const list = {
      label: "SESSIONS · 1 IN CHARGE · 6 HELPERS",
      in_charge_label: "IN CHARGE",
      helpers_label: "HELPERS",
      in_charge: { ...row(0), piece: null, name: "Developer" },
      helpers: [1, 2, 3, 4, 5, 6].map(row),
    };
    const html = renderToStaticMarkup(
      createElement(SessionList, { list, piece: null, onOpen: () => undefined }),
    );
    const box = html.slice(html.indexOf('class="session-list-helpers"'));
    for (const n of [1, 2, 3, 4, 5, 6]) expect(box).toContain(`40.${String(n)}`);
    expect(box).not.toContain("Developer");
    // Read as text with Node's fs (the test runner hands CSS imports over empty; the page has no Node types).
    const fsModule = "node:fs";
    const fs = (await import(fsModule)) as { readFileSync(path: string, encoding: "utf8"): string };
    const here = (import.meta as ImportMeta & { dirname: string }).dirname;
    const css = fs.readFileSync(`${here}/session.css`, "utf8");
    const rule = (selector: string) => new RegExp(`\\n\\${selector} \\{([^}]*)\\}`).exec(css)?.[1] ?? "";
    expect(rule(".session-list")).toContain("flex: none;");
    expect(rule(".session-list-helpers")).toMatch(/max-height: \d+px;/);
    expect(rule(".session-list-helpers")).toContain("overflow-y: auto;");
  });
});

describe("after a restart with helpers (d0133 audit 5)", () => {
  it("shows the one Resume with who it brings back", () => {
    const html = render({
      ...VIEW,
      session: null,
      recovery: { kind: "loading", text: "Reading Claude Code’s saved conversation…" },
      resume: {
        title: "Resume SHOP-40",
        line: "One item for the ticket. Resume continues, each in its own conversation:",
        rows: [
          { name: "Developer · in charge", title: "", status: "told about the restart" },
          { name: "40.2", title: "Wishlist button", status: "was working" },
        ],
        note: "40.1 is merged.",
      },
    });
    expect(html).toContain("Resume SHOP-40");
    expect(html).toContain("told about the restart");
    expect(html).toContain("Wishlist button");
    expect(html).toContain("40.1 is merged.");
  });
});
