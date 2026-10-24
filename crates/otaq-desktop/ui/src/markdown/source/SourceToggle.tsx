// The Markdown source toggle and the source text area (T-0251, d0078 D9), drawn from *crewboard.pen → 08 · Ticket editor
// → Row · Markdown editor · source, links, tables, code → d · Markdown source on*. `useSourceMode` holds the mode for one
// editor (source.ts has the rules); the screen gives `editorValue` to `useMarkdownEditor`, `<SourceToggle>` to the
// toolbar's `sourceToggle` slot and `<SourceArea>` to `MarkdownSurface`'s `source` slot while the source shows.
import type { Editor } from "@tiptap/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { editorValue, sourceStart, switchSource } from "./source";
import "./source.css";
import { PLAIN_TEXT } from "../../components";

export interface SourceMode {
  on: boolean;
  /** What to give `useMarkdownEditor` as its value. */
  editorValue: string;
  /** The user switched in this editor: the text that appears gets the caret. */
  switched: boolean;
  toggle: () => void;
}

/** The mode of one editor, opening `on` or not, while the parent holds `value`. */
export function useSourceMode(value: string, initial: boolean): SourceMode {
  const [state, setState] = useState(() => sourceStart(initial, value));
  return {
    on: state.on,
    editorValue: editorValue(state, value),
    switched: state.switched,
    toggle: () => {
      setState((s) => switchSource(s, value));
    },
  };
}

/** Puts the caret back in the rich text after the user leaves the source. */
export function useFocusOnLeave(editor: Editor, mode: SourceMode): void {
  const { on, switched } = mode;
  useEffect(() => {
    if (switched && !on) editor.commands.focus();
  }, [editor, on, switched]);
}

/**
 * The "Markdown" switch at the toolbar's right end (a 24 × 14 track and an 11 px label). Out of the tab order like the
 * toolbar's buttons, and pressing it doesn't take the focus: the text that appears gets it.
 */
export function SourceToggle({
  on,
  label,
  onChange,
}: {
  on: boolean;
  label: string;
  onChange: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      tabIndex={-1}
      className="toggle md-source-toggle"
      aria-checked={on}
      onMouseDown={(e) => {
        e.preventDefault();
      }}
      onClick={() => {
        onChange(!on);
      }}
    >
      <span className="toggle-track">
        <span className="toggle-knob" />
      </span>
      <span>{label}</span>
    </button>
  );
}

/**
 * The raw source: a plain text area in the mono font, as tall as its text. It sits in a `.md-content` box, so the
 * field's max height and scrolling are the rich text's (editor.css).
 */
export function SourceArea({
  value,
  label,
  takeFocus,
  onChange,
}: {
  value: string;
  label: string;
  /** Takes the caret when it appears (the user switched to it). */
  takeFocus: boolean;
  onChange: (value: string) => void;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    // Measured at its smallest, then grown to fit; the box keeps its scroll position through it.
    const box = el.parentElement;
    const top = box?.scrollTop ?? 0;
    el.style.height = "auto";
    el.style.height = `${String(el.scrollHeight)}px`;
    if (box) box.scrollTop = top;
  }, [value]);
  useEffect(() => {
    if (takeFocus) area.current?.focus({ preventScroll: true });
  }, [takeFocus]);
  return (
    <div className="md-content md-source-scroll">
      <textarea
        {...PLAIN_TEXT}
        ref={area}
        className="md-source"
        value={value}
        aria-label={label}
        onChange={(e) => {
          onChange(e.target.value);
        }}
      />
    </div>
  );
}
