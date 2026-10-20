// The native file and message dialogs, behind one object so automated UI checks (debug builds
// with __DEV_HOOKS__) can answer them without a person clicking.
import { ask, open, save } from "@tauri-apps/plugin-dialog";

export const pickers = { open, save, ask };
