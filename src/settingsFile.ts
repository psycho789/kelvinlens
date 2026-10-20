import type { Distribution, Params, Range } from "./api";
import type { Doc } from "./state";

export interface SettingsFile {
  kelvinlensSettings: 1;
  params: Params | null;
  palette: string;
  range: Range | null;
  distribution: Distribution;
  isotherms: Doc["isotherms"];
  measurements: Doc["measurements"];
  formulas: Doc["formulas"];
  presentation: Doc["presentation"];
}

const DISTRIBUTIONS = ["temperatureLinear", "signalLinear", "histogram", "detail"] as const;

/** Parses a settings file, keeping only the fields that have the right shape. Throws when it is not a Kelvinlens settings file. */
export function parseSettingsFile(text: string): Partial<SettingsFile> {
  const raw: unknown = JSON.parse(text);
  if (typeof raw !== "object" || raw === null || (raw as { kelvinlensSettings?: unknown }).kelvinlensSettings !== 1) {
    throw new Error("Not a Kelvinlens settings file");
  }
  const r = raw as Record<string, unknown>;
  const out: Partial<SettingsFile> = { kelvinlensSettings: 1 };
  if (typeof r.palette === "string") out.palette = r.palette;
  if (r.range === null || (Array.isArray(r.range) && r.range.length === 2 && r.range.every((n) => typeof n === "number"))) out.range = r.range as Range | null;
  if (typeof r.distribution === "string" && (DISTRIBUTIONS as readonly string[]).includes(r.distribution)) out.distribution = r.distribution as Distribution;
  if (Array.isArray(r.isotherms)) out.isotherms = r.isotherms as SettingsFile["isotherms"];
  if (Array.isArray(r.measurements)) out.measurements = r.measurements as SettingsFile["measurements"];
  if (Array.isArray(r.formulas)) out.formulas = r.formulas as SettingsFile["formulas"];
  if (typeof r.presentation === "object" && r.presentation !== null) out.presentation = r.presentation as SettingsFile["presentation"];
  if (typeof r.params === "object" && r.params !== null) out.params = r.params as Params;
  return out;
}
