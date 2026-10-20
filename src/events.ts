import { deleteMeasurement, refreshAnalysis } from "./analysis";
import { type Unit, api } from "./api";
import { batchDialog } from "./batch";
import { openPaths, pickFiles, pickFolder, selectDoc } from "./docs";
import { $, debounce, errorText, toast } from "./dom";
import { TOOLS } from "./layout";
import { reloadTemperatures } from "./params";
import { pickers } from "./pickers";
import { createReport } from "./reports";
import { saveDoc, saveEdits } from "./saveExport";
import { type Tool, currentDoc, state } from "./state";
import { markDirty, setTool, setUnit, updateZoomLabel } from "./status";
import { panel, viewer } from "./ui";
import { seek, togglePlay, writeVideo } from "./video";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function bindEvents(): void {
  $("#open").onclick = () => void pickFiles();
  $("#empty-open").onclick = () => void pickFiles();
  $("#open-folder").onclick = () => void pickFolder();
  $("#empty-folder").onclick = () => void pickFolder();
  $("#save").onclick = () => void saveEdits();
  $("#batch").onclick = () =>
    void batchDialog({
      applyParams: async (doc, params) => {
        const r = await api.setParams(doc.info.id, params);
        await reloadTemperatures(doc, r.params);
      },
      refreshAnalysis: (doc) => refreshAnalysis(doc, false),
      saveEdits: (doc) => saveDoc(doc),
      writeVideo: (doc, path) => writeVideo(doc, path),
      markDirty,
      redraw: () => selectDoc(state.current),
    });
  $("#zoom-in").onclick = () => viewer.zoomBy(1.25);
  $("#zoom-out").onclick = () => viewer.zoomBy(0.8);
  $("#zoom-fit").onclick = () => viewer.fit();
  document.querySelectorAll<HTMLButtonElement>("button.tool").forEach((b) => (b.onclick = () => setTool(b.dataset.tool as Tool)));
  document.querySelectorAll<HTMLButtonElement>("[data-unit]").forEach((b) => (b.onclick = () => setUnit(b.dataset.unit as Unit)));

  window.addEventListener("keydown", (e) => {
    const target = e.target as HTMLElement;
    const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
    const mod = e.metaKey || e.ctrlKey;
    if (document.querySelector("dialog[open]")) return;
    if (mod && e.key.toLowerCase() === "o") {
      e.preventDefault();
      void (e.shiftKey ? pickFolder() : pickFiles());
    } else if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void saveEdits();
    } else if (mod && e.key.toLowerCase() === "p") {
      e.preventDefault();
      void createReport();
    } else if (typing || mod || viewer.isDrawing()) {
      return;
    } else if ((e.key === "Delete" || e.key === "Backspace") && state.selected !== null) {
      e.preventDefault();
      deleteMeasurement(state.selected);
    } else if (e.key === "Escape") {
      state.selected = null;
      viewer.selected = null;
      setTool("select");
      viewer.draw();
      panel.renderMeasurements(currentDoc());
    } else if (currentDoc()?.video && (e.key === "," || e.key === ".")) {
      const v = currentDoc()!.video!;
      void seek(v.frame + (e.key === "," ? -1 : 1));
    } else if (currentDoc()?.video && e.key.toLowerCase() === "k") {
      togglePlay();
    } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      if (state.current < state.docs.length - 1) selectDoc(state.current + 1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      if (state.current > 0) selectDoc(state.current - 1);
    } else if (e.key === "f" || e.key === "F") {
      viewer.fit();
    } else if (e.key === "+" || e.key === "=") {
      viewer.zoomBy(1.25);
    } else if (e.key === "-") {
      viewer.zoomBy(0.8);
    } else {
      const tool = TOOLS.find(([, , key]) => key.toLowerCase() === e.key.toLowerCase());
      if (tool && currentDoc()) setTool(tool[0]);
    }
  });

  window.addEventListener("resize", debounce(() => updateZoomLabel(), 100));

  // Anything not caught by a call site still reaches the user instead of only the console.
  window.addEventListener("unhandledrejection", (ev) => {
    toast(errorText(ev.reason), "error");
  });
  window.addEventListener("error", (ev) => {
    toast(errorText(ev.error ?? ev.message), "error");
  });

  void getCurrentWebview().onDragDropEvent((event) => {
    const overlay = $("#drop");
    if (event.payload.type === "enter" || event.payload.type === "over") overlay.classList.add("visible");
    else overlay.classList.remove("visible");
    if (event.payload.type === "drop") void openPaths(event.payload.paths);
  });

  void getCurrentWindow().onCloseRequested(async (event) => {
    const dirty = state.docs.filter((d) => d.dirty);
    if (dirty.length === 0) return;
    const ok = await pickers.ask(`${dirty.length} image${dirty.length > 1 ? "s have" : " has"} unsaved edits. Quit anyway?`, {
      title: "Unsaved edits",
      kind: "warning",
      okLabel: "Quit",
      cancelLabel: "Cancel",
    });
    if (!ok) event.preventDefault();
  });
}
