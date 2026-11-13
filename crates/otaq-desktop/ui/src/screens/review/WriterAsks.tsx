// The delivery message's writer waits for permission (its tool asks before a command): Approve & merge and Approve &
// open PR say so in their line, and Answer in terminal beside it opens the writer's own terminal over the dialog. The
// plain message stays merge-ready meanwhile; the ticket in Review stays calm. The words are Rust's (`WriterAsks`).
import { useState } from "react";

import { Button, Dialog } from "../../components";
import type { WriterAsks } from "../../generated/WriterAsks";
import { Terminal } from "../../terminal/Terminal";

/** Answer in terminal, and the writer's terminal once pressed (closed with Esc, back to the dialog under it). */
export function AnswerInTerminal({ asks }: { asks: WriterAsks }) {
  const [open, setOpen] = useState(false);
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
          <div className="writer-terminal">
            <Terminal session={asks.session} label={asks.terminal_title} />
          </div>
        </Dialog>
      )}
    </>
  );
}
