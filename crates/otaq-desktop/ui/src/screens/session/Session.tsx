// The sheet's Session (SHT-04; d0022, d0035, d0048, d0063; canvas `s25-06`): the switch Agent · <check's agent> · Run
// · Shell, its dots turquoise while a side waits for you and green while it runs, leading the line of the side shown:
// the agent's or the check's terminal, the Run, or the Shell. The switch and what it shows are Rust's (`PanelView.sheet`,
// `session_tab.rs`); the side shown is the selection's (`agent`, `check`, `run`, `shell`).
import type { TicketProps } from "../../app/registry";
import { showTab } from "../../app/selection";
import type { SessionPartView } from "../../generated/SessionPartView";
import type { SheetContent } from "../../generated/SheetContent";
import { BuildAside } from "../overview/NeedsYou";
import { contentOf } from "../sheet/logic";
import { useSheet } from "../sheet/Sheet";
import { RunTab } from "./RunTab";
import { ShellTab } from "./ShellTab";
import { TerminalTab } from "./TerminalTab";

/** The switch: one button per part, the shown one checked. */
export function SessionSwitch({
  parts,
  shown,
  label,
  onPick,
}: {
  parts: readonly SessionPartView[];
  shown: SheetContent;
  label: string;
  onPick: (content: SheetContent) => void;
}) {
  return (
    <div className="segmented session-switch" role="radiogroup" aria-label={label}>
      {parts.map((part) => (
        <button
          key={part.content}
          type="button"
          role="radio"
          className="segment"
          aria-checked={part.content === shown}
          onClick={() => {
            onPick(part.content);
          }}
        >
          {part.label}
          {part.dot !== null && <span className="session-dot" data-dot={part.dot} />}
        </button>
      ))}
    </div>
  );
}

/** The part's body, led by the switch; Session shows only its own parts (`contentOf`), the agent's by default. */
function Part({ ticket, shown, head }: { ticket: string; shown: SheetContent; head: React.ReactNode }) {
  switch (shown) {
    case "check":
      return <TerminalTab key="check" ticket={ticket} side="check" head={head} />;
    case "run":
      return <RunTab ticket={ticket} head={head} />;
    case "shell":
      return <ShellTab ticket={ticket} head={head} />;
    case "agent":
    case "overview":
    case "files":
    case "activity":
    case "lead":
    case "tickets":
      return <TerminalTab key="agent" ticket={ticket} side="agent" head={head} />;
  }
}

/** The tab registered in the sheet. */
export function SessionTab({ ticket }: TicketProps) {
  const sheet = useSheet();
  const session = sheet?.view.sheet.tabs.find((t) => t.tab === "session");
  if (!sheet || !session) return null;
  const shown = contentOf(session, sheet.selected);
  const head = <SessionSwitch parts={session.parts} shown={shown} label={session.label} onPick={showTab} />;
  // A build set aside at Review (d0125): the Run and the Shell say so at their top, and while it comes back.
  const aside = (shown === "run" || shown === "shell") && sheet.view.build_aside;
  return (
    <div className="session" data-content={shown}>
      {aside && (
        <div className="session-build-aside">
          <BuildAside line={aside} />
        </div>
      )}
      <Part ticket={ticket} shown={shown} head={head} />
    </div>
  );
}
