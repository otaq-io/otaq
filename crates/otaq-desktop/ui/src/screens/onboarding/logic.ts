// Onboarding's moment of interaction, without the DOM: which step shows, what the Add project form holds, and what
// adding a project asks of the app. Every word and rule is Rust's (`onboarding_page.rs`, `onboarding.rs`,
// `add_project.rs`); this file only moves between them.
import { selectProject } from "../../app/selection";
import type { AddProjectFlowPick } from "../../generated/AddProjectFlowPick";
import type { AddProjectParams } from "../../generated/AddProjectParams";
import type { AddProjectView } from "../../generated/AddProjectView";
import type { OnboardingStepId } from "../../generated/OnboardingStepId";
import type { OnboardingTools } from "../../generated/OnboardingTools";
import { AGENT_PANEL_ROUTES, type AgentPanelRoutes } from "../agent_panel/logic";
import { openFlowBoard, suggest } from "../flow_board/logic";

/** The door's request call, or a fake one in the tests. */
export type Ask = (method: string, params?: unknown) => Promise<unknown>;

/** The steps in order; the board after Project is not a step (ONB-10). */
export const STEP_ORDER: readonly OnboardingStepId[] = ["welcome", "tools", "project"];

/** Where Continue on the Tools step goes: the Project step, or the end for a user who has projects already. */
export function afterTools(tools: OnboardingTools): "project" | "done" {
  return tools.has_projects ? "done" : "project";
}

/** A fresh form, with a folder already picked or dropped if given. */
export function newForm(firstRun: boolean, folder: string | null = null): AddProjectParams {
  return { folder, key: "", key_edited: false, base_branch: "", base_edited: false, first_run: firstRun };
}

/** The key field as typed; Rust normalizes it (uppercase, trimmed) and the next answer shows that. */
export function typeKey(form: AddProjectParams, key: string): AddProjectParams {
  return { ...form, key, key_edited: true };
}

export function typeBase(form: AddProjectParams, base: string): AddProjectParams {
  return { ...form, base_branch: base, base_edited: true };
}

export function pickFolder(form: AddProjectParams, folder: string): AddProjectParams {
  return { ...form, folder };
}

/** The dialog's Flow choice. */
export function pickFlow(form: AddProjectParams, flow: AddProjectFlowPick): AddProjectParams {
  return { ...form, flow };
}

/** The Flow option picked: Otaq default until the user picks another. */
export function flowPicked(form: AddProjectParams): AddProjectFlowPick {
  return form.flow ?? { kind: "default" };
}

/** Whether two Flow picks are the same option. */
export function sameFlowPick(a: AddProjectFlowPick, b: AddProjectFlowPick): boolean {
  if (a.kind === "same_as" && b.kind === "same_as") return a.project === b.project;
  return a.kind === b.kind;
}

/** The key field's text: the user's once they typed, else Rust's suggestion. */
export function keyText(form: AddProjectParams, view: AddProjectView | undefined): string {
  return form.key_edited ? form.key : (view?.key?.value ?? "");
}

/**
 * After Rust answered `asked`: the form with the key as Rust spells it (uppercase, trimmed), if the user hasn't typed
 * on meanwhile; otherwise `form` unchanged.
 */
export function adoptKey(
  form: AddProjectParams,
  asked: AddProjectParams,
  view: AddProjectView,
): AddProjectParams {
  const spelled = view.key?.value;
  if (form !== asked || !form.key_edited || spelled === undefined || spelled === form.key) return form;
  return { ...form, key: spelled };
}

export function baseText(form: AddProjectParams, view: AddProjectView | undefined): string {
  return form.base_edited ? form.base_branch : (view?.base?.value ?? "");
}

/** The base branch as its dropdown shows it: a remote branch by its short name (`origin/next` → `next`). */
export function baseLabel(form: AddProjectParams, view: AddProjectView | undefined): string {
  const id = baseText(form, view);
  return view?.base?.items.find((i) => !i.header && i.id === id)?.label ?? id;
}

/** Where a project just added goes: its board, and for "Suggest one", Edit flow with Noyan's agent drafting. */
export interface AddedRoutes {
  select: (project: string) => void;
  editFlow: (project: string) => void;
  suggest: (project: string) => Promise<unknown>;
}

export const ADDED_ROUTES: AddedRoutes = {
  select: (project) => {
    selectProject(project, AGENT_PANEL_ROUTES.selection);
  },
  editFlow: (project) => {
    openFlowBoard({ project });
  },
  suggest: (project) => suggest(project),
};

/**
 * A project was added (onboarding's Project step or the dialog): its board shows, on Otaq default or the flow it
 * copied. "Suggest one" opens Edit flow on it while Noyan's agent drafts a suggestion; the board stays on the default
 * until the user saves one.
 */
export function added(
  project: string,
  flow: AddProjectFlowPick = { kind: "default" },
  routes: AddedRoutes = ADDED_ROUTES,
): void {
  routes.select(project);
  if (flow.kind !== "suggest") return;
  routes.editFlow(project);
  routes.suggest(project).catch(() => undefined);
}

/** Kept for callers that only select: a project added later is selected, so its board shows. */
export function addedLater(project: string, routes: AgentPanelRoutes = AGENT_PANEL_ROUTES): void {
  selectProject(project, routes.selection);
}
