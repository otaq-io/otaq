// Menus (*01 · Components → Overlays & feedback → Context menu*) and dropdown lists: the rows come from Rust
// (`MenuItem`, e.g. `priority.rs`'s `menu_items`), with an icon, a label, a shortcut, a danger tone, a disabled
// state with a hint, separators and group headers (one with a hint says what the list is about: a stage's picker).
// The menu takes focus when it opens: ↑/↓ move (skipping headers and disabled rows, wrapping), Home/End jump, ↵ or Space chooses, Esc, Tab or a click outside closes. A list with
// `search` (the base-branch dropdowns) puts a search field on top that takes focus and filters the rows as you type;
// ↑/↓ and ↵ still work from the field. With `typed` too, what was typed is a last row of its own (Pick a skill…'s
// "Use `<typed text>`"), its `…` parts drawn as code.
import { type CSSProperties, type KeyboardEvent, useEffect, useId, useMemo, useRef, useState } from "react";

import type { MenuItem } from "../generated/MenuItem";
import type { MenuSearch } from "../generated/MenuSearch";
import { Icon } from "../theme/Icon";
import { CodeText } from "./CodeText";
import { type Anchor, useFloatingStyle } from "./Floating";
import { chooseMenu, filterMenu, firstMatch, menuKey, searchKey, selectable, withTyped } from "./logic";
import { PLAIN_TEXT } from "./plainText";

export type MenuClose = "key" | "outside";

export interface MenuProps {
  /** What the menu is, for screen readers ("Card actions"). */
  label: string;
  items: readonly MenuItem[];
  onChoose: (id: string) => void;
  /**
   * The menu should close: after a choice or Esc / Tab (`"key"`: focus may go back to the trigger), or because focus
   * left it (`"outside"`: leave focus where it went).
   */
  onClose: (how: MenuClose) => void;
  /** A dropdown's list: rows are options, the checked one is the current value. */
  list?: boolean;
  width?: number;
  style?: CSSProperties;
  /** A search field on top that filters the rows. */
  search?: MenuSearch;
  /** With `search`: the row for what was typed, last while searching (none for an empty query). */
  typed?: (query: string) => MenuItem | null;
  /** A head above the rows, not chosen: the project menu's name and folder ("shop-web", "~/code/shop-web"). */
  head?: { title: string; detail: string };
}

/** The menu panel itself, where its owner puts it. Most owners want `AnchoredMenu`. */
export function Menu({
  label,
  items: all,
  onChoose,
  onClose,
  list = false,
  width = 260,
  style,
  search,
  typed,
  head,
}: MenuProps) {
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => (search ? filterMenu(all, query) : all), [all, query, search]);
  const items = useMemo(
    () => (search ? withTyped(filtered, typed?.(query) ?? null) : filtered),
    [filtered, query, search, typed],
  );
  /** The row for what was typed, when there is one: the last. */
  const typedRow = items.length > filtered.length ? items.length - 1 : null;
  // A search list opens on the current row, and moves to the first match as the query changes.
  const [highlighted, setHighlighted] = useState<number | null>(() => (search ? firstMatch(all) : null));
  useEffect(() => {
    (field.current ?? root.current)?.focus();
  }, []);
  const row = (n: number) => `${id}-${String(n)}`;
  /** Highlights row `index`, scrolled into sight in a long list. */
  const highlight = (index: number | null) => {
    setHighlighted(index);
    const el = index === null ? null : document.getElementById(row(index));
    if (el && "scrollIntoView" in el) el.scrollIntoView({ block: "nearest" });
  };
  const choose = (index: number | null) => {
    const chosen = chooseMenu(items, index);
    if (chosen === null) return;
    onChoose(chosen);
    onClose("key");
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const result = (search ? searchKey : menuKey)(items, highlighted, e.key);
    switch (result.kind) {
      case "highlight":
        highlight(result.index);
        break;
      case "choose":
        onChoose(result.id);
        onClose("key");
        break;
      case "close":
        onClose("key");
        break;
      case "none":
        // ↵ in the search field with no match stays in the list (a form around it must not submit).
        if (search && e.key === "Enter") break;
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div
      ref={root}
      className="menu"
      data-list={list || undefined}
      role="menu"
      aria-label={label}
      tabIndex={-1}
      aria-activedescendant={highlighted === null ? undefined : row(highlighted)}
      style={{ width, ...style }}
      onKeyDown={onKeyDown}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) onClose("outside");
      }}
    >
      {head && (
        <div role="presentation">
          <div className="menu-head">
            <span className="menu-head-title">{head.title}</span>
            <span className="menu-head-detail">{head.detail}</span>
          </div>
          <div role="separator" className="menu-separator" />
        </div>
      )}
      {search && (
        <div className="menu-search">
          <Icon name="search" size={13} />
          <input
            {...PLAIN_TEXT}
            ref={field}
            className="menu-search-input"
            type="text"
            value={query}
            placeholder={search.placeholder}
            aria-label={search.placeholder}
            onChange={(e) => {
              setQuery(e.target.value);
              const q = e.target.value;
              setHighlighted(firstMatch(withTyped(filterMenu(all, q), typed?.(q) ?? null)));
            }}
          />
        </div>
      )}
      {search && items.length === 0 && <div className="menu-empty">{search.empty}</div>}
      {items.map((item, i) => {
        const separator = item.separated && i > 0 && <div role="separator" className="menu-separator" />;
        if (item.header) {
          return (
            <div key={`h-${item.label}`} role="presentation">
              {separator}
              {item.hint === null ? (
                <div className="menu-header">{item.label}</div>
              ) : item.label === "" ? (
                // A line of explanation between the rows (the Approve menu's "Your flow delivers either way…").
                <div className="menu-note">{item.hint}</div>
              ) : (
                // A header with a hint says what the list is about: a stage's name and what it is for.
                <div className="menu-header-described">
                  <span className="menu-header-title">{item.label}</span>
                  <span className="menu-header-hint">{item.hint}</span>
                </div>
              )}
            </div>
          );
        }
        return (
          <div key={item.id} role="presentation">
            {separator}
            <button
              id={row(i)}
              type="button"
              tabIndex={-1}
              role={list ? "menuitemradio" : "menuitem"}
              aria-checked={list ? item.checked : undefined}
              aria-disabled={item.disabled || undefined}
              className="menu-item"
              data-danger={item.danger || undefined}
              data-highlighted={i === highlighted || undefined}
              // Keeps focus on the menu, so the press isn't lost to its blur.
              onMouseDown={(e) => {
                e.preventDefault();
              }}
              onMouseMove={() => {
                if (selectable(item) && i !== highlighted) setHighlighted(i);
              }}
              onClick={() => {
                choose(i);
              }}
            >
              <span className="menu-item-line">
                {item.icon !== null && <Icon name={item.icon} />}
                <span className="menu-item-label">
                  {i === typedRow ? <CodeText text={item.label} /> : item.label}
                </span>
                {item.checked && <Icon name="check" size={13} label="current" />}
                {item.shortcut !== null && <span className="menu-item-keys">{item.shortcut}</span>}
              </span>
              {item.hint !== null && (
                <span className="menu-item-hint" data-indented={item.icon !== null || undefined}>
                  {item.hint}
                </span>
              )}
            </button>
          </div>
        );
      })}
    </div>
  );
}

export interface AnchoredMenuProps extends MenuProps {
  /** The element it opens from (under it), or the pointer's point for a context menu. */
  anchor: Anchor;
  align?: "start" | "end";
}

/**
 * A menu under its trigger, or at the pointer, kept inside the window. A press outside it (and outside its trigger,
 * whose own click decides) or a scroll behind it closes it (`useFloatingStyle`).
 */
export function AnchoredMenu({ anchor, align, ...menu }: AnchoredMenuProps) {
  const floating = useRef<HTMLDivElement>(null);
  const close = useRef(menu.onClose);
  useEffect(() => {
    close.current = menu.onClose;
  });
  const style = useFloatingStyle(anchor, floating, align, () => {
    close.current("outside");
  });
  return (
    <div ref={floating} className="floating" style={style}>
      <Menu {...menu} />
    </div>
  );
}
