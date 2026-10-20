// The right-hand panel: display, measurements, formulas, isotherms, automatic inspection, video,
// capture parameters, inspection details, notes and image details.
import type { Alarm, Distribution, Formula, Isotherm, Params, Range } from "./api";
import type { Doc, View } from "./state";
import { h } from "./dom";
import { renderDetails as renderDetailsImpl } from "./panelSections/details";
import { renderDisplay as renderDisplayImpl, alignRow as alignRowImpl, drawHistogram as drawHistogramImpl } from "./panelSections/display";
import { renderFormulas as renderFormulasImpl } from "./panelSections/formulas";
import { renderInspect as renderInspectImpl } from "./panelSections/inspect";
import { renderInspection as renderInspectionImpl } from "./panelSections/inspection";
import { renderIsotherms as renderIsothermsImpl } from "./panelSections/isotherms";
import { renderMeasurements as renderMeasurementsImpl, renderMeasurementDetail as renderMeasurementDetailImpl, deltaT as deltaTImpl } from "./panelSections/measurements";
import { renderNotes as renderNotesImpl } from "./panelSections/notes";
import { renderParams as renderParamsImpl } from "./panelSections/params";
import { renderVideo as renderVideoImpl } from "./panelSections/video";
import { section } from "./panelWidgets";

export interface PanelHandlers {
  setPalette(id: string): void;
  editPalettes(): void;
  setRange(range: Range | null): void;
  setDistribution(d: Distribution): void;
  setView(view: View): void;
  setBlend(value: number): void;
  setMsxStrength(value: number): void;
  setFusionRange(range: Range): void;
  setSmooth(on: boolean): void;
  setShowExtremes(on: boolean): void;
  setRotation(degrees: number): void;
  setAlign(align: [number, number, number] | null): void;
  startCrop(): void;
  clearCrop(): void;
  setParams(params: Params): void;
  resetParams(): void;
  applyParamsToAll(): void;
  select(index: number | null): void;
  renameMeasurement(index: number, label: string): void;
  deleteMeasurement(index: number): void;
  setAlarm(index: number, alarm: Alarm | null): void;
  setWandTolerance(index: number, tolerance: number): void;
  setFormulas(formulas: Formula[]): void;
  setIsotherms(isotherms: Isotherm[]): void;
  runInspect(): void;
  clearFindings(): void;
  findingsToMeasurements(): void;
  setAsset(asset: string): void;
  setField(name: string, value: string): void;
  setAnnotationText(index: number, text: string): void;
  deleteAnnotation(index: number): void;
  setNotes(notes: string): void;
  seek(frame: number): void;
  togglePlay(): void;
  setHsm(patch: { hsm?: boolean; hsmReference?: number; hsmSpan?: number; binning?: boolean }): void;
  setInOut(first: number, last: number): void;
  plotSeries(): void;
  exportVideo(): void;
  exportSegment(): void;
}

export class Panel {
  /** @internal used by panelSections */
  readonly els = {
    display: h("div"),
    measurements: h("div"),
    formulas: h("div"),
    isotherms: h("div"),
    inspect: h("div"),
    video: h("div"),
    params: h("div"),
    inspection: h("div"),
    notes: h("div"),
    details: h("div"),
  };
  /** @internal used by panelSections */
  videoSection: HTMLElement;
  /** @internal used by panelSections */
  profileFor = "";

  constructor(
    host: HTMLElement,
    readonly on: PanelHandlers,
  ) {
    this.videoSection = section("video", "Video", this.els.video);
    host.append(
      section("display", "Display", this.els.display),
      this.videoSection,
      section("measurements", "Measurements", this.els.measurements),
      section("formulas", "Formulas", this.els.formulas),
      section("isotherms", "Isotherms & alarms", this.els.isotherms),
      section("inspect", "Auto-inspect", this.els.inspect),
      section("params", "Parameters", this.els.params),
      section("inspection", "Inspection", this.els.inspection),
      section("notes", "Notes", this.els.notes),
      section("details", "Image details", this.els.details),
    );
  }

  renderAll(doc: Doc | null): void {
    this.renderDisplay(doc);
    this.renderVideo(doc);
    this.renderMeasurements(doc);
    this.renderFormulas(doc);
    this.renderIsotherms(doc);
    this.renderInspect(doc);
    this.renderParams(doc);
    this.renderInspection(doc);
    this.renderNotes(doc);
    this.renderDetails(doc);
  }

  /** @internal used by panelSections */
  deltaA = 0;
  /** @internal used by panelSections */
  deltaB = 1;

  renderDisplay(doc: Doc | null): void {
    renderDisplayImpl.call(this, doc);
  }

  /** @internal used by panelSections */
  alignRow(doc: Doc): HTMLElement {
    return alignRowImpl.call(this, doc);
  }

  /** @internal used by panelSections */
  drawHistogram(doc: Doc): void {
    drawHistogramImpl.call(this, doc);
  }

  renderVideo(doc: Doc | null): void {
    renderVideoImpl.call(this, doc);
  }

  renderParams(doc: Doc | null): void {
    renderParamsImpl.call(this, doc);
  }

  renderMeasurements(doc: Doc | null): void {
    renderMeasurementsImpl.call(this, doc);
  }

  /** @internal used by panelSections */
  renderMeasurementDetail(doc: Doc): void {
    renderMeasurementDetailImpl.call(this, doc);
  }

  /** @internal used by panelSections */
  deltaT(doc: Doc): HTMLElement {
    return deltaTImpl.call(this, doc);
  }

  renderFormulas(doc: Doc | null): void {
    renderFormulasImpl.call(this, doc);
  }

  renderIsotherms(doc: Doc | null): void {
    renderIsothermsImpl.call(this, doc);
  }

  renderInspect(doc: Doc | null): void {
    renderInspectImpl.call(this, doc);
  }

  renderInspection(doc: Doc | null): void {
    renderInspectionImpl.call(this, doc);
  }

  renderNotes(doc: Doc | null): void {
    renderNotesImpl.call(this, doc);
  }

  renderDetails(doc: Doc | null): void {
    renderDetailsImpl.call(this, doc);
  }
}

export { alarmOn, gradientCss, isothermText } from "./panelWidgets";
