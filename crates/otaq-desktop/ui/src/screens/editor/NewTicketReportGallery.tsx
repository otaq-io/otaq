// The gallery's New ticket report (T-0474; RPT-03): the place before a file, a file waiting for its line, a file with
// its line, and a pasted file, with Rust's words (`gallery.new_ticket_report`).
import { useCallback } from "react";

import type { EditorReportField } from "../../generated/EditorReportField";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { type BoxValue, EMPTY_BOX, fileAttachment, pastedAttachment } from "./commentRules";
import { NewTicketReport, type ReportAttach } from "./NewTicketReport";

const ignore = () => undefined;

const FILE = fileAttachment({
  path: "/Users/me/test-run-login.md",
  name: "test-run-login.md",
  size: "3 KB",
  image: false,
});

const STATES: readonly [string, BoxValue][] = [
  ["No report yet", EMPTY_BOX],
  ["A file waits for its line", { ...EMPTY_BOX, attachments: [FILE] }],
  ["With its line", { text: "Fails 2 in 20 on Safari; the run is attached.", attachments: [FILE] }],
  [
    "A pasted file",
    { text: "My notes from the Safari run.", attachments: [{ ...pastedAttachment("…"), bytes: 2048 }] },
  ],
];

function attach(value: BoxValue): ReportAttach {
  return { value, setValue: ignore, error: null, attachPath: ignore, onPaste: ignore };
}

export function NewTicketReportGallery() {
  const load = useCallback(() => request("gallery.new_ticket_report") as Promise<EditorReportField>, []);
  const { view, error } = useServiceView(load, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <div className="gallery-stack">
      {STATES.map(([caption, value]) => (
        <figure key={caption} className="gallery-stack-tight new-ticket-report-gallery">
          <figcaption className="gallery-caption">{caption}</figcaption>
          <NewTicketReport field={view} project="SHOP" attach={attach(value)} onSave={ignore} />
        </figure>
      ))}
      <figure className="gallery-stack-tight new-ticket-report-gallery">
        <figcaption className="gallery-caption">A file that can&apos;t be a report</figcaption>
        <NewTicketReport
          field={view}
          project="SHOP"
          attach={{
            ...attach(EMPTY_BOX),
            error:
              "run.log is 212 MB; the limit is 20 MB. Keep the part that matters, for example the failing tests.",
          }}
          onSave={ignore}
        />
      </figure>
    </div>
  );
}
