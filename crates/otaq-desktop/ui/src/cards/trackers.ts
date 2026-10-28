// Imported tickets' tracker badges (the otaq-marketplace README): the board provides each card's "Jira · PAY-12" by ticket
// key, and a card shows its own. Outside a board (the gallery, a test) there are none.
import { createContext, useContext } from "react";

export const TrackerBadges = createContext<ReadonlyMap<string, string>>(new Map());

export function useTrackerBadge(key: string): string | undefined {
  return useContext(TrackerBadges).get(key);
}
