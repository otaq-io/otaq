// Tabs with indicators, the segmented control and the toggle (*01 · Components → Badges & Inputs → Tabs &
// segmented*). Active tab: a 2 px accent underline and 600 weight; indicators are a dot or a short mono note.
import type { KeyboardEvent, ReactNode } from "react";

import type { IconName } from "../generated/IconName";
import { Icon } from "../theme/Icon";
import { Tooltip } from "./Tooltip";

/** What a tab shows after its label: a live dot (working or attention colour), a red problem dot, or a note. */
export type TabIndicator =
  { kind: "live" } | { kind: "live_attention" } | { kind: "problem" } | { kind: "note"; text: string };

export interface TabItem {
  icon: IconName;
  label: string;
  indicator?: TabIndicator;
  /** The tab's shortcut (the user's key, `1`…), shown in the tooltip of icon-only tabs. */
  keys?: string;
}

/** The index ←/→ move to among `count` items, wrapping; `null` for any other key. */
export function arrowIndex(key: string, current: number, count: number): number | null {
  if (count === 0) return null;
  if (key === "ArrowRight") return (current + 1) % count;
  if (key === "ArrowLeft") return (current + count - 1) % count;
  return null;
}

/** Focuses the `index`th `item` of the `group` around `from` (arrow keys move focus with the choice). */
function focusItem(from: Element, group: string, item: string, index: number) {
  const items = from.closest(`[role=${group}]`)?.querySelectorAll<HTMLElement>(`[role=${item}]`);
  items?.[index]?.focus();
}

function indicator(ind: TabIndicator): ReactNode {
  return ind.kind === "note" ? (
    <span className="tab-note">{ind.text}</span>
  ) : (
    <span className="tab-dot" data-kind={ind.kind} />
  );
}

export interface TabsProps {
  label: string;
  items: readonly TabItem[];
  active: number;
  onSelect: (index: number) => void;
  /** Narrow panels: icons only, the name (and shortcut) in a tooltip. */
  iconsOnly?: boolean;
}

export function Tabs({ label, items, active, onSelect, iconsOnly = false }: TabsProps) {
  const onKeyDown = (e: KeyboardEvent) => {
    const next = arrowIndex(e.key, active, items.length);
    if (next === null) return;
    e.preventDefault();
    onSelect(next);
    focusItem(e.currentTarget, "tablist", "tab", next);
  };
  return (
    <div className="tabs" role="tablist" aria-label={label}>
      {items.map((item, i) => {
        const tab = (
          <button
            key={item.label}
            type="button"
            role="tab"
            className="tab"
            aria-selected={i === active}
            aria-label={iconsOnly ? item.label : undefined}
            tabIndex={i === active ? 0 : -1}
            onKeyDown={onKeyDown}
            onClick={() => {
              onSelect(i);
            }}
          >
            <Icon name={item.icon} size={13} />
            {!iconsOnly && <span>{item.label}</span>}
            {item.indicator && indicator(item.indicator)}
          </button>
        );
        return iconsOnly ? (
          <Tooltip key={item.label} text={item.label} keys={item.keys}>
            {tab}
          </Tooltip>
        ) : (
          tab
        );
      })}
    </div>
  );
}

export interface SegmentedProps {
  label: string;
  options: readonly string[];
  selected: number;
  onSelect: (index: number) => void;
  /** Shown with its choice but switched off: nothing can be picked. */
  disabled?: boolean;
}

/** Two or more options, one chosen (Unified / Side-by-side). */
export function Segmented({ label, options, selected, onSelect, disabled = false }: SegmentedProps) {
  const onKeyDown = (e: KeyboardEvent) => {
    const next = arrowIndex(e.key, selected, options.length);
    if (next === null) return;
    e.preventDefault();
    onSelect(next);
    focusItem(e.currentTarget, "radiogroup", "radio", next);
  };
  return (
    <div className="segmented" role="radiogroup" aria-label={label} aria-disabled={disabled || undefined}>
      {options.map((option, i) => (
        <button
          key={option}
          type="button"
          role="radio"
          className="segment"
          aria-checked={i === selected}
          tabIndex={i === selected ? 0 : -1}
          disabled={disabled}
          onKeyDown={onKeyDown}
          onClick={() => {
            onSelect(i);
          }}
        >
          {option}
        </button>
      ))}
    </div>
  );
}

export interface ToggleProps {
  on: boolean;
  label: string;
  /** Shows the label next to the switch (otherwise it only names it for screen readers). */
  showLabel?: boolean;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}

/** An on/off switch. */
export function Toggle({ on, label, showLabel = true, disabled = false, onChange }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      className="toggle"
      aria-checked={on}
      aria-label={showLabel ? undefined : label}
      disabled={disabled}
      onClick={() => {
        onChange(!on);
      }}
    >
      <span className="toggle-track">
        <span className="toggle-knob" />
      </span>
      {showLabel && <span>{label}</span>}
    </button>
  );
}
