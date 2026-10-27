import type { Cell, QueryOutput } from "./api";

export function numericColumns(result: QueryOutput): number[] {
  return result.columns.flatMap((_, index) => result.rows.some(row => typeof row[index] === "number" && Number.isFinite(row[index])) ? [index] : []);
}
export function chartPoints(result: QueryOutput, label: number, value: number, limit = 50) {
  return result.rows.flatMap((row, index) => {
    const number = row[value];
    return typeof number === "number" && Number.isFinite(number) ? [{ label: label < 0 ? String(index + 1) : cellText(row[label]), value: number, row: index + 1 }] : [];
  }).slice(0, limit);
}
function cellText(cell: Cell | undefined): string {
  if (cell === null || cell === undefined) return "NULL";
  if (typeof cell === "object") return `<${cell.bytes} bytes>`;
  return String(cell);
}
export function resultCsv(result: QueryOutput): string {
  const quote = (text: string) => /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
  return [result.columns, ...result.rows.map(row => row.map(cell => cell === null ? "" : cellText(cell)))].map(row => row.map(quote).join(",")).join("\r\n");
}

export const resultToCsv = resultCsv;

/** A Markdown table of the given row indexes (all rows when omitted); numeric columns align right. */
export function resultMarkdown(result: QueryOutput, rows?: Iterable<number>): string {
  const line = (cells: string[]) => `| ${cells.map(text => text.replaceAll("|", "\\|").replace(/\r?\n/g, "<br>")).join(" | ")} |`;
  const numeric = new Set(numericColumns(result));
  const indexes = rows ? [...rows].sort((a, b) => a - b) : result.rows.map((_, index) => index);
  return [
    line(result.columns),
    `|${result.columns.map((_, index) => numeric.has(index) ? " ---: " : " --- ").join("|")}|`,
    ...indexes.flatMap(index => { const row = result.rows[index]; return row ? [line(row.map(cellText))] : []; }),
  ].join("\n");
}
