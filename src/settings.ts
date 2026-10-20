// Validation of persisted UI data (localStorage): keep what has the right type, drop the rest.
import type { Basis, Conditions, InspectMode, InspectSettings, Palette, Template } from "./api";
import { UNITS } from "./units";
import type { ExportOptions, Settings } from "./state"; // type-only: no import cycle at runtime

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const num = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const numOrNull = (v: unknown): v is number | null => v === null || num(v);
const MODES: InspectMode[] = ["hot", "cold", "both"];
const BASES: Basis[] = ["similar", "ambient"];
const CORNERS: ExportOptions["logoCorner"][] = ["top-left", "top-right", "bottom-left", "bottom-right"];

/** Remembered report fields, so Rapid report and the next dialog start from the last ones. */
export interface LastReport {
  title: string;
  site: string;
  fields: Record<string, string>;
  conditions: Conditions;
  includePhotos: boolean;
}

function parseInspect(o: Record<string, unknown>): Partial<InspectSettings> {
  const out: Partial<InspectSettings> = {};
  if (MODES.includes(o.mode as InspectMode)) out.mode = o.mode as InspectMode;
  if (BASES.includes(o.basis as Basis)) out.basis = o.basis as Basis;
  if (num(o.minDelta)) out.minDelta = o.minDelta;
  if (num(o.minPixels)) out.minPixels = o.minPixels;
  if (num(o.maxFindings)) out.maxFindings = o.maxFindings;
  if (numOrNull(o.referenceK)) out.referenceK = o.referenceK;
  if (numOrNull(o.insideK)) out.insideK = o.insideK;
  if (numOrNull(o.outsideK)) out.outsideK = o.outsideK;
  return out;
}

function parseExport(o: Record<string, unknown>): Partial<ExportOptions> {
  const out: Partial<ExportOptions> = {};
  if (o.format === "png" || o.format === "jpg") out.format = o.format;
  if (num(o.scale)) out.scale = o.scale;
  for (const k of ["colourBar", "labels", "fileName", "dateTime", "parameters", "extremes"] as const) {
    if (typeof o[k] === "boolean") out[k] = o[k];
  }
  if (typeof o.customText === "string") out.customText = o.customText;
  if (o.logo === null || typeof o.logo === "string") out.logo = o.logo;
  if (CORNERS.includes(o.logoCorner as ExportOptions["logoCorner"])) out.logoCorner = o.logoCorner as ExportOptions["logoCorner"];
  return out;
}

/** Keeps only the keys of `Settings` that have the right type; everything else is ignored. */
export function parseSettings(raw: unknown): Partial<Settings> {
  if (!isObj(raw)) return {};
  const o = raw, out: Partial<Settings> = {};
  if (typeof o.unit === "string" && (UNITS as string[]).includes(o.unit)) out.unit = o.unit as Settings["unit"];
  if (typeof o.defaultPalette === "string") out.defaultPalette = o.defaultPalette;
  if (typeof o.smooth === "boolean") out.smooth = o.smooth;
  if (typeof o.showExtremes === "boolean") out.showExtremes = o.showExtremes;
  if (typeof o.inspector === "string") out.inspector = o.inspector;
  if (o.paper === "a4" || o.paper === "us-letter" || o.paper === null) out.paper = o.paper;
  if (Array.isArray(o.customPalettes)) out.customPalettes = o.customPalettes.filter((p): p is Palette => isObj(p) && typeof p.id === "string" && Array.isArray(p.colors));
  if (Array.isArray(o.customTemplates)) out.customTemplates = o.customTemplates.filter((t): t is Template => isObj(t) && typeof t.id === "string");
  if (typeof o.templateId === "string") out.templateId = o.templateId;
  if (o.reportFormat === "pdf" || o.reportFormat === "html") out.reportFormat = o.reportFormat;
  if (num(o.wandTolerance)) out.wandTolerance = o.wandTolerance;
  if (num(o.msxStrength)) out.msxStrength = o.msxStrength;
  if (isObj(o.inspect)) out.inspect = parseInspect(o.inspect);
  if (isObj(o.exportOverlay)) out.exportOverlay = parseExport(o.exportOverlay);
  return out;
}

/** The remembered report fields, field by field over `fallback`. */
export function parseLastReport(raw: unknown, fallback: LastReport): LastReport {
  if (!isObj(raw)) return fallback;
  const out: LastReport = { ...fallback, fields: { ...fallback.fields }, conditions: { ...fallback.conditions } };
  if (typeof raw.title === "string") out.title = raw.title;
  if (typeof raw.site === "string") out.site = raw.site;
  if (isObj(raw.fields)) {
    out.fields = {};
    for (const [k, v] of Object.entries(raw.fields)) if (typeof v === "string") out.fields[k] = v;
  }
  if (typeof raw.includePhotos === "boolean") out.includePhotos = raw.includePhotos;
  if (isObj(raw.conditions)) {
    const c = raw.conditions;
    if (numOrNull(c.insideK)) out.conditions.insideK = c.insideK;
    if (numOrNull(c.outsideK)) out.conditions.outsideK = c.outsideK;
    if (numOrNull(c.humidity)) out.conditions.humidity = c.humidity;
    if (numOrNull(c.ambientK)) out.conditions.ambientK = c.ambientK;
    if (BASES.includes(c.basis as Basis)) out.conditions.basis = c.basis as Basis;
  }
  return out;
}
