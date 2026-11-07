// The gallery's new-ticket attachments: the empty place, several files (an image and a text file), and a file refused
// for its size, with Rust's words (`gallery.new_ticket_attachments`).
import { useCallback } from "react";

import type { EditorAttachmentsField } from "../../generated/EditorAttachmentsField";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { type BoxValue, EMPTY_BOX, fileAttachment, withAdded } from "./commentRules";
import { AttachmentsPlace } from "./NewTicketForm";

const ignore = () => undefined;

const SEVERAL = withAdded(EMPTY_BOX, [
  fileAttachment({ path: "/Users/me/before.png", name: "before.png", size: "212 KB", image: true }),
  fileAttachment({
    path: "/Users/me/test-run-login.md",
    name: "test-run-login.md",
    size: "3 KB",
    image: false,
  }),
]);

const STATES: readonly [string, BoxValue, readonly string[]][] = [
  ["Nothing attached", EMPTY_BOX, []],
  ["Several files", SEVERAL, []],
  [
    "A file over the limit",
    EMPTY_BOX,
    ["run.log is 212 MB; the limit is 20 MB. Keep the part that matters, for example the failing tests."],
  ],
];

export function NewTicketAttachmentsGallery() {
  const load = useCallback(
    () => request("gallery.new_ticket_attachments") as Promise<EditorAttachmentsField>,
    [],
  );
  const { view, error } = useServiceView(load, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <div className="gallery-stack">
      {STATES.map(([caption, value, refused]) => (
        <figure key={caption} className="gallery-stack-tight new-ticket-gallery">
          <figcaption className="gallery-caption">{caption}</figcaption>
          <AttachmentsPlace field={view} attachments={{ value, refused, addPaths: ignore, remove: ignore }} />
        </figure>
      ))}
    </div>
  );
}
