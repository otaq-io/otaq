// @vitest-environment happy-dom
// Edit flow never goes blank: the editor draws the draft the release test was left with (Validate dragged in front of
// Dev, its fails still going to Dev; the view is Rust's, `check_first.view.json`), and a view that doesn't come shows
// what went wrong with Discard the draft and Close instead of an empty screen. The box at its foot asks Noyan's agent
// for a change in the user's words, or for a whole flow, without opening the agent panel.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { shellLayout } from "../../app/layout";
import { RegistriesContext, registerScreens } from "../../app/registry";
import type { FlowBoardView } from "../../generated/FlowBoardView";
import type { SheetAnswer } from "../../generated/SheetAnswer";
import { request } from "../../tauri";
import CHECK_FIRST from "./check_first.view.json";
import { FlowBoard } from "./FlowBoard";

vi.mock("../../tauri", async (original) => ({
  ...(await original<typeof import("../../tauri")>()),
  request: vi.fn(),
}));

// The view loads at once, with no event stream; a refusal arrives as the view's error.
vi.mock("../../service", async (original) => {
  const service = await original<typeof import("../../service")>();
  const { useEffect, useState } = await import("react");
  return {
    ...service,
    useServiceView: (load: () => Promise<unknown>) => {
      const [state, setState] = useState<{ view?: unknown; error?: unknown }>({});
      useEffect(() => {
        load().then(
          (view) => {
            setState({ view });
          },
          (error: unknown) => {
            setState({ error: service.asRequestError(error) });
          },
        );
      }, [load]);
      return state;
    },
  };
});

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const app = registerScreens({});
let root: Root;
let host: HTMLElement;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
  vi.mocked(request).mockReset();
});

async function mount(view: (method: string) => Promise<unknown>) {
  vi.mocked(request).mockImplementation((method: string) =>
    method === "flow_board.view" ? view(method) : Promise.resolve(null),
  );
  await act(async () => {
    root.render(
      createElement(
        RegistriesContext,
        { value: app },
        createElement(FlowBoard, { open: { project: "SHOP" }, board: undefined }),
      ),
    );
    await Promise.resolve();
  });
}

describe("Edit flow never goes blank", () => {
  it("draws the editor for a draft with a check before every agent stage, its problem on the check", async () => {
    await mount(() => Promise.resolve(CHECK_FIRST as unknown as FlowBoardView));
    const bar = host.querySelector(".fb-bar");
    expect(bar?.textContent).toContain("Editing shop-web’s flow");
    const columns = [...host.querySelectorAll(".fb-column")].map((c) => c.getAttribute("aria-label"));
    expect(columns).toEqual(["Backlog", "Validate", "Dev", "Review", "Done"]);
    expect(host.textContent).toContain("Validate's fails go to Dev, which isn't an Agent stage before it");
  });

  it("says what went wrong when the view doesn't come, with Discard the draft and Close", async () => {
    await mount(() => Promise.reject(new Error("The draft doesn’t read: no stages")));
    const problem = host.querySelector(".fb-problem-box");
    expect(problem?.textContent).toContain("The flow editor can’t draw this flow");
    expect(problem?.textContent).toContain("The draft doesn’t read: no stages");
    const discard = [...host.querySelectorAll("button")].find((b) => b.textContent === "Discard the draft");
    expect(discard).toBeDefined();
    await act(async () => {
      discard?.click();
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith("flow_editor.cancel", { project: "SHOP" });
    expect([...host.querySelectorAll("button")].some((b) => b.textContent === "Close")).toBe(true);
  });
});

describe("Edit flow's box asks Noyan", () => {
  const VIEW = CHECK_FIRST as unknown as FlowBoardView;

  /** The box's request starts an agent; every other call answers nothing. */
  async function mountBox(view: FlowBoardView = VIEW) {
    vi.mocked(request).mockImplementation((method: string) => {
      if (method === "flow_board.view") return Promise.resolve(view);
      if (method === "flow_board.suggest") return Promise.resolve({ agent: "c9" });
      return Promise.resolve(null);
    });
    await act(async () => {
      root.render(
        createElement(
          RegistriesContext,
          { value: app },
          createElement(FlowBoard, { open: { project: "SHOP" }, board: undefined }),
        ),
      );
      await Promise.resolve();
    });
  }

  const box = () => host.querySelector<HTMLElement>(".fb-noyan");
  const button = (label: string) =>
    [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === label);

  function type(input: HTMLInputElement, value: string) {
    act(() => {
      Reflect.set(HTMLInputElement.prototype, "value", value, input);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }

  it("↵ sends the typed words to Noyan's agent on the flow skill, and the agent panel stays closed", async () => {
    shellLayout.set({ ...shellLayout.get(), agentOpen: false });
    await mountBox();
    const input = box()?.querySelector<HTMLInputElement>("input");
    if (!input) throw new Error("no box");
    expect(input.placeholder).toBe("Ask Noyan to change this flow…");
    expect(box()?.textContent).toContain("Proposes a draft · nothing changes until you save");
    type(input, "Add a security review after Validate");
    // Typed: the send button takes the shortcut's place.
    expect(button("Suggest a whole flow")).toBeUndefined();
    expect(button("Ask Noyan")).toBeDefined();
    await act(async () => {
      input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith("flow_board.suggest", {
      project: "SHOP",
      text: "Add a security review after Validate",
    });
    expect(shellLayout.get().agentOpen).toBe(false);
    expect(input.value).toBe("");
  });

  it("Suggest a whole flow asks for a whole flow, without words", async () => {
    await mountBox();
    await act(async () => {
      button("Suggest a whole flow")?.click();
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith("flow_board.suggest", { project: "SHOP" });
    expect(shellLayout.get().agentOpen).toBe(false);
  });

  /** The box while Strong (`c1`) drafts, as `flow_board.view` gives it. */
  function draftingView(waiting = false): FlowBoardView {
    const text = waiting
      ? "Strong wants your permission: Run command: @' …"
      : "Strong is drafting a change to shop-web’s flow…";
    return {
      ...VIEW,
      ask: {
        ...VIEW.ask,
        off: "Strong is drafting one",
        line: "Drag, + and × still work meanwhile",
        state: {
          kind: "drafting",
          needs_you: waiting ? "Needs you" : null,
          text,
          tooltip: waiting
            ? "Strong wants your permission: Run command: @'\n---\nname: update-docs\n---\n'@"
            : null,
          agent: "c1",
          show: waiting ? "Answer" : "Show",
          stop: "Stop",
        },
      },
    };
  }

  /** The box while Strong's proposal shows. */
  const PROPOSAL: FlowBoardView = {
    ...VIEW,
    suggestion: {
      title: "Strong’s suggestion for shop-web",
      line: "1 change, marked · nothing changes until you save",
      whys_title: "Why these changes",
      whys: [],
      discard: "Discard",
    },
    ask: {
      ...VIEW.ask,
      placeholder: "Ask for changes to Strong’s proposal…",
      whole: null,
      send: "Ask for changes",
      line: "Goes to the same conversation · nothing changes until you save",
      state: { kind: "proposal", agent: "c1", show: "Show conversation" },
    },
  };

  it("while Noyan drafts, the box shows it with Show and Stop, the shortcut off, and no line under the bar", async () => {
    shellLayout.set({ ...shellLayout.get(), agentOpen: false });
    await mountBox(draftingView());
    expect(box()?.textContent).toContain("Strong is drafting a change to shop-web’s flow…");
    expect(box()?.textContent).toContain("Drag, + and × still work meanwhile");
    expect(box()?.querySelector("input")).toBeNull();
    expect(button("Suggest a whole flow")?.disabled).toBe(true);
    expect(host.querySelector(".fb-screen > .fb-drafting")).toBeNull();
    expect(host.querySelector(".fb-bar + .fb-drafting")).toBeNull();
    act(() => {
      button("Show")?.click();
    });
    expect(shellLayout.get().agentOpen).toBe(true);
  });

  it("Stop ends the drafting conversation, as the agent panel's end does", async () => {
    await mountBox(draftingView());
    await act(async () => {
      button("Stop")?.click();
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith("agent_panel.end", { project: "SHOP", id: "c1", confirm: true });
  });

  it("names what the waiting agent needs on one line, with the whole of it in its tooltip", async () => {
    await mountBox(draftingView(true));
    const row = box()?.querySelector(".fb-drafting");
    expect(row?.hasAttribute("data-needs-you")).toBe(true);
    const text = row?.querySelector(".fb-drafting-text");
    expect(text?.textContent).toBe("Strong wants your permission: Run command: @' …");
    expect(text?.closest(".tooltip-anchor")).not.toBeNull();
    expect(row?.textContent).not.toContain("update-docs");
    expect(button("Answer")).toBeDefined();
    expect(button("Stop")).toBeDefined();
  });

  it("with a proposal, ↵ asks for changes in the same conversation and the bar has no Ask for changes…", async () => {
    await mountBox(PROPOSAL);
    const input = box()?.querySelector<HTMLInputElement>("input");
    if (!input) throw new Error("no box");
    expect(input.placeholder).toBe("Ask for changes to Strong’s proposal…");
    expect(button("Suggest a whole flow")).toBeUndefined();
    const bar = [...(host.querySelector(".fb-bar")?.querySelectorAll("button") ?? [])].map(
      (b) => b.textContent,
    );
    expect(bar).toEqual(expect.arrayContaining(["Watch a ticket", "Discard", VIEW.bar.save]));
    expect(bar.some((label) => label.startsWith("Ask for changes"))).toBe(false);
    type(input, "Keep Review");
    await act(async () => {
      input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith("flow_board.ask_changes", {
      project: VIEW.project,
      proposal: VIEW.proposal,
      text: "Keep Review",
    });
    expect(request).not.toHaveBeenCalledWith("flow_board.suggest", expect.anything());
  });

  it("Start from… lists the presets and Import, no Suggest", async () => {
    await mountBox();
    await act(async () => {
      button("Start from…")?.click();
      await Promise.resolve();
    });
    const start = document.querySelector(".fb-start");
    expect(start).not.toBeNull();
    expect(start?.textContent).toContain(VIEW.start_from.import);
    expect(start?.textContent).not.toMatch(/Suggest/);
  });

  /** The box while Strong (`c1`) asks `answer`, as `flow_board.view` gives it. */
  function askingView(answer: SheetAnswer): FlowBoardView {
    return {
      ...VIEW,
      ask: {
        ...VIEW.ask,
        off: "Strong is drafting one",
        line: "Drag, + and × still work meanwhile",
        state: {
          kind: "asking",
          head: "Needs you · Strong asks",
          question: { project: "SHOP", agent: "c1", title: "Noyan · Strong asks", answer },
          expand: "Open larger",
          collapse: "Back to the box",
        },
      },
    };
  }

  /** The brief's short question. */
  const SHORT: SheetAnswer = {
    question: "q-short",
    line: "Should the security review run before or after Validate?",
    context: null,
    rows: [
      {
        number: 1,
        label: "After Validate",
        recommended: true,
        picked: true,
        other: false,
        note: "Only code that passes the tests gets reviewed.",
      },
      {
        number: 2,
        label: "Before Validate",
        recommended: false,
        picked: false,
        other: false,
        note: "Finds problems sooner, but reviews code that may still fail its tests.",
      },
      { number: 3, label: "Other…", recommended: false, picked: false, other: true, note: null },
    ],
    recommended_label: "Recommended",
    other_placeholder: "Type your answer",
    hint: "1–3 pick · ↵ sends",
    in_terminal: "Answer in terminal",
    send: "Send answer",
  };

  /** The brief's long question, as `otaq-view` splits it. */
  const LONG: SheetAnswer = {
    ...SHORT,
    question: "q-long",
    line: "Which agent should run the new Security review stage?",
    context: [
      "Your project has a security skill in `.claude/skills/security-review`, and it expects the code to build " +
        "first: it runs `npm audit` and reads the lockfile. Two things to weigh:",
      "",
      "- **Verifier** already runs Validate on Codex. Using it again keeps one checking agent, but Codex doesn't " +
        "load `.claude/skills`, so it would fall back to Otaq's own review skill.",
      "- **Strong** (Claude Code) loads your skill as it is, but it's the agent that writes the code, so it would " +
        "review its own work.",
      "",
      "A third agent costs a new connection in Settings → Agents.",
    ].join("\n"),
  };

  /** The editor (the area under the bar) `width` × `height`, as the window lays it out. */
  function editorSize(width: number, height: number) {
    for (const prop of ["clientWidth", "clientHeight"] as const) {
      vi.spyOn(HTMLElement.prototype, prop, "get").mockImplementation(function (this: HTMLElement) {
        if (!this.classList.contains("fb-board")) return 0;
        return prop === "clientWidth" ? width : height;
      });
    }
  }

  /** The page's own style sheets (the test runner leaves CSS imports empty). */
  async function styles() {
    const fsModule = "node:fs";
    const fs = (await import(fsModule)) as { readFileSync(path: string, encoding: "utf8"): string };
    const here = (import.meta as ImportMeta & { dirname: string }).dirname;
    for (const file of ["../sheet/actions.css", "../agent_panel/agent_panel.css", "./flow_board.css"]) {
      const style = document.createElement("style");
      style.textContent = fs.readFileSync(`${here}/${file}`, "utf8");
      document.head.append(style);
    }
  }

  describe("when Noyan asks, the box is the answer box", () => {
    afterEach(() => {
      vi.restoreAllMocks();
      document.head.innerHTML = "";
    });

    /** The question's text scrolls past the cap; the options and Send sit outside it, Send never shrinking. */
    function expectSendInView(scope: HTMLElement) {
      const text = scope.querySelector<HTMLElement>(".sheet-answer-text");
      const send = button("Send answer");
      const options = scope.querySelector<HTMLElement>(".sheet-answer-options");
      const bar = scope.querySelector<HTMLElement>(".agent-question-bar");
      if (!text || !send || !options || !bar) throw new Error("no answer UI");
      expect(text.contains(send)).toBe(false);
      expect(text.contains(options)).toBe(false);
      expect(getComputedStyle(text).overflowY).toBe("auto");
      expect(Number.parseFloat(getComputedStyle(text).minHeight)).toBe(0);
      // The text gives way first, down to its floor (`QuestionText`); then the options scroll, never the bar
      // (NEW-d1).
      expect(Number(getComputedStyle(text).flexShrink)).toBeGreaterThanOrEqual(1000);
      expect(getComputedStyle(options).flexShrink).toBe("1");
      expect(getComputedStyle(options).overflowY).toBe("auto");
      expect(getComputedStyle(bar).flexShrink).toBe("0");
      expect(getComputedStyle(scope).overflow).toBe("hidden");
      expect(scope.contains(send)).toBe(true);
    }

    it("a short question fits: the box grows with it, its line bold over the options, nothing to scroll", async () => {
      await styles();
      editorSize(1376, 800);
      await mountBox(askingView(SHORT));
      const b = box();
      if (!b) throw new Error("no box");
      expect(b.dataset.state).toBe("asking");
      expect(b.querySelector(".fb-noyan-head")?.textContent).toBe("Needs you · Strong asks");
      expect(b.querySelector(".sheet-answer-line")?.textContent).toBe(SHORT.line);
      expect(b.querySelector(".sheet-answer-context")).toBeNull();
      expect([...b.querySelectorAll('[role="radio"]')].map((r) => r.getAttribute("aria-checked"))).toEqual([
        "true",
        "false",
        "false",
      ]);
      // As tall as the question needs, up to the cap; 760 px wide while asking.
      expect(b.style.height).toBe("");
      expect(b.style.maxHeight).toBe("440px");
      expect(b.style.width).toBe("760px");
      expect(b.querySelector("input")).toBeNull();
      expectSendInView(b);
    });

    it("the brief's long question grows to the cap, 55% of the editor, and scrolls with Send in view", async () => {
      await styles();
      editorSize(1376, 800);
      await mountBox(askingView(LONG));
      const b = box();
      if (!b) throw new Error("no box");
      expect(b.style.maxHeight).toBe("440px");
      const text = b.querySelector<HTMLElement>(".sheet-answer-text");
      expect(text?.querySelector(".sheet-answer-context")?.textContent).toContain("A third agent costs");
      expect(text?.querySelectorAll(".sheet-answer-context li")).toHaveLength(2);
      expectSendInView(b);
    });

    it("at 1280 × 800 the cap is 384 px and the box 680 px wide", async () => {
      editorSize(1216, 698);
      await mountBox(askingView(LONG));
      expect(box()?.style.maxHeight).toBe("384px");
      expect(box()?.style.width).toBe("680px");
    });

    it("⤢ opens it up to the editor's full height over a scrim; ⤡ and Esc bring it back", async () => {
      await styles();
      editorSize(1376, 800);
      await mountBox(askingView(LONG));
      act(() => {
        host.querySelector<HTMLButtonElement>('button[aria-label="Open larger"]')?.click();
      });
      const b = box();
      if (!b) throw new Error("no box");
      expect(b.hasAttribute("data-expanded")).toBe(true);
      expect(b.style.maxHeight).toBe("");
      expect(getComputedStyle(b).position).toBe("absolute");
      expect(getComputedStyle(b).bottom).toBe("20px");
      expect(getComputedStyle(b).maxHeight).toBe("calc(100% - 36px)");
      const scrim = host.querySelector(".fb-board .fb-noyan-scrim");
      expect(scrim).not.toBeNull();
      expectSendInView(b);

      act(() => {
        host.querySelector<HTMLButtonElement>('button[aria-label="Back to the box"]')?.click();
      });
      expect(box()?.hasAttribute("data-expanded")).toBe(false);
      expect(box()?.style.maxHeight).toBe("440px");
      expect(host.querySelector(".fb-noyan-scrim")).toBeNull();

      act(() => {
        host.querySelector<HTMLButtonElement>('button[aria-label="Open larger"]')?.click();
      });
      expect(box()?.hasAttribute("data-expanded")).toBe(true);
      act(() => {
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      });
      expect(box()?.hasAttribute("data-expanded")).toBe(false);
      expect(host.querySelector(".fb-noyan-scrim")).toBeNull();
    });

    it("Send answer answers Noyan's agent and brings the box back", async () => {
      await mountBox(askingView(LONG));
      vi.mocked(request).mockImplementation((method: string) =>
        method === "agent_panel.answer" ? Promise.resolve({ outcome: "done" }) : Promise.resolve(null),
      );
      act(() => {
        host.querySelector<HTMLButtonElement>('button[aria-label="Open larger"]')?.click();
      });
      expect(box()?.hasAttribute("data-expanded")).toBe(true);
      await act(async () => {
        button("Send answer")?.click();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(request).toHaveBeenCalledWith("agent_panel.answer", {
        project: "SHOP",
        agent: "c1",
        question: "q-long",
        pick: 1,
        other: "",
        clear: false,
      });
      expect(box()?.hasAttribute("data-expanded")).toBe(false);
    });

    it("Answer in terminal opens the agent panel on Noyan's conversation", async () => {
      shellLayout.set({ ...shellLayout.get(), agentOpen: false });
      await mountBox(askingView(SHORT));
      act(() => {
        button("Answer in terminal")?.click();
      });
      expect(shellLayout.get().agentOpen).toBe(true);
    });
  });

  it("sits under the columns in its own room, so it never covers the last card or Done's line", async () => {
    // No layout engine here: what keeps the cards clear at 1280 × 800 (as at any size) is that the box is the
    // columns' sibling below them in the editor's column, which gives the columns what is left, never drawn over them.
    await mountBox();
    const main = host.querySelector(".fb-main");
    expect([...(main?.children ?? [])].map((c) => c.className)).toEqual(["fb-columns", "fb-noyan"]);
    expect(host.querySelector(".fb-columns .fb-noyan")).toBeNull();
  });
});
