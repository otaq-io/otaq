// The theme switch's pure part (WEB-11, d0138 §1): which theme is shown, which one the switch flips to, and the
// visitor's pick in storage. ThemeToggle.astro wires it to the page; Base.astro's head script applies the pick
// before paint with the same key and values.

export type Theme = "dark" | "light";

/** The localStorage key of the visitor's pick: "dark" or "light"; absent = follow the system. */
export const STORAGE_KEY = "otaq-theme";

export function isTheme(value: unknown): value is Theme {
  return value === "dark" || value === "light";
}

/** The switch always flips to the other theme than the one shown. */
export function nextTheme(shown: Theme): Theme {
  return shown === "dark" ? "light" : "dark";
}

/** The theme on screen: the pick when there is one, else the system setting. */
export function shownTheme(pick: Theme | null, systemLight: boolean): Theme {
  return pick ?? (systemLight ? "light" : "dark");
}

/** The stored pick; null when it is missing, not a theme, or storage throws (blocked, private mode). */
export function readPick(read: (key: string) => string | null): Theme | null {
  try {
    const value = read(STORAGE_KEY);
    return isTheme(value) ? value : null;
  } catch {
    return null;
  }
}

/** Stores the pick; false when storage throws (the pick then lasts for the visit). */
export function savePick(write: (key: string, value: string) => void, theme: Theme): boolean {
  try {
    write(STORAGE_KEY, theme);
    return true;
  } catch {
    return false;
  }
}
