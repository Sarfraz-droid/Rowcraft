// Pure helpers for the result grid and the editor, kept apart from React so they are unit-tested.

import type { Cell } from "./api";

export const ROW_HEIGHT = 24;
export const GUTTER_WIDTH = 56;
const MIN_COL_WIDTH = 80;
const MAX_COL_WIDTH = 360;
const CHAR_WIDTH = 8;
const SAMPLE_ROWS = 200;

export interface CellLabel {
  text: string;
  /** NULL and binary placeholders are drawn dimmed so they are not mistaken for text. */
  dim: boolean;
  numeric: boolean;
}

export function cellLabel(cell: Cell): CellLabel {
  if (cell === null) return { text: "NULL", dim: true, numeric: false };
  switch (typeof cell) {
    case "boolean":
      return { text: String(cell), dim: false, numeric: false };
    case "number":
      return { text: String(cell), dim: false, numeric: true };
    case "string":
      // A newline would break the single-line row height.
      return { text: cell.replace(/[\r\n]/g, " "), dim: false, numeric: false };
    default:
      return { text: `<${cell.bytes} bytes>`, dim: true, numeric: false };
  }
}

/**
 * Column width from the header and a sample of the first rows, so wide data does not need a scan
 * of every row.
 */
export function columnWidth(name: string, rows: Cell[][], col: number): number {
  let widest = [...name].length;
  for (const row of rows.slice(0, SAMPLE_ROWS)) {
    const cell = row[col];
    if (cell !== undefined) widest = Math.max(widest, [...cellLabel(cell).text].length);
  }
  return Math.min(MAX_COL_WIDTH, Math.max(MIN_COL_WIDTH, widest * CHAR_WIDTH + 24));
}

/** The text to execute: the selection if there is one, otherwise the whole editor. */
export function statementToRun(selection: string, all: string): string | null {
  const text = (selection.trim() === "" ? all : selection).trim();
  return text === "" ? null : text;
}
