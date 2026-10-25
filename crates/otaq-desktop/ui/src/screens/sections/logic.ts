// The sections' events (T-0223). What they show comes from Rust (`src/api/sections.rs`).

/** The service events that change a section or Activity. */
export const SECTION_EVENTS = [
  "ticket_changed",
  "ticket_deleted",
  "session_state",
  "usage_changed",
  "messages_waiting",
  "message_delivered",
  "needs_you_changed",
] as const;
