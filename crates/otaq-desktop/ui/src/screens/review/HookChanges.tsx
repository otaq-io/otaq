// Show changes after "Your pre-commit hook changed 2 files" (the merged toast, History's Merged row): a hook changed
// the commit Approve & merge made and it landed as the hook left it. Each file's lines are `files.diff` from the commit
// that was squashed (what was reviewed) to the one that landed, drawn as the new commits after a review are. Its words
// are Rust's (`review_page.rs`, `HookChangesView`).
import { useEffect, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { Button, Dialog } from "../../components";
import type { DiffFileView } from "../../generated/DiffFileView";
import type { HookChangesView } from "../../generated/HookChangesView";
import { request } from "../../tauri";
import { NewCommitFile } from "./Dialogs";

/** Each changed file's lines, from what was reviewed to what landed; a file that can't be read is left out. */
export function loadHookChanges(ticket: string, changes: HookChangesView): Promise<DiffFileView[]> {
  return Promise.all(
    changes.files.map((path) =>
      (
        request("files.diff", {
          key: ticket,
          path,
          since: changes.reviewed,
          until: changes.landed,
        }) as Promise<DiffFileView>
      ).catch(() => null),
    ),
  ).then((loaded) => loaded.filter((f): f is DiffFileView => f !== null));
}

export function HookChangesDialog({ params, close }: OverlayProps<"hook_changes">) {
  const { ticket, changes } = params;
  const [files, setFiles] = useState<DiffFileView[]>([]);
  useEffect(() => {
    let current = true;
    loadHookChanges(ticket, changes).then(
      (loaded) => {
        if (current) setFiles(loaded);
      },
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, [ticket, changes]);
  return (
    <Dialog
      title={changes.title}
      width={640}
      onCancel={close}
      footer={<Button variant="primary" label={changes.close} onClick={close} />}
    >
      <div className="review-body">
        <p className="review-muted">{changes.note}</p>
        {files.map((f) => (
          <NewCommitFile key={f.path} file={f} />
        ))}
      </div>
    </Dialog>
  );
}
