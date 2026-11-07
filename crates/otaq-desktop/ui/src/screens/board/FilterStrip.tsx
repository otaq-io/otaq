// The board's filter strip, under the toolbar while the search filters the board: "Showing 3 of 17", the active
// filters as chips (each with its ✕), one filter to add ("+ is:needs-you") while it isn't on, and Clear. Its words are
// Rust's (`BoardView.filter_strip`); it changes the one filter the board keeps (`filter.ts`), which the search field
// reads too, so the two stay in step.
import type { FilterStrip as FilterStripView } from "../../generated/FilterStrip";
import { Icon } from "../../theme/Icon";
import { boardFilter, clearFilter, filterOf, removeToken, setFilter } from "./filter";

export function FilterStrip({ strip, project }: { strip: FilterStripView; project: string }) {
  const remove = (token: string | null) => {
    const now = filterOf(boardFilter.get(), project);
    if (token === null) {
      setFilter(project, { tokens: now.tokens, text: "" });
      return;
    }
    const at = now.tokens.indexOf(token);
    if (at >= 0) removeToken(project, at);
  };
  return (
    <div className="board-filter-strip" role="region" aria-label="Filter">
      <Icon name="search" size={13} />
      <span className="board-filter-showing">{strip.showing}</span>
      {strip.chips.map((chip) => (
        <span key={chip.label} className="board-filter-chip">
          <span>{chip.label}</span>
          <button
            type="button"
            className="board-filter-chip-remove"
            aria-label={`Remove ${chip.label}`}
            onClick={() => {
              remove(chip.token);
            }}
          >
            <Icon name="x" size={11} />
          </button>
        </span>
      ))}
      {strip.suggest !== null && strip.suggest_label !== null && (
        <button
          type="button"
          className="board-filter-chip"
          data-suggest
          onClick={() => {
            const token = strip.suggest;
            if (token === null) return;
            // Added beside what is typed: the words stay a filter too.
            const now = filterOf(boardFilter.get(), project);
            setFilter(project, { tokens: [...now.tokens, token], text: now.text });
          }}
        >
          {strip.suggest_label}
        </button>
      )}
      <span className="board-spacer" />
      <button
        type="button"
        className="board-filter-clear"
        onClick={() => {
          clearFilter(project);
        }}
      >
        {strip.clear}
      </button>
    </div>
  );
}
