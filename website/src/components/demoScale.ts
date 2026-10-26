// The live demo's frame: the 1280 × 800 board scaled to the page width, as the canvas's script does
// (Landing3.dc.html: gutter 16 px below 760 px wide, else 32; at most the content width, 1136 px = 1200 − 2 × 32;
// view-only below 900 px). Decision 0138 §3, WEB-05.

export const DEMO_WIDTH = 1280;
export const DEMO_HEIGHT = 800;
/** The widest the board is drawn: the page's content width. */
export const DEMO_MAX = 1136;

export interface DemoFrame {
  /** The frame's width and height in px, rounded. */
  width: number;
  height: number;
  /** The board's scale. */
  scale: number;
  /** Too narrow to click around: the board ignores the pointer and the note shows. */
  viewOnly: boolean;
}

export function demoFrame(viewportWidth: number): DemoFrame {
  const gutter = viewportWidth < 760 ? 16 : 32;
  const available = Math.max(280, Math.min(DEMO_MAX, viewportWidth - gutter * 2));
  const scale = available / DEMO_WIDTH;
  return {
    width: Math.round(available),
    height: Math.round(DEMO_HEIGHT * scale),
    scale,
    viewOnly: viewportWidth < 900,
  };
}
