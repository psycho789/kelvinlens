// Path helpers shared by the UI. Paths come from the OS file dialogs, so both separators occur.
function sepOf(path: string): string {
  return path.includes("\\") && !path.includes("/") ? "\\" : "/";
}

/** File name without its extension. */
export function stem(name: string): string {
  return name.replace(/\.[^.]+$/, "");
}

/** Last path component. */
export function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** `dir` + `name` with the separator `dir` already uses. */
export function join(dir: string, name: string): string {
  const sep = sepOf(dir);
  return dir.endsWith(sep) ? dir + name : dir + sep + name;
}

/** `name` in the same folder as the file at `path`. */
export function siblingPath(path: string, name: string): string {
  const sep = sepOf(path);
  return path.slice(0, path.lastIndexOf(sep) + 1) + name;
}
