// "Set by Dev on SHOP-27 · Undo" (d{RUN}): under the run command in Settings and on the Run tab, while the command an
// agent set is the project's. The words come from the Rust view (`run_page::SetByView`); Undo empties the command.
import type { RunCommandSetByView } from "../generated/RunCommandSetByView";
import { Button } from "./Button";
import { Tooltip } from "./Tooltip";

export function SetByLine({ setBy, onUndo }: { setBy: RunCommandSetByView; onUndo: () => void }) {
  return (
    <p className="set-by-line">
      <span>{setBy.text}</span>
      <Tooltip text={setBy.undo_hint}>
        <Button variant="ghost" compact label={setBy.undo} onClick={onUndo} />
      </Tooltip>
    </p>
  );
}
