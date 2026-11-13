# The Flow card never scrolls vertically (OTAQ-4)

How "Project settings → Flow shows a vertical scroll bar" is fixed. Builds on decisions 052 (the strip stays one row
and scrolls sideways, its look unchanged) and 053 (nothing inside the Flow card scrolls vertically; the page body
keeps its scroll). Desktop page code only (`crates/otaq-desktop/ui/src`), no Rust change.

## The cause

- `.project-settings-body` (`screens/project_settings/project_settings.css`) is the page's one scroll container for
  all six cards. Its vertical scroll is wanted (053) and stays as it is.
- Inside the Flow card, the only element with an `overflow` is the stage strip, `.settings-flow-strip`
  (`screens/settings/settings.css`): `overflow-x: auto`. CSS turns the other axis's `visible` into `auto` whenever one
  axis is not `visible`, so the strip is also a vertical scroll container. Any sub-pixel or scroll-bar-track excess in
  its height (classic scroll bars on Windows, fractional zoom) then shows a small vertical bar next to the sideways
  one. The title row, the waiting banner, the "changed by" line and the card (`.project-settings-card`) have no
  `overflow` and grow with their content, so they never scroll.

## Decision: the strip clips its vertical axis

```css
/* settings.css */
.settings-flow-strip {
  display: flex;
  overflow-x: auto;
  overflow-y: hidden; /* sideways only (decisions 052, 053): never a vertical bar in the strip */
  …
}
```

- The strip's height is `auto`, so its content always fits it: `hidden` clips nothing that is drawn today (the step
  boxes, the connector `::before` lines at `top: 50%` inside each step, the 4 px top and bottom padding). The steps
  aren't focusable, so no focus ring is clipped either.
- Nothing else in the card changes. No `overflow` is added to the card or its parts: they keep growing with their
  content, and the page body scrolls when the window is shorter than the cards (053).
- The look of the strip is unchanged (052): same boxes, connectors, ends, gap and sideways scroll.

### Considered

- **`overflow: auto hidden` shorthand or `scrollbar-gutter`.** Same effect, less readable next to the file's
  long-hand style; `scrollbar-gutter` would reserve space and change the strip's look.
- **Wrapping the strip into several rows.** Removes the sideways scroll too, but 052 keeps one row.
- **Turning each Project settings section into its own page** so the page body could stop scrolling on Flow. The
  user chose against it at Tech spec (053): a bigger change that affects every section.

## Tests

- A page test next to the other happy-dom page tests, `screens/settings/flow_strip.page.test.ts`, loads
  `settings.css` into the document as `screens/shell/header_end.page.test.ts` does (`readFileSync` of the CSS into a
  `<style>`), draws `FlowSectionBody` with a 7-stage flow and a waiting change, and asserts that
  `getComputedStyle(strip)` has `overflow-y: hidden` and `overflow-x: auto`, and that no element inside the card's
  `section` has a computed `overflow-y` of `auto` or `scroll`. It is the guard the ticket asked about: a later edit that
  brings back a vertical scroll container inside the card fails the build. happy-dom has no layout, so it checks the
  rules, not the painted bar.
- The existing `flow.test.ts` and `project_settings.test.ts` keep passing.
- Manual check on macOS and Windows: Project settings at 1280 × 800, a 7-stage flow (wider than the card) and a
  waiting change: the strip scrolls sideways with no vertical bar, nothing inside the Flow card scrolls vertically,
  and the page still scrolls down to Remove project.
