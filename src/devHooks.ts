import { refreshAnalysis, runInspect } from "./analysis";
import { api } from "./api";
import { openPaths, selectDoc } from "./docs";
import { exportPng, renderExport } from "./exportImage";
import { pickers } from "./pickers";
import { rapidInfo } from "./reportDialog";
import { reportItems, writeReport } from "./reports";
import { saveEdits, sidecarOf } from "./saveExport";
import { displayRange, lookOf, palette, state } from "./state";
import { setTool, setUnit } from "./status";
import { panel, viewer } from "./ui";
import { seek } from "./video";

export function installDevHooks(): void {
  if (!__DEV_HOOKS__) return;
  Object.assign(window, {
    kelvinlensDev: {
      state,
      api,
      viewer,
      panel,
      exportPng,
      renderExport,
      palette,
      displayRange,
      selectDoc,
      openPaths,
      refreshAnalysis,
      runInspect,
      seek,
      setTool,
      setUnit,
      saveEdits,
      writeReport,
      reportItems,
      rapidInfo,
      sidecarOf,
      lookOf,
      pickers,
      handlers: (panel as unknown as { on: unknown }).on,
    },
  });
}
