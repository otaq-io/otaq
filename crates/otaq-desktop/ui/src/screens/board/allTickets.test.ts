// @vitest-environment happy-dom
// The toolbar's All tickets asks the search field for the keyboard. Board passes `requestSearchFocus` to the button as
// it is, so a click must not hand it the click event.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, expect, it } from "vitest";

import { requestSearchFocus, searchFocusRequests } from "./filter";
import { Toolbar } from "./Toolbar";

const option = (label: string, selected: boolean) => ({
  label,
  icon: null,
  selected,
  change: { kind: "view", label },
});

const TOOLBAR = {
  views: { label: "View", options: [option("Board", true), option("List", false)] },
  lanes: null,
  group_by: null,
  dependencies: null,
  switch: null,
  subtitle: null,
  live: null,
  all_tickets: "All tickets",
  new_ticket: "New ticket",
  new_ticket_keys: "⌘N",
} as unknown as Parameters<typeof Toolbar>[0]["toolbar"];

let root: Root | null = null;

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  document.body.innerHTML = "";
  searchFocusRequests.set(0);
});

it("a click on All tickets asks the search field for the keyboard", () => {
  const host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() =>
    root?.render(
      createElement(Toolbar, {
        toolbar: TOOLBAR,
        onView: () => undefined,
        onAllTickets: requestSearchFocus,
        onNewTicket: () => undefined,
      }),
    ),
  );
  const button = [...host.querySelectorAll("button")].find((b) => b.textContent.includes("All tickets"));
  expect(button).toBeDefined();
  act(() => button?.click());
  expect(searchFocusRequests.get()).toBe(1);
});
