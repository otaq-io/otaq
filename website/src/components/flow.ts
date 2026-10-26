// How it works: SHOP-29's six beats across the five lanes, from the canvas's `LANES`, `BEATS` and `renderVals`
// (Landing3.dc.html). Plain data and functions; HowItWorks.astro draws the first beat and player.ts the rest.

export const BEAT_MS = 4200;

export interface Lane {
  title: string;
  color: string;
}

export const LANES: readonly Lane[] = [
  { title: "Ready for Dev", color: "var(--st-queued)" },
  { title: "Dev", color: "var(--st-working)" },
  { title: "Security", color: "var(--st-investigate)" },
  { title: "Review", color: "var(--st-review)" },
  { title: "Done", color: "var(--st-queued)" },
];

interface Beat {
  lane: number;
  label: string;
  title: string;
  text: string;
  /** Pill class, icon, label, and "spin" when the icon turns. */
  pill: [string, string, string, string?];
  meta: string;
  /** The agent tool's initials on the card (its avatar class is av-<tool>). */
  tool: string;
  attn?: boolean;
  /** The bubble: initials (av-<initials>), who, what. */
  bub?: [string, string, string];
  /** SHOP-20, the ghost card, is working in Dev (it waited for SHOP-29). */
  g?: boolean;
}

const BEATS: readonly Beat[] = [
  {
    lane: 0,
    label: "Filed",
    title: "Noyan, your planning agent, files it",
    text: "You said “add a coupon field at checkout”. Noyan read the code and filed SHOP-29 with what it found. It starts in priority order, once an agent is free.",
    pill: ["p-q", "activity", "Ready for Dev"],
    meta: "filed by Noyan · P2",
    tool: "CC",
  },
  {
    lane: 1,
    label: "Started",
    title: "An agent starts, in its own worktree",
    text: "Otaq makes a worktree for the ticket, runs your setup command, and starts Rust developer in Claude Code with its skill. Other agents never touch this checkout.",
    pill: ["p-w", "loader", "Working", "spin"],
    meta: "otaq/SHOP-29 · 1m",
    tool: "CC",
  },
  {
    lane: 1,
    label: "Asks you",
    title: "It asks when the ticket doesn’t say",
    text: "Stack coupons, or one per order? The card turns turquoise and lands in the Needs you strip. One click answers it, and the answer is kept on the ticket.",
    pill: ["p-a", "message-q", "Asks you"],
    meta: "Stack coupons, or one per order?",
    tool: "CC",
    attn: true,
    bub: ["U", "You", "answered: one per order"],
  },
  {
    lane: 1,
    label: "Failed → fixed",
    title: "Security fails it. It goes back and gets fixed.",
    text: "Security reviewer finds the coupon code reaching SQL unescaped and fails the check. The flow says fails return to Dev, so the same agent fixes it in the same worktree. Round 2 passes. You weren’t needed.",
    pill: ["p-pa", "undo", "Back from Security · round 2"],
    meta: "unescaped SQL → parameterized",
    tool: "CC",
    bub: ["CC", "Security reviewer", "failed round 1 · passed round 2"],
  },
  {
    lane: 3,
    label: "Review",
    title: "You review the diff and approve",
    text: "The diff, the Security report, the recorded decision, and two manual checks the agents couldn’t do. You run it, read it, and approve.",
    pill: ["p-r", "eye", "Your check"],
    meta: "+184 −22 · 1 report",
    tool: "CC",
    attn: true,
    bub: ["U", "You", "approved · Local merge"],
  },
  {
    lane: 4,
    label: "Merged",
    title: "It merges. The next one starts.",
    text: "It merges into main. In a PR flow, this happens when you mark the PR merged. SHOP-20, which was waiting for SHOP-29, starts on its own. You stepped in twice.",
    pill: ["p-d", "git-merge", "Merged"],
    meta: "squash 4f1c9e2",
    tool: "CC",
    g: true,
  },
];

export const BEAT_COUNT = BEATS.length;

/** The step buttons' labels, in order. */
export const STEP_LABELS: readonly string[] = BEATS.map((b) => b.label);

/** The player: which beat shows (0-based) and whether it moves on by itself. */
export interface Player {
  beat: number;
  playing: boolean;
}

/** The timer's tick: the next beat while playing, wrapping from the last to the first. */
export function tick(p: Player): Player {
  return p.playing ? { ...p, beat: (p.beat + 1) % BEAT_COUNT } : p;
}

/** A click on a step: jump there (the caller restarts the timer). */
export function jump(p: Player, beat: number): Player {
  return { ...p, beat: Math.min(Math.max(0, Math.trunc(beat)), BEAT_COUNT - 1) };
}

/** Pause / Play. */
export function toggle(p: Player): Player {
  return { ...p, playing: !p.playing };
}

/** A lane's left edge on the stage, as the canvas's `laneLeft`. */
export function laneLeft(i: number): string {
  return `calc(10px + (100% - 20px - 32px) / 5 * ${String(i)} + 8px * ${String(i)})`;
}

export interface Pill {
  pillClass: string;
  icon: string;
  label: string;
  spin: boolean;
}

/** Everything the stage and the text under it show for one beat (the canvas's `fx`, lanes and steps). */
export interface BeatView {
  /** "1 of 6" … */
  n: string;
  title: string;
  text: string;
  /** Which of the five lanes are lit. */
  lanesOn: boolean[];
  card: Pill & { left: string; attn: boolean; tool: string; meta: string };
  ghost: Pill & { left: string };
  bubble: { left: string; av: string; who: string; text: string } | null;
  steps: { cls: "on" | "past" | ""; current: boolean }[];
  /** The progress bar's animation, alternating so it restarts on every beat. */
  bar: "a" | "b";
}

export function beatView(beat: number): BeatView {
  const b = BEATS[beat] ?? BEATS[0];
  if (b === undefined) throw new Error("no beats");
  const g = b.g === true;
  return {
    n: `${String(beat + 1)} of ${String(BEAT_COUNT)}`,
    title: b.title,
    text: b.text,
    lanesOn: LANES.map((_, i) => i === b.lane || (g && i === 1)),
    card: {
      left: laneLeft(b.lane),
      attn: b.attn === true,
      tool: b.tool,
      pillClass: b.pill[0],
      icon: b.pill[1],
      label: b.pill[2],
      spin: b.pill[3] === "spin",
      meta: b.meta,
    },
    ghost: {
      left: laneLeft(g ? 1 : 0),
      pillClass: g ? "p-w" : "p-q",
      icon: g ? "loader" : "hourglass",
      label: g ? "Working" : "waits for SHOP-29",
      spin: g,
    },
    bubble: b.bub
      ? { left: laneLeft(Math.min(b.lane, 3)), av: b.bub[0], who: b.bub[1], text: b.bub[2] }
      : null,
    steps: BEATS.map((_, i) => ({ cls: i === beat ? "on" : i < beat ? "past" : "", current: i === beat })),
    bar: beat % 2 ? "b" : "a",
  };
}
