// @vitest-environment happy-dom
// The WebView's own menu (#11): off on the board and the rail, kept in text fields, the editor and terminals.
import { afterEach, describe, expect, it } from "vitest";

import { installContextMenu, keepsNativeMenu } from "./contextMenu";

let undo: () => void = () => undefined;

afterEach(() => {
  undo();
  document.body.innerHTML = "";
});

/** Right-clicks `element`; whether the WebView's menu would open. */
function rightClick(element: Element): boolean {
  const e = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
  element.dispatchEvent(e);
  return !e.defaultPrevented;
}

describe("the WebView's own menu", () => {
  it("opens only in text fields, editable regions and terminals", () => {
    document.body.innerHTML = `
      <button id="tile">DEMO</button>
      <input id="field" />
      <textarea id="box"></textarea>
      <div contenteditable="true"><p id="editor">text</p></div>
      <div class="xterm"><div id="term"></div></div>
      <div contenteditable="false"><span id="read">text</span></div>`;
    undo = installContextMenu(window);
    const at = (id: string) => document.getElementById(id) as Element;
    expect(rightClick(at("tile"))).toBe(false);
    expect(rightClick(at("read"))).toBe(false);
    expect(rightClick(at("field"))).toBe(true);
    expect(rightClick(at("box"))).toBe(true);
    expect(rightClick(at("editor"))).toBe(true);
    expect(rightClick(at("term"))).toBe(true);
    expect(keepsNativeMenu(null)).toBe(false);
  });
});
