import { api } from "./api";
import { openPaths, selectDoc } from "./docs";
import { gradientCss } from "./panel";
import { DEFAULT_INSPECT, loadSettings, palette, state } from "./state";
import { toast, errorText } from "./dom";
import { installDevHooks } from "./devHooks";
import { bindEvents } from "./events";
import { buildLayout } from "./layout";
import { createUi, viewer } from "./ui";

buildLayout();
createUi();
bindEvents();

async function start(): Promise<void> {
  const settings = loadSettings();
  if (settings.unit) state.unit = settings.unit;
  if (settings.smooth !== undefined) state.smooth = settings.smooth;
  if (settings.showExtremes !== undefined) state.showExtremes = settings.showExtremes;
  if (settings.msxStrength !== undefined) state.msxStrength = settings.msxStrength;
  if (settings.customPalettes) state.customPalettes = settings.customPalettes;
  if (settings.customTemplates) state.customTemplates = settings.customTemplates;
  if (settings.inspect) state.inspect = { ...DEFAULT_INSPECT, ...settings.inspect };
  [state.palettes, state.builtinTemplates] = await Promise.all([api.palettes(), api.templates()]);
  if (settings.defaultPalette && palette(settings.defaultPalette, null).id === settings.defaultPalette) state.defaultPalette = settings.defaultPalette;
  viewer.smooth = state.smooth;
  viewer.showExtremes = state.showExtremes;
  document.documentElement.style.setProperty("--brand-gradient", gradientCss(palette("iron", null).colors));
  selectDoc(-1);
  const initial = await api.initialPaths();
  if (initial.length) await openPaths(initial);
}

void start().catch((e) => toast(`Startup failed: ${errorText(e)}`, "error"));

installDevHooks();
