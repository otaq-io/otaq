// @vitest-environment happy-dom
// Edit flow never goes blank: the editor draws the draft the release test was left with (Validate dragged in front of
// Dev, its fails still going to Dev; the view is Rust's, `check_first.view.json`), and a view that doesn't come shows
// what went wrong with Discard the draft and Close instead of an empty screen.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

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
