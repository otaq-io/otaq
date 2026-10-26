import { describe, expect, it } from "vitest";

import { demoFrame } from "./demoScale";
import { BEAT_COUNT, beatView, jump, tick, toggle } from "./flow";
import { FAQS } from "./faq";
import { KIND_COLOR, PRESETS, isLong } from "./presets";
import { STORAGE_KEY, nextTheme, readPick, savePick, shownTheme } from "./theme";

describe("How it works player", () => {
  it("has 8 beats and the timer wraps from 8 to 1", () => {
    expect(BEAT_COUNT).toBe(8);
    let p = { beat: 0, playing: true };
    for (let i = 1; i < 8; i++) {
      p = tick(p);
      expect(p.beat).toBe(i);
    }
    expect(tick(p)).toEqual({ beat: 0, playing: true });
  });

  it("jumps to a clicked step and keeps playing or paused as it was", () => {
    expect(jump({ beat: 1, playing: true }, 5)).toEqual({ beat: 5, playing: true });
    expect(jump({ beat: 6, playing: false }, 2)).toEqual({ beat: 2, playing: false });
  });

  it("stays on its beat while paused, and Play carries on from there", () => {
    const paused = toggle({ beat: 3, playing: true });
    expect(paused).toEqual({ beat: 3, playing: false });
    expect(tick(paused)).toBe(paused);
    expect(tick(toggle(paused))).toEqual({ beat: 4, playing: true });
  });

  it("draws the beats as the canvas does", () => {
    const first = beatView(0);
    expect(first.n).toBe("1 of 8");
    expect(first.lanesOn).toEqual([true, false, false, false, false, false]);
    expect(first.ghost.label).toBe("Backlog");
    expect(first.bubble?.who).toBe("Project agent");
    expect(first.bar).toBe("a");

    const asks = beatView(3);
    expect(asks.card.attn).toBe(true);
    expect(asks.bubble).toMatchObject({ avClass: "p-q", initials: "U", who: "You" });
    expect(asks.steps.map((s) => s.cls)).toEqual(["past", "past", "past", "on", "", "", "", ""]);
    expect(asks.bar).toBe("b");

    expect(beatView(1).bubble).toBeNull();
    expect(beatView(1).ghost.label).toBe("waits for NOTE-14");

    const last = beatView(7);
    expect(last.lanesOn).toEqual([false, false, true, false, false, true]);
    expect(last.ghost).toMatchObject({ pillClass: "p-w", label: "Working" });
    expect(last.card.pillLabel).toBe("Merged");
  });
});

describe("demoFrame", () => {
  it.each([
    [360, { width: 328, height: 205, viewOnly: true }],
    [800, { width: 736, height: 460, viewOnly: true }],
    [1000, { width: 936, height: 585, viewOnly: false }],
    [1440, { width: 1280, height: 800, viewOnly: false }],
  ])("at %i px", (vw, frame) => {
    const f = demoFrame(vw);
    expect(f).toMatchObject(frame);
    expect(f.scale).toBeCloseTo(frame.width / 1280, 6);
  });
});

describe("theme", () => {
  it("flips to the other theme than the one shown", () => {
    expect(nextTheme("dark")).toBe("light");
    expect(nextTheme("light")).toBe("dark");
  });

  it("shows the pick, else the system setting", () => {
    expect(shownTheme(null, false)).toBe("dark");
    expect(shownTheme(null, true)).toBe("light");
    expect(shownTheme("dark", true)).toBe("dark");
    expect(shownTheme("light", false)).toBe("light");
  });

  it("reads a stored pick under otaq-theme", () => {
    const keys: string[] = [];
    const read = (key: string): string | null => {
      keys.push(key);
      return "light";
    };
    expect(readPick(read)).toBe("light");
    expect(keys).toEqual([STORAGE_KEY]);
    expect(STORAGE_KEY).toBe("otaq-theme");
  });

  it.each([
    ["missing", null],
    ["invalid", "solarized"],
    ["empty", ""],
  ])("treats a %s value as no pick", (_name, value) => {
    expect(readPick(() => value)).toBeNull();
  });

  it("treats storage that throws as no pick", () => {
    expect(
      readPick(() => {
        throw new Error("SecurityError");
      }),
    ).toBeNull();
  });

  it("saves the pick, and survives storage that throws", () => {
    const saved: [string, string][] = [];
    expect(savePick((key, value) => saved.push([key, value]), "dark")).toBe(true);
    expect(saved).toEqual([["otaq-theme", "dark"]]);
    expect(
      savePick(() => {
        throw new Error("QuotaExceededError");
      }, "light"),
    ).toBe(false);
  });
});

describe("presets", () => {
  it("has the five drawn presets, Your own first", () => {
    expect(PRESETS.map((p) => p.name)).toEqual([
      "Your own",
      "Otaq default",
      "Solo autopilot",
      "Team PR",
      "Spec first",
    ]);
    expect(new Set(PRESETS.map((p) => p.id)).size).toBe(PRESETS.length);
  });

  it("gives each preset its drawn stages", () => {
    expect(PRESETS.map((p) => p.stages.length)).toEqual([4, 6, 5, 5, 9]);
    expect(PRESETS.map(isLong)).toEqual([false, false, false, false, true]);
    expect(PRESETS[0]?.stages.filter((s) => s.add)).toHaveLength(2);
  });

  it("draws each stage in its kind's colour", () => {
    const kinds = (id: string): string[] => PRESETS.find((p) => p.id === id)?.stages.map((s) => s.kind) ?? [];
    expect(kinds("default")).toEqual(["wait", "work", "check", "you", "deliver", "wait"]);
    expect(kinds("spec")).toEqual(["work", "work", "work", "you", "work", "check", "you", "deliver", "wait"]);
    expect(KIND_COLOR.check).toBe("var(--st-investigate)");
  });

  it("sends every failed check back to a stage of the same preset", () => {
    for (const p of PRESETS) {
      const names = p.stages.map((s) => s.name);
      for (const s of p.stages) if (s.fail) expect(names).toContain(s.fail);
    }
  });
});

describe("faq", () => {
  it("has the seven drawn questions, tmux first", () => {
    expect(FAQS).toHaveLength(7);
    expect(FAQS[0]?.q).toBe("Why not tmux, a few worktrees and a script?");
    expect(FAQS.at(-1)?.q).toBe("What leaves my machine?");
    for (const f of FAQS) expect(f.a.length).toBeGreaterThan(0);
  });
});
