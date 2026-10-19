// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { api, type SourceProperties } from "../api";
import { Sidebar, WORKSPACE_VIEWS } from "./Sidebar";

vi.mock("../api", async original => ({ ...await original<typeof import("../api")>(), api: { sourceProperties: vi.fn() } }));

const details: SourceProperties = {
  name: "Beta", dialect: "mysql", driver: "mysql", host: "db.example", port: 3306,
  database: "app", user: "reader", connection_string: "mysql://reader@db.example:3306/app",
  connection_type: "Production", comment: "Reporting connection", read_only: true,
  ssl_mode: "require", auth: "user-password", ssh_tunnel: "Bastion", schema_pattern: null, object_filter: null,
};
const navigate = vi.fn();
const refresh = vi.fn();
const reconnect = vi.fn();
const newConsole = vi.fn();
const add = vi.fn();
const edit = vi.fn();
function start() {
  return render(<Sidebar tabs={[]} activeTabId={null} onSelectTab={vi.fn()} onCloseTab={vi.fn()} workspaces={[{ id: "work", name: "Work", sources: ["Alpha", "Beta"] }]} workspaceId="work" onSwitchWorkspace={vi.fn()} onCreateWorkspace={vi.fn()} onRenameWorkspace={vi.fn()}
    sources={["Alpha", "Beta"].map(name => ({ name, dialect: "mysql", host: "db.example", ssh_tunnel: null, color: null }))} selected="Alpha" connected="Alpha" view="dashboard" busy={false}
    onNavigate={navigate} onEdit={edit} onAdd={add} onRemove={vi.fn()} catalog={null} onOpenTable={vi.fn()} onRefresh={vi.fn()} onRefreshSource={refresh} onReconnect={reconnect} onNewConsole={newConsole} hidden={false} />);
}
async function openDots() {
  fireEvent.pointerDown(screen.getByRole("button", { name: "Actions for Beta" }), { button: 0, ctrlKey: false, pointerType: "mouse" });
  return screen.findByRole("menu", { name: "Actions for Beta" });
}
function rightClick() {
  fireEvent.contextMenu(screen.getByRole("button", { name: /Beta mysql/ }), { clientX: 50, clientY: 80 });
  return screen.findByRole("menu", { name: "Actions for Beta" });
}
beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  vi.mocked(api.sourceProperties).mockResolvedValue(details);
  vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("collapses connections independently of open editors and preserves the source filter", () => {
  start();
  const search = screen.getByRole("textbox", { name: "Find a data source" });
  fireEvent.change(search, { target: { value: "Beta" } });
  const connections = screen.getByRole("button", { name: "Connections" });
  fireEvent.click(connections);
  expect(connections.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByRole("textbox", { name: "Find a data source" })).toBeNull();
  expect(screen.getByRole("button", { name: "Open editors" }).getAttribute("aria-expanded")).toBe("true");
  fireEvent.click(connections);
  expect((screen.getByRole("textbox", { name: "Find a data source" }) as HTMLInputElement).value).toBe("Beta");
  expect(screen.getByRole("button", { name: /Beta mysql/ })).toBeTruthy();
  expect(screen.queryByRole("button", { name: /Alpha mysql/ })).toBeNull();
});

it("offers the same connection actions in the dots and right-click menus", async () => {
  start();
  const menu = await openDots();
  const labels = within(menu).getAllByRole("menuitem").map(item => item.textContent);
  for (const view of WORKSPACE_VIEWS) expect(within(menu).getByRole("menuitem", { name: view.label })).toBeTruthy();
  expect(within(menu).getByRole("menuitem", { name: "Properties…" })).toBeTruthy();
  fireEvent.keyDown(menu, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  const context = await rightClick();
  expect(within(context).getAllByRole("menuitem").map(item => item.textContent)).toEqual(labels);
  fireEvent.click(within(context).getByRole("menuitem", { name: /^SQL console$/ }));
  expect(navigate).toHaveBeenCalledWith("Beta", "console");
});

it("copies the clicked connection string without changing the selected source", async () => {
  start();
  const menu = await rightClick();
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Copy connection string" }));
  await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith(details.connection_string));
  expect(api.sourceProperties).toHaveBeenCalledWith("Beta");
  expect(navigate).not.toHaveBeenCalled();
});

it("opens the shared connection editor for the clicked source", async () => {
  start();
  const menu = await openDots();
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Properties…" }));
  expect(edit).toHaveBeenCalledWith("Beta");
  expect(navigate).not.toHaveBeenCalled();
});

it("offers opening a data source from the workspace background", async () => {
  start();
  fireEvent.contextMenu(screen.getByRole("complementary", { name: "Data sources" }), { clientX: 20, clientY: 300 });
  const menu = await screen.findByRole("menu", { name: "Workspace actions" });
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Open a data source…" }));
  expect(add).toHaveBeenCalledOnce();
});

it("resizes the sidebar by dragging, remembers the width, and resets on double-click", () => {
  start();
  const sidebar = screen.getByRole("complementary", { name: "Data sources" });
  vi.spyOn(sidebar, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 244, 600));
  const divider = screen.getByRole("separator", { name: "Resize data sources sidebar" });
  Object.defineProperty(divider, "setPointerCapture", { value: vi.fn() });
  fireEvent.pointerDown(divider, { button: 0, pointerId: 1, clientX: 244 });
  fireEvent.pointerMove(divider, { pointerId: 1, clientX: 364 });
  fireEvent.pointerUp(divider, { pointerId: 1 });
  expect(sidebar.style.width).toBe("364px");
  expect(localStorage.getItem("rowcraft.sidebar-width")).toBe("364");
  cleanup(); start();
  expect(screen.getByRole("complementary", { name: "Data sources" }).style.width).toBe("364px");
  fireEvent.doubleClick(screen.getByRole("separator", { name: "Resize data sources sidebar" }));
  expect(screen.getByRole("complementary", { name: "Data sources" }).style.width).toBe("");
  expect(localStorage.getItem("rowcraft.sidebar-width")).toBeNull();
});

it("supports keyboard resizing and enforces width limits", () => {
  start();
  const divider = screen.getByRole("separator", { name: "Resize data sources sidebar" });
  fireEvent.keyDown(divider, { key: "Home" });
  expect(divider.getAttribute("aria-valuenow")).toBe("160");
  fireEvent.keyDown(divider, { key: "ArrowLeft" });
  expect(divider.getAttribute("aria-valuenow")).toBe("160");
  fireEvent.keyDown(divider, { key: "ArrowRight" });
  expect(divider.getAttribute("aria-valuenow")).toBe("170");
  fireEvent.keyDown(divider, { key: "End" });
  expect(divider.getAttribute("aria-valuenow")).toBe(divider.getAttribute("aria-valuemax"));
  fireEvent.keyDown(divider, { key: "Enter" });
  expect(localStorage.getItem("rowcraft.sidebar-width")).toBeNull();
});
