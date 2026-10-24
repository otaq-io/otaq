// Open on a report chip (CMT-10, CMT-12): one door for the thread's "Report · 48 KB" chips and the box's "Pasted text"
// chip. The report view over the sheet is T-0447's (`ReportView.tsx`); it registers itself here with
// `setReportOpener`. Until it does, Open does nothing.

/** What Open shows: a posted comment's report (by its number), or the text pasted into a box, not posted yet. */
export type ReportTarget =
  | { kind: "comment"; ticket: string; n: number }
  | { kind: "pasted"; ticket: string; text: string; size: string };

let opener: (target: ReportTarget) => void = () => undefined;

/** Shows a report (the chip's Open). */
export function openReport(target: ReportTarget): void {
  opener(target);
}

/** The report view takes Open over (T-0447). */
export function setReportOpener(open: (target: ReportTarget) => void): void {
  opener = open;
}
