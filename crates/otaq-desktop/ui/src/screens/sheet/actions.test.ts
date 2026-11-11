// @vitest-environment happy-dom
// The ticket's actions (`actions.tsx`, `actionRoutes.ts`) and what Overview needs from you (`overview/NeedsYou.tsx`):
// mostly drawn to a string; a pick in the choices is mounted (its select opens a menu).
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { cardActions, requestCardAction, takeCardAction } from "../../app/cardActions";
import { overlays } from "../../app/overlays";
import { createRegistries, registered, registerScreens, RegistriesContext } from "../../app/registry";
import { NOTHING_SELECTED, requestTicketEdit, selection, ticketEditRequests } from "../../app/selection";
import type { CardActionKind } from "../../generated/CardActionKind";
import type { SheetAction } from "../../generated/SheetAction";
import type { SheetAnswerRow } from "../../generated/SheetAnswerRow";
import type { SheetView } from "../../generated/SheetView";
import { request } from "../../tauri";
import { sheet } from ".";
import {
  actionRoute,
  agentSettings,
  canSend,
  cardRoute,
  initialPick,
  menuRoute,
  pickByKey,
  picksOther,
} from "./actionRoutes";
import { SheetBody } from "./Sheet";
import { canAnswerQuestion } from "../overview/NeedsYou";
import type { SheetAnswer } from "../../generated/SheetAnswer";
import { renderSheet as render, VIEW } from "./sheet_fixtures";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

afterEach(() => {
  overlays.store.set([]);
  selection.set(NOTHING_SELECTED);
  ticketEditRequests.set(null);
  cardActions.set(null);
});

describe("where buttons and menu items go", () => {
  it("service buttons call sheet.action; the rest open a tab or a dialog", () => {
    const service: SheetAction[] = ["retry_start", "resume", "retry_resume", "reopen", "mark_merged"];
    for (const action of service) expect(actionRoute(action)).toEqual({ kind: "service" });
    // Start (`board.start`): into the first stage, the sheet's button and a Backlog card's alike.
    expect(actionRoute("start")).toEqual({ kind: "start" });
    expect(cardRoute("start")).toEqual({ kind: "start" });
    expect(actionRoute("answer_in_terminal")).toEqual({ kind: "terminal" });
    // Send again (d0118): `sheet.send_again`, which may ask about unsent text first.
    expect(actionRoute("send_again")).toEqual({ kind: "send_again" });
    expect(actionRoute("open_agent")).toEqual({ kind: "open_agent" });
    expect(cardRoute("open_agent")).toEqual({ kind: "open_agent" });
    // No PR provider for an automatic PR: Set up… opens app Settings where providers are set up.
    expect(cardRoute("set_up_provider")).toEqual({ kind: "provider_settings" });
    // Change agent… on a held card: the dialog picking another agent for the waiting stage.
    expect(cardRoute("change_agent")).toEqual({ kind: "overlay", id: "change_agent" });
    expect(agentSettings("spec-writer")).toEqual({ section: "agents", agent: "spec-writer" });
    expect(agentSettings(null)).toEqual({ section: "agents" });
    expect(actionRoute("approve")).toEqual({ kind: "overlay", id: "local_merge" });
    // A hook refused Approve & merge: its confirmation again, on the kept edit.
    expect(actionRoute("edit_message")).toEqual({ kind: "overlay", id: "local_merge" });
    expect(actionRoute("edit_pr_messages")).toEqual({ kind: "overlay", id: "open_pr" });
    expect(actionRoute("send_back")).toEqual({ kind: "overlay", id: "send_back" });
    // Your check in another flow (FLW-14) and an open PR (FLW-16).
    expect(actionRoute("pass")).toEqual({ kind: "pass" });
    expect(actionRoute("send_back_pr")).toEqual({ kind: "overlay", id: "send_back_pr" });
    expect(actionRoute("edit")).toEqual({ kind: "edit" });
    expect(actionRoute("mark_closed")).toEqual({ kind: "mark_closed" });
  });

  it("the validation buttons (d0063, d0064)", () => {
    const service: SheetAction[] = [
      "move_on_anyway",
      "another_round",
      "retry_validation",
      "retry_merge",
      "move_to_review",
    ];
    for (const action of service) expect(actionRoute(action)).toEqual({ kind: "service" });
    expect(actionRoute("open_validator")).toEqual({ kind: "check" });
    expect(actionRoute("answer_validator")).toEqual({ kind: "check" });
    expect(actionRoute("answer_agent")).toEqual({ kind: "answer_agent" });
    for (const item of ["stop-validation", "skip-to-review", "validate-and-submit"])
      expect(menuRoute(item)).toEqual({ kind: "service" });
  });

  it("Stop on a running merge check, in the sheet and on the card (d0120)", () => {
    expect(actionRoute("stop_merge_check")).toEqual({ kind: "service" });
    expect(cardRoute("stop_merge_check")).toEqual({ kind: "service" });
  });

  it("the … menu's ids from the view models", () => {
    expect(menuRoute("close-ticket")).toEqual({ kind: "overlay", id: "close_ticket" });
    expect(menuRoute("submit-for-review")).toEqual({ kind: "overlay", id: "submit_for_review" });
    expect(menuRoute("delete")).toEqual({ kind: "overlay", id: "delete_ticket" });
    expect(menuRoute("pin")).toEqual({ kind: "service" });
    expect(menuRoute("copy-branch")).toEqual({ kind: "copy" });
    expect(menuRoute("ask-noyan")).toEqual({ kind: "ask_noyan" });
    expect(menuRoute("something")).toEqual({ kind: "unknown" });
  });

  it("Edit shows Overview and asks it for its form", () => {
    selection.set({ ...NOTHING_SELECTED, ticket: "SHOP-19" });
    requestTicketEdit("SHOP-19");
    requestTicketEdit("SHOP-19");
    expect(selection.get().tab).toBe("overview");
    expect(ticketEditRequests.get()).toEqual({ ticket: "SHOP-19", seq: 2 });
  });
});

describe("a board card's button", () => {
  it("is taken once, by the sheet showing its ticket", () => {
    requestCardAction("SHOP-25", "resume");
    expect(takeCardAction("SHOP-18")).toBeNull();
    expect(takeCardAction("SHOP-25")).toMatchObject({ ticket: "SHOP-25", kind: "resume" });
    expect(takeCardAction("SHOP-25")).toBeNull();
  });

  it("runs as the GPUI app's card buttons did", () => {
    const service: CardActionKind[] = [
      "deliver_now",
      "resume",
      "retry_resume",
      "retry_start",
      "retry",
      "reopen",
      "mark_merged",
      "retry_validation",
      "retry_merge",
      "move_to_review",
    ];
    for (const kind of service) expect(cardRoute(kind)).toEqual({ kind: "service" });
    expect(cardRoute({ pick_option: 3 })).toEqual({ kind: "pick", number: 3 });
    expect(cardRoute("send_answer")).toEqual({ kind: "send_answer" });
    expect(cardRoute("start_from_scratch")).toEqual({ kind: "overlay", id: "start_from_scratch" });
    expect(cardRoute("push_to_pr")).toEqual({ kind: "overlay", id: "open_pr" });
    expect(cardRoute("mark_closed")).toEqual({ kind: "mark_closed" });
    expect(cardRoute("review")).toEqual({ kind: "none" });
    expect(cardRoute("run_chip")).toEqual({ kind: "run_tab" });
    expect(actionRoute("run")).toEqual({ kind: "run" });
  });

  it("the cards of checks and deliveries (FLW-13…FLW-16)", () => {
    for (const kind of ["move_on_anyway", "skip_check"] as const)
      expect(cardRoute(kind)).toEqual({ kind: "service" });
    expect(cardRoute("pass")).toEqual({ kind: "pass" });
    expect(cardRoute("fail")).toEqual({ kind: "overlay", id: "send_back" });
    expect(cardRoute("local_merge")).toEqual({ kind: "overlay", id: "local_merge" });
    expect(cardRoute("open_pr")).toEqual({ kind: "overlay", id: "open_pr" });
    expect(cardRoute("send_back_pr")).toEqual({ kind: "overlay", id: "send_back_pr" });
  });
});

describe("answer options", () => {
  const rows: SheetAnswerRow[] = [
    { number: 1, label: "Postmark", recommended: true, picked: true, other: false, note: null },
    { number: 2, label: "SES", recommended: false, picked: false, other: false, note: null },
    {
      number: 3,
      label: "Something else: type your own answer",
      recommended: false,
      picked: false,
      other: true,
      note: null,
    },
  ];

  it("starts on the recommendation; number keys pick; Other… needs text", () => {
    expect(initialPick(rows)).toBe(1);
    expect(initialPick(rows.map((r) => ({ ...r, picked: false })))).toBeNull();
    expect(pickByKey("2", rows)).toBe(2);
    expect(pickByKey("4", rows)).toBeNull();
    expect(pickByKey("a", rows)).toBeNull();
    expect(picksOther(3, rows)).toBe(true);
    expect(canSend(null, "", rows)).toBe(false);
    expect(canSend(2, "", rows)).toBe(true);
    expect(canSend(3, "  ", rows)).toBe(false);
    expect(canSend(3, "Mailgun", rows)).toBe(true);
  });

  it("an open question (no options) is answered in place, in the user's own words", () => {
    const html = render({
      ...VIEW,
      answer: {
        question: "q2",
        text: "Which greeting should it print?",
        rows: [],
        recommended_label: "Recommended",
        other_placeholder: "Type your answer",
        hint: "↵ sends",
        in_terminal: "Answer in terminal",
        send: "Send answer",
      },
    });
    const page = new DOMParser().parseFromString(html, "text/html");
    const card = page.querySelector(".overview-main .sheet-question");
    expect(card?.textContent).toContain("Which greeting should it print?");
    expect(card?.querySelector("textarea.sheet-answer-box")?.getAttribute("placeholder")).toBe(
      "Type your answer",
    );
    expect(card?.querySelectorAll('[role="radio"]')).toHaveLength(0);
    expect(html).toContain("Answer in terminal");
    expect(html).not.toContain("Open agent session");
    const buttons = [...(card?.querySelectorAll("button") ?? [])];
    expect(buttons.find((b) => b.textContent.includes("Send answer"))?.hasAttribute("disabled")).toBe(true);
    expect(canAnswerQuestion({ rows: [] } as unknown as SheetAnswer, null, "Hello from Otaq")).toBe(true);
    expect(canAnswerQuestion({ rows: [] } as unknown as SheetAnswer, null, "  ")).toBe(false);
  });

  it("replaces the action bar with the question and its options", () => {
    const html = render({
      ...VIEW,
      answer: {
        question: "q1",
        text: "Which email provider?",
        rows,
        recommended_label: "Recommended",
        other_placeholder: "Type your answer",
        hint: "1–4 pick · ↵ sends",
        in_terminal: "Answer in terminal",
        send: "Send answer",
      },
    });
    expect(html).toContain("Which email provider?");
    expect(html.match(/role="radio"/g)).toHaveLength(3);
    expect(html).toContain('aria-checked="true"');
    expect(html).toContain("Send answer");
    expect(html).not.toContain("Open agent session");
  });

  it("draws each option's note whole under its label, as the option's description (ATT-17)", () => {
    const long = "Pros and cons, ".repeat(40).trim();
    const html = render({
      ...VIEW,
      answer: {
        question: "q1",
        text: "Which email provider?",
        rows: rows.map((r) => (r.number === 1 ? { ...r, note: long } : r)),
        recommended_label: "Recommended",
        other_placeholder: "Type your answer",
        hint: "1–4 pick · ↵ sends",
        in_terminal: "Answer in terminal",
        send: "Send answer",
      },
    });
    expect(html).toContain(`>${long}</span>`);
    expect(html.match(/class="sheet-answer-note"/g)).toHaveLength(1);
    const described = [...html.matchAll(/aria-describedby="([^"]+)"/g)].map((m) => m[1]);
    expect(described).toHaveLength(1);
    expect(html).toContain(`id="${described[0] ?? ""}" class="sheet-answer-note"`);
  });
});

describe("what it needs from you, at the top of Overview", () => {
  it("draws the bar, the last signal and the registered section under the sheet's header and tabs", () => {
    const html = render(VIEW);
    for (const text of ["SHOP-19", "Price table", "Working", "In Dev · Strong · started by you 12 min ago"]) {
      expect(html).toContain(text);
    }
    expect(html).toContain("Open agent session");
    expect(html).toContain("the dependencies");
    expect(html.indexOf("sheet-tabs")).toBeLessThan(html.indexOf("sheet-bar"));
  });

  it("the queued notice, the conflict strip and the start callout", () => {
    const html = render({
      ...VIEW,
      notice: { text: "Waits for SHOP-18 to merge.", parent_closed: false, choices: [] },
      conflict: {
        line: "Conflicts with main · 1 file (a.ts)",
        moved: null,
        outcome: null,
        count: null,
        count_hint: "Conflicts and failed merge checks in a row.",
      },
      start_problem: {
        heading: "Start failed",
        message: "SHOP-19 stopped before its agent session started.",
        has_log: false,
        show_log: "View the setup log",
        hide_log: "Hide the setup log",
        retry_heading: "RETRY START DOES",
        retry_does: ["Runs the whole start again"],
        hook: null,
      },
    });
    for (const text of ["Waits for SHOP-18", "Conflicts with main", "RETRY START DOES"]) {
      expect(html).toContain(text);
    }
    expect(html).not.toContain("View the setup log");
    expect(html).not.toContain("sheet-hook-output");
  });

  it("a start a hook refused: the hook's whole output under the message, then Retry start's list", () => {
    const html = render({
      ...VIEW,
      start_problem: {
        heading: "Start failed: post-checkout hook refused the start",
        message: "Your post-checkout hook refused the start when Otaq made the ticket\u2019s worktree.",
        has_log: false,
        show_log: "View the setup log",
        hide_log: "Hide the setup log",
        retry_heading: "RETRY START DOES",
        retry_does: ["Runs the whole start again"],
        hook: {
          name: "post-checkout",
          label: "Output of your post-checkout hook",
          note: null,
          output: "npm install\nERR! missing ~/.npmrc",
        },
      },
    });
    expect(html).toContain("Start failed: post-checkout hook refused the start");
    expect(html).toContain("Output of your post-checkout hook");
    expect(html).toContain("ERR! missing ~/.npmrc");
    expect(html.indexOf("sheet-hook-output")).toBeLessThan(html.indexOf("RETRY START DOES"));
  });

  it("a permission prompt: the command above Answer in terminal", () => {
    const atPrompt: SheetView = {
      ...VIEW,
      permission_prompt: {
        heading: "Developer wants to run a command",
        text: "npm install",
        command: true,
      },
      buttons: [
        {
          action: "answer_in_terminal",
          label: "Answer in terminal",
          tooltip: null,
          icon: "square-terminal",
          look: "attention",
        },
      ],
    };
    const html = render(atPrompt);
    expect(html).toContain("Developer wants to run a command");
    expect(html).toContain("npm install");
    expect(html).toContain("sheet-permission-command");
    expect(html.indexOf("npm install")).toBeLessThan(html.indexOf("Answer in terminal"));
    // The tool's own words when the prompt isn't a command.
    const words = render({
      ...atPrompt,
      permission_prompt: {
        heading: "Developer wants your permission",
        text: "Claude needs your permission to use Bash",
        command: false,
      },
    });
    expect(words).toContain("Claude needs your permission to use Bash");
    expect(words).not.toContain("sheet-permission-command");
    expect(render(VIEW)).not.toContain("sheet-permission");
  });

  it("the conflict strip says what moved, that nothing was merged, and the count", () => {
    const html = render({
      ...VIEW,
      conflict: {
        line: "Conflicts with main · 1 file (README.md)",
        moved: "SHOP-6 changed README.md",
        outcome: "Nothing was merged into main. SHOP-19 returns to Review when it’s fixed.",
        count: "1 of 3",
        count_hint: "Conflicts and failed merge checks in a row.",
      },
    });
    for (const text of [
      "Conflicts with main · 1 file (README.md)",
      "SHOP-6 changed README.md",
      "Nothing was merged into main. SHOP-19 returns to Review when it’s fixed.",
      "1 of 3",
    ]) {
      expect(html).toContain(text);
    }
    const failed = render({
      ...VIEW,
      conflict: {
        line: "Merge check failed on main",
        moved: null,
        outcome: "Nothing was merged into main. SHOP-19 returns to Review when it’s fixed.",
        count: "2 of 3",
        count_hint: "Conflicts and failed merge checks in a row.",
      },
    });
    expect(failed).toContain("Merge check failed on main");
    expect(failed).not.toContain("Conflicts with");
    expect(failed).toContain("2 of 3");
  });

  it("the M12 extras: Auto-merge stop message and the Approve caret", () => {
    const html = render({
      ...VIEW,
      stop_message: "Keeps conflicting with otaq/SHOP-40 (3 tries in a row).",
      buttons: [
        { action: "approve", label: "Approve · Local merge", tooltip: null, icon: null, look: "primary" },
      ],
      approve_more: "Approve choices",
    });
    expect(html).toContain("Keeps conflicting with otaq/SHOP-40");
    expect(html).toContain("▾");
    expect(render(VIEW)).not.toContain("▾");
  });

  it("the sheet registers the ticket's two dialogs from its folder", () => {
    const app = registerScreens({ sheet });
    expect(registered(app)).toMatchObject({
      slots: ["sheet"],
      overlays: ["close_ticket", "submit_for_review", "change_agent"],
    });
  });
});

describe("no plumbing in Overview (the stage's agents are set in Edit ticket…)", () => {
  it("has no STAGES, no Path and no right column without attachments", () => {
    const out = render(VIEW);
    expect(out).not.toContain("overview-stages");
    expect(out).not.toContain("sheet-agents");
    expect(out).not.toContain("overview-side");
  });
});

describe("the held roles' callouts (M23)", () => {
  it("SHOP-49 on Old GPT: Model unavailable above the bar, amber, with Open agent…", () => {
    const html = render({
      ...VIEW,
      holds: [
        {
          role: "developer",
          tone: "amber",
          avatar: null,
          heading: "Model unavailable",
          text: "Codex no longer lists gpt-5.4, the model of Old GPT. Give Old GPT another model in Settings → Agents.",
          model: "gpt-5.4",
          buttons: [
            { label: "Open agent…", action: { kind: "open_agent", agent: "old-gpt" }, primary: true },
          ],
        },
      ],
    });
    const holds = html.indexOf("sheet-holds");
    expect(holds).toBeGreaterThan(0);
    expect(holds).toBeLessThan(html.indexOf("sheet-bar"));
    expect(html).toContain('data-tone="amber"');
    expect(html).toContain("Codex no longer lists gpt-5.4");
    expect(html).toContain("Open agent…");
  });

  it("nothing held: no callouts", () => {
    expect(render(VIEW)).not.toContain("sheet-holds");
  });
});

describe("the ticket's keys (0097)", () => {
  it("Close ticket and Re-run validation do what the sheet's menu item and button do, while it offers them", () => {
    const app = createRegistries();
    const host = document.body.appendChild(document.createElement("div"));
    const root = createRoot(host);
    const show = (view: SheetView) => {
      act(() => {
        root.render(
          createElement(
            RegistriesContext,
            { value: app },
            createElement(SheetBody, { view, tab: null, fullPage: false, width: 1040 }),
          ),
        );
      });
    };
    show(VIEW);
    expect(app.shortcuts.run({ action: "close_ticket" })).toBe(false);
    expect(app.shortcuts.run({ action: "rerun_validation" })).toBe(false);
    show({
      ...VIEW,
      menu: [
        {
          id: "close-ticket",
          label: "Close ticket…",
          icon: null,
          shortcut: null,
          danger: true,
          disabled: false,
          hint: null,
          checked: false,
          header: false,
          separated: false,
        },
      ],
      buttons: [
        {
          action: "retry_validation",
          label: "Retry validation",
          tooltip: null,
          icon: null,
          look: "secondary",
        },
      ],
    });
    expect(app.shortcuts.run({ action: "close_ticket" })).toBe(true);
    expect(overlays.store.get().map((o) => o.id)).toEqual(["close_ticket"]);
    expect(app.shortcuts.run({ action: "rerun_validation" })).toBe(true);
    expect(vi.mocked(request)).toHaveBeenCalledWith("sheet.action", {
      key: "SHOP-19",
      action: "retry_validation",
    });
    act(() => {
      root.unmount();
    });
    host.remove();
  });
});

describe("the action bar's fit (T-0572)", () => {
  it("wraps its buttons onto a second line instead of running past Overview's column", async () => {
    // Read as text with Node's fs (the test runner hands CSS imports over empty; the page has no Node types).
    const fsModule = "node:fs";
    const fs = (await import(fsModule)) as { readFileSync(path: string, encoding: "utf8"): string };
    const here = (import.meta as ImportMeta & { dirname: string }).dirname;
    const css = fs.readFileSync(`${here}/actions.css`, "utf8");
    const bar = /\n\.sheet-bar \{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(bar).toContain("flex-wrap: wrap;");
    expect(bar).toContain("min-width: 0;");
  });
});

describe("Change agent…", () => {
  it("opens on the agent Rust marks picked, else the first; with none, nothing", async () => {
    const { firstPick } = await import("./Dialogs");
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
    const view = {
      title: "Change Dev’s agent",
      stage_id: "dev",
      intro: "",
      empty: null,
      confirm: "Change agent",
      items: [item("Light", false), item("Codex developer", true)],
    };
    expect(firstPick(view)).toBe("Codex developer");
    expect(firstPick({ ...view, items: [item("Light", false)] })).toBe("Light");
    expect(firstPick({ ...view, items: [] })).toBeNull();
  });
});
