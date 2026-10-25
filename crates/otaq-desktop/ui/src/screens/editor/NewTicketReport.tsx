// The New ticket form's report (RPT-03, d0109; T-0474): a file picked with Add, dropped on the form, or copied in
// Finder and pasted outside a text field, with its one line. Create posts both as the ticket's first comment, yours,
// "at creation"; agents open your reports before they start (RPT-01). Until a file is there the place shows Add and
// the drop line; then the comment box (`CommentBox`, a dialog's: no button of its own) with the file's chip and the
// line. A comment always has its own line (d0109): Create is refused without it (Rust's `report_error`). Its words
// come from Rust (`EditorView.report`); the form keeps the box (`formReport` turns it into the form's fields).
import { type ClipboardEvent, type RefObject, useCallback, useEffect, useState } from "react";

import type { EditorReportField } from "../../generated/EditorReportField";
import type { OverviewFileChip } from "../../generated/OverviewFileChip";
import { asRequestError } from "../../service";
import { pickReportFile, request } from "../../tauri";
import { dropTargets } from "../../terminal/drops";
import { Icon } from "../../theme/Icon";
import { CommentBox } from "./CommentBox";
import { type BoxValue, EMPTY_BOX, withFile, withPasted } from "./commentRules";
import { formReport } from "./logic";

/** Whether a paste lands in something that takes text itself (a field, the description editor). */
function inTextField(target: EventTarget): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || target.closest("input, textarea, [contenteditable='true']") !== null)
  );
}

export interface ReportAttach {
  value: BoxValue;
  setValue: (value: BoxValue) => void;
  error: string | null;
  /** A file by its path (Add, a drop): its chip, or why it can't be a report. */
  attachPath: (path: string) => void;
  /** A file pasted outside a text field: its text becomes the pasted report. */
  onPaste: (e: ClipboardEvent<HTMLElement>) => void;
}

/**
 * The form's report and the file drops on `root` (the form's body). `onChange` gets the form's fields for it
 * (`formReport`); `enabled` is false while the form takes no report.
 */
export function useReportAttach(
  root: RefObject<HTMLElement | null>,
  enabled: boolean,
  onChange: (fields: ReturnType<typeof formReport>) => void,
): ReportAttach {
  const [value, setBox] = useState<BoxValue>(EMPTY_BOX);
  const [error, setError] = useState<string | null>(null);
  const setValue = useCallback(
    (next: BoxValue) => {
      setBox(next);
      onChange(formReport(next));
    },
    [onChange],
  );
  const attachPath = useCallback(
    (path: string) => {
      setError(null);
      (request("overview.file", { path }) as Promise<OverviewFileChip>).then(
        (file) => {
          setBox((v) => {
            const next = withFile(v, file);
            onChange(formReport(next));
            return next;
          });
        },
        (e: unknown) => {
          setError(asRequestError(e).message);
        },
      );
    },
    [onChange],
  );
  useEffect(() => {
    const el = root.current;
    if (!el || !enabled) return;
    return dropTargets.register(el, {
      takesTyping: () => false,
      paste: () => undefined,
      takeFiles: (paths) => {
        if (paths[0] !== undefined) attachPath(paths[0]);
      },
    });
  }, [root, enabled, attachPath]);
  const onPaste = (e: ClipboardEvent<HTMLElement>) => {
    const file = (e.clipboardData.files as FileList | undefined)?.[0];
    if (!enabled || file === undefined || inTextField(e.target)) return;
    e.preventDefault();
    setError(null);
    file.text().then(
      (text) => {
        setBox((v) => {
          const next = withPasted(v, text);
          onChange(formReport(next));
          return next;
        });
      },
      () => undefined,
    );
  };
  return { value, setValue, error, attachPath, onPaste };
}

/** "REPORT  + Add", the hint, then the drop line or the box with the file's chip and its line. */
export function NewTicketReport({
  field,
  project,
  attach,
  onSave,
}: {
  field: EditorReportField;
  /** What the pasted chip's report view names (no ticket yet). */
  project: string;
  attach: ReportAttach;
  /** ⌘↵ in the line saves the form. */
  onSave: () => void;
}) {
  const attached = attach.value.file !== null || attach.value.pasted !== null;
  return (
    <div className="field editor-report">
      <div className="editor-report-head">
        <span className="field-label">{field.label}</span>
        <span className="editor-spacer" />
        <button
          type="button"
          className="overview-add"
          onClick={() => {
            pickReportFile(field.pick_title).then(
              (path) => {
                if (path !== null) attach.attachPath(path);
              },
              () => undefined,
            );
          }}
        >
          <Icon name="plus" size={12} />
          {field.add}
        </button>
      </div>
      {attached ? (
        <CommentBox
          ticket={project}
          view={field.line_box}
          value={attach.value}
          onChange={attach.setValue}
          onSubmit={onSave}
        />
      ) : (
        <div className="overview-drop">
          <Icon name="upload" size={13} />
          <span>{field.drop}</span>
        </div>
      )}
      {attach.error !== null ? (
        <span className="field-note" data-error role="alert">
          {attach.error}
        </span>
      ) : (
        <span className="field-note">{field.hint}</span>
      )}
    </div>
  );
}
