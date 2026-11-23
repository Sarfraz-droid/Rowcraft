// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { api, type McpStatus } from "../api";
import { McpConnect } from "./McpConnect";

const status = (registered: boolean): McpStatus => ({
  clients: [
    { id: "claude", label: "Claude Code", installed: true, registered },
    { id: "codex", label: "Codex", installed: false, registered: false },
  ],
  command: ["/app/rowcraft-desktop", "mcp"],
  config_json: '{ "mcpServers": {} }',
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("connects an installed client and offers to disconnect it", async () => {
  vi.spyOn(api, "mcpStatus").mockResolvedValue(status(false));
  const connect = vi.spyOn(api, "mcpConnect").mockResolvedValue(status(true));
  render(<McpConnect />);
  await screen.findByText("Claude Code");
  expect((screen.getAllByRole("button", { name: "Connect" })[1] as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getAllByRole("button", { name: "Connect" })[0] as HTMLElement);
  await screen.findByText("Claude Code · connected");
  expect(connect).toHaveBeenCalledWith("claude", false);
  expect(screen.getByText("Codex · not found")).toBeTruthy();
  const disconnect = vi.spyOn(api, "mcpDisconnect").mockResolvedValue(status(false));
  fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
  await screen.findByText("Claude Code");
  expect(disconnect).toHaveBeenCalledWith("claude", false);
});

it("passes the write permission and reports failures", async () => {
  vi.spyOn(api, "mcpStatus").mockResolvedValue(status(false));
  vi.spyOn(api, "mcpConnect").mockRejectedValue(new Error("claude exploded"));
  render(<McpConnect />);
  await screen.findByText("Claude Code");
  fireEvent.click(screen.getByRole("checkbox"));
  await screen.findByText(/Connect again after changing this/);
  fireEvent.click(screen.getAllByRole("button", { name: "Connect" })[0] as HTMLElement);
  await screen.findByText("claude exploded");
  expect(api.mcpConnect).toHaveBeenCalledWith("claude", true);
});
