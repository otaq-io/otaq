import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { actionsOf, BannerRows, LINE_HANDLERS, RestartBody, ToastStack } from ".";
import type { LimitLine } from "../../generated/LimitLine";
import type { ToolAvatarView } from "../../generated/ToolAvatarView";
import { request } from "../../tauri";
import { createOverlayStack } from "../../app/overlays";
import { INITIAL_LAYOUT, type ShellLayout } from "../../app/layout";
import { createSelection, type TerminalFocusRequest } from "../../app/selection";
import { createStore } from "../../app/store";
import type { Banner } from "../../generated/Banner";
import type { Notices } from "../../generated/Notices";
import type { PageEvent } from "../../generated/PageEvent";
import type { Toast } from "../../generated/Toast";
import type { ToastAction } from "../../generated/ToastAction";
import type { NotificationTarget } from "../../generated/NotificationTarget";
import { createPageEvents } from "../../service";
import type { PaneFocusRequest, ShownAgents } from "../pane/logic";
import {
  followNotifications,
  limitLinesOf,
  onlyRestartAllowed,
  openTarget,
  pressBanner,
  watchNotices,
} from "./notices";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

/** An event stream the test feeds by hand; `open()` resolves the subscription. */
function fakeStream() {
  let send: (event: unknown) => void = () => undefined;
  let open: () => void = () => undefined;
  const events = createPageEvents(
    (onEvent) =>
      new Promise<void>((resolve) => {
        send = onEvent;
        open = resolve;
      }),
  );
  return {
    events,
    open: () => {
      open();
    },
    send: (event: PageEvent) => {
      send(event);
    },
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const RESTART_NOW: Banner["buttons"][number] = {
  id: "action",
  label: "Restart now",
  icon: "rotate-cw",
  enabled: true,
  primary: false,
};

const banner = (over: Partial<Banner> = {}): Banner => ({
  kind: "service_update_ready",
  tone: "info",
  icon: "refresh-cw",
  title: "Service update ready",
  text: "Restart now (3 agents working, 2 waiting) · or later, when it suits you",
  buttons: [RESTART_NOW],
  dismissable: true,
  tooltip: null,
  lines: [],
  ...over,
});

const notices = (banners: Banner[], only = false): Notices => ({
  banners,
  toasts: [],
  only_restart_allowed: only,
  terminal_note: null,
  service_down: false,
  input_enabled: !only,
  summary_for: null,
});

describe("the notices", () => {
  it("load once the stream is open, then follow each notices event; a newer event wins over the answer", async () => {
    const stream = fakeStream();
    let answer: (value: unknown) => void = () => undefined;
    const ask = vi.fn(() => new Promise<unknown>((resolve) => (answer = resolve)));
    const seen: Notices[] = [];
    const stop = watchNotices((n) => seen.push(n), stream.events, ask);
    stream.open();
    await settle();
    expect(ask).toHaveBeenCalledWith("notices.view");
    stream.send({ kind: "notices", notices: notices([banner()], false) });
    answer(notices([]));
    await settle();
    expect(seen).toEqual([notices([banner()])]);
    stop();
    stream.send({ kind: "notices", notices: notices([]) });
    expect(seen).toHaveLength(1);
  });

  it("say when only Restart works", () => {
    expect(onlyRestartAllowed(undefined)).toBe(false);
    expect(onlyRestartAllowed(notices([], true))).toBe(true);
  });
});

describe("banner buttons", () => {
  const routes = () => {
    const ask = vi.fn<(method: string, params?: unknown) => Promise<unknown>>(() => Promise.resolve(null));
    const stack = createOverlayStack();
    return { ask, stack, routes: { ask, overlays: stack } };
  };

  it("Restart now asks first; Start service starts at once", async () => {
    const r = routes();
    await pressBanner("service_update_ready", "action", r.routes);
    await pressBanner("restart_required", "action", r.routes);
    expect(r.stack.store.get().map((o) => o.id)).toEqual(["restart_service"]);
    expect(r.ask).not.toHaveBeenCalled();
    await pressBanner("service_stopped", "action", r.routes);
    expect(r.ask).toHaveBeenCalledWith("notices.restart");
  });

  it("Resume all after interrupted sessions opens the confirmation with its ticket list", async () => {
    const r = routes();
    await pressBanner("sessions_interrupted", "action", r.routes);
    expect(r.stack.store.get().map((o) => o.id)).toEqual(["resume_all"]);
    expect(r.ask).not.toHaveBeenCalled();
  });

  it("Review & resume opens the summary sheet; the limit's Resume and Read the docs are requests", async () => {
    const r = routes();
    await pressBanner("sessions_interrupted", "secondary", r.routes);
    expect(r.stack.store.get().map((o) => o.id)).toEqual(["after_restart"]);
    await pressBanner("usage_limit", "resume", r.routes);
    await pressBanner("permission_warning", "action", r.routes);
    await pressBanner("reconnecting", "action", r.routes);
    await pressBanner("trust_failed", "action", r.routes);
    await pressBanner("trust_note", "action", r.routes);
    expect(r.ask.mock.calls.map((c) => c[0])).toEqual(["notices.resume_after_limit", "notices.open_docs"]);
  });

  it("the notifications ask's Allow and Not now, and the blocked banner's Open System Settings, are requests", async () => {
    const r = routes();
    await pressBanner("notifications_ask", "action", r.routes);
    await pressBanner("notifications_ask", "secondary", r.routes);
    await pressBanner("notifications_blocked", "action", r.routes);
    expect(r.ask.mock.calls.map((c) => c[0])).toEqual([
      "notices.notifications_allow",
      "notices.notifications_not_now",
      "notices.open_notification_settings",
    ]);
    expect(r.stack.store.get()).toEqual([]);
  });
});

describe("notification clicks", () => {
  const routes = () => {
    const selection = createSelection();
    return {
      selection,
      focus: createStore<TerminalFocusRequest | null>(null),
      next: vi.fn(),
      pane: {
        selection,
        layout: createStore<ShellLayout>({ ...INITIAL_LAYOUT, agentOpen: false }),
        shown: createStore<ShownAgents>({}),
        focus: createStore<PaneFocusRequest | null>(null),
      },
    };
  };

  it("a ticket: its project, Session on its agent, its terminal focused; again is a new focus request", () => {
    const r = routes();
    openTarget({ kind: "ticket", project: "SHOP", ticket: "SHOP-22" }, r);
    expect(r.selection.get()).toMatchObject({ project: "SHOP", ticket: "SHOP-22", tab: "agent" });
    expect(r.focus.get()).toEqual({ ticket: "SHOP-22", seq: 1 });
    openTarget({ kind: "ticket", project: "SHOP", ticket: "SHOP-22" }, r);
    expect(r.focus.get()?.seq).toBe(2);
    expect(r.next).not.toHaveBeenCalled();
  });

  it("a validator's prompt: Session on the check's side, its terminal focused", () => {
    const r = routes();
    openTarget({ kind: "validator", project: "NOTE", ticket: "NOTE-3" }, r);
    expect(r.selection.get()).toMatchObject({ project: "NOTE", ticket: "NOTE-3", tab: "check" });
    expect(r.focus.get()).toEqual({ ticket: "NOTE-3", seq: 1 });
  });

  it("a Project agent: its project, the agent in the pane (shown if hidden), focused", () => {
    const r = routes();
    openTarget({ kind: "project_agent", project: "NOTE", agent: "pa7" }, r);
    expect(r.selection.get().project).toBe("NOTE");
    expect(r.pane.shown.get()).toEqual({ NOTE: "pa7" });
    expect(r.pane.layout.get().agentOpen).toBe(true);
    expect(r.pane.focus.get()).toEqual({ seq: 1, agent: "pa7" });
    expect(r.focus.get()).toBeNull();
  });

  it("an epic lead: the epic's Lead tab with the lead's terminal focused", () => {
    const r = routes();
    openTarget({ kind: "lead", project: "NOTE", epic: "NOTE-40" }, r);
    expect(r.selection.get()).toMatchObject({ project: "NOTE", ticket: "NOTE-40", tab: "lead" });
    expect(r.focus.get()).toEqual({ ticket: "NOTE-40", seq: 1 });
  });

  it("a summary opens the Needs you strip's next item (NYS-03); the board for a tool that isn't available", () => {
    const r = routes();
    openTarget({ kind: "needs_you" }, r);
    expect(r.next).toHaveBeenCalledTimes(1);
    expect(r.selection.get()).toMatchObject({ project: null, ticket: null });
    const b = routes();
    openTarget({ kind: "board", project: null }, b);
    expect(b.selection.get()).toMatchObject({ project: null, ticket: null });
    openTarget({ kind: "board", project: "NOTE" }, b);
    expect(b.selection.get()).toMatchObject({ project: "NOTE", ticket: null });
    expect(b.next).not.toHaveBeenCalled();
    expect(b.focus.get()).toBeNull();
  });

  it("open what the app was started with, then each clicked notification, bringing the window forward", async () => {
    const stream = fakeStream();
    const ask = vi.fn((method: string, params?: unknown): Promise<unknown> => {
      if (method === "notices.open_at_start")
        return Promise.resolve({ kind: "ticket", project: "SHOP", ticket: "SHOP-9" });
      const target = (params as { target: { kind: string; key?: string } }).target;
      return Promise.resolve(
        target.kind === "ticket"
          ? { kind: "ticket", project: "SHOP", ticket: target.key }
          : { kind: "needs_you" },
      );
    });
    const opened: NotificationTarget[] = [];
    const forward = vi.fn(() => Promise.resolve());
    const stop = followNotifications(stream.events, ask, (t) => opened.push(t), forward);
    stream.open();
    await settle();
    expect(opened.map((t) => t.kind)).toEqual(["ticket"]);
    expect(forward).not.toHaveBeenCalled();
    stream.send({
      kind: "service",
      name: "open_from_notification",
      event: { kind: "open_from_notification", target: { kind: "ticket", key: "SHOP-22" } },
    });
    stream.send({
      kind: "service",
      name: "open_from_notification",
      event: { kind: "open_from_notification", target: { kind: "inbox" } },
    });
    stream.send({ kind: "service", name: "ticket_changed", event: { kind: "ticket_changed" } });
    await settle();
    expect(ask).toHaveBeenCalledWith("notices.open_target", { target: { kind: "ticket", key: "SHOP-22" } });
    expect(opened).toEqual([
      { kind: "ticket", project: "SHOP", ticket: "SHOP-9" },
      { kind: "ticket", project: "SHOP", ticket: "SHOP-22" },
      { kind: "needs_you" },
    ]);
    expect(forward).toHaveBeenCalledTimes(2);
    stop();
  });
});

describe("drawn", () => {
  it("banners: one row each, with their buttons and the close button only where it can close", () => {
    const html = renderToStaticMarkup(
      createElement(BannerRows, {
        banners: [
          banner({
            kind: "usage_limit",
            tone: "attention",
            title: "Usage limit reset",
            buttons: [{ id: "resume", label: "Resume (8)", icon: "play", enabled: true, primary: true }],
            dismissable: false,
          }),
          banner({ buttons: [{ ...RESTART_NOW, label: "Restarting…", enabled: false }] }),
        ],
      }),
    );
    expect(html.match(/class="banner"/g)).toHaveLength(2);
    expect(html).toContain('data-tone="attention"');
    expect(html).toContain("Resume (8)");
    expect(html).toMatch(/<button[^>]*disabled[^>]*>.*Restarting…/);
    expect(html.match(/aria-label="Dismiss"/g)).toHaveLength(1);
    expect(renderToStaticMarkup(createElement(BannerRows, { banners: [] }))).toBe("");
  });

  it("folder trust: the note is its text alone with a close; the warning has neither", () => {
    const html = renderToStaticMarkup(
      createElement(BannerRows, {
        banners: [
          banner({
            kind: "trust_failed",
            tone: "warning",
            icon: "shield-x",
            title: "Claude Code doesn’t trust 2 projects’ folders:",
            text: "NOTE, SHOP. Their agents will wait on Claude Code’s trust screen.",
            buttons: [],
            dismissable: false,
            tooltip: {
              title: "Claude Code doesn’t trust 2 projects’ folders",
              lines: ["NOTE: why.", "SHOP: why."],
            },
          }),
          banner({
            kind: "trust_note",
            title: "",
            text: "Otaq let Claude Code work in your projects’ folders, as adding a project now does.",
            buttons: [],
          }),
        ],
      }),
    );
    expect(html.match(/class="banner-title"/g)).toHaveLength(1);
    expect(html).toContain('class="tooltip-anchor"');
    expect(html.match(/aria-label="Dismiss"/g)).toHaveLength(1);
    expect(html).not.toMatch(/class="button"/);
  });

  it("toasts: their links, and a ticket's title opens it", () => {
    const toast = (over: Partial<Toast>): Toast => ({
      id: 1,
      level: "success",
      title: "Local merge done · SHOP-18",
      text: "Squashed into main as 9f3e2c1",
      icon: null,
      icon_tone: null,
      link: null,
      ticket: null,
      ...over,
    });
    const html = renderToStaticMarkup(
      createElement(ToastStack, {
        toasts: [
          toast({ ticket: "SHOP-18" }),
          toast({
            id: 2,
            level: "error",
            title: "gh is not signed in",
            text: "Run gh auth login, then retry Open PR",
            link: { kind: "copy", label: "Copy command", text: "gh auth login" },
          }),
        ],
      }),
    );
    expect(html).toContain(
      '<button type="button" class="toast-title toast-open">Local merge done · SHOP-18</button>',
    );
    expect(html).toContain('<span class="toast-title">gh is not signed in</span>');
    expect(html).toContain("Copy command");
    expect(html).toContain('role="alert"');
  });

  it("the epic queue toast: its buttons follow the title and a press goes to Rust", async () => {
    const { request } = await import("../../tauri");
    const undo: ToastAction = { kind: "undo_epic_queue", epic: "SHOP-45", keys: ["SHOP-45.1", "SHOP-45.2"] };
    const toast: Toast = {
      id: 7,
      level: "success",
      title: "Moved 2 children of SHOP-45 to Ready for Dev",
      text: null,
      icon: null,
      icon_tone: null,
      link: {
        kind: "actions",
        actions: [{ label: "Undo", action: undo }],
      },
      ticket: null,
    };
    const html = renderToStaticMarkup(createElement(ToastStack, { toasts: [toast] }));
    expect(html).toMatch(
      /toast-line.*toast-title">Moved 2 children of SHOP-45 to Ready for Dev.*toast-actions.*Undo/,
    );
    expect(html).not.toContain("toast-text");
    vi.mocked(request).mockResolvedValue(null);
    actionsOf(toast)?.[0]?.onClick();
    expect(vi.mocked(request)).toHaveBeenCalledWith("notices.toast_action", {
      id: 7,
      label: "Undo",
      action: undo,
    });
  });

  it("the restart dialog's body: the warning callout, then the rest", () => {
    const html = renderToStaticMarkup(
      createElement(RestartBody, {
        dialog: {
          title: "Restart the background service?",
          message: "3 working agents and 2 waiting ones will be interrupted. You can Resume all afterwards.",
          detail: "Waiting tickets keep their questions.",
          confirm: "Restart service",
        },
      }),
    );
    expect(html).toMatch(
      /notices-restart-message.*3 working agents.*notices-restart-detail.*Waiting tickets/,
    );
  });
});

const avatar = (tool: "claude_code" | "codex", letters: string): ToolAvatarView => ({
  tool,
  letters,
  background: `tool-${letters.toLowerCase()}`,
  foreground: `tool-${letters.toLowerCase()}-fg`,
  hover: tool === "claude_code" ? "Claude Code" : "Codex",
});

const claudeLine = (over: Partial<LimitLine> = {}): LimitLine => ({
  tool: "claude",
  avatar: avatar("claude_code", "CC"),
  tone: "neutral",
  icon: "gauge",
  title: "Claude Code limit reached",
  text: "Resets 3:45 pm · 4 tickets waiting",
  buttons: [
    {
      label: "Resume Claude Code",
      icon: "play",
      action: { kind: "resume_tool", tool: "claude" },
      enabled: false,
      primary: false,
      tooltip: null,
    },
    {
      label: "Use fallback now",
      icon: "arrow-right-left",
      action: { kind: "use_fallback_now", tool: "claude" },
      enabled: true,
      primary: false,
      tooltip: "Starts the waiting tickets on Codex",
    },
  ],
  ...over,
});

const codexLine: LimitLine = {
  tool: "codex",
  avatar: avatar("codex", "CX"),
  tone: "warning",
  icon: "triangle-alert",
  title: "Codex isn’t available",
  text: "signed out · 5 tickets on Claude Code",
  buttons: [
    {
      label: "Sign in…",
      icon: "plug",
      action: { kind: "sign_in", tool: "codex" },
      enabled: true,
      primary: false,
      tooltip: null,
    },
  ],
};

const limitBanner = (lines: LimitLine[]): Banner =>
  banner({
    kind: "usage_limit",
    tone: "neutral",
    icon: "gauge",
    title: lines[0]?.title ?? "",
    text: lines[0]?.text ?? "",
    buttons: [{ id: "resume", label: "Resume", icon: "play", enabled: false, primary: false }],
    dismissable: false,
    lines,
  });

describe("the limit banner per tool", () => {
  it("draws one line per tool with its avatar, Resume <tool>, Sign in… and Use fallback now", () => {
    const b = limitBanner([claudeLine(), codexLine]);
    expect(limitLinesOf(b)).toHaveLength(2);
    const html = renderToStaticMarkup(createElement(BannerRows, { banners: [b] }));
    expect(html.match(/notices-line"/g)).toHaveLength(2);
    expect(html).toMatch(
      /data-tone="neutral" data-tool="claude".*Claude Code limit reached.*Resume Claude Code/,
    );
    expect(html).toMatch(/data-tone="warning" data-tool="codex".*Codex isn’t available.*Sign in…/);
    expect(html).toContain(">Use fallback now<");
    expect(html).not.toContain(">Resume<");
    expect(html).not.toContain('data-tone="attention"');
  });

  it("the one-tool banner stays M22's: no avatar, no lines, no Use fallback now", () => {
    const one = limitBanner([claudeLine({ avatar: null, buttons: claudeLine().buttons.slice(0, 1) })]);
    expect(limitLinesOf(one)).toEqual([]);
    const html = renderToStaticMarkup(createElement(BannerRows, { banners: [one] }));
    expect(html).not.toContain("notices-line");
    expect(html).not.toContain("Use fallback now");
    expect(html).not.toContain("tool-avatar");
    expect(html).toContain(">Resume<");
  });

  it("Resume <tool> calls limits.resume_tool with its tool; Use fallback now calls limits.use_fallback_now", () => {
    const ask = vi.mocked(request);
    ask.mockReset();
    ask.mockResolvedValue(null);
    LINE_HANDLERS.onAction({ kind: "resume_tool", tool: "claude" });
    LINE_HANDLERS.onAction({ kind: "resume_tool", tool: "codex" });
    LINE_HANDLERS.onAction({ kind: "use_fallback_now", tool: "claude" });
    expect(ask.mock.calls).toEqual([
      ["limits.resume_tool", { tool: "claude" }],
      ["limits.resume_tool", { tool: "codex" }],
      ["limits.use_fallback_now", { tool: "claude" }],
    ]);
  });
});
