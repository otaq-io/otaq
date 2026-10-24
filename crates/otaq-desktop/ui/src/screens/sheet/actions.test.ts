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
import type { PanelAction } from "../../generated/PanelAction";
import type { PanelAnswerRow } from "../../generated/PanelAnswerRow";
import type { PanelView } from "../../generated/PanelView";
import type { StageChoicesView } from "../../generated/StageChoicesView";
import type { TicketPathView } from "../../generated/TicketPathView";
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
import { renderSheet as render, sheetRegistries, VIEW } from "./sheet_fixtures";

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
  it("service buttons call panel.action; the rest open a tab or a dialog", () => {
    const service: PanelAction[] = [
      "retry_start",
      "resume",
      "retry_resume",
      "reopen",
      "mark_merged",
      "resume_lead",
    ];
    for (const action of service) expect(actionRoute(action)).toEqual({ kind: "service" });
    // Start opens DPV-03's dialog for a blocked ticket (`board.start`).
    expect(actionRoute("start")).toEqual({ kind: "start" });
    expect(actionRoute("answer_in_terminal")).toEqual({ kind: "terminal" });
    // Send again (d0118): `panel.send_again`, which may ask about unsent text first.
    expect(actionRoute("send_again")).toEqual({ kind: "send_again" });
    expect(actionRoute("open_agent")).toEqual({ kind: "open_agent" });
    expect(cardRoute("open_agent")).toEqual({ kind: "open_agent" });
    expect(agentSettings("griller")).toEqual({ section: "agents", agent: "griller" });
    expect(agentSettings(null)).toEqual({ section: "agents" });
    expect(actionRoute("approve")).toEqual({ kind: "overlay", id: "local_merge" });
    expect(actionRoute("send_back")).toEqual({ kind: "overlay", id: "send_back" });
    // Your check in another flow (FLW-14) and an open PR (FLW-16).
    expect(actionRoute("pass")).toEqual({ kind: "pass" });
    expect(actionRoute("fail")).toEqual({ kind: "overlay", id: "fail" });
    expect(actionRoute("send_back_pr")).toEqual({ kind: "overlay", id: "send_back_pr" });
    expect(actionRoute("add_child_from_pr")).toEqual({ kind: "service" });
    expect(actionRoute("edit")).toEqual({ kind: "edit" });
    expect(actionRoute("mark_closed")).toEqual({ kind: "mark_closed" });
  });

  it("the validation buttons (d0063, d0064)", () => {
    const service: PanelAction[] = [
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

  it("the … menu's ids from the view models", () => {
    expect(menuRoute("close-ticket")).toEqual({ kind: "overlay", id: "close_ticket" });
    expect(menuRoute("submit-for-review")).toEqual({ kind: "overlay", id: "submit_for_review" });
    expect(menuRoute("delete")).toEqual({ kind: "overlay", id: "delete_ticket" });
    expect(menuRoute("pin")).toEqual({ kind: "service" });
    expect(menuRoute("copy-attach")).toEqual({ kind: "copy", what: "attach" });
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
  it("is taken once, by the panel showing its ticket", () => {
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
      "use_fallback",
      "use_validator_fallback",
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
    for (const kind of ["move_on_anyway", "skip_check", "add_child_from_pr"] as const)
      expect(cardRoute(kind)).toEqual({ kind: "service" });
    expect(cardRoute("pass")).toEqual({ kind: "pass" });
    expect(cardRoute("fail")).toEqual({ kind: "overlay", id: "fail" });
    expect(cardRoute("local_merge")).toEqual({ kind: "overlay", id: "local_merge" });
    expect(cardRoute("open_pr")).toEqual({ kind: "overlay", id: "open_pr" });
    expect(cardRoute("send_back_pr")).toEqual({ kind: "overlay", id: "send_back_pr" });
  });
});

describe("answer options", () => {
  const rows: PanelAnswerRow[] = [
    { number: 1, label: "Postmark", recommended: true, picked: true, other: false, note: null },
    { number: 2, label: "SES", recommended: false, picked: false, other: false, note: null },
    {
      number: 3,
      label: "Other… type your own answer",
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

  it("replaces the action bar with the question and its options", () => {
    const html = render({
      ...VIEW,
      answer: {
        asker: null,
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
    expect(html).not.toContain("Open terminal");
  });

  it("draws each option's note whole under its label, as the option's description (ATT-17)", () => {
    const long = "Pros and cons, ".repeat(40).trim();
    const html = render({
      ...VIEW,
      answer: {
        asker: null,
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
    expect(html.match(/class="panel-answer-note"/g)).toHaveLength(1);
    const described = [...html.matchAll(/aria-describedby="([^"]+)"/g)].map((m) => m[1]);
    expect(described).toHaveLength(1);
    expect(html).toContain(`id="${described[0] ?? ""}" class="panel-answer-note"`);
  });
});

describe("what it needs from you, at the top of Overview", () => {
  it("draws the bar, the last signal and the registered section under the sheet's header and tabs", () => {
    const html = render(VIEW);
    for (const text of ["SHOP-19", "Price table", "Working", "otaq/SHOP-19", "from main", "40s ago"]) {
      expect(html).toContain(text);
    }
    expect(html).toContain("Open terminal");
    expect(html).toContain("the dependencies");
    expect(html.indexOf("sheet-tabs")).toBeLessThan(html.indexOf("panel-bar"));
  });

  it("the queued notice, the conflict strip and the start callout", () => {
    const html = render({
      ...VIEW,
      notice: { text: "Waits for SHOP-18 to merge.", parent_closed: false, choices: [] },
      conflict: { line: "Conflicts with main · 1 file (a.ts)" },
      start_problem: {
        heading: "Start failed",
        message: "SHOP-19 stopped before its agent session started.",
        has_log: false,
        show_log: "View the setup log",
        hide_log: "Hide the setup log",
        retry_heading: "RETRY START DOES",
        retry_does: ["Runs the whole start again"],
      },
    });
    for (const text of ["Waits for SHOP-18", "Conflicts with main", "RETRY START DOES"]) {
      expect(html).toContain(text);
    }
    expect(html).not.toContain("View the setup log");
  });

  it("the M12 extras: token split, Auto-merge stop message and the Approve caret", () => {
    const html = render({
      ...VIEW,
      token_split: "agent 58k · validation 9k (1 round)",
      stop_message: "Keeps conflicting with otaq/SHOP-40 (3 tries in a row).",
      buttons: [
        { action: "approve", label: "Approve · Local merge", tooltip: null, icon: null, look: "primary" },
      ],
      approve_more: "Approve choices",
    });
    expect(html).toContain("agent 58k · validation 9k (1 round)");
    expect(html).toContain("Keeps conflicting with otaq/SHOP-40");
    expect(html).toContain("▾");
    expect(render(VIEW)).not.toContain("▾");
  });

  it("the sheet registers the ticket's two dialogs from its folder", () => {
    const app = registerScreens({ sheet });
    expect(registered(app)).toMatchObject({
      slots: ["sheet"],
      overlays: ["close_ticket", "submit_for_review"],
    });
  });
});

describe("the choices and the path in the panel (T-0411, AGM-09)", () => {
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
  /** SHOP-29 mid-flow (*05 → Row · Stage choices and path*). */
  const CHOICES: StageChoicesView = {
    heading: "Choices",
    set_by: "set by the Project agent",
    rows: [
      {
        stage_id: "grill",
        label: "Grill:",
        value: "required",
        items: [],
        lock: "Locked: Grill has started",
        change: null,
      },
      {
        stage_id: "dev",
        label: "Dev:",
        value: "Rust developer",
        items: [],
        lock: "Locked: Dev has started",
        change: {
          text: "Dev: Quick developer → Rust developer · by Griller · touches session handling",
          agent_made: true,
          icon: "bot",
        },
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
    note: null,
  };
  const PATH: TicketPathView = {
    heading: "Path",
    summary: "Grill · Griller · sonnet · Dev · Rust developer · sonnet · Review · Done",
    rows: [
      {
        stage: "Grill",
        state: "passed",
        icon: "circle-check",
        current: false,
        detail: "Griller",
        model: "claude-sonnet-5-5",
        lock: "Locked: it ran on this model",
      },
      {
        stage: "Dev",
        state: "running",
        icon: "activity",
        current: true,
        detail: "Rust developer · working",
        model: "sonnet",
        lock: null,
      },
      {
        stage: "Review",
        state: "your_check",
        icon: "user-check",
        current: false,
        detail: "your check",
        model: "—",
        lock: null,
      },
    ],
  };

  it("sit under the action bar in Overview, with the token line", () => {
    const out = render({
      ...VIEW,
      choices: CHOICES,
      path: PATH,
      token_split: "Rust developer 61k · no validation yet",
    });
    expect(out).toContain("Rust developer 61k · no validation yet");
    const at = (text: string) => out.indexOf(text);
    expect(at("sheet-tabs")).toBeLessThan(at("panel-bar"));
    expect(at("panel-bar")).toBeLessThan(at("panel-agents"));
    expect(out).toContain("set by the Project agent");
    expect(out).toContain('aria-label="Locked: Dev has started"');
    expect(out).toContain('data-agent="true"');
    expect(out).toContain("Dev: Quick developer → Rust developer · by Griller · touches session handling");
    expect(out).toContain(PATH.summary);
    expect(out).toContain('data-state="running" data-current="true"');
    // On the default flow only the path shows; an epic shows neither.
    const only = render({ ...VIEW, path: PATH });
    expect(only).not.toContain('class="stage-choices"');
    expect(only).toContain("ticket-path");
    expect(render(VIEW)).not.toContain("panel-agents");
  });

  it("a pick goes to the service at once (panel.set_choice)", () => {
    const ask = vi.mocked(request);
    ask.mockReset();
    ask.mockResolvedValue(null);
    const host = document.body.appendChild(document.createElement("div"));
    const root = createRoot(host);
    act(() => {
      root.render(
        createElement(
          RegistriesContext,
          { value: sheetRegistries() },
          createElement(SheetBody, {
            view: { ...VIEW, choices: CHOICES },
            tab: null,
            fullPage: false,
            width: 1040,
          }),
        ),
      );
    });
    const press = (el: Element | null | undefined) => {
      act(() => {
        el?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
        el?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      });
    };
    press(host.querySelector(".stage-choice-select button"));
    press(
      [...document.querySelectorAll('[role="menuitemradio"]')].find((i) => i.textContent.includes("skip")),
    );
    expect(ask).toHaveBeenCalledWith("panel.set_choice", { key: VIEW.key, stage: "review", choice: "skip" });
    act(() => {
      root.unmount();
    });
    host.remove();
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
    const holds = html.indexOf("panel-holds");
    expect(holds).toBeGreaterThan(0);
    expect(holds).toBeLessThan(html.indexOf("panel-bar"));
    expect(html).toContain('data-tone="amber"');
    expect(html).toContain("Codex no longer lists gpt-5.4");
    expect(html).toContain("Open agent…");
  });

  it("nothing held: no callouts", () => {
    expect(render(VIEW)).not.toContain("panel-holds");
  });
});

describe("the ticket's keys (0097)", () => {
  it("Close ticket and Re-run validation do what the panel's menu item and button do, while it offers them", () => {
    const app = createRegistries();
    const host = document.body.appendChild(document.createElement("div"));
    const root = createRoot(host);
    const show = (view: PanelView) => {
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
    expect(vi.mocked(request)).toHaveBeenCalledWith("panel.action", {
      key: "SHOP-19",
      action: "retry_validation",
    });
    act(() => {
      root.unmount();
    });
    host.remove();
  });
});
