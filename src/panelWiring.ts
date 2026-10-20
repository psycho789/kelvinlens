import type { PanelHandlers } from "./panel";import { deleteMeasurement, refreshAnalysis, runInspect, updateAlarms } from "./analysis";
import { type Distribution } from "./api";
import { paletteEditor } from "./paletteEditor";
import { applyParams, applyParamsToAll, resetParams } from "./params";
import { type View, currentDoc, saveSettings, state } from "./state";
import { markDirty, renderFiles, setTool } from "./status";
import { panel, panelHandlers, refreshView, viewer } from "./ui";
import { exportSegment, exportVideo, plotSeries, seek, togglePlay, updateHsm } from "./video";

export function panelEvents(): PanelHandlers {
  return {
      setPalette: (id) => {
        const doc = currentDoc();
        if (!doc) return;
        doc.palette = id;
        if (id !== "camera") {
          state.defaultPalette = id;
          saveSettings({ defaultPalette: id });
        }
        markDirty(doc);
        refreshView();
        panel.renderDisplay(doc);
        renderFiles();
      },
      editPalettes: () =>
        void paletteEditor().then((id) => {
          const doc = currentDoc();
          if (id && doc) panelHandlers().setPalette(id);
          else panel.renderDisplay(doc);
        }),
      setRange: (range) => {
        const doc = currentDoc();
        if (!doc) return;
        doc.range = range;
        markDirty(doc);
        refreshView();
        panel.renderDisplay(doc);
      },
      setDistribution: (d: Distribution) => {
        const doc = currentDoc();
        if (!doc) return;
        doc.distribution = d;
        markDirty(doc);
        refreshView();
        panel.renderDisplay(doc);
      },
      setView: (view: View) => {
        state.view = view;
        viewer.view = view;
        viewer.resetComposite();
        viewer.fit();
        refreshView();
        panel.renderDisplay(currentDoc());
      },
      setBlend: (value) => {
        state.blend = value;
        viewer.blend = value;
        viewer.draw();
      },
      setMsxStrength: (value) => {
        state.msxStrength = value;
        saveSettings({ msxStrength: value });
        refreshView();
      },
      setFusionRange: (range) => {
        state.fusionRange = range[0] <= range[1] ? range : [range[1], range[0]];
        refreshView();
        panel.renderDisplay(currentDoc());
      },
      setSmooth: (on) => {
        state.smooth = on;
        viewer.smooth = on;
        saveSettings({ smooth: on });
        viewer.draw();
      },
      setShowExtremes: (on) => {
        state.showExtremes = on;
        viewer.showExtremes = on;
        saveSettings({ showExtremes: on });
        viewer.draw();
      },
      setRotation: (deg) => {
        const doc = currentDoc();
        if (!doc) return;
        const r = ((deg % 360) + 360) % 360;
        doc.presentation = { ...doc.presentation, rotation: Math.round(r * 100) / 100 };
        markDirty(doc);
        viewer.fit();
        panel.renderDisplay(doc);
      },
      setAlign: (align) => {
        const doc = currentDoc();
        if (!doc) return;
        const neutral = !align || (align[0] === 0 && align[1] === 0 && align[2] === 1);
        doc.presentation = { ...doc.presentation, align: neutral ? null : align };
        markDirty(doc);
        refreshView();
        panel.renderDisplay(doc);
      },
      startCrop: () => {
        setTool("crop");
      },
      clearCrop: () => {
        const doc = currentDoc();
        if (!doc) return;
        doc.presentation = { ...doc.presentation, crop: null };
        markDirty(doc);
        viewer.fit();
        panel.renderDisplay(doc);
      },
      setParams: (params) => {
        const doc = currentDoc();
        if (doc) void applyParams(doc, params);
      },
      resetParams: () => {
        const doc = currentDoc();
        if (doc) void resetParams(doc);
      },
      applyParamsToAll: () => void applyParamsToAll(),
      select: (index) => {
        state.selected = index;
        viewer.selected = index;
        viewer.draw();
        panel.renderMeasurements(currentDoc());
      },
      renameMeasurement: (index, label) => {
        const doc = currentDoc();
        const m = doc?.measurements[index];
        if (!doc || !m) return;
        m.label = label;
        markDirty(doc);
        viewer.draw();
        void refreshAnalysis(doc);
      },
      deleteMeasurement: (index) => deleteMeasurement(index),
      setAlarm: (index, alarm) => {
        const doc = currentDoc();
        const m = doc?.measurements[index];
        if (!doc || !m) return;
        m.alarm = alarm && (alarm.above != null || alarm.below != null) ? alarm : alarm ? { ...alarm } : null;
        markDirty(doc);
        updateAlarms(doc);
        panel.renderMeasurements(doc);
      },
      setWandTolerance: (index, tolerance) => {
        const doc = currentDoc();
        const m = doc?.measurements[index];
        if (!doc || !m || m.shape.kind !== "wand") return;
        m.shape = { ...m.shape, tolerance };
        state.wandTolerance = tolerance;
        markDirty(doc);
        void refreshAnalysis(doc);
      },
      setFormulas: (formulas) => {
        const doc = currentDoc();
        if (!doc) return;
        doc.formulas = formulas;
        markDirty(doc);
        void refreshAnalysis(doc);
      },
      setIsotherms: (isotherms) => {
        const doc = currentDoc();
        if (!doc) return;
        doc.isotherms = isotherms;
        markDirty(doc);
        refreshView();
        panel.renderIsotherms(doc);
        void refreshAnalysis(doc);
      },
      runInspect: () => void runInspect(),
      clearFindings: () => {
        const doc = currentDoc();
        if (!doc) return;
        doc.findings = [];
        markDirty(doc);
        viewer.draw();
        panel.renderInspect(doc);
      },
      findingsToMeasurements: () => {
        const doc = currentDoc();
        if (!doc) return;
        for (const f of doc.findings) {
          if (!doc.measurements.some((m) => m.label === f.label)) doc.measurements.push({ label: f.label, shape: structuredClone(f.area) });
        }
        markDirty(doc);
        void refreshAnalysis(doc);
        viewer.draw();
      },
      setAsset: (asset) => {
        const doc = currentDoc();
        if (!doc) return;
        doc.asset = asset;
        markDirty(doc);
        renderFiles();
      },
      setField: (name, value) => {
        const doc = currentDoc();
        if (!doc) return;
        if (value.trim()) doc.fields[name] = value;
        else delete doc.fields[name];
        markDirty(doc);
      },
      setAnnotationText: (index, text) => {
        const doc = currentDoc();
        const a = doc?.annotations[index];
        if (!doc || !a || a.kind !== "text") return;
        if (text.trim()) a.text = text.trim();
        else doc.annotations.splice(index, 1);
        markDirty(doc);
        viewer.draw();
        panel.renderInspection(doc);
      },
      deleteAnnotation: (index) => {
        const doc = currentDoc();
        if (!doc) return;
        doc.annotations.splice(index, 1);
        markDirty(doc);
        viewer.draw();
        panel.renderInspection(doc);
      },
      setNotes: (notes) => {
        const doc = currentDoc();
        if (!doc) return;
        doc.notes = notes;
        markDirty(doc);
      },
      seek: (frame) => void seek(frame),
      togglePlay: () => togglePlay(),
      setHsm: (patch) => {
        const doc = currentDoc();
        if (!doc?.video) return;
        Object.assign(doc.video, patch);
        void updateHsm(doc);
        panel.renderVideo(doc);
      },
      setInOut: (first, last) => {
        const doc = currentDoc();
        if (!doc?.video) return;
        doc.video.first = first;
        doc.video.last = last;
        panel.renderVideo(doc);
      },
      plotSeries: () => void plotSeries(),
      exportVideo: () => void exportVideo(),
      exportSegment: () => void exportSegment(),
  };
}
