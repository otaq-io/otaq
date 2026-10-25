// Where a terminal's file links open (d0117 point 8), by the terminal's host: a ticket's terminal (agent, validator,
// Shell, Run) in the ticket's Files tab, the Project agent and Project terminals in the project's Files view. With a line, as File scrolled to it with the line marked.
import { openFileIn } from "../screens/files/FilesTab";
import { openProjectFile } from "../screens/files_view/logic";

export type FilesPlace = { kind: "ticket"; key: string } | { kind: "project"; project: string };

/** Opens `path` (inside the session's folder) where `place` says. */
export function openPathIn(place: FilesPlace, path: string, line: number | null): void {
  if (place.kind === "ticket") {
    openFileIn(place.key, { path, child: null, mode: line === null ? null : "file", line });
  } else {
    openProjectFile(place.project, path, line);
  }
}
