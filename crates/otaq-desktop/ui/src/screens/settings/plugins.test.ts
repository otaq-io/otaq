// The plugin screens' pure parts (the otaq-marketplace README): a map setting as `name = value` lines and back, and the
// board's Import menu entries.
import { describe, expect, it } from "vitest";

import type { ProjectPluginsView } from "../../generated/ProjectPluginsView";
import { importEntries } from "../board/plugins";
import { mapText, parseMapText } from "./Plugins";

describe("plugin settings", () => {
  it("writes and reads a map setting as lines", () => {
    expect(mapText({ P1: "urgent", P2: "high" })).toBe("P1 = urgent\nP2 = high");
    expect(parseMapText("P1 = urgent\n\nbad line\n = x\nP2=high ")).toEqual({ P1: "urgent", P2: "high" });
    expect(mapText(null)).toBe("");
  });

  it("offers Import only for plugins turned on", () => {
    const view = {
      project: "SHOP",
      plugins: [
        { plugin: "jira", name: "Jira", enabled: true },
        { plugin: "github-issues", name: "GitHub Issues", enabled: false },
      ],
      texts: { import_title: "Import from" },
    } as unknown as ProjectPluginsView;
    expect(importEntries(view)).toEqual([{ plugin: "jira", label: "Import from Jira" }]);
    expect(importEntries(null)).toEqual([]);
  });
});
