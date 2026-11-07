// Your flow: the presets the visitor flips between, from the canvas's `PRESETS` (Landing3.dc.html), drawn as the app
// draws a flow: Backlog and Done at the fixed ends, Done carrying how work leaves the flow. Flow.astro draws every
// preset's track ("Your own" first, shown); the preset picker script shows one at a time.

/** What a column is, by its colour: an agent works, an agent checks, you check, Done (how work leaves the flow), or
 * Backlog, where tickets wait to start. */
export type StageKind = "work" | "check" | "you" | "deliver" | "wait";

export interface Stage {
  name: string;
  kind: StageKind;
  /** The agent on it ("You" on a stage you check). */
  who?: string;
  /** The stage a failed check returns to. */
  fail?: string;
  /** A short line under the name: what Backlog does, or how Done delivers. */
  note?: string;
  /** A dashed "Add a stage" placeholder. */
  add?: boolean;
}

export interface Preset {
  id: string;
  name: string;
  desc: string;
  stages: readonly Stage[];
}

export const KIND_COLOR: Readonly<Record<StageKind, string>> = {
  work: "var(--st-working)",
  check: "var(--st-investigate)",
  you: "var(--st-review)",
  deliver: "var(--st-approved)",
  wait: "var(--st-queued)",
};

// Backlog and Done are every flow's fixed ends; Done's line is the flow's delivery, in the app's words.
const BACKLOG: Stage = { name: "Backlog", kind: "wait", note: "Start puts a ticket into the first stage" };
const DEV: Stage = { name: "Dev", kind: "work", who: "Developer" };
const VALIDATE: Stage = { name: "Validate", kind: "check", who: "Checker", fail: "Dev" };
const done = (note: string): Stage => ({ name: "Done", kind: "deliver", note });
const ON_APPROVE = "merge or PR · on Approve";

export const PRESETS: readonly Preset[] = [
  {
    id: "blank",
    name: "Your own",
    desc: "Start blank and add stages",
    stages: [
      BACKLOG,
      { name: "Add a stage", kind: "wait", add: true, note: "an agent works · an agent checks · you check" },
      { name: "Add a stage", kind: "wait", add: true, note: "as many as you need, in your order" },
      done(ON_APPROVE),
    ],
  },
  {
    id: "default",
    name: "Otaq default",
    desc: "Agents build and check; you review",
    stages: [
      BACKLOG,
      DEV,
      VALIDATE,
      { name: "Review", kind: "you", who: "You", fail: "Dev" },
      done(ON_APPROVE),
    ],
  },
  {
    id: "solo",
    name: "Solo autopilot",
    desc: "No review; merges when checks pass",
    stages: [BACKLOG, DEV, VALIDATE, done("merges by itself once Validate passes")],
  },
  {
    id: "team",
    name: "Team PR",
    desc: "Your team reviews the PR",
    stages: [BACKLOG, DEV, VALIDATE, done("opens a PR by itself · you mark it merged")],
  },
  {
    id: "spec",
    name: "Spec first",
    desc: "Plan on the board, then build",
    stages: [
      BACKLOG,
      { name: "Product spec", kind: "work", who: "Spec writer" },
      { name: "Tech spec", kind: "work", who: "Architect" },
      { name: "Milestone plan", kind: "work", who: "Milestone planner" },
      { name: "Spec review", kind: "you", who: "You", fail: "Product spec" },
      DEV,
      VALIDATE,
      { name: "Code review", kind: "you", who: "You", fail: "Dev" },
      done(ON_APPROVE),
    ],
  },
];

/** A track with more than six stages is drawn narrower, without arrows, so it fits without scrolling. */
export function isLong(preset: Preset): boolean {
  return preset.stages.length > 6;
}
