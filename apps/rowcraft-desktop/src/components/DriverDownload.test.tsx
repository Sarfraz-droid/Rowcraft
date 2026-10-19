// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, type DriverPackage } from "../api";
import { DriverDownload } from "./DriverDownload";
const missing: DriverPackage = { distribution: "maven", version: "2.7.4", source: null, installed: false, files: [] };
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("downloads on demand, shows progress, prevents duplicate requests, and reports installed files", async () => {
  vi.spyOn(api, "driverPackage").mockResolvedValue(missing);
  let finish!: (value: DriverPackage) => void;
  const download = vi.spyOn(api, "downloadDriver").mockImplementation((_id, progress) => {
    progress("Downloading file 1 of 2: driver.jar");
    return new Promise(resolve => { finish = resolve; });
  });
  const installed = vi.fn();
  render(<DriverDownload id="hsqldb_local" onInstalled={installed} />);
  fireEvent.click(await screen.findByRole("button", { name: "Download driver" }));
  await screen.findByText("Downloading file 1 of 2: driver.jar");
  expect((screen.getByRole("button", { name: "Downloading…" }) as HTMLButtonElement).disabled).toBe(true);
  expect(download).toHaveBeenCalledTimes(1);
  const files = ["/app/jdbc/hsqldb_local/driver.jar"];
  finish({ ...missing, installed: true, files });
  await screen.findByText("Driver 2.7.4 installed");
  expect(installed).toHaveBeenCalledWith(files);
});

it("allows retry after a failed download", async () => {
  vi.spyOn(api, "driverPackage").mockResolvedValue(missing);
  const download = vi.spyOn(api, "downloadDriver").mockRejectedValueOnce(new Error("Check your internet connection and retry."))
    .mockResolvedValueOnce({ ...missing, installed: true });
  render(<DriverDownload id="hsqldb_local" />);
  fireEvent.click(await screen.findByRole("button", { name: "Download driver" }));
  await screen.findByRole("alert");
  fireEvent.click(screen.getByRole("button", { name: "Download driver" }));
  await screen.findByText("Driver 2.7.4 installed");
  expect(download).toHaveBeenCalledTimes(2);
});

it("ignores a late download result after switching drivers", async () => {
  vi.spyOn(api, "driverPackage").mockResolvedValue(missing);
  let finish!: (value: DriverPackage) => void;
  vi.spyOn(api, "downloadDriver").mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  const installed = vi.fn();
  const view = render(<DriverDownload id="hsqldb_local" onInstalled={installed} />);
  fireEvent.click(await screen.findByRole("button", { name: "Download driver" }));
  view.rerender(<DriverDownload id="duckdb" onInstalled={installed} />);
  await screen.findByRole("button", { name: "Download driver" });
  finish({ ...missing, installed: true });
  await waitFor(() => expect(screen.queryByText("Driver 2.7.4 installed")).toBeNull());
  expect(installed).not.toHaveBeenCalled();
});

it("hides native downloads and opens the supplier for vendor-only drivers", async () => {
  vi.spyOn(api, "driverPackage").mockResolvedValueOnce({ ...missing, distribution: "native", installed: true })
    .mockResolvedValueOnce({ ...missing, distribution: "vendor", source: "https://vendor.example/drivers" });
  const open = vi.spyOn(api, "openDriverVendor").mockResolvedValue();
  const view = render(<DriverDownload id="postgresql" />);
  await waitFor(() => expect(screen.queryByText("Checking driver files…")).toBeNull());
  expect(screen.queryByRole("button")).toBeNull();
  view.rerender(<DriverDownload id="athena" />);
  fireEvent.click(await screen.findByRole("button", { name: "Get driver from vendor" }));
  expect(open).toHaveBeenCalledWith("athena");
});
