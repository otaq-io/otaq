// The delivery message's writer waits for permission (its tool asks before a command): Approve & merge and Approve &
// open PR say so in their line, and Answer in terminal beside it opens the writer's own terminal over the dialog. The
// plain message stays merge-ready meanwhile; the ticket in Review stays calm. The words are Rust's (`WriterAsks`).
import { useEffect, useRef, useState } from "react";

import { Button, Dialog } from "../../components";
import type { WriterAsks } from "../../generated/WriterAsks";
import { Terminal } from "../../terminal/Terminal";

/** Answer in terminal, and the writer's terminal once pressed: it takes the keyboard as soon as it is live, so the
 * first key answers the prompt (F-F-2). Esc closes it, back to the dialog under it, and never reaches the tool, where
 * it would decline the prompt (decision 044); the user answers with the tool's own keys. */
export function AnswerInTerminal({ asks }: { asks: WriterAsks }) {
  const [open, setOpen] = useState(false);
  const well = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = well.current;
    if (!open || !el) return;
    // Heard while capturing, before xterm.js takes the key for the tool. The terminal's find bar keeps its own Esc.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.isComposing) return;
      if (e.target instanceof Element && e.target.closest(".terminal-find")) return;
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
    };
    el.addEventListener("keydown", onKey, true);
    return () => {
      el.removeEventListener("keydown", onKey, true);
    };
  }, [open]);
  return (
    <>
      <span className="writer-asks">
        <Button
          label={asks.answer}
          icon="square-terminal"
          variant="secondary"
          compact
          onClick={() => {
            setOpen(true);
          }}
        />
      </span>
      {open && (
        <Dialog
          title={asks.terminal_title}
          hint="close"
          width={720}
          height="480px"
          footer={null}
          onCancel={() => {
            setOpen(false);
          }}
        >
          <div className="writer-terminal" ref={well}>
            <Terminal session={asks.session} label={asks.terminal_title} focusWhenLive />
          </div>
        </Dialog>
      )}
    </>
  );
}
