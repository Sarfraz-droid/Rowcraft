import { describe, expect, it } from "vitest";

import { cellLabel, columnWidth, statementToRun } from "./grid";

describe("cellLabel", () => {
  it("tells NULL, text and binary apart", () => {
    expect(cellLabel(null)).toEqual({ text: "NULL", dim: true, numeric: false });
    expect(cellLabel("NULL")).toEqual({ text: "NULL", dim: false, numeric: false });
    expect(cellLabel({ bytes: 3 }).text).toBe("<3 bytes>");
    expect(cellLabel({ bytes: 3 }).dim).toBe(true);
  });

  it("keeps rows single-line", () => {
    expect(cellLabel("a\nb\r\nc").text).toBe("a b  c");
  });

  it("marks numbers so they can be right-aligned", () => {
    expect(cellLabel(42)).toEqual({ text: "42", dim: false, numeric: true });
    expect(cellLabel(true).numeric).toBe(false);
  });
});

describe("columnWidth", () => {
  const rows = [["x", "y".repeat(500)]];

  it("follows the content within bounds", () => {
    expect(columnWidth("id", rows, 0)).toBe(80);
    expect(columnWidth("big", rows, 1)).toBe(360);
  });

  it("widens for a long header", () => {
    expect(columnWidth("a_rather_long_column_name", [], 0)).toBeGreaterThan(80);
  });

  it("only samples the first rows", () => {
    const many = Array.from({ length: 1000 }, (_, i) => [i < 200 ? "a" : "z".repeat(40)]);
    expect(columnWidth("c", many, 0)).toBe(80);
  });
});

describe("statementToRun", () => {
  it("prefers the selection over the whole editor", () => {
    expect(statementToRun("select 2", "select 1;\nselect 2;")).toBe("select 2");
  });

  it("falls back to the whole editor when the selection is blank", () => {
    expect(statementToRun("  \n", " select 1; ")).toBe("select 1;");
    expect(statementToRun("", "select 1")).toBe("select 1");
  });

  it("has nothing to run in an empty editor", () => {
    expect(statementToRun("", " \n ")).toBeNull();
  });
});
