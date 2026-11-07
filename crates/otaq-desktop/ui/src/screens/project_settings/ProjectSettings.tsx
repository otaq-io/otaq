// The project's Settings page (the view switch's Settings): everything about this repository on one page in the
// project's main area. On the left the project's name and folder over the section list, which follows the scroll and
// jumps to a card on a click, and the line saying where app settings are; on the right one card per section, each
// drawn by the component registered for its id (`app.settings.project`). Every word and row comes from Rust
// (`project_settings.view`); an edit (`project_settings.edit`) answers with the fresh view.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { useRegistries } from "../../app/registry";
import type { ProjectSectionId } from "../../generated/ProjectSectionId";
import type { ProjectSettingsView } from "../../generated/ProjectSettingsView";
import type { RemoveProjectDialog } from "../../generated/RemoveProjectDialog";
import type { RequestError } from "../../generated/RequestError";
import type { SettingsButtonAction } from "../../generated/SettingsButtonAction";
import type { SettingsValue } from "../../generated/SettingsValue";
import { asRequestError, watchView } from "../../service";
import { request } from "../../tauri";
import { SettingsContext, type SettingsContextValue } from "../settings/context";
import { RemoveDialog } from "../settings/Dialogs";
import { activeCard } from "./logic";

/** The service events after which the page is fetched again: a setting, the project, its warm build, its flow. */
const REFETCH_ON = ["settings_changed", "projects_changed", "warm_build_changed", "flow_changed"];

export interface ProjectSettingsProps {
  project: string;
  /** The section asked for from outside, counted: each new count scrolls to it once. */
  wanted?: { section: ProjectSectionId | null; n: number } | null;
}

export function ProjectSettings({ project, wanted = null }: ProjectSettingsProps) {
  const app = useRegistries();
  const [view, setView] = useState<ProjectSettingsView | null>(null);
  const [error, setError] = useState<RequestError | null>(null);
  const [remove, setRemove] = useState<RemoveProjectDialog | null>(null);
  const [active, setActive] = useState(0);
  const body = useRef<HTMLDivElement>(null);

  const load = useCallback(
    () => request("project_settings.view", { project }) as Promise<ProjectSettingsView>,
    [project],
  );
  useEffect(
    () =>
      watchView(load, REFETCH_ON, (state) => {
        if (state.view) setView(state.view);
        setError(state.error ?? null);
      }),
    [load],
  );
  // Another project's page is never drawn while this one loads.
  const shown = view?.key === project ? view : null;

  const failed = useCallback((e: unknown) => {
    setError(asRequestError(e));
  }, []);
  const edit = useCallback(
    (field: string, value: SettingsValue) => {
      request("project_settings.edit", { project, field, value }).then((fresh) => {
        setView(fresh as ProjectSettingsView);
        setError(null);
      }, failed);
    },
    [project, failed],
  );
  const act = useCallback(
    (action: SettingsButtonAction, target?: string | null) => {
      switch (action) {
        // "Set by Dev on SHOP-27 · Undo" (agents run the app): the page refetches on the `projects_changed` it sends.
        case "undo_run_command":
          request("run.command_undo", { project, app: target ?? null }).catch(failed);
          return;
        // Add app and Remove app (agents run the app): the row's own field, sent as an edit.
        case "edit_field":
          if (target) edit(target, "");
          return;
        case "remove_project":
          request("project_settings.remove_preview", { project }).then((d) => {
            setRemove(d as RemoveProjectDialog);
          }, failed);
          return;
        // The app's own rows: never on a project's page.
        case "cleanup":
        case "credits":
        case "allow_notifications":
        case "open_system_settings":
          return;
      }
    },
    [project, edit, failed],
  );

  const page = shown?.page;
  const context = useMemo<SettingsContextValue | null>(
    () => (page ? { page, project, edit, act } : null),
    [page, project, edit, act],
  );
  const cards = useMemo(
    () =>
      (shown?.nav ?? []).flatMap((item) => {
        const entry = app.settings.project.get(item.id);
        return entry ? [{ id: item.id, label: item.label, Component: entry.Component }] : [];
      }),
    [shown, app],
  );

  const cardElements = useCallback(
    () => Array.from(body.current?.querySelectorAll<HTMLElement>(":scope > .project-settings-card") ?? []),
    [],
  );
  const follow = useCallback(() => {
    const box = body.current;
    if (!box) return;
    const atBottom = box.scrollTop + box.clientHeight >= box.scrollHeight - 2;
    setActive(
      activeCard(
        cardElements().map((c) => c.offsetTop),
        box.scrollTop,
        atBottom,
      ),
    );
  }, [cardElements]);
  const jump = (i: number) => {
    const box = body.current;
    const card = cardElements()[i];
    if (!box || !card) return;
    box.scrollTo({ top: i === 0 ? 0 : card.offsetTop });
    setActive(i);
  };

  // Opened at a section from outside: scroll to it once its card is drawn.
  const scrolled = useRef(0);
  useLayoutEffect(() => {
    if (!wanted || wanted.n === scrolled.current || cards.length === 0) return;
    scrolled.current = wanted.n;
    const i = wanted.section === null ? 0 : cards.findIndex((c) => c.id === wanted.section);
    const box = body.current;
    const card = cardElements()[Math.max(0, i)];
    if (box && card) box.scrollTop = i <= 0 ? 0 : card.offsetTop;
    follow();
  }, [wanted, cards, cardElements, follow]);

  if (!shown || !context)
    return (
      <div className="project-settings" aria-busy={!error || undefined}>
        {error && (
          <p className="settings-problem" role="alert">
            {error.message}
          </p>
        )}
      </div>
    );
  return (
    <div className="project-settings">
      <nav className="project-settings-nav" aria-label={shown.name}>
        <div className="project-settings-head">
          <span className="project-settings-name">{shown.name}</span>
          <span className="project-settings-path">{shown.path}</span>
        </div>
        {cards.map((card, i) => (
          <button
            key={card.id}
            type="button"
            className="project-settings-nav-item"
            data-section={card.id}
            aria-current={i === active || undefined}
            onClick={() => {
              jump(i);
            }}
          >
            {card.label}
          </button>
        ))}
        <p className="project-settings-note">{shown.app_settings}</p>
      </nav>
      <div ref={body} className="project-settings-body" onScroll={follow}>
        {error && (
          <p className="settings-error" role="alert">
            {error.message}
          </p>
        )}
        <SettingsContext value={context}>
          {cards.map(({ id, Component }) => (
            <div key={`${project}-${id}`} className="project-settings-card" data-section={id}>
              <Component project={project} />
            </div>
          ))}
        </SettingsContext>
      </div>
      {remove !== null && (
        <RemoveDialog
          dialog={remove}
          onCancel={() => {
            setRemove(null);
          }}
          onConfirm={() => {
            setRemove(null);
            request("project_settings.remove", { project }).catch(failed);
          }}
        />
      )}
    </div>
  );
}
