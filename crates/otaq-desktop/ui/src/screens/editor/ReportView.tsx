// The report view (CMT-12, d0109; *05 → Row · Ticket comments and a report*, "Opening a report", `s05-41`/`s05-42`):
// a comment's report, or a paste not posted yet, read-only over the panel: "Report · 48 KB · 2,310 lines ·
// read-only", who posted it and on which comment, Copy, Download, search (⌘F) with "2 of 6" and next / previous,
// line numbers, the match highlighted. Open on a report chip lands here (`setReportOpener`).
//
// The page never holds the report whole: `report.open` indexes it on the Rust side, the list below asks for the pages
// of lines it shows (`report.page`), search runs on the Rust side (`report.search`, `report.step`), and Copy and
// Download go from the file there (`report.copy`, `report.download`). Every word comes from `ReportView`.
import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayProps } from "../../app/registry";
import { Tooltip, PLAIN_TEXT } from "../../components";
import { useEnterExit } from "../../components/motion";
import type { IconName } from "../../generated/IconName";
import type { ReportLine } from "../../generated/ReportLine";
import type { ReportOpenParams } from "../../generated/ReportOpenParams";
import type { ReportPage } from "../../generated/ReportPage";
import type { ReportSearch } from "../../generated/ReportSearch";
import type { ReportView as View } from "../../generated/ReportView";
import { asRequestError } from "../../service";
import { request, saveReportFile } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { Mark } from "./CommentThread";
import { setReportOpener } from "./reportOpener";
import {
  exact,
  firstLine,
  listHeight,
  pagesFor,
  ROW_HEIGHT,
  scrollFor,
  SEARCH_DELAY,
  visibleRows,
} from "./reportLogic";

const ignore = () => undefined;

/** Open on a chip shows the view over the panel. */
export function registerReportOpener(): void {
  setReportOpener((target) => {
    const params: ReportOpenParams =
      target.kind === "comment"
        ? { kind: "comment", ticket: target.ticket, n: target.n }
        : { kind: "pasted", ticket: target.ticket, text: target.text };
    openOverlay("report", params);
  });
}

/** An icon button in the head with its tooltip; `done` swaps in a check and the done word for a moment. */
function HeadButton({
  icon,
  label,
  done,
  onClick,
}: {
  icon: IconName;
  label: string;
  done?: string | null;
  onClick: () => void;
}) {
  return (
    <Tooltip text={done ?? label}>
      <button type="button" className="report-view-button" aria-label={label} onClick={onClick}>
        <Icon name={done ? "check" : icon} size={14} />
      </button>
    </Tooltip>
  );
}

function Line({ line }: { line: ReportLine }) {
  return (
    <div className="report-view-line" data-current={line.current || undefined}>
      <span className="report-view-number">{line.number}</span>
      <span className="report-view-text">
        {line.segments.map((s, i) =>
          s.hit ? (
            <mark key={i} className="report-view-hit" data-current={s.current || undefined}>
              {s.text}
            </mark>
          ) : (
            <span key={i}>{s.text}</span>
          ),
        )}
      </span>
    </div>
  );
}

/** The lines, drawn only where the list is scrolled to; their pages come from Rust as they are needed. */
function Lines({
  view,
  version,
  jump,
}: {
  view: View;
  /** Changes when the search does: the pages are asked for again (the marks moved). */
  version: number;
  /** The line to bring into view (the current match), and a counter so the same line jumps again. */
  jump: { line: number; n: number } | null;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(400);
  // The pages fetched for the search `version` they were marked for; older ones show until theirs come.
  const [pages, setPages] = useState<{ version: number; lines: ReadonlyMap<number, readonly ReportLine[]> }>({
    version,
    lines: new Map(),
  });
  const asked = useRef({ version, pages: new Set<number>() });
  const total = view.lines;

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      setHeight(el.clientHeight);
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, []);

  const first = firstLine(top, height, total);
  const rows = visibleRows(height);
  useEffect(() => {
    if (asked.current.version !== version) asked.current = { version, pages: new Set() };
    const wanted = asked.current.pages;
    for (const page of pagesFor(first, rows, view.page_lines, total)) {
      if (wanted.has(page)) continue;
      wanted.add(page);
      const params = { handle: view.handle, start: page * view.page_lines, count: view.page_lines };
      (request("report.page", params) as Promise<ReportPage>).then(
        (answer) => {
          setPages((all) => {
            // A page asked for before a newer search came back late.
            if (all.version > version) return all;
            const lines =
              all.version === version ? new Map(all.lines) : new Map<number, readonly ReportLine[]>();
            return { version, lines: lines.set(page, answer.lines) };
          });
        },
        () => {
          wanted.delete(page);
        },
      );
    }
  }, [first, rows, total, view.handle, view.page_lines, version]);

  useEffect(() => {
    const el = scroller.current;
    if (!el || jump === null) return;
    el.scrollTop = scrollFor(jump.line, el.clientHeight, total);
  }, [jump, total]);

  // A line whose page hasn't come yet keeps its place, empty.
  const shown: { index: number; line: ReportLine | undefined }[] = [];
  for (let i = first; i < Math.min(total, first + rows); i++) {
    shown.push({ index: i, line: pages.lines.get(Math.floor(i / view.page_lines))?.[i % view.page_lines] });
  }
  const spacer = listHeight(total);
  // Exact rows scroll smoothly; past the browser's tallest box the rows move a line at a time.
  const offset = exact(total) ? -(top % ROW_HEIGHT) : 0;
  return (
    <div
      ref={scroller}
      className="report-view-scroll"
      // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- the lines scroll with the arrow and page keys
      tabIndex={0}
      onScroll={(e) => {
        setTop(e.currentTarget.scrollTop);
      }}
    >
      <div className="report-view-window" style={{ height }}>
        <div className="report-view-lines" style={{ transform: `translateY(${String(offset)}px)` }}>
          {shown.map(({ index, line }) =>
            line ? <Line key={index} line={line} /> : <div key={index} className="report-view-line" />,
          )}
        </div>
      </div>
      <div style={{ height: Math.max(0, spacer - height) }} />
    </div>
  );
}

export function ReportViewOverlay({ params, close }: OverlayProps<"report">) {
  const [view, setView] = useState<View | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<ReportSearch | null>(null);
  const [version, setVersion] = useState(0);
  const [jump, setJump] = useState<{ line: number; n: number } | null>(null);
  const [done, setDone] = useState<{ copy: string | null; download: string | null }>({
    copy: null,
    download: null,
  });
  const search = useRef<HTMLInputElement>(null);
  const backdrop = useRef<HTMLDivElement>(null);
  useEnterExit(backdrop, "backdrop", { selector: ".report-view", preset: "dialog" });

  useEffect(() => {
    let handle: string | null = null;
    let live = true;
    (request("report.open", params) as Promise<View>).then(
      (v) => {
        handle = v.handle;
        if (live) setView(v);
        else request("report.close", { handle }).then(ignore, ignore);
      },
      (e: unknown) => {
        if (live) setError(asRequestError(e).message);
      },
    );
    return () => {
      live = false;
      if (handle !== null) request("report.close", { handle }).then(ignore, ignore);
    };
  }, [params]);

  const show = useCallback((answer: ReportSearch) => {
    setFound(answer);
    setVersion((v) => v + 1);
    const line = answer.line;
    if (line !== null) setJump((j) => ({ line, n: (j?.n ?? 0) + 1 }));
  }, []);

  // The search runs on the Rust side a moment after typing stops.
  useEffect(() => {
    if (!view) return;
    const timer = setTimeout(() => {
      (request("report.search", { handle: view.handle, query }) as Promise<ReportSearch>).then(show, ignore);
    }, SEARCH_DELAY);
    return () => {
      clearTimeout(timer);
    };
  }, [view, query, show]);

  const step = (forward: boolean) => {
    if (!view || !found || found.count === 0) return;
    (request("report.step", { handle: view.handle, forward }) as Promise<ReportSearch>).then(show, ignore);
  };
  const flash = (which: "copy" | "download", word: string) => {
    setDone((d) => ({ ...d, [which]: word }));
    setTimeout(() => {
      setDone((d) => ({ ...d, [which]: null }));
    }, 1500);
  };
  const copy = () => {
    if (!view) return;
    (request("report.copy", { handle: view.handle }) as Promise<string>).then(
      (word) => {
        flash("copy", word);
      },
      (e: unknown) => {
        setError(asRequestError(e).message);
      },
    );
  };
  const download = () => {
    if (!view) return;
    saveReportFile(view.save_title, view.file_name).then((to) => {
      if (to === null) return;
      (request("report.download", { handle: view.handle, to }) as Promise<string>).then(
        (word) => {
          flash("download", word);
        },
        (e: unknown) => {
          setError(asRequestError(e).message);
        },
      );
    }, ignore);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key.toLowerCase() === "f" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      e.stopPropagation();
      search.current?.focus();
      search.current?.select();
    }
  };
  useEffect(() => {
    search.current?.focus();
  }, [view]);

  return (
    <div
      ref={backdrop}
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- the view's own keys: Esc, ⌘F */}
      <div
        className="report-view"
        role="dialog"
        aria-modal="true"
        aria-label={view?.title ?? "Report"}
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        <div className="report-view-head">
          <Icon name="file-text" size={15} />
          <span className="report-view-title">{view?.title}</span>
          <span className="report-view-meta">{view?.meta}</span>
          <span className="report-view-spacer" />
          {view && (
            <>
              <HeadButton icon="copy" label={view.copy} done={done.copy} onClick={copy} />
              <HeadButton icon="download" label={view.download} done={done.download} onClick={download} />
              <HeadButton icon="x" label={view.close} onClick={close} />
            </>
          )}
        </div>
        {view?.poster && (
          <div className="report-view-poster">
            <Mark row={view.poster} />
            <span className="report-view-heading">{view.poster.heading}</span>
            <span className="report-view-on">{view.poster.on}</span>
          </div>
        )}
        {view && (
          <div className="report-view-search">
            <label className="report-view-field">
              <Icon name="search" size={13} />
              <input
                {...PLAIN_TEXT}
                ref={search}
                className="report-view-input"
                value={query}
                placeholder={view.search}
                aria-label={view.search}
                onChange={(e) => {
                  setQuery(e.target.value);
                }}
                onKeyDown={(e) => {
                  if (e.key !== "Enter") return;
                  e.preventDefault();
                  step(!e.shiftKey);
                }}
              />
              {found && found.label !== "" && (
                <span className="report-view-count" role="status">
                  {found.label}
                </span>
              )}
              <button
                type="button"
                className="report-view-step"
                aria-label={view.previous}
                disabled={!found || found.count === 0}
                onClick={() => {
                  step(false);
                }}
              >
                <Icon name="chevron-up" size={13} />
              </button>
              <button
                type="button"
                className="report-view-step"
                aria-label={view.next}
                disabled={!found || found.count === 0}
                onClick={() => {
                  step(true);
                }}
              >
                <Icon name="chevron-down" size={13} />
              </button>
            </label>
          </div>
        )}
        {error !== null && (
          <p className="report-view-error" role="alert">
            {error}
          </p>
        )}
        {view && <Lines view={view} version={version} jump={jump} />}
      </div>
    </div>
  );
}
