// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { ResultDocuments } from "./ResultDocuments";
import type { QueryOutput } from "../api";
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const result = (): QueryOutput => ({ columns: ["_id", "profile", "code", "active", "missing"], rows: Array.from({ length: 26 }, (_, index) => [String(index), '{"tags":["a","b"],"age":31}', "001", true, null]), summary: "26 records" });

it("keeps document IDs visible on collapsed records, including ObjectIds and zero", () => {
  render(<ResultDocuments result={{ columns: ["_id"], rows: [['{"$oid":"65ae471bef7c5c856aa7ae0e"}'], [0]], summary: "2 documents" }} />);
  fireEvent.click(screen.getByRole("button", { name: "Collapse records" }));
  expect(within(screen.getByRole("region", { name: "Record 1" })).getByTitle("_id: 65ae471bef7c5c856aa7ae0e").textContent).toBe("_id: 65ae471bef7c5c856aa7ae0e");
  expect(within(screen.getByRole("region", { name: "Record 2" })).getByTitle("_id: 0").textContent).toBe("_id: 0");
});

it("shows typed document fields, lazily expands nested JSON, and pages through records", () => {
  render(<ResultDocuments result={result()} />);
  const record = screen.getByRole("region", { name: "Record 1" });
  expect(record.textContent).toContain('"code": "001"');
  expect(record.textContent).toContain('"active": true');
  expect(record.textContent).toContain('"missing": null');
  expect(within(record).queryByText('"tags":')).toBeNull();
  fireEvent.click(within(record).getByRole("button", { name: "Expand Record 1.profile" }));
  expect(within(record).getByText('"tags":')).toBeTruthy();
  fireEvent.click(within(record).getByRole("button", { name: "Expand Record 1.profile.tags" }));
  expect(within(record).getByText('"a"')).toBeTruthy();
  expect(screen.queryByText("Record 26")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  expect(screen.getByText("Record 26")).toBeTruthy();
  expect(screen.getByText("26–26 of 26 records")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Next" }).hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Previous" }));
  expect(screen.getByText("Record 1")).toBeTruthy();
});

it("collapses and expands records and resets pagination when results change", () => {
  const { rerender } = render(<ResultDocuments result={result()} />);
  fireEvent.click(screen.getByRole("button", { name: "Collapse records" }));
  expect(screen.queryByText('"code":')).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Expand records" }));
  expect(screen.getAllByText('"code":')).toHaveLength(25);
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  rerender(<ResultDocuments result={{ columns: ["name"], rows: [["fresh"]], summary: "1 record" }} />);
  expect(screen.getByText('"fresh"')).toBeTruthy();
  expect(screen.getByText("1–1 of 1 records")).toBeTruthy();
});

it("copies the complete document as JSON even when nested fields are collapsed", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  render(<ResultDocuments result={result()} />);
  fireEvent.click(screen.getByRole("button", { name: "Copy Record 1 as JSON" }));
  await waitFor(() => expect(screen.getByRole("status").textContent).toBe("Record 1 copied as JSON."));
  expect(JSON.parse(writeText.mock.calls[0]![0])).toEqual({ _id: "0", profile: { tags: ["a", "b"], age: 31 }, code: "001", active: true, missing: null });
});

it("explains empty results and clipboard failures", async () => {
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: vi.fn().mockRejectedValue(new Error("unavailable")) } });
  const { rerender } = render(<ResultDocuments result={result()} />);
  fireEvent.click(screen.getByRole("button", { name: "Copy Record 1 as JSON" }));
  await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Could not copy"));
  rerender(<ResultDocuments result={{ columns: [], rows: [], summary: "0 documents" }} />);
  expect(screen.getByText("No documents matched this query.")).toBeTruthy();
});
