// Your flow: the presets the visitor flips between, from the canvas's `PRESETS` (Landing3.dc.html). Flow.astro draws
// every preset's track ("Your own" first, shown); the preset picker script shows one at a time.

/** What a stage is, by its colour: an agent works, a check, you check, a delivery, or a column that waits. */
export type StageKind = "work" | "check" | "you" | "deliver" | "wait";

export interface Stage {
  name: string;
  kind: StageKind;
  /** The agent on it ("You" on a stage you check). */
  who?: string;
  /** The stage a failed check returns to. */
  fail?: string;
  /** A short line under the name, for stages without an agent. */
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

const READY: Stage = { name: "Ready for Dev", kind: "wait", note: "starts in priority order" };
const DEV: Stage = { name: "Dev", kind: "work", who: "Developer" };
const VALIDATE: Stage = { name: "Validate", kind: "check", who: "Validator", fail: "Dev" };
const DONE: Stage = { name: "Done", kind: "wait", note: "merged" };

export const PRESETS: readonly Preset[] = [
  {
    id: "blank",
    name: "Your own",
    desc: "Start blank and add stages",
    stages: [
      READY,
      { name: "Add a stage", kind: "wait", add: true, note: "an agent works · a check · a delivery" },
      { name: "Add a stage", kind: "wait", add: true, note: "as many as you need, in your order" },
      { name: "Done", kind: "wait", note: "when your last stage says so" },
    ],
  },
  {
    id: "default",
    name: "Otaq default",
    desc: "Agents build and check; you review",
    stages: [
      READY,
      DEV,
      VALIDATE,
      { name: "Review", kind: "you", who: "You", fail: "Dev" },
      { name: "Ship", kind: "deliver", note: "local merge, on your click" },
      DONE,
    ],
  },
  {
    id: "solo",
    name: "Solo autopilot",
    desc: "No review; merges when checks pass",
    stages: [
      READY,
      DEV,
      VALIDATE,
      { name: "Ship", kind: "deliver", note: "merges on its own once Validate passes" },
      DONE,
    ],
  },
  {
    id: "team",
    name: "Team PR",
    desc: "Your team reviews the PR",
    stages: [
      READY,
      DEV,
      VALIDATE,
      { name: "Ship", kind: "deliver", note: "opens a PR with gh · your team reviews it" },
      { name: "Done", kind: "wait", note: "when you mark the PR merged" },
    ],
  },
  {
    id: "spec",
    name: "Spec first",
    desc: "Plan on the board, then build",
    stages: [
      { name: "Product spec", kind: "work", who: "Spec writer" },
      { name: "Tech spec", kind: "work", who: "Architect" },
      { name: "Milestone plan", kind: "work", who: "Milestone planner" },
      { name: "Spec review", kind: "you", who: "You", fail: "Product spec" },
      DEV,
      VALIDATE,
      { name: "Code review", kind: "you", who: "You", fail: "Dev" },
      { name: "Ship", kind: "deliver", note: "local merge, on your click" },
      DONE,
    ],
  },
];

/** A track with more than six stages is drawn narrower, without arrows, so it fits without scrolling. */
export function isLong(preset: Preset): boolean {
  return preset.stages.length > 6;
}
