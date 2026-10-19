// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createWorkspaceTab } from "../workspace-tabs";
import { TableWorkspace } from "./TableWorkspace";

afterEach(cleanup);
it.each(["mongo", "redis", "neo4j", "graphql"])("does not claim relational constraints or nullability for %s metadata", dialect => {
  render(<TableWorkspace dialect={dialect} tab={createWorkspaceTab("source", "table", {table:"items"})}
    metadata={{name:"items",kind:"Table",columns:[{name:"id",native_type:"string",nullable:false}]}}
    connected busy={false} onRefresh={vi.fn()} onShowAll={vi.fn()} onOpenConsole={vi.fn()} onExport={vi.fn()} onResultView={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", {name:/Structure/}));
  expect(screen.getByText("id")).toBeTruthy();
  expect(screen.queryByRole("columnheader", {name:"Nullable"})).toBeNull();
  expect(screen.queryByRole("heading", {name:"Constraints"})).toBeNull();
});
it("retains relational schema metadata for SQL sources", () => {
  render(<TableWorkspace dialect="pg" tab={createWorkspaceTab("source", "table", {table:"items"})}
    metadata={{name:"items",kind:"Table",columns:[{name:"id",native_type:"integer",nullable:false}]}}
    connected busy={false} onRefresh={vi.fn()} onShowAll={vi.fn()} onOpenConsole={vi.fn()} onExport={vi.fn()} onResultView={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", {name:/Structure/}));
  expect(screen.getByRole("columnheader", {name:"Nullable"})).toBeTruthy();
  expect(screen.getByRole("heading", {name:"Constraints"})).toBeTruthy();
});
