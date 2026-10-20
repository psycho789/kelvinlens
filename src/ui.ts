import { measureSoon, refreshAnalysis } from "./analysis";
import { api } from "./api";
import { $ } from "./dom";
import { promptAt } from "./modal";
import { Panel, alarmOn } from "./panel";
import { panelEvents } from "./panelWiring";
import { type Doc, currentDoc, displayRange, lookOf, nextLabel, state } from "./state";
import { markDirty, setTool, showReadout, updateZoomLabel } from "./status";
import { Viewer } from "./viewer";

export let viewer: Viewer;
export let panel: Panel;

export function createUi(): void {
  viewer = new Viewer($("#viewer"), {
    hover: (pos) => showReadout(pos),
    measurementsChanged: (final) => {
      const doc = currentDoc();
      if (!doc) return;
      markDirty(doc);
      if (final) void refreshAnalysis(doc);
      else measureSoon(doc);
    },
    select: (index) => {
      state.selected = index;
      panel.renderMeasurements(currentDoc());
    },
    zoomChanged: () => updateZoomLabel(),
    newLabel: (kind) => nextLabel(currentDoc()!, kind),
    annotationsChanged: () => {
      const doc = currentDoc();
      if (!doc) return;
      markDirty(doc);
      panel.renderInspection(doc);
    },
    cropChanged: (crop) => {
      const doc = currentDoc();
      if (!doc) return;
      doc.presentation = { ...doc.presentation, crop };
      markDirty(doc);
      setTool("select");
      refreshView();
      panel.renderDisplay(doc);
    },
    requestText: (at) => promptAt($(".stage"), at, "Note text, then Enter"),
    wandTolerance: () => state.wandTolerance,
    sketchColor: () => state.sketchColor,
    renderThermal: (doc) => api.render(doc.info.id, lookOf(doc), doc.info.width, doc.info.height),
    renderComposite: (doc) => {
      const range = displayRange(doc);
      const fusion =
        state.view === "msx"
          ? { mode: "msx" as const, strength: state.msxStrength }
          : { mode: "interval" as const, lo: (state.fusionRange ?? [range[0] + (range[1] - range[0]) * 0.6, range[1]])[0], hi: (state.fusionRange ?? [range[0], range[1]])[1] };
      return api.fuse(doc.info.id, lookOf(doc), fusion, 1600, doc.presentation.align ?? null);
    },
  });
  panel = new Panel($("#panel"), panelEvents());
}

export function lookKey(doc: Doc): string {
  const l = lookOf(doc);
  return `${l.palette.id}|${l.palette.colors.length}|${l.palette.colors[0]}|${l.palette.colors[l.palette.colors.length - 1]}|${l.range[0]}|${l.range[1]}|${l.distribution}|${JSON.stringify(l.isotherms)}`;
}

export function panelHandlers() {
  return (panel as unknown as { on: { setPalette(id: string): void } }).on;
}

export function refreshView(): void {
  const doc = currentDoc();
  viewer.setDoc(doc, false);
  if (!doc) return;
  viewer.lookKey = lookKey(doc);
  const range = displayRange(doc);
  viewer.fusionKey = `${state.view === "msx" ? `msx${state.msxStrength}` : `iv${JSON.stringify(state.fusionRange ?? range)}`}|${JSON.stringify(doc.presentation.align ?? null)}`;
  viewer.selected = state.selected;
  viewer.alarms = doc.measurements.map((_, i) => alarmOn(doc, i));
  viewer.draw();
}
