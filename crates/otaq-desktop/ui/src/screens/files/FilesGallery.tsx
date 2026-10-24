// The gallery's Files tab (T-0468; `s25-07`, `s25-08`, `s25-09`): SHOP-22's tree with src/ open beside
// middleware.ts, a search for "log", SHOP-18's All changes with a line comment not sent yet and the box, and the
// review bar without and with comments. The fixtures are Rust's (`gallery.files`).
import { useCallback } from "react";

import { Segmented } from "../../components";
import type { FilesGallery as Fixtures } from "../../generated/FilesGallery";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { FileBody, SearchResults, TreeRowButton } from "./FilesTab";
import { type CommentContext, Hunk } from "./Hunks";
import { threadsByAnchor } from "./logic";
import { ReviewBarBody } from "./ReviewBar";

const ignore = () => undefined;

export function FilesGallery() {
  const load = useCallback(() => request("gallery.files") as Promise<Fixtures>, []);
  const { view: g, error } = useServiceView(load, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!g) return null;
  const ctx: CommentContext = {
    ticket: "SHOP-18",
    comments: g.comments,
    threads: threadsByAnchor(g.comments),
    composer: null,
    dispatch: ignore,
    save: ignore,
  };
  const [top, ...rest] = g.tab.rows;
  const src = rest[0];
  return (
    <div className="gallery-stack">
      <figure className="gallery-stack-tight">
        <figcaption className="gallery-caption">SHOP-22 · the tree and a file (File)</figcaption>
        <div className="files" style={{ height: 420 }}>
          <aside className="files-side">
            <div className="files-source">
              <Icon name="folder-git-2" size={14} />
              <div className="files-source-text">
                <span className="files-source-title">{g.tab.source.title}</span>
                <span className="files-source-sub">{g.tab.source.subtitle}</span>
              </div>
            </div>
            <Segmented label={g.tab.source.title} options={g.tab.filters} selected={0} onSelect={ignore} />
            <div className="files-list">
              {top && <TreeRowButton row={top} open={false} selected={false} onPress={ignore} />}
              {src && <TreeRowButton row={src} open selected={false} onPress={ignore} />}
              {g.src.map((row) => (
                <TreeRowButton
                  key={row.path}
                  row={row}
                  open={false}
                  selected={row.path === g.file.path}
                  onPress={ignore}
                />
              ))}
              {rest.slice(1).map((row) => (
                <TreeRowButton key={row.path} row={row} open={false} selected={false} onPress={ignore} />
              ))}
            </div>
          </aside>
          <div className="files-main">
            <FileBody
              ticket="SHOP-22"
              page={g.file}
              file={{ path: g.file.path, child: null, mode: "file" }}
              ctx={ctx}
              width={1200}
              onMode={ignore}
              onSplit={ignore}
            />
          </div>
        </div>
      </figure>
      <figure className="gallery-stack-tight">
        <figcaption className="gallery-caption">Search files by name · “log”</figcaption>
        <div className="files-side">
          <SearchResults found={g.search} selected={0} onOpen={ignore} />
        </div>
      </figure>
      <figure className="gallery-stack-tight">
        <figcaption className="gallery-caption">
          SHOP-18 · All changes · a line comment not sent yet
        </figcaption>
        <div className="files-main">
          <div className="files-changes">
            <div className="files-main-head">
              <div className="files-main-title">
                <span className="files-title">{g.changes.title}</span>
                <span className="files-subtitle">
                  {g.changes.summary} {g.changes.note}
                </span>
              </div>
              <Segmented label="Unified / Split" options={g.changes.modes} selected={0} onSelect={ignore} />
            </div>
            <div className="files-card">
              {g.diff.hunks.map((hunk, i) => (
                <Hunk key={i} hunk={hunk} sideBySide={false} ctx={ctx} />
              ))}
            </div>
          </div>
        </div>
      </figure>
      <figure className="gallery-stack-tight">
        <figcaption className="gallery-caption">
          The review bar · in your check, then with a comment
        </figcaption>
        {g.bars.map((bar) => (
          <ReviewBarBody key={bar.title} ticket="SHOP-18" bar={bar} />
        ))}
      </figure>
    </div>
  );
}
