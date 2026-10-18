// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ResultGrid } from "./ResultGrid";
import { chooseOption } from "../test-utils/select";
import { ROW_HEIGHT } from "../grid";

// Layout is unavailable in happy-dom. Render every row in the requested virtual
// range so these tests exercise pagination and indexing independently of sizing.
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * ROW_HEIGHT,
    getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ key: index, index, size: ROW_HEIGHT, start: index * ROW_HEIGHT })),
  }),
}));
afterEach(cleanup);
const result = { columns: ["Name"], rows: Array.from({ length: 63 }, (_, index) => [`Record ${index + 1}`]), summary: "63 rows" };

it("pages returned rows with absolute row numbers and disables navigation at the boundaries", () => {
  render(<ResultGrid result={result} />);
  expect(screen.getByRole("status").textContent).toBe("1–25 of 63 rows");
  expect(screen.getByRole("button", { name: "Previous page" }).hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  expect(screen.getByRole("status").textContent).toBe("26–50 of 63 rows");
  expect(screen.queryByText("Record 1")).toBeNull();
  expect(screen.getByRole("rowheader", { name: "26" }).parentElement?.getAttribute("aria-rowindex")).toBe("27");
  expect(screen.getByText("Record 26")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Last page" }));
  expect(screen.getByRole("status").textContent).toBe("51–63 of 63 rows");
  expect(screen.getAllByRole("gridcell")).toHaveLength(13);
  expect(screen.getByRole("button", { name: "Next page" }).hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "First page" }));
  expect(screen.getByText("Record 1")).toBeTruthy();
});

it("resets to the first page when changing page size or receiving a new result", async () => {
  const view = render(<ResultGrid result={result} />);
  fireEvent.click(screen.getByRole("button", { name: "Last page" }));
  await chooseOption(screen.getByRole("combobox", { name: "Rows per page" }), "50");
  expect(screen.getByRole("status").textContent).toBe("1–50 of 63 rows");
  expect(screen.getAllByRole("gridcell")).toHaveLength(50);
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  view.rerender(<ResultGrid result={{ ...result, rows: [["Fresh result"]] }} />);
  expect(screen.getByRole("status").textContent).toBe("1–1 of 1 rows");
  expect(screen.getByText("Fresh result")).toBeTruthy();
});

it("handles empty results without an invalid range or an enabled next page", () => {
  render(<ResultGrid result={{ ...result, rows: [] }} />);
  expect(screen.getByRole("status").textContent).toBe("0 of 0 rows");
  expect(screen.getByText("Page 1 of 1")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Next page" }).hasAttribute("disabled")).toBe(true);
});
