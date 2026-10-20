import type { Panel } from "../panel";
import { fill, h } from "../dom";
import { type Doc } from "../state";

export function renderDetails(this: Panel, doc: Doc | null): void {
  const el = this.els.details;
  if (!doc) {
    fill(el);
    return;
  }
  const c = doc.info.camera;
  const v = doc.info.video;
  const rows: [string, string | null][] = [
    ["Camera", c.description],
    ["Captured", c.dateTime],
    ["Serial", c.serialNumber],
    ["Lens", c.lens ? `${c.lens}${c.fieldOfView ? ` · ${c.fieldOfView.toFixed(1)}°` : ""}` : c.fieldOfView ? `${c.fieldOfView.toFixed(1)}°` : null],
    ["GPS", c.latitude != null && c.longitude != null ? `${c.latitude.toFixed(6)}, ${c.longitude.toFixed(6)}` : null],
    ["Altitude", c.altitude != null ? `${c.altitude.toFixed(1)} m${c.relativeAltitude != null ? ` (${c.relativeAltitude.toFixed(1)} m above take-off)` : ""}` : null],
    ["Heading", c.heading != null ? `${c.heading.toFixed(0)}°` : null],
    ["Gimbal", c.gimbal ? `roll ${c.gimbal[0].toFixed(1)}° · pitch ${c.gimbal[1].toFixed(1)}° · yaw ${c.gimbal[2].toFixed(1)}°` : null],
    ["Format", doc.info.formatLabel],
    ["Size", `${doc.info.width} × ${doc.info.height}${doc.photo ? ` · photo ${doc.photo.width} × ${doc.photo.height}` : ""}`],
    ["Video", v ? `${v.frames} frames${v.frameRate ? ` · ${v.frameRate.toFixed(1)} fps` : ""}${v.compressed ? " · CSQ" : " · SEQ"}` : null],
    ["File", doc.info.path],
  ];
  fill(el, h("dl", { class: "details" }, rows.filter(([, v]) => v).map(([k, v]) => [h("dt", {}, k), h("dd", { title: v ?? "" }, v ?? "")]).flat()));
}
