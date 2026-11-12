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

describe("Needs you on the drafting line", () => {
  it("names a long command on one line, with the command whole in its tooltip", async () => {
    const view: FlowBoardView = {
      ...(CHECK_FIRST as unknown as FlowBoardView),
      drafting: {
        needs_you: "Needs you",
        text: "Docs stage skill wants your permission: Run command: @' …",
        tooltip:
          "Docs stage skill wants your permission: Run command: @'\n---\nname: update-docs\n---\n'@ | Set-Content SKILL.md",
        agent: "c1",
        show: "Answer",
      },
    };
    await mount(() => Promise.resolve(view));
    const line = host.querySelector(".fb-drafting");
    expect(line?.hasAttribute("data-needs-you")).toBe(true);
    const text = line?.querySelector(".fb-drafting-text");
    expect(text?.textContent).toBe("Docs stage skill wants your permission: Run command: @' …");
    expect(text?.closest(".tooltip-anchor")).not.toBeNull();
    expect(line?.textContent).not.toContain("update-docs");
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

  it("while Noyan drafts, the shortcut is off and the box says why", async () => {
    const drafting = "Strong is drafting a flow for shop-web…";
    await mountBox({ ...VIEW, ask: { ...VIEW.ask, off: drafting } });
    expect(button("Suggest a whole flow")?.disabled).toBe(true);
    expect(box()?.textContent).toContain(drafting);
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

  it("sits under the columns in its own room, so it never covers the last card or Done's line", async () => {
    // No layout engine here: what keeps the cards clear at 1280 × 800 (as at any size) is that the box is the
    // columns' sibling below them in the editor's column, which gives the columns what is left, never drawn over them.
    await mountBox();
    const main = host.querySelector(".fb-main");
    expect([...(main?.children ?? [])].map((c) => c.className)).toEqual(["fb-columns", "fb-noyan"]);
    expect(host.querySelector(".fb-columns .fb-noyan")).toBeNull();
  });
});
