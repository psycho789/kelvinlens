import type { Panel } from "../panel";
import { fill, h } from "../dom";
import { type Doc } from "../state";

export function renderNotes(this: Panel, doc: Doc | null): void {
  const el = this.els.notes;
  if (!doc) {
    fill(el);
    return;
  }
  if (el.contains(document.activeElement)) return;
  fill(
    el,
    h("textarea", {
      rows: 3,
      placeholder: "Findings, location, recommended action… (included in the report)",
      value: doc.notes,
      oninput: (e: Event) => this.on.setNotes((e.target as HTMLTextAreaElement).value),
    }),
  );
}
