// How a code block is drawn (T-0254): *08 → g · Code block · language picker* in edit mode, *05 → c · Read mode* in
// read mode, and *08 → h* for a long line. Edit mode shows the language label, which opens the picker; read mode shows
// the language and, on hover, "Copy". Both sets are always in the page and code.css shows one by the text's editable
// attribute, as markdown.css tells the modes apart, so the look never lags the mode.

import { NodeViewContent, NodeViewWrapper, type ReactNodeViewProps } from "@tiptap/react";
import { type KeyboardEvent, type RefObject, useLayoutEffect, useRef, useState } from "react";

import { useEscape, useFloatingStyle, PLAIN_TEXT } from "../../components";
import { Icon } from "../../theme/Icon";
import {
  type CodeLanguage,
  filterLanguages,
  isCurrentLanguage,
  languageLabel,
  setCodeLanguage,
} from "./languages";
import "./code.css";

const ignore = () => undefined;

/** Sets `data-overflow` on the scrolling code while more of it is hidden at the right, for the right-edge fade. */
function useRightOverflow(scroller: RefObject<HTMLElement | null>, text: string) {
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const update = () => {
      el.toggleAttribute("data-overflow", el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
    };
    update();
    el.addEventListener("scroll", update, { passive: true });
    const resize = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    resize?.observe(el);
    return () => {
      el.removeEventListener("scroll", update);
      resize?.disconnect();
    };
  }, [scroller, text]);
}

export function CodeBlockView({ node, editor, getPos }: ReactNodeViewProps) {
  const language = node.attrs.language as string | null;
  const scroller = useRef<HTMLPreElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const [picking, setPicking] = useState(false);
  useRightOverflow(scroller, node.textContent);

  const choose = (row: CodeLanguage) => {
    setPicking(false);
    const pos = getPos();
    if (pos !== undefined) setCodeLanguage(editor, pos, row.id);
    editor.view.focus();
  };

  return (
    <NodeViewWrapper className="md-code">
      {/* Code stays as typed (#41): no spell check inside the block. */}
      <pre ref={scroller} spellCheck={false}>
        <NodeViewContent<"code"> as="code" style={{ whiteSpace: "pre" }} />
      </pre>
      <div className="md-code-tools" contentEditable={false}>
        <button
          ref={trigger}
          type="button"
          className="md-code-language"
          aria-haspopup="dialog"
          aria-expanded={picking}
          aria-label={`Language: ${languageLabel(language)}`}
          onClick={() => {
            setPicking((open) => !open);
          }}
        >
          <span>{languageLabel(language)}</span>
          <Icon name="chevron-down" size={10} />
        </button>
        {language !== null && language !== "" && <span className="md-code-label">{language}</span>}
        <button
          type="button"
          className="md-code-copy"
          onClick={() => {
            navigator.clipboard.writeText(node.textContent).catch(ignore);
          }}
        >
          <Icon name="copy" size={11} />
          Copy
        </button>
        {picking && (
          <LanguagePicker
            anchor={trigger}
            language={language}
            onChoose={choose}
            onClose={() => {
              setPicking(false);
              // After Esc the caret goes back to the code; after a press elsewhere, focus stays where it went.
              setTimeout(() => {
                if (document.activeElement === document.body) editor.view.focus();
              });
            }}
          />
        )}
      </div>
    </NodeViewWrapper>
  );
}

interface LanguagePickerProps {
  anchor: RefObject<HTMLElement | null>;
  language: string | null;
  onChoose: (row: CodeLanguage) => void;
  onClose: () => void;
}

/** The short list under the label: a filter on top, ↑ ↓ to move, ↵ to choose, Esc or a press outside to close. */
function LanguagePicker({ anchor, language, onChoose, onClose }: LanguagePickerProps) {
  const floating = useRef<HTMLDivElement>(null);
  const style = useFloatingStyle(anchor, floating, "end", onClose);
  useEscape(onClose);
  const [query, setQuery] = useState("");
  const rows = filterLanguages(query);
  const [highlighted, setHighlighted] = useState(() =>
    Math.max(
      0,
      rows.findIndex((row) => isCurrentLanguage(row, language)),
    ),
  );
  const shown = Math.min(highlighted, rows.length - 1);

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (rows.length > 0) {
        const step = e.key === "ArrowDown" ? 1 : -1;
        setHighlighted((shown + step + rows.length) % rows.length);
      }
    } else if (e.key === "Enter") {
      const row = rows[shown];
      if (row !== undefined) onChoose(row);
    } else {
      return;
    }
    e.preventDefault();
  };

  return (
    <div
      ref={floating}
      className="popover md-code-picker"
      role="dialog"
      aria-label="Code language"
      style={style}
    >
      <label className="md-code-filter">
        <Icon name="search" size={12} />
        <input
          {...PLAIN_TEXT}
          // eslint-disable-next-line jsx-a11y/no-autofocus -- the picker opens to be typed into
          autoFocus
          value={query}
          placeholder="Filter languages…"
          aria-label="Filter languages"
          onChange={(e) => {
            setQuery(e.target.value);
            setHighlighted(0);
          }}
          onKeyDown={onKeyDown}
        />
      </label>
      <div role="listbox" aria-label="Languages">
        {rows.map((row, i) => {
          const current = isCurrentLanguage(row, language);
          return (
            <button
              key={row.label}
              type="button"
              role="option"
              aria-selected={current}
              tabIndex={-1}
              className="md-code-option"
              data-plain={row.id === null || undefined}
              data-highlighted={i === shown || undefined}
              onMouseDown={(e) => {
                e.preventDefault();
              }}
              onMouseMove={() => {
                if (i !== shown) setHighlighted(i);
              }}
              onClick={() => {
                onChoose(row);
              }}
            >
              <span className="md-code-option-label">{row.label}</span>
              {row.id === null && <span className="md-code-option-note">none</span>}
              {current && <Icon name="check" size={13} label="current" />}
            </button>
          );
        })}
      </div>
    </div>
  );
}
