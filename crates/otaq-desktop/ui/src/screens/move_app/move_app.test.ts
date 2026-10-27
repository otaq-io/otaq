import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { MoveAppView } from "../../generated/MoveAppView";
import { afterMove } from "./logic";
import { MoveBody } from "./MoveApp";

const nothing = () => undefined;

const offer: MoveAppView = {
  window_title: "App",
  title: "Move App to Applications",
  error: null,
  text: "App is running from the disk image.",
  quit: "Quit",
  move_to: "Move to Applications",
  try_again: null,
  show_in_finder: null,
};

const failed: MoveAppView = {
  ...offer,
  title: "App couldn’t be moved",
  error: "You don’t have permission to change /Applications or ~/Applications.",
  text: "Drag App from the disk image’s window onto its Applications shortcut, then open it from Applications.",
  move_to: null,
  try_again: "Try again",
  show_in_finder: "Show in Finder",
};

const render = (view: MoveAppView) =>
  renderToStaticMarkup(
    createElement(MoveBody, { view, busy: false, onQuit: nothing, onMove: nothing, onShowInFinder: nothing }),
  );

describe("the Move window", () => {
  it("offers Quit and Move to Applications", () => {
    const html = render(offer);
    expect(html).toContain("Move App to Applications");
    expect(html).toContain("Quit");
    expect(html).toContain("Move to Applications");
    expect(html).not.toContain("Try again");
    expect(html).not.toContain('role="alert"');
  });

  it("after a failed move says why, with Try again and Show in Finder", () => {
    const html = render(failed);
    expect(html).toContain('role="alert"');
    expect(html).toContain("permission to change /Applications");
    expect(html).toContain("Try again");
    expect(html).toContain("Show in Finder");
    expect(html.indexOf("Try again")).toBeLessThan(html.indexOf("Show in Finder"));
    expect(html).not.toContain("Move to Applications<");
  });

  it("quits once moved, else shows the answer", () => {
    expect(afterMove({ moved: true })).toEqual({ quit: true });
    expect(afterMove(failed)).toEqual({ quit: false, view: failed });
  });
});
