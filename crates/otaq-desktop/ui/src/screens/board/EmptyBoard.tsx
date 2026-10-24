// The empty board of a project with no tickets (*14 → 4 · Done · empty board → Board empty* `ugHTD`, *03 → Empty
// column & empty board* `kItEl`; UI-08): a centred card over the empty columns. Its button runs its shortcut's
// action, so it does what the key does (New ticket ⌘N; the Project agent is the pane on the left, ⌘L, d0087); so do
// the hints under it (d0126: "⌘L ask the Project agent" opens the panel with its box focused). Its words are
// `EmptyBoardView`'s (`empty_board.rs`).
import { Button, Kbd } from "../../components";
import { useRegistries } from "../../app/registry";
import type { EmptyBoardView } from "../../generated/EmptyBoardView";

export function EmptyBoard({ view }: { view: EmptyBoardView }) {
  const app = useRegistries();
  return (
    <div className="board-empty-layer">
      <div className="board-empty" role="region" aria-label={view.title}>
        <h2 className="board-empty-title">{view.title}</h2>
        <p className="board-empty-text">{view.text}</p>
        <div className="board-empty-buttons">
          {view.buttons.map((b) => (
            <Button
              key={b.label}
              label={b.label}
              icon={b.icon}
              variant={b.primary ? "primary" : "secondary"}
              onClick={() => {
                app.shortcuts.run(b.action);
              }}
            />
          ))}
        </div>
        <div className="board-empty-hints">
          {view.hints.map((h) => (
            <button
              key={h.label}
              type="button"
              className="board-empty-hint"
              onClick={() => {
                app.shortcuts.run(h.action);
              }}
            >
              {h.keys !== null && <Kbd keys={h.keys} />}
              <span>{h.label}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
