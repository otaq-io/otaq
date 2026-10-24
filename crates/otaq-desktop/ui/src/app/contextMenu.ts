// The WebView's own right-click menu (Look Up, Translate, Speech, Services) is off app-wide (#11): only text fields,
// the Markdown editor and terminals keep it, for Cut, Copy, Paste and spelling. Places with a menu of their own (a
// card, a rail tile) open theirs and prevent this one themselves.

/** Where the WebView's menu still opens: a text field, an editable region, a terminal. */
const KEEPS_NATIVE_MENU = "input, textarea, [contenteditable]:not([contenteditable='false']), .xterm";

/** What `keepsNativeMenu` reads of the right-clicked node; an `Element` is one. */
export interface MenuTargetLike {
  closest?: (selector: string) => unknown;
}

/** Whether a right-click on `target` may open the WebView's own menu. */
export function keepsNativeMenu(target: MenuTargetLike | null): boolean {
  return Boolean(target?.closest?.(KEEPS_NATIVE_MENU));
}

/** Turns the WebView's menu off outside text fields and terminals; returns the undo. */
export function installContextMenu(target: Window = window): () => void {
  const onMenu = (e: MouseEvent) => {
    if (!keepsNativeMenu(e.target as MenuTargetLike | null)) e.preventDefault();
  };
  target.addEventListener("contextmenu", onMenu);
  return () => {
    target.removeEventListener("contextmenu", onMenu);
  };
}
