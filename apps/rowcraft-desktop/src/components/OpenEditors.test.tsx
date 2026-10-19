// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createWorkspaceTab } from "../workspace-tabs";
import { OpenEditors } from "./OpenEditors";

afterEach(cleanup);
it("runs header actions while collapsed without toggling the accordion", () => {
  const onNew = vi.fn(), onSaveAll = vi.fn(), onCloseAll = vi.fn();
  const { rerender } = render(<OpenEditors tabs={tabs} activeId={null} busy={false} onSelect={vi.fn()} onClose={vi.fn()} onNew={onNew} onSaveAll={onSaveAll} onCloseAll={onCloseAll} />);
  const heading = screen.getByRole("button", { name: "Open editors" });
  fireEvent.click(heading);
  const toolbar = within(screen.getByRole("toolbar", { name: "Editor actions" }));
  fireEvent.click(toolbar.getByRole("button", { name: "New console" }));
  fireEvent.click(toolbar.getByRole("button", { name: "Save all editors locally" }));
  fireEvent.click(toolbar.getByRole("button", { name: "Close all editors" }));
  expect(onNew).toHaveBeenCalledOnce();
  expect(onSaveAll).toHaveBeenCalledOnce();
  expect(onCloseAll).toHaveBeenCalledOnce();
  expect(heading.getAttribute("aria-expanded")).toBe("false");
  rerender(<OpenEditors tabs={[]} activeId={null} busy={false} onSelect={vi.fn()} onClose={vi.fn()} onNew={onNew} onSaveAll={onSaveAll} onCloseAll={onCloseAll} />);
  expect((toolbar.getByRole("button", { name: "Save all editors locally" }) as HTMLButtonElement).disabled).toBe(true);
  expect((toolbar.getByRole("button", { name: "Close all editors" }) as HTMLButtonElement).disabled).toBe(true);
  expect((toolbar.getByRole("button", { name: "New console" }) as HTMLButtonElement).disabled).toBe(false);
});
const tabs = [
  createWorkspaceTab("Local", "console", { title: "Console 1" }),
  createWorkspaceTab("Production", "console", { title: "Console 1" }),
  createWorkspaceTab("Production", "table", { schema: "public", table: "users" }),
];

it("switches and closes the correct tab when titles repeat across sources", () => {
  const onSelect = vi.fn(), onClose = vi.fn();
  render(<OpenEditors tabs={tabs} activeId={tabs[1]!.id} busy={false} onSelect={onSelect} onClose={onClose} />);
  const active = screen.getByRole("button", { name: "Console 1 Production" });
  expect(active.getAttribute("aria-current")).toBe("page");
  fireEvent.click(active);
  expect(onSelect).toHaveBeenCalledWith(tabs[1]!.id);
  fireEvent.click(screen.getByRole("button", { name: "Close Console 1 for Local" }));
  expect(onClose).toHaveBeenCalledWith(tabs[0]!.id);
  expect(onSelect).toHaveBeenCalledTimes(1);
});

it("filters by title, source, and schema and collapses without losing the filter", () => {
  render(<OpenEditors tabs={tabs} activeId={null} busy={false} onSelect={vi.fn()} onClose={vi.fn()} />);
  const search = screen.getByRole("textbox", { name: "Find an open editor" });
  const list = screen.getByRole("list", { name: "Open tabs" });
  for (const [query, count] of [["users", 1], ["Production", 2], ["public", 1], ["missing", 0]] as const) {
    fireEvent.change(search, { target: { value: query } });
    expect(within(list).queryAllByRole("listitem")).toHaveLength(count);
  }
  expect(screen.getByText("No matching tabs.")).toBeTruthy();
  const heading = screen.getByRole("button", { name: "Open editors" });
  fireEvent.click(heading);
  expect(heading.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByRole("list", { name: "Open tabs" })).toBeNull();
  fireEvent.click(heading);
  expect(search.getAttribute("value") ?? (search as HTMLInputElement).value).toBe("missing");
  fireEvent.click(screen.getByRole("button", { name: "Clear editor search" }));
  expect(within(list).getAllByRole("listitem")).toHaveLength(3);
});

it("disables tab actions while busy and handles an empty workspace", () => {
  const { rerender } = render(<OpenEditors tabs={tabs} activeId={null} busy onSelect={vi.fn()} onClose={vi.fn()} />);
  for (const button of within(screen.getByRole("list")).getAllByRole("button")) expect((button as HTMLButtonElement).disabled).toBe(true);
  rerender(<OpenEditors tabs={[]} activeId={null} busy={false} onSelect={vi.fn()} onClose={vi.fn()} />);
  expect(screen.getByText("No open editors.")).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
});
