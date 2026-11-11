// A hook's refusal on the sheet: "Output of your post-checkout hook", a sentence above the output when the hook needs
// one, and the hook's whole output (paths under the home already shortened). The same box for every one of Otaq's git
// steps a user's hook refuses; the words come from Rust (`SheetHookOutput`).
import type { SheetHookOutput } from "../../generated/SheetHookOutput";

export function HookOutput({ hook }: { hook: SheetHookOutput }) {
  return (
    <div className="sheet-hook-output" data-hook={hook.name}>
      <span className="sheet-hook-output-label">{hook.label}</span>
      {hook.note !== null && <p className="sheet-hook-output-note">{hook.note}</p>}
      <pre className="sheet-setup-log">{hook.output}</pre>
    </div>
  );
}
