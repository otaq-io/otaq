# Plan · floating-layers-above-panels

- **Ticket:** OTAQ-3
- **Date:** 2026-11-13
- **Decisions:** none (a layering bug fix; Product spec settled nothing)
- **Tech spec:** tuq/tech-specs/floating-layers-above-panels.md
- **Orchestrator prompt:** none (the ticket's agent builds from OTAQ-3's Plan)

<!-- tuq:plan:start -->

The fix follows tuq/tech-specs/floating-layers-above-panels.md: every floating layer portals into the overlay host
(#overlay-host, z-index 10) unless it is already inside a layer. Two pieces, one behaviour each: the shared rule with
the shared components that use it (1), then the six components that call useFloatingStyle themselves and the scan
test that keeps every future one on the rule (2), which builds on 1's FloatingPortal. No Rust change; desktop page
code only (crates/otaq-desktop/ui/src).

## Pieces

1. FloatingPortal: shared popovers, menus and tooltips draw in the overlay host — waits for nothing
   Spec: tuq/tech-specs/floating-layers-above-panels.md (read "The cause" and "Decision"). Cause: .frame-sheet
   (app/frame.css, position absolute, z-index 8) is a stacking context, so a position: fixed popover inside it paints
   under the expanded Noyan body (z 9) and .frame-new-work (z 8) whatever its own z-index.
   In components/Floating.tsx add a React context InLayer (boolean, default false) and FloatingPortal: not inside a
   layer and #overlay-host (OVERLAY_HOST_ID, app/Frame.tsx) present → createPortal into the host wrapped in
   InLayer=true; inside a layer, or no host (unit tests, gallery) → render inline as today. Move inOverlayHost from
   components/Dialog.tsx to Floating.tsx (keep the components/index.ts re-export), taking an optional target slot and
   always wrapping its child in InLayer=true. Use it from Dialog, the frame's Overlays (app/Frame.tsx renders
   <Overlays /> into the host directly: wrap it in the provider) and markdown/fullwindow/FullView.tsx (its
   createPortal(…, slot) becomes inOverlayHost(…, slot)). Settings' Keyboard.tsx and Agents.tsx header-slot portals
   stay as they are. Route the roots of Popover, Menu (both its menus), Tooltip and Hint through FloatingPortal.
   Update Floating.tsx's header comment ("drawn inside the anchor's subtree…") to the new rule.
   Must not break: usePressOutside's element.contains, escape.ts's DOM-containment Esc order, Dialog's
   inKeyLayer/Tab trap (that is why layers inside a layer stay inline); useEnterExit's leaving animation; CSS that
   reaches a layer through an ancestor selector or custom property (grep the contents' CSS; Settings' .menu selectors
   stay inline); native addEventListener listeners on the sheet or Noyan panel that expected to hear the popover.
   Test: components/floatingPortal.page.test.ts (happy-dom, like escape.page.test.ts): with an #overlay-host, a
   Popover inside a .frame-sheet lands in the host; one inside a Dialog stays in the dialog box; a Menu opened from a
   portaled popover stays inside the popover; a press in that menu doesn't close the popover; Esc closes the menu,
   then the popover; a closing popover stays mounted through its leave; with no host it renders inline. Existing
   escape.page, Floating, Hint, Tooltip, Dependencies and New ticket tests keep passing.
2. The other floating layers and the scan test — waits for 1
   Spec: tuq/tech-specs/floating-layers-above-panels.md, "Keeping every floating layer on the rule". Wrap the root
   of each direct useFloatingStyle user in FloatingPortal (from components/Floating.tsx, piece 1):
   markdown/code/CodeBlockView.tsx (the code language picker), cards/TicketCard.tsx (the lifted card, z-index 30: it
   must still draw above the board and the sheet while dragged), screens/settings/ModelPicker.tsx,
   screens/strip/Strip.tsx (two), screens/board/Search.tsx. Check each one's CSS for ancestor-dependent selectors or
   custom properties that a portal would cut off.
   Add to components/Floating.test.ts a scan of src/**/*.tsx that fails when a file calling useFloatingStyle( does
   not also use FloatingPortal (Floating.tsx itself excepted). Run the touched components' tests.

<!-- tuq:plan:end -->
