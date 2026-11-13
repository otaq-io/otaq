// Terminal links (d0117 points 7 and 9): every link opens with ⌘-click (Ctrl-click on Windows and Linux), whoever
// found it: plain https:// links, links a program hides behind text (OSC 8), file paths (T-0485). A plain click or drag
// always selects. Holding the key underlines the link under the mouse and shows "⌘-click to open"; without it nothing
// is underlined. One gate does this for every kind of link: each link provider is wrapped by it.
import type { IDisposable, ILink, ILinkHandler, ILinkProvider, Terminal } from "@xterm/xterm";

import { keymap } from "../app/keyboard";

/** The modifier state of a mouse or key event. */
export interface GestureLike {
  metaKey: boolean;
  ctrlKey: boolean;
}

/** Whether `e` is the open gesture: ⌘ held on macOS (without Ctrl), Ctrl held elsewhere (without the Windows key). */
export function isOpenGesture(e: GestureLike, mac: boolean): boolean {
  return mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}

/** The hint shown at a link while the open key is held. */
export function openHint(mac: boolean): string {
  return mac ? "⌘-click to open" : "Ctrl-click to open";
}

/** Whether keys are the macOS ones: the keymap's platform, or the browser's guess before it arrived. */
export function macKeys(): boolean {
  const map = keymap.get();
  return map ? map.platform === "mac" : navigator.userAgent.includes("Mac");
}

/** Where a link a program sent (OSC 8) leads: a web page, or a file (opened as T-0485 opens paths). */
export type LinkTarget = { kind: "web"; url: string } | { kind: "file"; path: string };

/** The target of an OSC 8 link: only http, https and file links have one (d0117 point 7); others never open. */
export function hiddenLinkTarget(uri: string): LinkTarget | undefined {
  let url: URL;
  try {
    url = new URL(uri);
  } catch {
    return undefined;
  }
  switch (url.protocol) {
    case "http:":
    case "https:":
      return { kind: "web", url: uri };
    case "file:":
      // `file://host/path` from `ls --hyperlink` names this Mac; only the path counts.
      try {
        return { kind: "file", path: decodeURIComponent(url.pathname) };
      } catch {
        return undefined;
      }
    default:
      return undefined;
  }
}

/** The hint "⌘-click to open" at the mouse (page coordinates), or none. */
export interface LinkHint {
  label: string;
  x: number;
  y: number;
}

export interface LinkGateOptions {
  /** Whether the keys are the macOS ones (read on each event: the keymap can arrive late). */
  mac: () => boolean;
  /** Hears the hint to show, or `null` to hide it. */
  onHint: (hint: LinkHint | null) => void;
  /** The terminal's element: mouse moves over it update the pointer's place and the held key. */
  element: HTMLElement;
  /** Where key presses are heard (the window; the keyboard may be elsewhere while the mouse is on a link). */
  keys?: Window;
}

export interface LinkGate {
  /**
   * The provider with its links gated: each opens only on the open gesture, and is underlined (with the pointer cursor)
   * only while the open key is held. `title` (OSC 8) shows a link's address as a tooltip on hover; `keep` drops the
   * links that would never open, so they aren't links at all.
   */
  wrap(provider: ILinkProvider, options?: WrapOptions): ILinkProvider;
  /** Registers `provider` with `term`, gated. */
  register(term: Pick<Terminal, "registerLinkProvider">, provider: ILinkProvider): IDisposable;
  dispose(): void;
}

export interface WrapOptions {
  title?: boolean;
  keep?: (text: string) => boolean;
}

interface Hovered {
  link: ILink;
  x: number;
  y: number;
}

export function createLinkGate({ mac, onHint, element, keys = window }: LinkGateOptions): LinkGate {
  let held = false;
  let hovered: Hovered | null = null;

  const show = () => {
    onHint(held && hovered ? { label: openHint(mac()), x: hovered.x, y: hovered.y } : null);
  };
  // xterm.js tracks a hovered link's decorations once it is hovered: setting them redraws the underline.
  const decorate = () => {
    const decorations = hovered?.link.decorations;
    if (!decorations) return;
    decorations.underline = held;
    decorations.pointerCursor = held;
  };
  const hold = (e: GestureLike) => {
    const now = isOpenGesture(e, mac());
    if (now === held) return;
    held = now;
    decorate();
    show();
  };
  const onMouse = (e: MouseEvent) => {
    if (hovered) {
      hovered.x = e.clientX;
      hovered.y = e.clientY;
    }
    hold(e);
    if (hovered && held) show();
  };
  const onKey = (e: KeyboardEvent) => {
    hold(e);
  };
  const release = () => {
    hold({ metaKey: false, ctrlKey: false });
  };
  // Heard on the way down: the terminal may keep a move from going further (to keep a selection on Windows).
  element.addEventListener("mousemove", onMouse, true);
  keys.addEventListener("keydown", onKey, true);
  keys.addEventListener("keyup", onKey, true);
  keys.addEventListener("blur", release);

  const gateLink = (link: ILink, title: boolean): ILink => {
    const gated: ILink = {
      ...link,
      decorations: { underline: held, pointerCursor: held },
      activate: (event, text) => {
        if (isOpenGesture(event, mac())) link.activate(event, text);
      },
      hover: (event, text) => {
        hovered = { link: gated, x: event.clientX, y: event.clientY };
        if (title) element.title = text;
        hold(event);
        show();
        // xterm.js starts tracking the decorations right after this call: catch up with a key pressed meanwhile.
        queueMicrotask(decorate);
        link.hover?.(event, text);
      },
      leave: (event, text) => {
        if (hovered?.link === gated) hovered = null;
        if (title) element.removeAttribute("title");
        show();
        link.leave?.(event, text);
      },
    };
    return gated;
  };

  const wrap: LinkGate["wrap"] = (provider, options = {}) => ({
    provideLinks: (line, callback) => {
      provider.provideLinks(line, (links) => {
        const kept = options.keep ? links?.filter((link) => options.keep?.(link.text)) : links;
        callback(kept?.map((link) => gateLink(link, options.title === true)));
      });
    },
  });

  return {
    wrap,
    register: (term, provider) => term.registerLinkProvider(wrap(provider)),
    dispose: () => {
      element.removeEventListener("mousemove", onMouse, true);
      keys.removeEventListener("keydown", onKey, true);
      keys.removeEventListener("keyup", onKey, true);
      keys.removeEventListener("blur", release);
      element.removeAttribute("title");
      hovered = null;
      onHint(null);
    },
  };
}

/**
 * `term` as an add-on sees it, with the link providers it registers gated (the web links add-on registers its own).
 */
export function withGatedLinks(term: Terminal, gate: LinkGate): Terminal {
  return new Proxy(term, {
    get(target, key) {
      if (key === "registerLinkProvider") {
        return (provider: ILinkProvider) => gate.register(target, provider);
      }
      const value: unknown = Reflect.get(target, key, target);
      return typeof value === "function" ? (value as (...args: unknown[]) => unknown).bind(target) : value;
    },
  });
}

/** How OSC 8 links open: web links with `openWeb`, file links with `openFile` (T-0485; none yet: not opened). */
export interface HiddenLinkOptions {
  mac: () => boolean;
  openWeb: (url: string) => void;
  /** Opens a file link's path as T-0485 opens paths, which checks it is inside the session's folder. */
  openFile?: (path: string) => void;
}

/** Whether an OSC 8 link to `uri` would open: http and https, and file links once they can open. */
export function hiddenLinkOpens(uri: string, options: HiddenLinkOptions): boolean {
  const target = hiddenLinkTarget(uri);
  return target?.kind === "web" || (target?.kind === "file" && options.openFile !== undefined);
}

/** xterm.js's handler of OSC 8 links: opens on the open gesture only, as `options` says; other schemes are ignored. */
export function hiddenLinkHandler(options: HiddenLinkOptions): ILinkHandler {
  return {
    // Every scheme reaches `activate`, which picks; xterm.js alone would drop file links.
    allowNonHttpProtocols: true,
    activate: (event, text) => {
      if (!isOpenGesture(event, options.mac())) return;
      const target = hiddenLinkTarget(text);
      if (target?.kind === "web") options.openWeb(target.url);
      else if (target?.kind === "file") options.openFile?.(target.path);
    },
  };
}

interface WithLinkProviders {
  _core?: { _linkProviderService?: { linkProviders?: ILinkProvider[] } };
}

/**
 * Gates xterm.js's own provider of OSC 8 links, keeping only the links that would open (a `javascript:` link is no
 * link). xterm.js registers that provider inside the terminal, out of reach of the public API, so it is wrapped in
 * place (its first provider). Returns whether it was found; without it OSC 8 links still open only on the gesture
 * (`hiddenLinkHandler` checks too), but are underlined on a plain hover.
 */
export function gateHiddenLinks(term: Terminal, gate: LinkGate, options: HiddenLinkOptions): boolean {
  const providers = (term as unknown as WithLinkProviders)._core?._linkProviderService?.linkProviders;
  const own = providers?.[0];
  if (!providers || !own) {
    console.warn("the terminal's OSC 8 links couldn't be gated");
    return false;
  }
  providers[0] = gate.wrap(own, { title: true, keep: (uri) => hiddenLinkOpens(uri, options) });
  return true;
}
