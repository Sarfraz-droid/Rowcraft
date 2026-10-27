// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ResultGrid, type GridEditing } from "./ResultGrid";
import type { RowChanges } from "../api";
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

const editable = (onSave = vi.fn(async (_changes: RowChanges) => true), extra: Partial<GridEditing> = {}): GridEditing => ({ editable: [true], key: [false], onSave, ...extra });

it("selects rows with click, modifier and shift clicks, marks them deleted, and saves", async () => {
  const onSave = vi.fn(async (_changes: RowChanges) => true);
  render(<ResultGrid result={result} editing={editable(onSave)} />);
  const row = (n: number) => screen.getByRole("rowheader", { name: String(n) }).parentElement!;
  fireEvent.click(row(2));
  fireEvent.click(row(5), { shiftKey: true });
  fireEvent.click(row(10), { metaKey: true });
  expect(row(3).getAttribute("aria-selected")).toBe("true");
  expect(row(6).getAttribute("aria-selected")).toBe("false");
  expect(screen.getByText("5 selected")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /Delete rows/ }));
  expect(onSave).not.toHaveBeenCalled();
  expect(row(3).className).toContain("deleted");
  expect(screen.getByText("5 pending changes")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /Save/ }));
  await vi.waitFor(() => expect(onSave).toHaveBeenCalledWith({ deletes: [[`Record 2`], [`Record 3`], [`Record 4`], [`Record 5`], [`Record 10`]], updates: [], inserts: [] }));
  await vi.waitFor(() => expect(screen.queryByText("5 pending changes")).toBeNull());
});

it("edits cells in place, adds rows, and discards or saves them together", async () => {
  const onSave = vi.fn(async (_changes: RowChanges) => true);
  const data = { columns: ["id", "name"], rows: [[1, "Ada"], [2, null]], summary: "2 rows" };
  render(<ResultGrid result={data} editing={{ editable: [false, true], key: [true, false], onSave }} />);
  fireEvent.doubleClick(screen.getByText("Ada"));
  const input = screen.getByRole("textbox", { name: "Edit name" });
  fireEvent.change(input, { target: { value: "Grace" } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(screen.getByText("Grace").className).toContain("edited");
  // The id column is not editable: double-clicking it opens nothing.
  fireEvent.doubleClick(screen.getAllByRole("gridcell")[0]!);
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Add row" }));
  const added = screen.getByRole("textbox", { name: "Edit name" });
  fireEvent.change(added, { target: { value: "Linus" } });
  fireEvent.keyDown(added, { key: "Enter" });
  expect(screen.getByText("DEFAULT")).toBeTruthy();
  expect(screen.getByText("2 pending changes")).toBeTruthy();
  fireEvent.keyDown(screen.getByRole("grid"), { key: "s", metaKey: true });
  await vi.waitFor(() => expect(onSave).toHaveBeenCalledWith({ deletes: [], updates: [{ row: [1, "Ada"], values: { name: "Grace" } }], inserts: [{ name: "Linus" }] }));
});

it("drops an edit that restores the original value and cancels with Escape", () => {
  const data = { columns: ["n"], rows: [[42]], summary: "1 row" };
  render(<ResultGrid result={data} editing={editable()} />);
  fireEvent.doubleClick(screen.getByText("42"));
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "7" } });
  fireEvent.keyDown(screen.getByRole("textbox"), { key: "Escape" });
  expect(screen.getByText("42")).toBeTruthy();
  fireEvent.doubleClick(screen.getByText("42"));
  fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
  expect(screen.queryByText(/pending change/)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Add row" }));
  fireEvent.keyDown(screen.getByRole("textbox"), { key: "Escape" });
  expect(screen.getByText("1 pending change")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
  expect(screen.queryByText(/pending change/)).toBeNull();
});

it("sets NULL from the context menu", () => {
  const data = { columns: ["name"], rows: [["Ada"]], summary: "1 row" };
  render(<ResultGrid result={data} editing={editable()} />);
  fireEvent.contextMenu(screen.getByText("Ada"));
  fireEvent.click(screen.getByRole("menuitem", { name: "Set to NULL" }));
  expect(screen.getByText("NULL").className).toContain("edited");
  expect(screen.queryByRole("menu")).toBeNull();
});

it("selects the page from the header and offers no editing without a handler", () => {
  render(<ResultGrid result={result} readOnly="Joins are read-only." />);
  fireEvent.click(screen.getByRole("checkbox", { name: "Select all rows on this page" }));
  expect(screen.getByText("25 selected")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /Delete/ })).toBeNull();
  expect(screen.queryByRole("button", { name: "Add row" })).toBeNull();
  expect(screen.getByText("Read-only").getAttribute("title")).toBe("Joins are read-only.");
  fireEvent.keyDown(screen.getByRole("grid"), { key: "Escape" });
  expect(screen.queryByText("25 selected")).toBeNull();
});

it("keeps changes disabled with the reason, and locks existing rows of a table without a key", () => {
  const view = render(<ResultGrid result={result} editing={editable(undefined, { disabled: "Connect first." })} />);
  fireEvent.click(screen.getByRole("rowheader", { name: "1" }).parentElement!);
  const button = screen.getByRole("button", { name: /Delete row/ });
  expect(button.hasAttribute("disabled")).toBe(true);
  expect(button.getAttribute("title")).toBe("Connect first.");
  view.rerender(<ResultGrid result={result} editing={editable(undefined, { locked: "No primary key." })} />);
  expect(screen.getByRole("button", { name: /Delete row/ }).getAttribute("title")).toBe("No primary key.");
  fireEvent.doubleClick(screen.getByText("Record 1"));
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.getByRole("button", { name: "Add row" }).hasAttribute("disabled")).toBe(false);
});
