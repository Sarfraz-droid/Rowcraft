// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { api, type DownloadInventory } from "../api";
import { DownloadedDrivers } from "./DownloadedDrivers";
import { SettingsDialog } from "./SettingsDialog";
import { DEFAULT_EDITOR_PREFERENCES } from "../preferences";
import { chooseOption } from "../test-utils/select";

const inventory: DownloadInventory = {
  native: [{ id: "postgresql", name: "PostgreSQL", version: null }],
  directory: "/app/jdbc", available: [{ id: "duckdb", name: "DuckDB", version: "1.5.6.0" }, { id: "hsqldb_local", name: "HyperSQL", version: "2.7.4" }],
  packages: [{ id: "duckdb", name: "DuckDB", version: "1.5.6.0", available_version: "1.5.6.0", verified: true,
    directory: "/app/jdbc/duckdb", size: 2048, files: [{ name: "duckdb.jar", path: "/app/jdbc/duckdb/duckdb.jar", size: 2048 }] }],
};
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("opens downloaded drivers from Settings and shows version, files, and storage", async () => {
  Object.defineProperty(HTMLDialogElement.prototype, "showModal", { configurable: true, value: function (this: HTMLDialogElement) { this.open = true; } });
  vi.spyOn(api, "downloadedDrivers").mockResolvedValue(inventory);
  vi.spyOn(api, "driverPackage").mockResolvedValue({ distribution: "maven", version: "1.5.6.0", installed: true, source: null, files: [] });
  render(<SettingsDialog initialSection="general" theme="light" editorPreferences={DEFAULT_EDITOR_PREFERENCES} onSaveGeneral={vi.fn()} onClose={vi.fn()} />);
  expect(api.downloadedDrivers).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Drivers" }));
  await screen.findByText("2.0 KB stored");
  fireEvent.click(screen.getByText("Built-in connectors · 1 included with app"));
  expect(screen.getByText("PostgreSQL")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Remove PostgreSQL" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: /DuckDB.*v1.5.6.0/ }));
  await screen.findByText("duckdb.jar");
  expect(screen.getByText("/app/jdbc/duckdb")).toBeTruthy();
  await screen.findByRole("button", { name: "Download again" });
});

it("requires explicit removal confirmation and updates the list", async () => {
  vi.spyOn(api, "downloadedDrivers").mockResolvedValue(inventory);
  const remove = vi.spyOn(api, "removeDownloadedDriver").mockResolvedValue();
  const busy = vi.fn();
  render(<DownloadedDrivers onBusyChange={busy} />);
  fireEvent.click(await screen.findByRole("button", { name: "Remove DuckDB" }));
  expect(remove).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Keep driver" }));
  expect(screen.queryByRole("button", { name: "Remove driver" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Remove DuckDB" }));
  fireEvent.click(screen.getByRole("button", { name: "Remove driver" }));
  await screen.findByText("No downloaded drivers");
  expect(remove).toHaveBeenCalledExactlyOnceWith("duckdb");
  expect(busy).toHaveBeenLastCalledWith(false);
});

it("keeps the package visible when removal fails and allows retry", async () => {
  vi.spyOn(api, "downloadedDrivers").mockResolvedValue(inventory);
  const remove = vi.spyOn(api, "removeDownloadedDriver").mockRejectedValueOnce(new Error("Close connections and try again.")).mockResolvedValueOnce();
  render(<DownloadedDrivers onBusyChange={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Remove DuckDB" }));
  fireEvent.click(screen.getByRole("button", { name: "Remove driver" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("article", { name: "DuckDB" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Remove driver" }));
  await screen.findByText("No downloaded drivers");
  expect(remove).toHaveBeenCalledTimes(2);
});

it("loads empty storage, downloads a selected driver, and refreshes the inventory", async () => {
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  const load = vi.spyOn(api, "downloadedDrivers").mockResolvedValueOnce({ ...inventory, packages: [] }).mockResolvedValueOnce(inventory);
  vi.spyOn(api, "driverPackage").mockResolvedValue({ distribution: "maven", version: "1.5.6.0", installed: false, source: null, files: [] });
  const download = vi.spyOn(api, "downloadDriver").mockResolvedValue({ distribution: "maven", version: "1.5.6.0", installed: true, source: null, files: ["/app/jdbc/duckdb/duckdb.jar"] });
  render(<DownloadedDrivers onBusyChange={vi.fn()} />);
  await screen.findByText("No downloaded drivers");
  await chooseOption(screen.getByRole("combobox", { name: "Download a driver" }), "DuckDB · 1.5.6.0");
  fireEvent.click(await screen.findByRole("button", { name: "Download driver" }));
  await screen.findByRole("article", { name: "DuckDB" });
  expect(download).toHaveBeenCalledWith("duckdb", expect.any(Function));
  expect(load).toHaveBeenCalledTimes(2);
});

it("shows load errors and supports refreshing again", async () => {
  vi.spyOn(api, "downloadedDrivers").mockRejectedValueOnce(new Error("Storage unavailable")).mockResolvedValueOnce(inventory);
  render(<DownloadedDrivers onBusyChange={vi.fn()} />);
  await screen.findByText("Storage unavailable");
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await screen.findByText("Installed");
});

it("labels damaged files as needing repair and old versions as updateable", async () => {
  vi.spyOn(api, "downloadedDrivers").mockResolvedValue({ ...inventory, packages: [{ ...inventory.packages[0]!, verified: false }] });
  const view = render(<DownloadedDrivers onBusyChange={vi.fn()} />);
  await screen.findByText("Needs repair");
  vi.mocked(api.downloadedDrivers).mockResolvedValue({ ...inventory, packages: [{ ...inventory.packages[0]!, verified: false, version: "1.0" }] });
  fireEvent.click(within(view.container).getByRole("button", { name: "Refresh" }));
  await waitFor(() => expect(screen.getByText("Update available")).toBeTruthy());
});
