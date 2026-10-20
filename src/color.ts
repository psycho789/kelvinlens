// Colour helpers shared by the panel and the palette editor. `stopsToColors` interpolates colour
// stops to 256 colours the same way as the built-in palettes (palette.rs `stops_to_colors`).
export type Rgb = [number, number, number];
export type Stop = [number, Rgb];

export function rgbHex(c: Rgb): string {
  return `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

export function hexRgb(s: string): Rgb {
  const n = parseInt(s.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function stopsToColors(stops: Stop[]): [number, number, number][] {
  const s = [...stops].sort((a, b) => a[0] - b[0]);
  if (s.length === 0) return Array.from({ length: 256 }, () => [128, 128, 128]);
  if (s.length === 1) return Array.from({ length: 256 }, () => s[0]![1]);
  return Array.from({ length: 256 }, (_, i) => {
    const t = i / 255;
    let upper = s.findIndex(([at]) => at >= t);
    if (upper < 0) upper = s.length - 1;
    upper = Math.max(1, upper);
    const [a, ca] = s[upper - 1]!;
    const [b, cb] = s[upper]!;
    const f = b > a ? Math.min(1, Math.max(0, (t - a) / (b - a))) : 0;
    return [0, 1, 2].map((c) => Math.round(ca[c]! + (cb[c]! - ca[c]!) * f)) as [number, number, number];
  });
}
