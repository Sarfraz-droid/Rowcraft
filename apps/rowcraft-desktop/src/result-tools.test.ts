import { describe, expect, it } from "vitest";
import { chartPoints, numericColumns, resultMarkdown, resultToCsv } from "./result-tools";
import type { QueryOutput } from "./api";

describe("query visualizations", () => {
  const result: QueryOutput = { columns: ["name", "value"], rows: [["negative", -3], ["zero", 0], ["missing", null], ["text number", "42"], ["infinite", Infinity]], summary: "" };
  it("accepts finite numeric cells without coercing null or big-integer text", () => {
    expect(numericColumns(result)).toEqual([1]);
    expect(chartPoints(result, 0, 1)).toEqual([{ label: "negative", value: -3, row: 1 }, { label: "zero", value: 0, row: 2 }]);
  });
  it("preserves original row numbers when omitting nonnumeric values and capping charts", () => {
    expect(chartPoints(result, -1, 1, 1)).toEqual([{ label: "1", value: -3, row: 1 }]);
  });
});

describe("CSV export", () => {
  it("escapes headers, commas, quotes and line breaks while preserving false and zero", () => {
    expect(resultToCsv({ columns: ["label, name", 'a"b'], rows: [["line\nbreak", null], [false, 0]], summary: "" })).toBe('"label, name","a""b"\r\n"line\nbreak",\r\nfalse,0');
  });
});

describe("markdown copy", () => {
  const result: QueryOutput = { columns: ["genaiMappedAnswer", "COUNT(*)"], rows: [["Other", 767], ["Prep | material", 5], ["Multi\nline", 8], [null, 2]], summary: "4 rows" };

  it("renders a Markdown table with numeric columns right-aligned and pipes escaped", () => {
    expect(resultMarkdown(result)).toBe([
      "| genaiMappedAnswer | COUNT(*) |",
      "| --- | ---: |",
      "| Other | 767 |",
      "| Prep \\| material | 5 |",
      "| Multi<br>line | 8 |",
      "| NULL | 2 |",
    ].join("\n"));
  });

  it("copies only the selected rows, in result order, ignoring unsaved new rows", () => {
    expect(resultMarkdown(result, new Set([3, 0, 9])).split("\n").slice(2)).toEqual(["| Other | 767 |", "| NULL | 2 |"]);
  });
});
