import type { CSSProperties } from "react";

/** One of the demo's `d-*` icons (./icons.ts). Decorative: screen readers skip it (WEB-06). */
export function Ic({ name, cls = "", style }: { name: string; cls?: string; style?: CSSProperties }) {
  return (
    <svg className={("ic " + cls).trim()} style={style} aria-hidden="true" focusable="false">
      <use href={"#d-" + name}></use>
    </svg>
  );
}
