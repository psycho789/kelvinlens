// CSV field escaping shared by the analysis exports.
/** One CSV field: quoted when it holds a comma, quote or newline; quotes doubled. */
export function csvCell(s: string): string {
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}
