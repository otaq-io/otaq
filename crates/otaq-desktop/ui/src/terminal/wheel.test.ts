import { describe, expect, it } from "vitest";

import { NOTCH_SENSITIVITY, SCROLL_SENSITIVITY, wheelSensitivity } from "./wheel";

describe("wheelSensitivity", () => {
  it("tells a notch's fixed-point line count from a trackpad's whole pixels", () => {
    // Measured in WKWebView: a 0.1-line notch is 3.9996 px down, a 1-line notch 40, 4 px of trackpad 4.
    expect(wheelSensitivity({ deltaY: 3.9996337890625, deltaMode: 0 })).toBe(NOTCH_SENSITIVITY);
    expect(wheelSensitivity({ deltaY: -8.00048828125, deltaMode: 0 })).toBe(NOTCH_SENSITIVITY);
    expect(wheelSensitivity({ deltaY: 40, deltaMode: 0 })).toBe(SCROLL_SENSITIVITY);
    expect(wheelSensitivity({ deltaY: -4, deltaMode: 0 })).toBe(SCROLL_SENSITIVITY);
    expect(wheelSensitivity({ deltaY: 0, deltaMode: 0 })).toBe(SCROLL_SENSITIVITY);
    // Lines or pages (not WebKit's): left to xterm.js.
    expect(wheelSensitivity({ deltaY: 0.5, deltaMode: 1 })).toBe(SCROLL_SENSITIVITY);
  });
});
