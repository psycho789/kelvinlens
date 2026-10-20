import { refreshAnalysis } from "./analysis";
import { seriesDialog } from "./analysisDialogs";
import { api } from "./api";
import { $, errorText, toast } from "./dom";
import { encode, renderExport } from "./exportImage";
import { progress } from "./modal";
import { basename, siblingPath, stem } from "./paths";
import { pickers } from "./pickers";
import { DEFAULT_EXPORT, type Doc, currentDoc, displayRange, finiteRange, loadSettings, state } from "./state";
import { setBusy } from "./status";
import { panel, refreshView, viewer } from "./ui";

// ---------------------------------------------------------------------------- video

export async function seek(frame: number): Promise<void> {
  const doc = currentDoc();
  if (doc) await seekDoc(doc, frame);
}

export async function seekDoc(doc: Doc, frame: number): Promise<void> {
  const v = doc.video;
  if (!v) return;
  const n = doc.info.video!.frames;
  frame = Math.max(0, Math.min(n - 1, frame));
  if (frame === v.frame && !v.playing) {
    if (doc === currentDoc()) panel.renderVideo(doc);
    return;
  }
  try {
    const r = await api.selectFrame(doc.info.id, frame);
    v.frame = frame;
    doc.params = r.params;
    doc.kelvin = await api.thermalData(doc.info.id);
    doc.dataRange = finiteRange(doc.kelvin);
    if (v.hsm) await updateHsm(doc, false);
    await refreshAnalysis(doc, false);
    if (doc === currentDoc()) {
      viewer.dataChanged();
      refreshView();
      panel.renderVideo(doc);
      panel.renderMeasurements(doc);
      panel.renderFormulas(doc);
      panel.renderDisplay(doc);
      $("#status-file").textContent = `${doc.info.name} · frame ${frame + 1}/${n} · ${doc.info.width} × ${doc.info.height}`;
    }
  } catch (e) {
    v.playing = false;
    toast(errorText(e), "error");
  }
}

export async function updateHsm(doc: Doc, redraw = true): Promise<void> {
  const v = doc.video;
  if (!v) return;
  if (!v.hsm) {
    v.hsmData = null;
  } else {
    try {
      v.hsmData = await api.hsm(doc.info.id, v.frame, v.hsmReference, v.binning);
    } catch (e) {
      v.hsm = false;
      v.hsmData = null;
      toast(errorText(e), "error");
    }
  }
  if (redraw && doc === currentDoc()) {
    viewer.dataChanged();
    refreshView();
  }
}

export function togglePlay(): void {
  const doc = currentDoc();
  const v = doc?.video;
  if (!doc || !v) return;
  v.playing = !v.playing;
  panel.renderVideo(doc);
  if (!v.playing) return;
  const info = doc.info.video!;
  const fps = Math.min(30, info.frameRate ?? 10);
  const step = async () => {
    if (!v.playing || doc !== currentDoc()) {
      v.playing = false;
      panel.renderVideo(currentDoc());
      return;
    }
    const started = performance.now();
    const next = v.frame + 1 > v.last ? v.first : v.frame + 1;
    await seek(next);
    const wait = Math.max(0, 1000 / fps - (performance.now() - started));
    setTimeout(() => void step(), wait);
  };
  void step();
}

export async function plotSeries(): Promise<void> {
  const doc = currentDoc();
  const v = doc?.video;
  if (!doc || !v) return;
  if (doc.measurements.length === 0) {
    toast("Add measurements first; they are tracked across the frames.", "error");
    return;
  }
  setBusy("Measuring every frame…");
  try {
    const series = await api.videoSeries(doc.info.id, v.first, v.last, doc.measurements);
    setBusy("");
    await seriesDialog(doc, series, v.first);
  } catch (e) {
    setBusy("");
    toast(errorText(e), "error");
  }
}

export async function exportSegment(): Promise<void> {
  const doc = currentDoc();
  const v = doc?.video;
  if (!doc || !v) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc.info.path, `${stem(doc.info.name)}-frames-${v.first + 1}-${v.last + 1}.seq`), filters: [{ name: "FLIR SEQ video", extensions: ["seq"] }] });
  if (!path) return;
  try {
    const n = await api.exportSegment(doc.info.id, v.first, v.last, path);
    toast(`Saved ${n} frames to ${basename(path)}`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

export async function exportVideo(): Promise<void> {
  const doc = currentDoc();
  if (!doc?.video) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc.info.path, `${stem(doc.info.name)}.avi`), filters: [{ name: "AVI video", extensions: ["avi"] }] });
  if (!path) return;
  const p = progress("Exporting video");
  try {
    const n = await writeVideo(doc, path, (done, total, text) => {
      p.set(done, total, text);
      return !p.cancelled();
    });
    if (n > 0) toast(`Saved ${n} frames to ${basename(path)}`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  } finally {
    p.close();
  }
}

/** Renders the doc's segment (in to out) as an MJPEG AVI. `onFrame` returns false to cancel.
 * Returns the number of frames written (0 if cancelled). */
export async function writeVideo(doc: Doc, path: string, onFrame: (done: number, total: number, text: string) => boolean = () => true): Promise<number> {
  const v = doc.video!;
  const wasPlaying = v.playing;
  v.playing = false;
  const startFrame = v.frame;
  // One colour scale for the whole clip, so colours mean the same temperature in every frame.
  const autoRange = doc.range === null;
  if (autoRange) doc.range = [...displayRange(doc)];
  const opts = { ...DEFAULT_EXPORT, ...(loadSettings().exportOverlay ?? {}), scale: 0 };
  let size: [number, number] | null = null;
  let cancelled = false;
  try {
    await api.aviReset();
    const total = v.last - v.first + 1;
    for (let f = v.first; f <= v.last; f++) {
      if (!onFrame(f - v.first, total, `Frame ${f + 1}`)) {
        cancelled = true;
        break;
      }
      await seekDoc(doc, f);
      const canvas = await renderExport(doc, state.unit, state.smooth, { ...opts, fileName: true });
      // Frame sizes must not change within a video.
      if (!size) size = [canvas.width, canvas.height];
      const fixed = size[0] === canvas.width && size[1] === canvas.height ? canvas : resizeCanvas(canvas, size[0], size[1]);
      await api.aviAddFrame(await encode(fixed, "jpg"));
    }
    if (cancelled || !size) {
      await api.aviReset();
      return 0;
    }
    return await api.aviFinish(path, size[0], size[1], doc.info.video!.frameRate ?? 10);
  } finally {
    if (autoRange) doc.range = null;
    await seekDoc(doc, startFrame);
    if (wasPlaying && doc === currentDoc()) togglePlay();
  }
}

export function resizeCanvas(c: HTMLCanvasElement, w: number, h: number): HTMLCanvasElement {
  const out = document.createElement("canvas");
  out.width = w;
  out.height = h;
  out.getContext("2d")!.drawImage(c, 0, 0, w, h);
  return out;
}

