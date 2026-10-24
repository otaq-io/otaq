// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";

import { type DropTarget, DropTargets, filesDropped, routeDrop } from "./drops";

function target(typing: boolean): DropTarget & { pasted: string[] } {
  const pasted: string[] = [];
  return {
    pasted,
    takesTyping: () => typing,
    paste: (text) => {
      pasted.push(text);
    },
  };
}

/** A terminal's well with xterm.js's screen inside it, as the page has it. */
function well(): { well: HTMLElement; screen: HTMLElement } {
  const outer = document.createElement("div");
  const screen = document.createElement("canvas");
  outer.append(document.createElement("div"));
  outer.firstElementChild?.append(screen);
  document.body.append(outer);
  return { well: outer, screen };
}

const DROP = { paths: ["/tmp/a b.png", "/tmp/c"], paste: "/tmp/a\\ b.png /tmp/c ", x: 10, y: 20 };

describe("where a dropped file goes", () => {
  it("into a terminal that takes typing, nowhere for one that doesn't, as before elsewhere", () => {
    expect(routeDrop(target(true))).toBe("paste");
    expect(routeDrop(target(false))).toBe("ignore");
    expect(routeDrop(undefined)).toBe("elsewhere");
  });

  it("a place that takes files (Overview, RPT-02) gets every path, not a paste", () => {
    const targets = new DropTargets();
    const { well: w, screen } = well();
    const taken: (readonly string[])[] = [];
    targets.register(w, { ...target(false), takeFiles: (paths) => taken.push(paths) });
    expect(routeDrop(targets.at(screen))).toBe("files");
    expect(
      filesDropped(
        DROP,
        () => undefined,
        targets,
        () => screen,
      ),
    ).toBe("files");
    expect(taken).toEqual([DROP.paths]);
  });

  it("a terminal is found from any element inside it, and is gone once unregistered", () => {
    const targets = new DropTargets();
    const { well: w, screen } = well();
    const t = target(true);
    const unregister = targets.register(w, t);
    expect(targets.at(screen)).toBe(t);
    expect(targets.at(w)).toBe(t);
    expect(targets.at(document.body)).toBeUndefined();
    expect(targets.at(null)).toBeUndefined();
    unregister();
    expect(targets.at(screen)).toBeUndefined();
  });

  it("an old unregister doesn't remove the terminal registered on the same element since", () => {
    const targets = new DropTargets();
    const { well: w } = well();
    const first = targets.register(w, target(true));
    const second = target(true);
    targets.register(w, second);
    first();
    expect(targets.at(w)).toBe(second);
  });

  it("only the terminal under the pointer gets the paste, which carries every path", () => {
    const targets = new DropTargets();
    const panel = well();
    const tab = well();
    const [inAgentPanel, inTab] = [target(true), target(true)];
    targets.register(panel.well, inAgentPanel);
    targets.register(tab.well, inTab);
    const elsewhere: string[] = [];
    const at = (x: number) => (x < 100 ? panel.screen : tab.screen);

    expect(filesDropped({ ...DROP, x: 150 }, (p) => elsewhere.push(p), targets, at)).toBe("paste");
    expect(inTab.pasted).toEqual(["/tmp/a\\ b.png /tmp/c "]);
    expect(inAgentPanel.pasted).toEqual([]);
    expect(filesDropped({ ...DROP, x: 50 }, (p) => elsewhere.push(p), targets, at)).toBe("paste");
    expect(inAgentPanel.pasted).toEqual(["/tmp/a\\ b.png /tmp/c "]);
    expect(elsewhere).toEqual([]);
  });

  it("a terminal that doesn't take typing ignores it: no paste, no Add project", () => {
    const targets = new DropTargets();
    const { well: w, screen } = well();
    const ended = target(false);
    targets.register(w, ended);
    const elsewhere: string[] = [];
    expect(
      filesDropped(
        DROP,
        (p) => elsewhere.push(p),
        targets,
        () => screen,
      ),
    ).toBe("ignore");
    expect(ended.pasted).toEqual([]);
    expect(elsewhere).toEqual([]);
  });

  it("anywhere else the first path goes on as before", () => {
    const targets = new DropTargets();
    const elsewhere: string[] = [];
    expect(
      filesDropped(
        DROP,
        (p) => elsewhere.push(p),
        targets,
        () => document.body,
      ),
    ).toBe("elsewhere");
    expect(
      filesDropped(
        DROP,
        (p) => elsewhere.push(p),
        targets,
        () => null,
      ),
    ).toBe("elsewhere");
    expect(elsewhere).toEqual(["/tmp/a b.png", "/tmp/a b.png"]);
  });
});
