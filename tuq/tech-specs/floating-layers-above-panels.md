# Floating layers above the panels (OTAQ-3)

How the fix for "the Waits for picker is drawn behind the layout and the Noyan panel" is built. Builds on no product
decision: Product spec settled nothing (a layering bug; the expected behaviour is in the ticket). Desktop page code
only (`crates/otaq-desktop/ui/src`), no Rust change.

## The cause

Every floating layer (Popover, Menu, Tooltip, Hint, and the six components that call `useFloatingStyle` themselves)
is a `position: fixed` element drawn **inside its anchor's subtree** (`components/Floating.tsx`, header comment). Fixed
positioning escapes a scrolling or clipping parent, but not a parent's **stacking context**. `.frame-sheet`
(`app/frame.css`) is `position: absolute; z-index: 8`, so it is a stacking context, and a popover's own `z-index: 20`
counts only inside the sheet. The whole sheet, popover included, is then painted at 8:

- under the expanded Noyan panel body (`.frame-agent[data-expanded] .frame-agent-body`, z-index 9): "behind the Noyan
  panel";
- under the box for new work (`.frame-new-work`, z-index 8, later in the page): the "parts of the layout";
- and, for a layer whose own parent is a stacking context deeper in the sheet (`screens/sheet/sheet.css` z-index 1 and
  7, `editor.css` z-index 1), under the sheet's own sticky parts too.

The same holds for every floating layer opened from the ticket panel, not only this picker: the New ticket form's
Waits for picker, Fields' Add, the code block language picker, tooltips near the panel's edge.

## Decision: floating layers render into the overlay host, unless already inside a layer

The fix goes in the shared floating layer, so every popover, menu and tooltip gets it, not only this picker.

```tsx
// Popover.tsx (and Menu, Tooltip, Hint, and each direct user of useFloatingStyle)
return (
  <FloatingPortal>
    <div ref={floating} className="popover" role="dialog" aria-label={label} style={style}>
      {children}
    </div>
  </FloatingPortal>
);

// Dialog.tsx, the frame's Overlays, FullView: whatever is drawn into the overlay host
return inOverlayHost(modal);          // portals into #overlay-host and marks the subtree "inside a layer"
return inOverlayHost(view, slot);     // FullView: into its own slot in the host, same marking
```

The rule, in one place (`components/Floating.tsx`):

1. `FloatingPortal` reads a React context, `InLayer` (a boolean, default false).
2. **Not inside a layer** (a popover opened from the ticket sheet, the board, the header, the Noyan panel): it
   portals its child into `#overlay-host` (`OVERLAY_HOST_ID`, `app/Frame.tsx`), wrapped in `InLayer = true`. The host
   is `.frame-layer` (fixed, z-index 10), above the sheet (8), the expanded Noyan body (9) and the box for new work
   (8), and its `.popover` z-index 20 keeps it above a dialog drawn earlier in the same host.
3. **Already inside a layer** (a menu opened from a popover, a picker in a dialog, Settings, the full-window view):
   it renders inline, exactly as today. That keeps the DOM nesting the code relies on:
   - `usePressOutside` decides "inside" with `element.contains(target)`: a menu inside a popover must stay in the
     popover's DOM, or a press in the menu would close the popover;
   - `escape.ts` orders Esc layers by DOM containment ("a layer inside another's element is above it");
   - `Dialog`'s own keys skip a key layer inside its box (`inKeyLayer(e.target, box)`), and its Tab trap walks only
     its box.
4. **No host** (a unit test without the frame, the components gallery, a static render): inline, as today.
5. `inOverlayHost` moves from `Dialog.tsx` to `Floating.tsx` next to `FloatingPortal` (re-exported from
   `components/index.ts` as now) and always wraps its child in `InLayer = true`. Everything that draws into the
   overlay host goes through it: `Dialog`, the frame's `OverlayHost` (`<Overlays />` wrapped in the provider, since
   it renders into the host directly rather than through a portal), and `FullView`, whose `createPortal(…, slot)`
   becomes `inOverlayHost(…, slot)`. `Keyboard.tsx` and `Agents.tsx` portal into a header slot inside Settings, which
   is already inside the overlay host's subtree; they stay as they are.

React context and React synthetic events both follow the React tree through a portal, so a portaled popover still
gets its owner's providers, and the owner's `onKeyDown`/`onClick` handlers still hear it.

### Keeping every floating layer on the rule

Each of these renders its root through `FloatingPortal`: `Popover`, `Menu` (both its menus), `Tooltip`, `Hint`,
`markdown/code/CodeBlockView.tsx`, `cards/TicketCard.tsx`, `screens/settings/ModelPicker.tsx`,
`screens/strip/Strip.tsx` (two), `screens/board/Search.tsx`. So that the next one can't forget it, a test in
`components/Floating.test.ts` scans `src/**/*.tsx` and fails when a file that calls `useFloatingStyle(` doesn't also
use `FloatingPortal` (the "lists kept in step by hand" sign: the build fails when they disagree). The header comment
of `Floating.tsx` changes from "drawn inside the anchor's subtree, so the anchor's theme applies to it" to the rule
above; the theme is set on the root element (`data-theme`), so a portaled layer keeps it.

### Considered

- **The browser's top layer (the HTML `popover` attribute and `showPopover()`).** It lifts an element above every
  stacking context while leaving it in place in the DOM, so containment, Esc and keys would all keep working with no
  context. It lost on support: Otaq sets no minimum macOS, so WKWebView may be older than Safari 17 (the first with
  `popover`), and the Linux builds (AppImage, deb) use the distribution's WebKitGTK. A fallback for those would leave
  the bug in place there, and the UA styles of `[popover]` (inset, margin, border, padding, colours, light dismiss)
  would need resetting on every layer.
- **Always portal, even inside a layer.** Simpler rule, but it breaks the DOM nesting listed above: a press in a menu
  opened from a popover would close the popover, Esc ordering would fall back to "newest", and a picker inside a
  Dialog would leave its Tab trap. Fixing those means a second notion of "inside" (a registry of child layers),
  which is more code than the context.
- **Change the z-indexes** (sheet above the Noyan body, or no stacking context on the sheet). The expanded Noyan
  panel is meant to lie over the sheet (frame.css, T-0502), and the sheet needs its z-index to sit over the board's
  fixed chain arrows and legend (5, 6). Any order of the panels leaves one of them over a popover from the other.
- **Fix only the Waits for picker.** Leaves the same bug in every other popover in the sheet (the ticket's own open
  question), and adds a second way to draw a floating layer.

## Risks Dev checks

- **Styles that depend on an ancestor.** A portaled layer leaves its owner's DOM, so a CSS selector like
  `.some-panel .popover …` or a custom property set on an ancestor (e.g. `--row-fg` in `agent_panel.css`) no longer
  reaches it. Grep the CSS of each converted layer's contents; today's only descendant selectors on `.menu…`
  (`settings.css` `.settings-model-list`, `.settings-choice`) are inside Settings, which stays inline.
- **Native listeners on the owner.** A listener added with `addEventListener` on an ancestor element (not a React
  prop) no longer hears events from a portaled layer. `Dialog`'s is the known one and stays inline; check the sheet
  and the Noyan panel for others.
- **Leaving animation.** `useEnterExit` keeps a closing layer mounted while it fades out; the portal must not unmount
  it earlier (the portal's child is the same element, so this should hold; the page test below checks it).
- **The lifted card** (`TicketCard`, z-index 30) moves into the host; it must still draw above the board and the
  sheet while dragged.

## Tests

- `components/floatingPortal.page.test.ts` (happy-dom, like `escape.page.test.ts`): with an `#overlay-host` in the
  page, a `Popover` rendered inside a `.frame-sheet` lands in the host; one inside a `Dialog` stays inside the dialog
  box; a `Menu` opened from a portaled popover stays inside the popover; a press in that menu doesn't close the
  popover; Esc closes the menu first, then the popover; with no host, the popover renders inline.
- The scan test in `components/Floating.test.ts` (above).
- The existing `escape.page.test.ts`, `Floating.test.ts`, `Hint.test.ts`, `Tooltip.test.ts` and the Dependencies /
  New ticket tests keep passing.
- happy-dom has no layout, so the painting itself is a manual check on macOS and Windows: with the Noyan panel
  expanded and a ticket open, the Waits for picker, the New ticket form's Waits for picker and the description's code
  language picker draw above the Noyan panel, the box for new work and the sheet's edges.
