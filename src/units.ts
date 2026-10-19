// Temperature units and number formatting. Everything is stored in kelvin.
import type { Unit } from "./api";

export const UNITS: Unit[] = ["C", "F", "K"];

export function symbol(unit: Unit): string {
  return unit === "K" ? "K" : unit === "F" ? "°F" : "°C";
}

export function fromKelvin(k: number, unit: Unit): number {
  if (unit === "C") return k - 273.15;
  if (unit === "F") return (k - 273.15) * 1.8 + 32;
  return k;
}

export function toKelvin(v: number, unit: Unit): number {
  if (unit === "C") return v + 273.15;
  if (unit === "F") return (v - 32) / 1.8 + 273.15;
  return v;
}

export function deltaFromKelvin(dk: number, unit: Unit): number {
  return unit === "F" ? dk * 1.8 : dk;
}

export function temp(k: number | null | undefined, unit: Unit, digits = 1): string {
  if (k == null || !Number.isFinite(k)) return "–";
  return `${fromKelvin(k, unit).toFixed(digits)} ${symbol(unit)}`;
}

export function delta(dk: number | null | undefined, unit: Unit, digits = 1): string {
  if (dk == null || !Number.isFinite(dk)) return "–";
  const v = deltaFromKelvin(dk, unit);
  return `${v >= 0 ? "+" : ""}${v.toFixed(digits)} ${unit === "K" ? "K" : symbol(unit)}`;
}

/** Value for a temperature <input>, rounded to 0.1. */
export function inputValue(k: number | null | undefined, unit: Unit): string {
  if (k == null || !Number.isFinite(k)) return "";
  return (Math.round(fromKelvin(k, unit) * 10) / 10).toString();
}
