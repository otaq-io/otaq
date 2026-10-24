// The one terminal component (T-0216, UIT-05, SES-09): the Terminal tab, the Shell tab, Project terminals and Project
// agents all show a session with it. xterm.js draws it; the session's output comes over its own door channel (stored
// scrollback first, then live output, see `src/terminal.rs`), and what it sends goes through `terminal.*` requests.
// See the crate README, "The terminal component".
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal as XTerm } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { type Ref, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";

import type { FileLink } from "../generated/FileLink";
import type { TerminalFeed } from "../generated/TerminalFeed";
import type { TerminalPhase } from "../generated/TerminalPhase";
import type { TerminalSettings } from "../generated/TerminalSettings";
import { keymap } from "../app/keyboard";
import { useShortcut } from "../app/registry";
import { focusTerminal, selection } from "../app/selection";
import { watchView } from "../service";
import { attachTerminal, request } from "../tauri";
import { Icon } from "../theme/Icon";
import { terminalColors } from "../theme/theme";
import { copyFromPrograms } from "./clipboard";
import { countWidthsAsUnicode11, MINIMUM_CONTRAST_RATIO } from "./drawing";
import { dropTargets } from "./drops";
import { filePathLinks } from "./file_links";
import { type FilesPlace, openPathIn } from "./files_place";
import { binaryBytes, createSender, decodeBase64, encodeBase64, isReply, type Outgoing } from "./input";
import {
  createLinkGate,
  gateHiddenLinks,
  type HiddenLinkOptions,
  hiddenLinkHandler,
  type LinkHint,
  macKeys,
  withGatedLinks,
} from "./links";
import {
  clampFontSize,
  findCount,
  INITIAL_STATE,
  jumpLabel,
  NEW_LINE,
  fontSizeOf,
  opaqueHex,
  type PickedSize,
  rendererFor,
  takesInput,
  type TerminalState,
  terminalKey,
  typingIn,
} from "./state";
import "./terminal.css";
import { FAST_SCROLL_SENSITIVITY, followWheel, SCROLL_SENSITIVITY } from "./wheel";
import { PLAIN_TEXT, plainTextOff } from "../components";

/** What a host can ask of a terminal (its header strip's buttons). */
export interface TerminalHandle {
  /** Gives it the keyboard: now, or right after it opens when it isn't open yet (its font still loading). */
  focus(): void;
  /** Opens the find bar (as ⌘F / Ctrl+Shift+F does). */
  openFind(): void;
  /** Scrolls to the newest output. */
  jumpToLatest(): void;
  /** One point larger or smaller, as ⌘+ / ⌘− (the header strip's A+ / A−). */
  changeFontSize(delta: number): void;
}

export interface TerminalProps {
  /** The session shown (`SessionInfo.id`). Another one attaches anew. */
  session: string;
  /** The terminal's accessible name ("SHOP-19 · agent terminal"). */
  label: string;
  /** Output only: nothing typed is sent (the Run tab). */
  readOnly?: boolean;
  /** Hears every change of what a header or hint strip shows. */
  onState?: (state: TerminalState) => void;
  /** Where file paths in its output open (d0117 point 8); without it they aren't links. */
  files?: FilesPlace;
  /** Takes the keyboard as soon as it opens (a terminal the user just asked for, d0126); read when its session
   * attaches. What is typed before the session is live is sent once it is. */
  focusWhenLive?: boolean;
  ref?: Ref<TerminalHandle>;
}

/** Lines kept above the screen. The service keeps its own, larger scrollback for replays. */
const SCROLLBACK = 10_000;
const SEARCH_LIMIT = 1000;

interface Find {
  open: boolean;
  text: string;
  index: number;
  count: number;
}

const CLOSED_FIND: Find = { open: false, text: "", index: -1, count: 0 };

/** The font a terminal opens with, until the setting's font stack arrives (`TerminalSettings.font_family`). */
function fontFamily(element: Element): string {
  return getComputedStyle(element).getPropertyValue("--font-mono").trim() || "monospace";
}

function searchOptions(element: Element) {
  const style = getComputedStyle(element);
  const read = (name: string) => opaqueHex(style.getPropertyValue(`--${name}`));
  return {
    decorations: {
      matchBackground: read("st-attn-bg"),
      matchOverviewRuler: read("st-attn"),
      activeMatchBackground: read("st-attn"),
      activeMatchColorOverviewRuler: read("st-attn"),
    },
  };
}

export function Terminal({
  session,
  label,
  readOnly = false,
  onState,
  files,
  focusWhenLive = false,
  ref,
}: TerminalProps) {
  const well = useRef<HTMLDivElement>(null);
  const host = useRef<HTMLDivElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const xterm = useRef<XTerm | null>(null);
  // Fits the terminal to its box, except while a replay is written at the size it was drawn at.
  const fitBox = useRef<(() => void) | null>(null);
  const search = useRef<SearchAddon | null>(null);
  // The terminal is open in the page (after its font loaded); the font size and the renderer apply from then on.
  const [opened, setOpened] = useState(false);
  const findInput = useRef<HTMLInputElement>(null);
  const [state, setState] = useState<TerminalState>(INITIAL_STATE);
  const [settings, setSettings] = useState<TerminalSettings>();
  const [picked, setPicked] = useState<PickedSize>();
  const [find, setFind] = useState<Find>(CLOSED_FIND);
  // "⌘-click to open" at a link while ⌘ is held, in the box's coordinates.
  const [linkHint, setLinkHint] = useState<(LinkHint & { above: boolean }) | null>(null);
  const webglFailed = useRef(false);
  // A focus asked for before xterm.js opened: served right after `term.open` (d0126).
  const focusPending = useRef(false);
  const wantsFocus = useRef(focusWhenLive);
  useEffect(() => {
    wantsFocus.current = focusWhenLive;
  }, [focusWhenLive]);
  // Read at each click, so a host's new object doesn't attach the terminal again.
  const filesPlace = useRef(files);
  useEffect(() => {
    filesPlace.current = files;
  }, [files]);
  const update = useCallback((change: Partial<TerminalState>) => {
    setState((s) => {
      const next = { ...s, ...change };
      return Object.entries(change).every(([k, v]) => s[k as keyof TerminalState] === v) ? s : next;
    });
  }, []);

  // The settings every terminal follows (font size, font, renderer), again on each change.
  useEffect(
    () =>
      watchView(
        () => request("terminal.settings") as Promise<TerminalSettings>,
        ["settings_changed"],
        (s) => {
          if (s.view) setSettings(s.view);
        },
      ),
    [],
  );
  // A setting change replaces a size picked with the keys.
  const fontSize = fontSizeOf(settings?.font_size, picked);
  const settingSize = useRef(INITIAL_STATE.fontSize);
  useEffect(() => {
    settingSize.current = settings?.font_size ?? INITIAL_STATE.fontSize;
  }, [settings?.font_size]);
  useEffect(() => onState?.({ ...state, fontSize }), [onState, state, fontSize]);

  const openFind = useCallback(() => {
    setFind((f) => ({ ...f, open: true }));
    // Already open: take the keyboard back to the field.
    findInput.current?.select();
  }, []);
  const jumpToLatest = useCallback(() => {
    xterm.current?.scrollToBottom();
  }, []);
  useImperativeHandle(
    ref,
    () => ({
      focus: () => {
        const term = xterm.current;
        if (term?.element) term.focus();
        else focusPending.current = true;
      },
      openFind,
      jumpToLatest,
      changeFontSize: (delta: number) => {
        setPicked({ setting: settingSize.current, size: clampFontSize(fontSize + delta) });
      },
    }),
    [openFind, jumpToLatest, fontSize],
  );
  // ⌘F / Ctrl+Shift+F while this terminal has the keyboard; other terminals pass it on.
  useShortcut(
    "find_in_terminal",
    useCallback(() => {
      if (selection.get().focusedTerminal !== session) return false;
      openFind();
      return true;
    }, [session, openFind]),
  );

  // The terminal itself, attached to the session, for as long as the session stays the same.
  useEffect(() => {
    const element = host.current;
    if (!element) return;
    let disposed = false;
    let phase: TerminalPhase = "attaching";
    let focused = false;
    let attach: number | undefined;
    let replaying = false;
    let newLines = 0;
    // Typed while attaching, sent in order once live (`typingIn`).
    let held: Outgoing[] = [];
    if (wantsFocus.current) focusPending.current = true;
    const term = new XTerm({
      allowProposedApi: true,
      scrollback: SCROLLBACK,
      fontFamily: fontFamily(element),
      fontSize: INITIAL_STATE.fontSize,
      theme: terminalColors(getComputedStyle(element)),
      // Typing is taken while attaching too, and held until live (`typingIn`).
      disableStdin: readOnly,
      cursorBlink: false,
      macOptionIsMeta: true,
      // Option-drag selects, also inside a tool that takes the mouse (d0117 point 3).
      macOptionClickForcesSelection: true,
      minimumContrastRatio: MINIMUM_CONTRAST_RATIO,
      drawBoldTextInBrightColors: false,
      // About 3 lines per wheel notch in WKWebView; set again per wheel event (`followWheel`, T-0502).
      scrollSensitivity: SCROLL_SENSITIVITY,
      fastScrollSensitivity: FAST_SCROLL_SENSITIVITY,
    });
    const unwheel = followWheel(element, term);
    // Before anything is written, the replay too.
    countWidthsAsUnicode11(term);
    const fit = new FitAddon();
    const searchAddon = new SearchAddon({ highlightLimit: SEARCH_LIMIT });
    term.loadAddon(fit);
    term.loadAddon(searchAddon);
    // Links (d0117 points 7 and 9): every kind opens on ⌘-click only, through the gate; OSC 8 links too.
    const openWeb = (url: string) => {
      request("terminal.open_link", { url }).catch((e: unknown) => {
        console.error("the link couldn't open", e);
      });
    };
    // File paths and `file:` links (d0117 point 8): only an existing file inside the session's folder, checked by Rust,
    // opens where the host says.
    const openFileLink = (path: string) => {
      (request("file_links.check", { session, path }) as Promise<string | null>).then(
        (file) => {
          const place = filesPlace.current;
          if (file !== null && place) openPathIn(place, file, null);
        },
        (e: unknown) => {
          console.warn("the file link couldn't be checked", e);
        },
      );
    };
    const hidden: HiddenLinkOptions = {
      mac: macKeys,
      openWeb,
      get openFile() {
        return filesPlace.current ? openFileLink : undefined;
      },
    };
    term.options.linkHandler = hiddenLinkHandler(hidden);
    const links = createLinkGate({
      mac: macKeys,
      element,
      onHint: (hint) => {
        const at = box.current?.getBoundingClientRect();
        setLinkHint(
          hint && at
            ? { ...hint, x: hint.x - at.left, y: hint.y - at.top, above: at.bottom - hint.y < 48 }
            : null,
        );
      },
    });
    gateHiddenLinks(term, links, hidden);
    const webLinks = new WebLinksAddon((event, uri) => {
      event.preventDefault();
      openWeb(uri);
    });
    term.loadAddon({
      activate: (t) => {
        webLinks.activate(withGatedLinks(t, links));
      },
      dispose: () => {
        webLinks.dispose();
      },
    });
    // File paths in the output (d0117 point 8), after the add-ons' providers.
    const paths = links.register(
      term,
      filePathLinks({
        buffer: () => term.buffer.active,
        rows: () => term.rows,
        cols: () => term.cols,
        find: (lines) =>
          filesPlace.current
            ? (request("file_links.find", { session, lines }) as Promise<FileLink[][]>)
            : Promise.resolve(lines.map(() => [])),
        open: (path, line) => {
          const place = filesPlace.current;
          if (place) openPathIn(place, path, line);
        },
      }),
    );
    // Programs copy to the clipboard (OSC 52), never from a replay; they can't read it.
    const copying = copyFromPrograms(
      term,
      () => replaying,
      (text) => {
        navigator.clipboard.writeText(text).catch((err: unknown) => {
          console.warn("a program's copy failed", err);
        });
      },
    );
    xterm.current = term;
    const fitToBox = () => {
      if (term.element && !replaying) fit.fit();
    };
    fitBox.current = fitToBox;
    search.current = searchAddon;

    const sender = createSender(
      (item: Outgoing) =>
        item.kind === "input"
          ? request("terminal.input", { session, data: encodeBase64(item.data), reply: item.reply })
          : request("terminal.resize", { session, cols: item.cols, rows: item.rows }),
      (e: unknown) => {
        console.warn(`terminal ${session}: not sent`, e);
      },
    );
    const sendInput = (data: Uint8Array, reply: boolean) => {
      // Answers to questions in the replayed output were for an earlier moment of the program: not sent.
      if (readOnly || (reply && replaying)) return;
      const typing = typingIn(phase);
      if (typing === "send") sender.push({ kind: "input", data, reply });
      // The program's answers are for the moment they were asked; what the user typed waits for live.
      else if (typing === "hold" && !reply) held.push({ kind: "input", data, reply });
    };
    const reportFocus = (now: boolean) => {
      request("terminal.focus", { session, focused: now }).catch(() => undefined);
    };

    const typed = term.onData((data) => {
      sendInput(new TextEncoder().encode(data), isReply(data));
    });
    const binary = term.onBinary((data) => {
      sendInput(binaryBytes(data), true);
    });
    const resized = term.onResize(({ cols, rows }) => {
      if (takesInput(phase)) sender.push({ kind: "resize", cols, rows });
    });
    const scrolledUp = () => term.buffer.active.baseY - term.buffer.active.viewportY;
    const followScroll = () => {
      const up = scrolledUp();
      if (up === 0) newLines = 0;
      update({ scrolledUp: up, newLines });
    };
    const scrolled = term.onScroll(followScroll);
    const lineFed = term.onLineFeed(() => {
      if (scrolledUp() > 0) newLines++;
    });
    const parsed = term.onWriteParsed(followScroll);
    const results = searchAddon.onDidChangeResults(({ resultIndex, resultCount }) => {
      setFind((f) => ({ ...f, index: resultIndex, count: resultCount }));
    });

    // ⌘+ / ⌘−: this terminal's size, until the setting changes.
    const pickSize = (delta: number) => {
      const setting = settingSize.current;
      setPicked({ setting, size: clampFontSize((term.options.fontSize ?? setting) + delta) });
    };
    // Its own keys: copy, paste, font size and new line (from the Rust map); everything else is the session's.
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      const key = terminalKey(keymap.get(), e);
      if (key === undefined) return true;
      e.preventDefault();
      switch (key) {
        case "copy":
          if (term.hasSelection()) {
            navigator.clipboard.writeText(term.getSelection()).catch((err: unknown) => {
              console.warn("copy failed", err);
            });
          }
          break;
        case "paste":
          navigator.clipboard.readText().then(
            (text) => {
              term.paste(text);
            },
            (err: unknown) => {
              console.warn("paste failed", err);
            },
          );
          break;
        case "font_bigger":
          pickSize(1);
          break;
        case "font_smaller":
          pickSize(-1);
          break;
        case "new_line":
          // Typing, as if xterm.js had sent it: the same ordered path, not a reply.
          sendInput(new TextEncoder().encode(NEW_LINE), false);
          break;
      }
      return false;
    });

    const setPhase = (next: TerminalPhase) => {
      phase = next;
      term.options.disableStdin = readOnly || typingIn(next) === "drop";
      term.options.cursorInactiveStyle = takesInput(next) ? "outline" : "none";
      if (takesInput(next)) {
        // The PTY follows this terminal's size; a new connection holds no focus until told (ATT-03).
        sender.push({ kind: "resize", cols: term.cols, rows: term.rows });
        if (focused) reportFocus(true);
        for (const item of held) sender.push(item);
        held = [];
      } else {
        sender.clear();
        if (typingIn(next) === "drop") held = [];
      }
      update({ phase: next });
    };
    const onFeed = (feed: TerminalFeed) => {
      if (disposed) return;
      switch (feed.kind) {
        case "replay":
          // A fresh start: the first attach, or again after a reconnect. The output's cursor moves were drawn for
          // the PTY's size, so it is written at that size; written at another, they land on the wrong lines. Then
          // the terminal fits its box, and the PTY follows it.
          term.reset();
          replaying = true;
          if (feed.size) term.resize(feed.size.cols, feed.size.rows);
          term.write(decodeBase64(feed.data), () => {
            replaying = false;
            fitToBox();
          });
          break;
        case "live":
          term.write(decodeBase64(feed.data));
          break;
        case "phase":
          setPhase(feed.phase);
          break;
      }
    };
    attachTerminal(session, onFeed, readOnly).then(
      (number) => {
        if (disposed) {
          request("terminal.detach", { attach: number }).catch(() => undefined);
        } else {
          attach = number;
        }
      },
      (e: unknown) => {
        console.error(`terminal ${session}: couldn't attach`, e);
        setPhase("no_session");
      },
    );

    // Files dropped on it paste their paths, as ⌘V pastes (d0099); the page finds it under the pointer.
    const undrop = dropTargets.register(well.current ?? element, {
      takesTyping: () => !readOnly && takesInput(phase),
      paste: (text) => {
        term.paste(text);
        term.focus();
      },
    });

    const onFocus = () => {
      focused = true;
      focusTerminal(session);
      reportFocus(true);
      update({ focused: true });
    };
    const onBlur = () => {
      focused = false;
      if (selection.get().focusedTerminal === session) focusTerminal(null);
      reportFocus(false);
      update({ focused: false });
    };

    // The colours follow the theme live: the frame swaps `data-theme` on the root element (T-0213).
    const themeWatch = new MutationObserver(() => {
      term.options.theme = terminalColors(getComputedStyle(element));
    });
    themeWatch.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    const sizeWatch = new ResizeObserver(fitToBox);

    // Opened once the monospace font is loaded, so xterm.js measures its cells with it.
    const family = fontFamily(element);
    const fontLoaded = document.fonts.load(`${String(INITIAL_STATE.fontSize)}px ${family}`).catch(() => []);
    void fontLoaded.then(() => {
      if (disposed) return;
      term.open(element);
      // xterm.js sets these on its helper textarea itself today; kept here so an upgrade can't bring autocorrect back.
      if (term.textarea) plainTextOff(term.textarea);
      term.textarea?.addEventListener("focus", onFocus);
      term.textarea?.addEventListener("blur", onBlur);
      sizeWatch.observe(element);
      fitToBox();
      setOpened(true);
      if (focusPending.current) {
        focusPending.current = false;
        term.focus();
      }
    });

    return () => {
      disposed = true;
      undrop();
      unwheel();
      if (focused) onBlur();
      if (attach !== undefined) request("terminal.detach", { attach }).catch(() => undefined);
      themeWatch.disconnect();
      sizeWatch.disconnect();
      for (const d of [typed, binary, resized, scrolled, lineFed, parsed, results, copying, paths, links])
        d.dispose();
      term.textarea?.removeEventListener("focus", onFocus);
      term.textarea?.removeEventListener("blur", onBlur);
      term.dispose();
      xterm.current = null;
      fitBox.current = null;
      search.current = null;
      setOpened(false);
    };
  }, [session, readOnly, update]);

  // The font size (the setting, or ⌘+ / ⌘−), and the renderer the setting picks.
  useEffect(() => {
    const term = xterm.current;
    if (!opened || !term) return;
    term.options.fontSize = fontSize;
    fitBox.current?.();
  }, [opened, fontSize]);
  // The font (d0117 point 5): every open terminal follows the setting at once and keeps its scrollback. The font is
  // loaded first so xterm.js measures its cells with it, and the glyph atlas (WebGL) is drawn again with it.
  const family = settings?.font_family;
  useEffect(() => {
    const term = xterm.current;
    if (!opened || !term || family === undefined || term.options.fontFamily === family) return;
    let live = true;
    const size = String(term.options.fontSize ?? INITIAL_STATE.fontSize);
    void document.fonts
      .load(`${size}px ${family}`)
      .catch(() => [])
      .then(() => {
        if (!live) return;
        term.options.fontFamily = family;
        term.clearTextureAtlas();
        fitBox.current?.();
      });
    return () => {
      live = false;
    };
  }, [opened, family]);
  const setting = settings?.renderer;
  useEffect(() => {
    const term = xterm.current;
    if (!opened || !term || setting === undefined) return;
    if (rendererFor(setting, webglFailed.current) === "dom") return;
    // WebGL that fails to start falls back to DOM; one that starts and draws garbage can't be told apart, so the
    // setting stays the user's choice (Q37).
    let webgl: WebglAddon | null = null;
    try {
      webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        webgl?.dispose();
        webgl = null;
        webglFailed.current = true;
      });
      term.loadAddon(webgl);
    } catch (e) {
      console.warn("WebGL couldn't start; the terminal draws with DOM", e);
      webgl = null;
      webglFailed.current = true;
    }
    return () => {
      webgl?.dispose();
    };
  }, [opened, setting]);

  const closeFind = () => {
    search.current?.clearDecorations();
    setFind(CLOSED_FIND);
    xterm.current?.focus();
  };
  const runFind = (text: string, backwards: boolean, incremental = false) => {
    const element = host.current;
    const addon = search.current;
    if (!element || !addon) return;
    if (text === "") {
      addon.clearDecorations();
      setFind((f) => ({ ...f, index: -1, count: 0 }));
      return;
    }
    const options = { ...searchOptions(element), incremental };
    if (backwards) addon.findPrevious(text, options);
    else addon.findNext(text, options);
  };

  return (
    <div className="terminal-well" ref={well} data-phase={state.phase}>
      {find.open && (
        <div className="terminal-find" role="search">
          <Icon name="search" />
          <input
            {...PLAIN_TEXT}
            ref={findInput}
            className="terminal-find-input"
            aria-label="Find in terminal"
            value={find.text}
            // eslint-disable-next-line jsx-a11y/no-autofocus -- the find bar opens on the user's ⌘F, focused like any find field
            autoFocus
            onChange={(e) => {
              const text = e.target.value;
              setFind((f) => ({ ...f, text }));
              runFind(text, false, true);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                closeFind();
              } else if (e.key === "Enter") {
                e.preventDefault();
                runFind(find.text, e.shiftKey);
              }
            }}
          />
          {find.text !== "" && (
            <span className="terminal-find-count">{findCount(find.index, find.count, SEARCH_LIMIT)}</span>
          )}
          <button
            type="button"
            className="terminal-icon-button"
            aria-label="Previous match"
            onClick={() => {
              runFind(find.text, true);
            }}
          >
            <Icon name="chevron-up" />
          </button>
          <button
            type="button"
            className="terminal-icon-button"
            aria-label="Next match"
            onClick={() => {
              runFind(find.text, false);
            }}
          >
            <Icon name="chevron-down" />
          </button>
          <button type="button" className="terminal-icon-button" aria-label="Close find" onClick={closeFind}>
            <Icon name="x" />
          </button>
        </div>
      )}
      <div className="terminal-box" ref={box}>
        <div className="terminal-host" ref={host} role="group" aria-label={label} />
        {linkHint && (
          <div
            className="terminal-link-hint"
            data-above={linkHint.above}
            style={{ left: linkHint.x, top: linkHint.y }}
            aria-hidden="true"
          >
            {linkHint.label}
          </div>
        )}
        {state.scrolledUp > 0 && (
          <button type="button" className="terminal-jump" onClick={jumpToLatest}>
            <Icon name="arrow-down" />
            {jumpLabel(state.newLines)}
          </button>
        )}
        {state.phase === "reconnecting" && (
          <div className="terminal-reconnecting" role="status">
            <Icon name="refresh-cw" />
            Reconnecting… agents keep running
          </div>
        )}
      </div>
    </div>
  );
}
