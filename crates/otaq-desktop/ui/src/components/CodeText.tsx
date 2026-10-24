// Text from a view model with its `…` parts drawn as code (the Merge check's help and dialog line, d0120).
import { codeSpans } from "./logic";

export function CodeText({ text }: { text: string }) {
  return <>{codeSpans(text).map((span, i) => (span.code ? <code key={i}>{span.text}</code> : span.text))}</>;
}
