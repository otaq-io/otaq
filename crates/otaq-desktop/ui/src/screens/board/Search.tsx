// The header's search and filter field (*03 · Board & columns → Search & filter*; UI-02, EPC-01, KEY-09), drawn
// by the header in place of its search entry (the `header_search` slot). Filter the board (`/`) focuses it; typing filters the board as
// you type; a finished token (`is:needs-you `) becomes a chip with its ×; the list under the field suggests filters
// with their counts (↑/↓ and ↵, or a click, add one); ↵ on a half-typed token adds the first suggestion, and on
// "cleanup" opens Recovery and Cleanup; Esc clears the filter and leaves. What it says is `board.search`'s.
import { type KeyboardEvent, type RefObject, useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { Kbd, useFloatingStyle, PLAIN_TEXT } from "../../components";
import type { BoardFilter } from "../../generated/BoardFilter";
import type { SearchParams } from "../../generated/SearchParams";
import type { SearchSuggestion } from "../../generated/SearchSuggestion";
import type { SearchView } from "../../generated/SearchView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { BOARD_EVENTS } from "./Board";
import {
  addToken,
  boardFilter,
  clearFilter,
  filterOf,
  finishedTokens,
  removeToken,
  searchFocusRequests,
  setFilter,
} from "./filter";

/** The suggestion row ↑/↓ lands on next (`null`: none highlighted). */
export function stepSuggestion(
  count: number,
  current: number | null,
  key: "ArrowDown" | "ArrowUp",
): number | null {
  if (count === 0) return null;
  if (current === null) return key === "ArrowDown" ? 0 : count - 1;
  return (current + (key === "ArrowDown" ? 1 : count - 1)) % count;
}

interface SuggestionsProps {
  anchor: RefObject<HTMLElement | null>;
  header: string;
  suggestions: readonly SearchSuggestion[];
  highlighted: number | null;
  onPick: (token: string) => void;
}

function Suggestions({ anchor, header, suggestions, highlighted, onPick }: SuggestionsProps) {
  const floating = useRef<HTMLDivElement>(null);
  const style = useFloatingStyle(anchor, floating);
  return (
    <div ref={floating} className="board-search-list" role="listbox" style={style}>
      <div className="board-search-list-header">{header}</div>
      {suggestions.map((s, i) => (
        <button
          key={s.token}
          type="button"
          role="option"
          tabIndex={-1}
          aria-selected={i === highlighted}
          className="board-search-option"
          data-highlighted={i === highlighted || undefined}
          // Keeps the keyboard in the field, so the press isn't lost to its blur.
          onMouseDown={(e) => {
            e.preventDefault();
          }}
          onClick={() => {
            onPick(s.token);
          }}
        >
          <span className="board-search-option-token">{s.token}</span>
          <span className="board-search-option-text">{s.description}</span>
          <span className="board-search-option-count">{s.count}</span>
        </button>
      ))}
    </div>
  );
}

export interface SearchBoxProps {
  filter: BoardFilter;
  view: SearchView | undefined;
  focused: boolean;
  highlighted: number | null;
  onType: (text: string) => void;
  onRemove: (index: number) => void;
  onPick: (token: string) => void;
  onKeyDown: (e: KeyboardEvent<HTMLInputElement>) => void;
  onFocusChange: (focused: boolean) => void;
  input?: RefObject<HTMLInputElement | null>;
}

/** The field itself, from the filter and Rust's view of it. */
export function SearchBox({
  filter,
  view,
  focused,
  highlighted,
  onType,
  onRemove,
  onPick,
  onKeyDown,
  onFocusChange,
  input,
}: SearchBoxProps) {
  const box = useRef<HTMLDivElement>(null);
  const tail = view?.results ?? null;
  return (
    // A press anywhere in the field (beside its chips' buttons) puts the caret in its input; the keys are the input's.
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- only forwards presses to the input inside
    <div
      ref={box}
      className="board-search"
      data-focused={focused || undefined}
      // The header is the window's drag region; the field is not.
      data-tauri-drag-region="false"
      onMouseDown={(e) => {
        if (e.target === input?.current || (e.target as HTMLElement).closest("button")) return;
        e.preventDefault();
        input?.current?.focus();
      }}
    >
      <Icon name="search" size={13} />
      {filter.tokens.map((token, i) => (
        <span key={token} className="board-search-chip">
          <span>{token}</span>
          <button
            type="button"
            aria-label={`Remove ${token}`}
            onClick={() => {
              onRemove(i);
            }}
          >
            <Icon name="x" size={10} />
          </button>
        </span>
      ))}
      <input
        {...PLAIN_TEXT}
        ref={input}
        className="board-search-input"
        type="text"
        value={filter.text}
        placeholder={filter.tokens.length === 0 ? (view?.placeholder ?? "") : ""}
        aria-label={view?.placeholder ?? "Search"}
        autoComplete="off"
        onChange={(e) => {
          onType(e.target.value);
        }}
        onKeyDown={onKeyDown}
        onFocus={() => {
          onFocusChange(true);
        }}
        onBlur={() => {
          onFocusChange(false);
        }}
      />
      {tail !== null ? (
        <span className="board-search-results">{tail}</span>
      ) : (
        view?.keys != null && <Kbd keys={view.keys} />
      )}
      {focused && view && view.suggestions.length > 0 && (
        <Suggestions
          anchor={box}
          header={view.suggestions_header}
          suggestions={view.suggestions}
          highlighted={highlighted}
          onPick={onPick}
        />
      )}
    </div>
  );
}

/** The `header_search` slot: the shown project's filter. */
export function SearchField() {
  const { project, ticket } = useSelection();
  const filter = filterOf(useStore(boardFilter), project);
  const focusRequest = useStore(searchFocusRequests);
  const input = useRef<HTMLInputElement>(null);
  const [focused, setFocused] = useState(false);
  // The highlighted suggestion, for the text it was made with: typing on drops it.
  const [highlight, setHighlight] = useState<{ text: string; index: number } | null>(null);

  useEffect(() => {
    if (focusRequest > 0) input.current?.focus();
  }, [focusRequest]);

  const load = useCallback(() => {
    if (project === null) return new Promise<never>(() => undefined);
    const sent: BoardFilter = { tokens: filter.tokens, text: filter.text };
    const params: SearchParams = { project, filter: sent, selected: ticket };
    return (request("board.search", params) as Promise<SearchView>).then((view) => ({ project, sent, view }));
  }, [project, filter, ticket]);
  const loaded = useServiceView(load, BOARD_EVENTS).view;
  const answer = loaded?.project === project ? loaded : undefined;
  const view = answer?.view;

  // A token Rust finished moves into the chips (unless the user typed on meanwhile).
  useEffect(() => {
    if (!answer || project === null) return;
    const next = finishedTokens(answer.sent, answer.view.filter, filterOf(boardFilter.get(), project));
    if (next) setFilter(project, next);
  }, [answer, project]);

  const count = view?.suggestions.length ?? 0;
  const highlighted =
    highlight !== null && highlight.text === filter.text && highlight.index < count ? highlight.index : null;

  if (project === null) return null;
  const pick = (token: string) => {
    addToken(project, token);
    setHighlight(null);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    const suggestions = view?.suggestions ?? [];
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        clearFilter(project);
        e.currentTarget.blur();
        return;
      case "ArrowDown":
      case "ArrowUp":
        e.preventDefault();
        setHighlight({
          text: filter.text,
          index: stepSuggestion(suggestions.length, highlighted, e.key) ?? 0,
        });
        return;
      case "Enter": {
        if (e.nativeEvent.isComposing) return;
        e.preventDefault();
        if (view?.enter_opens_cleanup) {
          clearFilter(project);
          e.currentTarget.blur();
          openOverlay("cleanup");
          return;
        }
        const chosen = highlighted !== null ? suggestions[highlighted]?.token : view?.enter_adds;
        if (chosen) pick(chosen);
        return;
      }
      default:
        return;
    }
  };
  return (
    <SearchBox
      filter={filter}
      view={view}
      focused={focused}
      highlighted={highlighted}
      input={input}
      onType={(text) => {
        setFilter(project, { tokens: filter.tokens, text });
      }}
      onRemove={(index) => {
        removeToken(project, index);
      }}
      onPick={pick}
      onKeyDown={onKeyDown}
      onFocusChange={setFocused}
    />
  );
}
