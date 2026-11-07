// The Cleanup overlay (*16 → Cleanup · 3 items, Cleanup · 20 items, Cleanup · empty, Delete · confirm, Success, Dark ·
// cleanup*; REC-04, d0018): the leftovers no ticket owns, grouped, each with its path, why, size and age. It scans
// when it opens; nothing is ticked at first; long lists open with their groups folded. Delete selected… asks with
// every item named, and only the confirmation's button deletes (`recovery.cleanup_delete`, which toasts what went).
// Settings → Data and `cleanup` + ↵ in search open it.
//
// Under the action bar, the kept warm builds (*Row · Cleanup with warm builds (0082)*, WRM-07): no checkboxes, each
// with a Clear… that asks first (`recovery.warm_clear` only from the confirmation). A Clear waiting for a copy shows
// on its row; the section is listed again on `warm_build_changed`, and after a row goes focus moves to the Clear…
// the Rust side names.
import { useEffect, useRef, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { Button, ConfirmDialog, IconButton, useEnterExit } from "../../components";
import type { CleanupConfirm } from "../../generated/CleanupConfirm";
import type { CleanupGroupPage } from "../../generated/CleanupGroupPage";
import type { CleanupGroupParams } from "../../generated/CleanupGroupParams";
import type { CleanupPage } from "../../generated/CleanupPage";
import type { CleanupParams } from "../../generated/CleanupParams";
import type { CleanupTarget } from "../../generated/CleanupTarget";
import type { CleanupToggleParams } from "../../generated/CleanupToggleParams";
import type { CleanupWarmClearParams } from "../../generated/CleanupWarmClearParams";
import type { CleanupWarmRow } from "../../generated/CleanupWarmRow";
import type { CleanupWarmSection } from "../../generated/CleanupWarmSection";
import { asRequestError, pageEvents } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { cleanupScans, startFolded, toggleFold } from "./logic";

export interface CleanupBodyProps {
  page: CleanupPage;
  folded: ReadonlySet<string>;
  error: string | null;
  onFold: (group: string) => void;
  onToggle: (target: CleanupTarget) => void;
  onToggleGroup: (group: string) => void;
  onDelete: () => void;
  onClose: () => void;
  /** A warm build's Clear…: opens its confirmation. */
  onClear: (row: CleanupWarmRow) => void;
  /** Where each Clear… button is, by its warm build's folder (for the focus after a Clear). */
  clearRef?: (repo: string, button: HTMLButtonElement | null) => void;
}

/** The warm builds' section: the head with the total, the note, and one row per warm build, largest first. */
export function WarmSection({
  section,
  onClear,
  clearRef,
}: { section: CleanupWarmSection } & Pick<CleanupBodyProps, "onClear" | "clearRef">) {
  return (
    <section className="cleanup-warm" aria-label={section.title}>
      <div className="cleanup-warm-head">
        <Icon name={section.icon} size={13} />
        <span className="cleanup-group-title">{section.title}</span>
        <span className="cleanup-mono cleanup-muted">{section.count}</span>
        <span className="cleanup-spacer" />
        <span className="cleanup-mono cleanup-warm-total">{section.total}</span>
      </div>
      <p className="cleanup-warm-note">{section.note}</p>
      {section.rows.map((row) => (
        <div key={row.repo} className="cleanup-warm-row">
          <Icon name={section.icon} size={13} />
          <span className="cleanup-row-text">
            <span className="cleanup-warm-name">
              <strong>{row.name}</strong>
              <span className="cleanup-mono">{row.path}</span>
            </span>
            {row.clearing !== null ? (
              <span className="cleanup-warm-clearing">
                <Icon name="loader-circle" size={11} />
                {row.clearing}
              </span>
            ) : (
              row.built_at !== null && <span className="cleanup-row-detail">{row.built_at}</span>
            )}
          </span>
          <span className="cleanup-mono cleanup-size">{row.size}</span>
          <span className="cleanup-mono cleanup-age">{row.age}</span>
          <Button
            ref={(button) => {
              clearRef?.(row.repo, button);
            }}
            variant="secondary"
            label={row.clear}
            loading={!row.clear_enabled}
            disabled={!row.clear_enabled}
            onClick={() => {
              onClear(row);
            }}
          />
        </div>
      ))}
    </section>
  );
}

function Group({
  group,
  folded,
  onFold,
  onToggle,
  onToggleGroup,
}: { group: CleanupGroupPage; folded: boolean } & Pick<
  CleanupBodyProps,
  "onFold" | "onToggle" | "onToggleGroup"
>) {
  return (
    <div className="cleanup-group" data-warning={group.group === "kept_worktrees" || undefined}>
      <div className="cleanup-group-head">
        <button
          type="button"
          className="cleanup-fold"
          aria-expanded={!folded}
          aria-label={group.title}
          onClick={() => {
            onFold(group.group);
          }}
        >
          <Icon name={folded ? "chevron-right" : "chevron-down"} size={12} />
        </button>
        <input
          type="checkbox"
          className="checkbox"
          aria-label={group.title}
          checked={group.all_checked}
          onChange={() => {
            onToggleGroup(group.group);
          }}
        />
        <Icon name={group.icon} size={13} />
        <span className="cleanup-group-title">{group.title}</span>
        <span className="cleanup-mono cleanup-muted">{group.count}</span>
        {folded && !group.all_checked && (
          <button
            type="button"
            className="cleanup-select-all"
            onClick={() => {
              onToggleGroup(group.group);
            }}
          >
            {group.select_all}
          </button>
        )}
      </div>
      {!folded &&
        group.rows.map((row) => (
          <label key={row.target.id} className="cleanup-row" data-checked={row.checked || undefined}>
            <input
              type="checkbox"
              className="checkbox"
              checked={row.checked}
              onChange={() => {
                onToggle(row.target);
              }}
            />
            <span className="cleanup-row-text">
              <span className="cleanup-row-title">{row.title}</span>
              <span className="cleanup-row-detail" data-warning={row.warning || undefined}>
                {row.detail}
              </span>
            </span>
            <span className="cleanup-mono cleanup-size">{row.size}</span>
            <span className="cleanup-mono cleanup-age">{row.age}</span>
          </label>
        ))}
    </div>
  );
}

/** The window's content for a loaded scan (also what the markup tests render). */
export function CleanupBody({
  page,
  folded,
  error,
  onFold,
  onToggle,
  onToggleGroup,
  onDelete,
  onClose,
  onClear,
  clearRef,
}: CleanupBodyProps) {
  return (
    <>
      <div className="cleanup-head">
        <div className="cleanup-head-row">
          <Icon name="brush-cleaning" size={16} />
          <h2 className="cleanup-title">{page.title}</h2>
          <span className="cleanup-muted">{page.count}</span>
          <span className="cleanup-spacer" />
          <IconButton icon="x" label="Close" onClick={onClose} />
        </div>
        <p className="cleanup-text">{page.text}</p>
      </div>
      {page.empty ? (
        <div className="cleanup-empty">
          <span className="cleanup-empty-tile">
            <Icon name="check" size={20} />
          </span>
          <strong>{page.empty.title}</strong>
          <span>{page.empty.text}</span>
        </div>
      ) : (
        <div className="cleanup-list">
          {page.groups.map((g) => (
            <Group
              key={g.group}
              group={g}
              folded={folded.has(g.group)}
              onFold={onFold}
              onToggle={onToggle}
              onToggleGroup={onToggleGroup}
            />
          ))}
        </div>
      )}
      {error !== null && (
        <p className="cleanup-error" role="alert">
          {error}
        </p>
      )}
      {!page.empty && (
        <div className="cleanup-foot">
          <span>{page.footer}</span>
          <span className="cleanup-spacer" />
          <span className="cleanup-delete">
            <Button
              variant="secondary"
              icon="trash-2"
              label={page.delete}
              disabled={page.confirm === null}
              onClick={onDelete}
            />
          </span>
        </div>
      )}
      {page.warm && <WarmSection section={page.warm} onClear={onClear} clearRef={clearRef} />}
    </>
  );
}

/** *Warm build · Clear · confirm (0082)*: what is deleted and how big ("up to"), Cancel the default. */
export function WarmConfirmDialog({
  row,
  clearing,
  onConfirm,
  onCancel,
}: {
  row: CleanupWarmRow;
  clearing: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const confirm = row.confirm;
  return (
    <ConfirmDialog
      title={confirm.title}
      width={480}
      destructive
      confirmLabel={confirm.confirm}
      confirmIcon="trash-2"
      confirmDisabled={clearing}
      body={
        <>
          <p className="dialog-text">{confirm.text}</p>
          <p className="dialog-text cleanup-warm-shared">{confirm.shared}</p>
          <ul className="cleanup-confirm-lines">
            <li>
              <Icon name={confirm.line.icon} size={13} />
              <span>{confirm.line.text}</span>
            </li>
          </ul>
        </>
      }
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

/** *Delete · confirm*: every item named, Cancel the default. */
export function CleanupConfirmDialog({
  confirm,
  deleting,
  onConfirm,
  onCancel,
}: {
  confirm: CleanupConfirm;
  deleting: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <ConfirmDialog
      title={confirm.title}
      width={520}
      destructive
      confirmLabel={confirm.confirm}
      confirmIcon="trash-2"
      confirmDisabled={deleting}
      body={
        <>
          <p className="dialog-text">{confirm.text}</p>
          <ul className="cleanup-confirm-lines">
            {confirm.lines.map((line) => (
              <li key={line.text}>
                <Icon name={line.icon} size={13} />
                <span>{line.text}</span>
              </li>
            ))}
          </ul>
        </>
      }
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

const CLEANUP_BOX = { selector: ".cleanup", preset: "dialog" } as const;

export function Cleanup({ close }: OverlayProps<"cleanup">) {
  const [page, setPage] = useState<CleanupPage | null>(null);
  const [folded, setFolded] = useState<ReadonlySet<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [toClear, setToClear] = useState<CleanupWarmRow | null>(null);
  const [clearing, setClearing] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const backdrop = useRef<HTMLDivElement>(null);
  // Comes and goes like a dialog (OT-6).
  useEnterExit(backdrop, "backdrop", CLEANUP_BOX);
  const clearButtons = useRef(new Map<string, HTMLButtonElement>());

  useEffect(() => {
    let live = true;
    const params: CleanupParams = { scan: true };
    (request("recovery.cleanup", params) as Promise<CleanupPage>).then(
      (p) => {
        if (!live) return;
        setPage(p);
        setFolded(startFolded(p));
        cleanupScans.update((n) => n + 1);
        box.current?.focus();
      },
      (e: unknown) => {
        if (live) setError(asRequestError(e).message);
      },
    );
    return () => {
      live = false;
    };
  }, []);

  // A warm build kept, replaced or cleared (also a waiting Clear that ended): its section again.
  useEffect(() => {
    const listening = pageEvents.listen((event) => {
      if (event.kind !== "service" || event.name !== "warm_build_changed") return;
      const params: CleanupParams = { scan: false, warm: true };
      (request("recovery.cleanup", params) as Promise<CleanupPage>).then(setPage, (e: unknown) => {
        setError(asRequestError(e).message);
      });
    });
    return () => {
      listening.stop();
    };
  }, []);

  // A cleared row is gone: focus to the Clear… now in its place, or the window when none is left.
  const focusRepo = page?.warm?.focus?.repo;
  const focusSeq = page?.warm?.focus?.seq;
  useEffect(() => {
    if (focusRepo === undefined) return;
    (clearButtons.current.get(focusRepo) ?? box.current)?.focus();
  }, [focusRepo, focusSeq]);

  // Esc closes the window; while a confirmation asks, the confirmation has it.
  const asking = confirming || toClear !== null;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || asking) return;
      e.stopPropagation();
      close();
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
    };
  }, [asking, close]);

  const apply = (answer: Promise<unknown>) => {
    answer.then(
      (p) => {
        setPage(p as CleanupPage);
        setError(null);
      },
      (e: unknown) => {
        setError(asRequestError(e).message);
      },
    );
  };

  return (
    <div
      ref={backdrop}
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !asking) close();
      }}
    >
      <div
        ref={box}
        className="cleanup"
        data-warm={page?.warm ? true : undefined}
        role="dialog"
        aria-modal="true"
        aria-label="Cleanup"
        tabIndex={-1}
      >
        {page ? (
          <CleanupBody
            page={page}
            folded={folded}
            error={error}
            onFold={(group) => {
              setFolded((f) => toggleFold(f, group));
            }}
            onToggle={(target) => {
              const params: CleanupToggleParams = { target };
              apply(request("recovery.cleanup_toggle", params));
            }}
            onToggleGroup={(group) => {
              const params: CleanupGroupParams = { group };
              apply(request("recovery.cleanup_toggle_group", params));
            }}
            onDelete={() => {
              setConfirming(true);
            }}
            onClose={close}
            onClear={setToClear}
            clearRef={(repo, button) => {
              if (button) clearButtons.current.set(repo, button);
              else clearButtons.current.delete(repo);
            }}
          />
        ) : (
          <p className="cleanup-error">{error}</p>
        )}
      </div>
      {confirming && page?.confirm && (
        <CleanupConfirmDialog
          confirm={page.confirm}
          deleting={deleting}
          onConfirm={() => {
            setDeleting(true);
            (request("recovery.cleanup_delete") as Promise<CleanupPage>).then(
              (p) => {
                setPage(p);
                setDeleting(false);
                setConfirming(false);
                cleanupScans.update((n) => n + 1);
              },
              (e: unknown) => {
                setDeleting(false);
                setConfirming(false);
                setError(asRequestError(e).message);
              },
            );
          }}
          onCancel={() => {
            setConfirming(false);
          }}
        />
      )}
      {toClear && (
        <WarmConfirmDialog
          row={toClear}
          clearing={clearing}
          onConfirm={() => {
            setClearing(true);
            const params: CleanupWarmClearParams = { repo: toClear.repo };
            (request("recovery.warm_clear", params) as Promise<CleanupPage>).then(
              (p) => {
                setPage(p);
                setClearing(false);
                setToClear(null);
              },
              (e: unknown) => {
                setClearing(false);
                setToClear(null);
                setError(asRequestError(e).message);
              },
            );
          }}
          onCancel={() => {
            setToClear(null);
          }}
        />
      )}
    </div>
  );
}
