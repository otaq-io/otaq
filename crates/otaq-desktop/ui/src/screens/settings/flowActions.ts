// The Flow card's actions (FLW-07…FLW-11, FED-09): what each button asks of Rust, and where the user goes next. Edit
// flow opens the flow editor on the project's flow (d0115); every other action but Export puts its flow in the
// editor's draft (`settings.flow_propose`) and opens the editor on it. Export and Import go through the system's save
// and open dialogs. Nothing starts until the user asks the editor's agent.
import type { FlowSettings } from "../../generated/FlowSettings";
import type { FlowSource } from "../../generated/FlowSource";
import { pickFlowFile, request, saveFlowFile } from "../../tauri";
import { changeFlowOpen, openFlowEditor } from "../flow_editor/logic";
import { AGENT_PANEL_ROUTES, type AgentPanelRoutes } from "../agent_panel/logic";
import { selectProject } from "../../app/selection";

export interface FlowRoutes {
  ask: typeof request;
  /** Opens the flow editor on the project's flow (its draft, when an action put one there). */
  openEditor: (project: string) => void;
  pickFile: typeof pickFlowFile;
  saveFile: typeof saveFlowFile;
  agentPanel: AgentPanelRoutes;
}

export const FLOW_ROUTES: FlowRoutes = {
  ask: request,
  openEditor: (project) => {
    openFlowEditor(changeFlowOpen(project));
  },
  pickFile: (title) => pickFlowFile(title),
  saveFile: (title, fileName) => saveFlowFile(title, fileName),
  agentPanel: AGENT_PANEL_ROUTES,
};

/** The draft from `source` (a preset, another project, the previous flow, the default, the shipped agents file), then
 * the editor on it. A refusal rejects, and the page shows it. */
export async function proposeFrom(
  project: string,
  source: FlowSource,
  routes: FlowRoutes = FLOW_ROUTES,
): Promise<void> {
  await routes.ask("settings.flow_propose", { project, source });
  routes.openEditor(project);
}

/** Import flow…: the open dialog, then the editor on the file's flow. Cancelling the dialog does nothing. */
export async function importFlow(section: FlowSettings, routes: FlowRoutes = FLOW_ROUTES): Promise<void> {
  const path = await routes.pickFile(section.import_title);
  if (path === null) return;
  await routes.ask("settings.flow_import", { project: section.project, path });
  routes.openEditor(section.project);
}

/** Export: the save dialog offering "shop-web-flow.yaml", then Rust writes the flow's file there. Whether a file was
 * saved. */
export async function exportFlow(section: FlowSettings, routes: FlowRoutes = FLOW_ROUTES): Promise<boolean> {
  const path = await routes.saveFile(section.export_title, section.export_file);
  if (path === null) return false;
  await routes.ask("settings.flow_export", { project: section.project, path });
  return true;
}

/** Edit flow: the flow editor opens on the project; nothing starts. */
export function changeFlow(section: FlowSettings, routes: FlowRoutes = FLOW_ROUTES): void {
  selectProject(section.project, routes.agentPanel.selection);
  routes.openEditor(section.project);
}

/** The ⋯ menu's choice, by its id from Rust (Use a preset… and Copy from another project… open their own lists). */
export function menuChoice(
  section: FlowSettings,
  id: string,
  routes: FlowRoutes = FLOW_ROUTES,
): Promise<unknown> {
  switch (id) {
    case "import":
      return importFlow(section, routes);
    case "reset":
      return proposeFrom(section.project, { from: "default" }, routes);
    default:
      return Promise.resolve();
  }
}
