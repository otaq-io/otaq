// The gallery's project Files view (T-0478, T-0479; `s28-03`…`s28-10`): shop-web on All with src/lib open and
// pricing.ts picked (File, Diff of SHOP-29, and narrow with the line of chips), Changed with All changes grouped by
// ticket, Needs you, the file states (only on release/2.3, deleted, binary, huge, a long path), the loading and empty
// states, the picker and SHOP-29's worktree's note. The fixtures are Rust's (`gallery.files_view`).
import { type ReactNode, useCallback } from "react";

import type { FilesFileMode } from "../../generated/FilesFileMode";
import type { FilesViewFile } from "../../generated/FilesViewFile";
import type { FilesViewGallery as Fixtures } from "../../generated/FilesViewGallery";
import type { FilesViewPage } from "../../generated/FilesViewPage";
import type { FilesTreeRow } from "../../generated/FilesTreeRow";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { AllChangesView } from "./AllChangesView";
import { FileViewer } from "./FileViewer";
import { FilesViewBody, PickerBody } from "./FilesView";
import { diffOwner, shownFileMode } from "./logic";

const ignore = () => undefined;

function Frame({
  caption,
  page,
  rows,
  file,
  pane,
  width,
}: {
  caption: string;
  page: FilesViewPage;
  rows: readonly FilesTreeRow[];
  file?: string;
  pane?: ReactNode;
  /** The main area's width: 1100 shows the narrow layout (1280 px, or the agent panel open). */
  width?: number;
}) {
  const open = new Set(rows.filter((r) => r.kind === "folder").map((r) => r.path));
  return (
    <figure className="gallery-stack-tight">
      <figcaption className="gallery-caption">{caption}</figcaption>
      <div style={{ height: 560, display: "flex", ...(width !== undefined && { width }) }}>
        <FilesViewBody
          project={page.project}
          page={page}
          rows={rows}
          open={open}
          file={file ?? null}
          found={null}
          query=""
          onQuery={ignore}
          selectedMatch={-1}
          onOpenMatch={ignore}
          onRow={ignore}
          onFilter={ignore}
          {...(pane !== undefined && { pane })}
          source={
            <div className="files-source files-view-source">
              <Icon name="git-branch" size={14} />
              <span className="files-source-text">
                <span className="files-source-title">{page.source.title}</span>
                <span className="files-source-sub">{page.source.subtitle}</span>
              </span>
              <Icon name="chevron-down" size={12} />
            </div>
          }
        />
      </div>
    </figure>
  );
}

/** A picked file as the view draws it, its Diff on `diff`'s ticket when `mode` is Diff. */
function Viewer({ view, mode, g }: { view: FilesViewFile; mode?: FilesFileMode; g: Fixtures }) {
  const shown = shownFileMode(mode ?? null, view);
  const owner = diffOwner(null, view);
  const diff =
    shown === "diff"
      ? { state: "loaded" as const, view: owner === view.diff_of && view.diff ? view.diff : g.diff }
      : undefined;
  return (
    <FileViewer
      view={view}
      mode={shown}
      owner={owner}
      diff={diff}
      onMode={ignore}
      onOwner={ignore}
      onNewTicket={ignore}
      onCopyPath={ignore}
      onOpen={ignore}
    />
  );
}

export function FilesViewGallery() {
  const load = useCallback(() => request("gallery.files_view") as Promise<Fixtures>, []);
  const { view: g, error } = useServiceView(load, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!g) return null;
  return (
    <div className="gallery-stack">
      <Frame
        caption="shop-web · All · src/lib open, pricing.ts picked"
        page={g.all}
        rows={g.all_rows}
        file="src/lib/pricing.ts"
        pane={<Viewer view={g.file} g={g} />}
      />
      <Frame
        caption="pricing.ts · Diff of SHOP-29 (the one that changed it last)"
        page={g.all}
        rows={g.all_rows}
        file="src/lib/pricing.ts"
        pane={<Viewer view={g.file} mode="diff" g={g} />}
      />
      <Frame
        caption="Narrow (1280 px, or the agent panel open) · Changed by as chips, New ticket as +"
        page={g.all}
        rows={g.all_rows}
        file="src/lib/pricing.ts"
        width={1100}
        pane={<Viewer view={g.file} g={g} />}
      />
      <Frame
        caption="Changed · All changes, grouped by ticket, the ones that need you first"
        page={g.changed}
        rows={g.changed.rows}
        pane={
          <div className="files-main">
            <AllChangesView
              changes={g.changes}
              pane={null}
              width={900}
              onMode={ignore}
              onOpenFile={ignore}
              onOpen={ignore}
            />
          </div>
        }
      />
      <Frame
        caption="Only on release/2.3 · previews as SHOP-36's diff, File off"
        page={g.all}
        rows={g.all_rows}
        file="src/lib/vat-table.ts"
        pane={<Viewer view={g.not_on_base} g={g} />}
      />
      <Frame
        caption="A deleted file · as it is on main, with SHOP-19's bar"
        page={g.all}
        rows={g.all_rows}
        file="src/lib/legacy-price.ts"
        pane={<Viewer view={g.deleted} g={g} />}
      />
      <Frame
        caption="A binary file · sizes, no preview"
        page={g.all}
        rows={g.all_rows}
        pane={<Viewer view={g.binary} g={g} />}
      />
      <Frame
        caption="A huge file · only the parts tickets change, the rest folded"
        page={g.all}
        rows={g.all_rows}
        pane={<Viewer view={g.huge} g={g} />}
      />
      <Frame
        caption="A long path · cut in the middle, the full path on hover"
        page={g.all}
        rows={g.all_rows}
        pane={<Viewer view={g.long_path} g={g} />}
      />
      <Frame
        caption="Needs you · the files of the tickets that need you"
        page={g.needs_you}
        rows={g.needs_you.rows}
      />
      <Frame caption="Loading · the base at once, the counts follow" page={g.loading} rows={g.loading_rows} />
      <Frame caption="Nothing changing" page={g.empty} rows={[]} />
      <figure className="gallery-stack-tight">
        <figcaption className="gallery-caption">The picker · Find a ticket</figcaption>
        <div className="popover" style={{ position: "relative", width: "fit-content" }}>
          <PickerBody picker={g.picker} query="" onQuery={ignore} onPick={ignore} />
        </div>
      </figure>
      <figure className="gallery-stack-tight">
        <figcaption className="gallery-caption">
          SHOP-29’s worktree · the back link, the source and the note
        </figcaption>
        <div className="files-side" style={{ width: 300 }}>
          <button type="button" className="files-view-back">
            <Icon name="arrow-left" size={12} />
            <span>{g.picked.back}</span>
          </button>
          <div className="files-source">
            <Icon name="folder-git-2" size={14} />
            <div className="files-source-text">
              <span className="files-source-title">{g.picked.tab.source.title}</span>
              <span className="files-source-sub">{g.picked.tab.source.subtitle}</span>
            </div>
          </div>
        </div>
        <div className="files-view-picked-note">
          <span className="files-quiet">{g.picked.note}</span>
          <button type="button" className="files-view-open">
            <Icon name="external-link" size={12} />
            <span>{g.picked.open}</span>
          </button>
        </div>
      </figure>
    </div>
  );
}
