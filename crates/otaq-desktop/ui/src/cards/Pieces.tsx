// A build stage's pieces on the card (d0133; canvas `s30-02` "One card in Dev, a row per piece", `s30-03` "The card in
// Dev", `s30-04` "A fix is a new piece"): "Back from Validate" after a check's Send back, "PIECES · 4", then one row per
// piece with its display name, its title and one status ("working", "waits for 40.2, 40.3", "merged"); past four rows
// the merged ones fold into one ("3 merged"). Once every piece is merged, the one line "4 pieces merged · moving to
// Validate". Every word is Rust's (`pieces.rs`).
import type { CardPiecesView } from "../generated/CardPiecesView";
import { toneInk } from "./look";

export function PieceRows({ pieces }: { pieces: CardPiecesView }) {
  return (
    <div className="card-pieces">
      {pieces.back_from !== null && <div className="card-pieces-back">{pieces.back_from}</div>}
      {pieces.label !== null && <div className="card-pieces-label">{pieces.label}</div>}
      {pieces.rows.map((row) => (
        <div key={row.name || row.status} className="card-piece">
          {row.name !== "" && <span className="card-piece-name">{row.name}</span>}
          {row.title !== "" && <span className="card-piece-title">{row.title}</span>}
          <span className="card-piece-status" style={{ color: toneInk(row.tone).fg }}>
            {row.status}
          </span>
        </div>
      ))}
      {pieces.done !== null && <div className="card-pieces-done">{pieces.done}</div>}
    </div>
  );
}
