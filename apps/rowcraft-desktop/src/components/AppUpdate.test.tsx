// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { AppUpdate } from "./AppUpdate";

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("0.0.8") }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: vi.fn(() => true) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));

beforeEach(() => vi.mocked(isTauri).mockReturnValue(true));
afterEach(() => { cleanup(); vi.clearAllMocks(); });

it("renders nothing outside the desktop app", () => {
  vi.mocked(isTauri).mockReturnValue(false);
  const { container } = render(<AppUpdate />);
  expect(container.innerHTML).toBe("");
});

it("reports when the installed version is current", async () => {
  vi.mocked(check).mockResolvedValue(null);
  render(<AppUpdate />);
  fireEvent.click(await screen.findByRole("button", { name: "v0.0.8" }));
  expect(await screen.findByText("Rowcraft 0.0.8 is up to date")).toBeTruthy();
});

it("downloads with progress, then restarts only when asked", async () => {
  let finish!: () => void;
  const downloadAndInstall = vi.fn((onEvent: (event: unknown) => void) => {
    onEvent({ event: "Started", data: { contentLength: 200 } });
    onEvent({ event: "Progress", data: { chunkLength: 50 } });
    return new Promise<void>(resolve => { finish = resolve; });
  });
  vi.mocked(check).mockResolvedValue({ version: "0.0.9", body: "Faster grids", downloadAndInstall } as unknown as Update);
  render(<AppUpdate />);
  fireEvent.click(await screen.findByRole("button", { name: "v0.0.8" }));
  const update = await screen.findByRole("button", { name: "Update to 0.0.9" });
  expect(update.title).toContain("Faster grids");
  fireEvent.click(update);
  expect(await screen.findByText("Downloading update 25%")).toBeTruthy();
  finish();
  const restart = await screen.findByRole("button", { name: "Restart to update" });
  expect(relaunch).not.toHaveBeenCalled();
  fireEvent.click(restart);
  expect(relaunch).toHaveBeenCalledOnce();
});

it("shows a failed check with its reason and lets the user retry", async () => {
  vi.mocked(check).mockRejectedValueOnce(new Error("signature verification failed")).mockResolvedValueOnce(null);
  render(<AppUpdate />);
  fireEvent.click(await screen.findByRole("button", { name: "v0.0.8" }));
  const retry = await screen.findByRole("button", { name: "Update failed · Retry" });
  expect(retry.title).toContain("signature verification failed");
  fireEvent.click(retry);
  expect(await screen.findByText("Rowcraft 0.0.8 is up to date")).toBeTruthy();
});
