// The link popover, the link bubble and read mode's link tooltip (T-0252): *08 → e · Link popover and link bubble*
// and *05 → c · Read mode · Dark*. One `<LinkLayer />` inside `MarkdownSurface` draws them for its editor. They float
// fixed to the window (the field's overflow never clips them) and follow the text on every transaction, scroll and
// resize.
import type { Editor } from "@tiptap/core";
import { useCurrentEditor, useEditorState } from "@tiptap/react";
import {
  type CSSProperties,
  type MouseEvent,
  type RefObject,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import { useStore } from "../../app/store";
import { Button, Kbd, PLAIN_TEXT } from "../../components";
import { placeFloating, type Rect } from "../../components/logic";
import { TOOLTIP_DELAY } from "../../components/Tooltip";
import { Icon } from "../../theme/Icon";
import {
  applyLink,
  closeLinkPopover,
  type LinkTarget,
  linkTargetAt,
  linkTargetOf,
  linkUi,
  removeLink,
  setLinkPopover,
} from "./links";
import { checkLinkUrl, openInBrowser, opensInBrowser, shortUrl } from "./url";
import "./link.css";

/** How long the bubble stays after the pointer leaves the link, so the pointer can reach it. */
const BUBBLE_GRACE = 250;

/** The box of the text from `from` to `to` (its first line when it wraps). */
function rangeRect(editor: Editor, from: number, to: number): Rect | null {
  try {
    const start = editor.view.coordsAtPos(from, 1);
    const end = editor.view.coordsAtPos(to, -1);
    const oneLine = Math.abs(end.top - start.top) < 2;
    return {
      left: start.left,
      top: start.top,
      width: oneLine ? Math.max(0, end.right - start.left) : 0,
      height: (oneLine ? end.bottom : start.bottom) - start.top,
    };
  } catch {
    return null;
  }
}

/**
 * The fixed position of `floating` under the box `rect` gives (`placeFloating`), measured again on each editor
 * transaction, scroll and resize. Hidden until measured.
 */
function useFloatingNear(
  editor: Editor,
  rect: () => Rect | null,
  floating: RefObject<HTMLElement | null>,
  key: unknown,
): CSSProperties {
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);
  const latest = useRef(rect);
  useLayoutEffect(() => {
    latest.current = rect;
  });
  useLayoutEffect(() => {
    const place = () => {
      const f = floating.current;
      const box = latest.current();
      if (!f || !box) {
        setPosition(null);
        return;
      }
      const next = placeFloating(
        box,
        { width: f.offsetWidth, height: f.offsetHeight },
        { width: window.innerWidth, height: window.innerHeight },
      );
      setPosition((now) => (now?.left === next.left && now.top === next.top ? now : next));
    };
    place();
    editor.on("transaction", place);
    window.addEventListener("scroll", place, true);
    window.addEventListener("resize", place);
    return () => {
      editor.off("transaction", place);
      window.removeEventListener("scroll", place, true);
      window.removeEventListener("resize", place);
    };
  }, [editor, floating, key]);
  return position
    ? { position: "fixed", left: position.left, top: position.top }
    : { position: "fixed", left: 0, top: 0, visibility: "hidden" };
}

/** Keeps the editor's caret and focus when a floating button is pressed. */
const keepFocus = (e: MouseEvent) => {
  e.preventDefault();
};

/** *e · Link popover*: the URL field (focused), Apply (↵), Remove; Esc or a press outside closes it. */
function LinkPopover({ editor, target }: { editor: Editor; target: LinkTarget }) {
  const panel = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLInputElement>(null);
  // The link's address or a selected URL; no clipboard pre-fill without a clipboard plugin (Q53).
  const [text, setText] = useState(target.href);
  const [error, setError] = useState<string | null>(null);
  const style = useFloatingNear(editor, () => rangeRect(editor, target.from, target.to), panel, target);

  // Focused once placed: a hidden field can't take the focus.
  const placed = style.visibility !== "hidden";
  const focused = useRef(false);
  useEffect(() => {
    if (!placed || focused.current) return;
    focused.current = true;
    field.current?.focus();
    field.current?.select();
  }, [placed]);

  const close = useCallback(
    (backToText: boolean) => {
      closeLinkPopover(editor);
      if (backToText) editor.view.focus();
    },
    [editor],
  );

  // Esc closes the popover alone (capturing, before the form's or the panel's Esc); a press outside closes it and
  // leaves the focus where the press put it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      close(true);
    };
    const onPress = (e: PointerEvent) => {
      if (!panel.current?.contains(e.target as Node | null)) close(false);
    };
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("pointerdown", onPress);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("pointerdown", onPress);
    };
  }, [close]);

  const apply = () => {
    const checked = checkLinkUrl(text);
    if (!checked.ok) {
      setError(checked.error);
      field.current?.focus();
      return;
    }
    closeLinkPopover(editor);
    applyLink(editor, target, checked.href);
  };

  return (
    <div ref={panel} className="md-link-popover" role="dialog" aria-label="Link" style={style}>
      <div className="md-link-field" data-invalid={error !== null || undefined}>
        <Icon name="link" size={13} />
        <input
          {...PLAIN_TEXT}
          ref={field}
          type="text"
          value={text}
          placeholder="https://"
          aria-label="URL"
          aria-invalid={error !== null}
          aria-describedby={error === null ? undefined : "md-link-error"}
          autoComplete="off"
          onChange={(e) => {
            setText(e.target.value);
            setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              apply();
            }
          }}
        />
      </div>
      {error !== null && (
        <p id="md-link-error" className="md-link-error" role="alert">
          {error}
        </p>
      )}
      <div className="md-link-buttons">
        <span className="md-link-hint">
          <Kbd keys="↵" />
          <span>apply</span>
          <Kbd keys="Esc" />
          <span>close</span>
        </span>
        <Button
          label="Remove"
          variant="ghost"
          disabled={!target.linked}
          onClick={() => {
            closeLinkPopover(editor);
            removeLink(editor, target);
          }}
        />
        <Button label="Apply" onClick={apply} />
      </div>
    </div>
  );
}

/** What the bubble is about: a hovered link element, or the link the caret is in. */
type BubbleSubject = { kind: "hover"; element: HTMLAnchorElement } | { kind: "caret"; target: LinkTarget };

/** *e · Link bubble*: the URL (shortened), Open, Edit and Remove, for the hovered link or the caret's. */
function LinkBubble({
  editor,
  subject,
  onEnter,
  onLeave,
}: {
  editor: Editor;
  subject: BubbleSubject;
  onEnter: () => void;
  onLeave: () => void;
}) {
  const bubble = useRef<HTMLDivElement>(null);
  const href = subject.kind === "hover" ? (subject.element.getAttribute("href") ?? "") : subject.target.href;
  const rect = () => {
    if (subject.kind === "caret") return rangeRect(editor, subject.target.from, subject.target.to);
    if (!subject.element.isConnected) return null;
    const box = subject.element.getBoundingClientRect();
    return { left: box.left, top: box.top, width: box.width, height: box.height };
  };
  const style = useFloatingNear(
    editor,
    rect,
    bubble,
    subject.kind === "hover" ? subject.element : subject.target,
  );
  const target = () => (subject.kind === "caret" ? subject.target : linkTargetOf(editor, subject.element));
  const canOpen = opensInBrowser(href);
  return (
    <div
      ref={bubble}
      className="md-link-bubble"
      role="toolbar"
      aria-label="Link"
      style={style}
      onMouseDown={keepFocus}
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
    >
      <Icon name="globe" size={12} />
      <span className="md-link-url" title={href}>
        {shortUrl(href)}
      </span>
      <span className="md-link-divider" aria-hidden />
      <button
        type="button"
        className="md-link-action"
        disabled={!canOpen}
        title={canOpen ? "Open in the browser" : "Only web links open from Otaq"}
        onClick={() => {
          openInBrowser(href);
        }}
      >
        <Icon name="external-link" size={13} />
        <span>Open</span>
      </button>
      <button
        type="button"
        className="md-link-action"
        onClick={() => {
          const t = target();
          if (t) setLinkPopover(editor, t);
        }}
      >
        <Icon name="pencil" size={13} />
        <span>Edit</span>
      </button>
      <button
        type="button"
        className="md-link-action"
        onClick={() => {
          const t = target();
          linkUi(editor).update((s) => ({ ...s, hover: null }));
          if (t) removeLink(editor, t);
        }}
      >
        <Icon name="unlink" size={13} />
        <span>Remove</span>
      </button>
    </div>
  );
}

/** Read mode: hovering a link shows where it goes (a click opens it, see `LinkUi`). */
function LinkTooltip({ editor, element }: { editor: Editor; element: HTMLAnchorElement }) {
  const tip = useRef<HTMLDivElement>(null);
  const style = useFloatingNear(
    editor,
    () => {
      if (!element.isConnected) return null;
      const box = element.getBoundingClientRect();
      return { left: box.left, top: box.top, width: box.width, height: box.height };
    },
    tip,
    element,
  );
  return (
    <div ref={tip} role="tooltip" className="tooltip md-link-tooltip" style={style}>
      <span>{element.getAttribute("href")}</span>
      <Icon name="external-link" size={12} />
    </div>
  );
}

/**
 * The hovered link as the layer shows it: after `delay` (read mode's tooltip waits like every tooltip), and held for a
 * moment after the pointer leaves it, and while the pointer is on the bubble, so the bubble can be reached.
 */
function useHeldHover(hover: HTMLAnchorElement | null, delay: number, grace: number) {
  const [held, setHeld] = useState<HTMLAnchorElement | null>(null);
  const over = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  const schedule = useCallback((next: HTMLAnchorElement | null, wait: number) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      setHeld(next);
    }, wait);
  }, []);
  useEffect(() => {
    if (hover) schedule(hover, delay);
    else if (!over.current) schedule(null, grace);
  }, [hover, delay, grace, schedule]);
  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
    },
    [],
  );
  return {
    held: held?.isConnected === true ? held : null,
    enter: () => {
      over.current = true;
      window.clearTimeout(timer.current);
    },
    leave: () => {
      over.current = false;
      schedule(null, grace);
    },
  };
}

function Layer({ editor }: { editor: Editor }) {
  const { popover, hover } = useStore(linkUi(editor));
  const caret = useEditorState({
    editor,
    selector: ({ editor: e }): LinkTarget | null => {
      if (!e.isEditable || !e.isFocused || !e.state.selection.empty) return null;
      const t = linkTargetAt(e.state);
      return t.linked ? t : null;
    },
  });
  // Read when the hover changes, which is when it matters; `setEditable` sends no transaction to watch.
  const editable = editor.isEditable;
  const { held, enter, leave } = useHeldHover(
    hover,
    editable ? 0 : TOOLTIP_DELAY,
    editable ? BUBBLE_GRACE : 0,
  );

  if (!editable) return held ? <LinkTooltip editor={editor} element={held} /> : null;
  if (popover) return <LinkPopover editor={editor} target={popover} />;
  const subject: BubbleSubject | null = held
    ? { kind: "hover", element: held }
    : caret
      ? { kind: "caret", target: caret }
      : null;
  if (!subject) return null;
  return <LinkBubble editor={editor} subject={subject} onEnter={enter} onLeave={leave} />;
}

/** Drawn once inside `MarkdownSurface`: the link popover, bubble and tooltip of the surface's editor. */
export function LinkLayer() {
  const { editor } = useCurrentEditor();
  return editor ? <Layer editor={editor} /> : null;
}
