import type { CmdError } from "./api";

export function isCmdError(e: unknown): e is CmdError {
  return typeof e === "object" && e !== null && typeof (e as CmdError).kind === "string" && typeof (e as CmdError).message === "string";
}

/** The text to show for anything a command call can reject with. */
export function errorText(e: unknown): string {
  if (isCmdError(e)) return e.kind === "internal" ? `Internal error: ${e.message}` : e.message;
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
