// "Export image": the thermal image with measurements and a colour scale, as PNG bytes.
import type { Palette, Range, Unit } from "./api";
import type { Doc } from "./state";
import { temp } from "./units";
import { drawMeasurements, renderThermal } from "./viewer";

export async function exportPng(doc: Doc, palette: Palette, range: Range, unit: Unit, smooth: boolean): Promise<Uint8Array> {
  const { width: w, height: h } = doc.info;
  // At least 1280 px wide so small sensors (80×60) still export legibly.
  const scale = Math.max(1, Math.ceil(1280 / w));
  const outW = w * scale;
  const u = outW / 640;
  const footer = Math.round(46 * u);

  const canvas = document.createElement("canvas");
  canvas.width = outW;
  canvas.height = h * scale + footer;
  const ctx = canvas.getContext("2d")!;

  const thermal = document.createElement("canvas");
  thermal.width = w;
  thermal.height = h;
  thermal.getContext("2d")!.putImageData(renderThermal(doc.kelvin, w, h, range, palette), 0, 0);
  ctx.imageSmoothingEnabled = smooth;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(thermal, 0, 0, outW, h * scale);

  drawMeasurements(ctx, doc.measurements, (x, y) => [x * scale, y * scale], { scale: u * 1.2, selected: null, labels: true });

  // Colour scale
  const top = h * scale;
  ctx.fillStyle = "#0f131a";
  ctx.fillRect(0, top, outW, footer);
  const barX = 14 * u;
  const barW = outW - 28 * u;
  const bar = ctx.createLinearGradient(barX, 0, barX + barW, 0);
  palette.colors.forEach((c, i) => bar.addColorStop(i / 255, `rgb(${c[0]},${c[1]},${c[2]})`));
  ctx.fillStyle = bar;
  ctx.fillRect(barX, top + 8 * u, barW, 12 * u);
  ctx.fillStyle = "#e5e7eb";
  ctx.font = `500 ${11 * u}px system-ui, -apple-system, "Segoe UI", sans-serif`;
  ctx.textBaseline = "top";
  ctx.textAlign = "left";
  ctx.fillText(temp(range[0], unit), barX, top + 25 * u);
  ctx.textAlign = "right";
  ctx.fillText(temp(range[1], unit), barX + barW, top + 25 * u);
  ctx.textAlign = "center";
  ctx.fillStyle = "#9ca3af";
  ctx.fillText(`${doc.info.name} · ${palette.name}`, outW / 2, top + 25 * u);

  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("Could not encode PNG"))), "image/png"),
  );
  return new Uint8Array(await blob.arrayBuffer());
}
