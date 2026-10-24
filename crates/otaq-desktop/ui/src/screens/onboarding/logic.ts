// Onboarding's moment of interaction, without the DOM: which step shows, what the Add project form holds, and what
// ending onboarding asks of the app. Every word and rule is Rust's (`onboarding_page.rs`, `onboarding.rs`,
// `add_project.rs`); this file only moves between them. The Flow step is the flow editor (FED-09, `flow_editor/`).
import { selectProject } from "../../app/selection";
import type { AddProjectParams } from "../../generated/AddProjectParams";
import type { AddProjectView } from "../../generated/AddProjectView";
import type { OnboardingStepId } from "../../generated/OnboardingStepId";
import type { OnboardingTools } from "../../generated/OnboardingTools";
import { PANE_ROUTES, type PaneRoutes, showAgent, showPane } from "../pane/logic";

/** The door's request call, or a fake one in the tests. */
export type Ask = (method: string, params?: unknown) => Promise<unknown>;

/** The steps in order; the empty board follows Flow and is not a step (ONB-10). */
export const STEP_ORDER: readonly OnboardingStepId[] = ["welcome", "theme", "tools", "project", "flow"];

/**
 * Where Continue on the tools step goes: back to the Flow step when first run's project is still in Flow setup (Back
 * to Tools), else the project step, or the end for a user who has projects already.
 */
export function afterTools(
  tools: OnboardingTools,
  flowProject: string | null = null,
): "project" | "flow" | "done" {
  if (flowProject !== null) return "flow";
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

/**
 * Ending onboarding: the permission-mode banner when the warning was shown and not closed (T-0231's
 * `notices.permission_warning`). Returns whether it was raised.
 */
export async function finishOnboarding(warningShown: boolean, ask: Ask): Promise<boolean> {
  if (!warningShown) return false;
  await ask("notices.permission_warning");
  return true;
}

/** A project added later (Add project): it is selected, so its Flow setup page (the flow editor) shows. */
export function addedLater(project: string, routes: PaneRoutes = PANE_ROUTES): void {
  selectProject(project, routes.selection);
}

/**
 * The Flow step is over (Use this flow, Skip, use the default): the project's board shows, with the agent the editor's
 * panel showed (the flow agent whose flow was used stays) open in the Project agent panel.
 */
export function afterFlowStep(project: string, agent: string | null, routes: PaneRoutes = PANE_ROUTES): void {
  selectProject(project, routes.selection);
  if (agent !== null) {
    showAgent(project, agent, routes.shown);
    showPane(routes);
  }
}
