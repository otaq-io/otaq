// The shown project's tracker plugins for the board (the otaq-marketplace README): the Import menu's entries and the
// dialog's words, refetched when plugins change.
import { useEffect, useState } from "react";

import type { ProjectPluginsView } from "../../generated/ProjectPluginsView";
import type { TrackerLink } from "../../generated/TrackerLink";
import { watchView } from "../../service";
import { request } from "../../tauri";

export interface ProjectPlugins {
  view: ProjectPluginsView | null;
  importFrom: { plugin: string; label: string }[];
}

/** The menu's entries: one "Import from <name>" per plugin turned on for the project. */
export function importEntries(view: ProjectPluginsView | null): { plugin: string; label: string }[] {
  if (view === null) return [];
  return view.plugins
    .filter((p) => p.enabled)
    .map((p) => ({ plugin: p.plugin, label: `${view.texts.import_title} ${p.name}` }));
}

export function useProjectPlugins(project: string | null): ProjectPlugins {
  const [view, setView] = useState<ProjectPluginsView | null>(null);
  useEffect(() => {
    if (project === null) return undefined;
    return watchView(
      () => request("plugins.project", { project }) as Promise<ProjectPluginsView>,
      ["plugins_changed"],
      (state) => {
        setView(state.view ?? null);
      },
    );
  }, [project]);
  // Another project's plugins never show while this one's load.
  const shown = project !== null && view !== null && view.project === project ? view : null;
  return { view: shown, importFrom: importEntries(shown) };
}

/** The cards' tracker badges by ticket key ("Jira · PAY-12"), refetched as tickets and plugins change. */
export function useTrackerBadges(project: string | null): ReadonlyMap<string, string> {
  const [links, setLinks] = useState<{ project: string; links: TrackerLink[] } | null>(null);
  useEffect(() => {
    if (project === null) return undefined;
    return watchView(
      () => request("plugins.links", { project }) as Promise<TrackerLink[]>,
      ["plugins_changed", "ticket_changed"],
      (state) => {
        if (state.view) setLinks({ project, links: state.view });
      },
    );
  }, [project]);
  const shown = links !== null && links.project === project ? links.links : [];
  return new Map(shown.map((l) => [l.ticket, `${l.tracker} · ${l.key}`]));
}
