// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ErrorNotice } from "./ErrorNotice";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("shows a cause, recovery, complete diagnostics, and retry", async () => {
  const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  const retry = vi.fn();
  const error = "query failed: syntax error\nSQLSTATE: 42601\nPosition: 12";
  render(<ErrorNotice error={error} title="Query failed" context="Local" onRetry={retry} />);
  expect(screen.getByRole("alert").textContent).toContain("Query failed");
  expect(screen.getByText("query failed: syntax error").title).toBe(error);
  expect(screen.getByText(/Review the SQL/)).toBeTruthy();
  const summary = screen.getByText("Show details");
  expect(summary.getAttribute("title")).toContain("complete error");
  fireEvent.click(summary);
  expect(screen.getByLabelText("Technical error details").textContent).toBe(error);
  fireEvent.click(screen.getByRole("button", { name: "Copy error" }));
  await waitFor(() => expect(writeText).toHaveBeenCalledWith(`Query failed\n\nLocal\n\n${error}`));
  await screen.findByText("Copied");
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  expect(retry).toHaveBeenCalledOnce();
});

it("provides a manual copy fallback when clipboard access fails", async () => {
  vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new Error("denied"));
  render(<ErrorNotice error="connection refused" />);
  fireEvent.click(screen.getByText("Show details"));
  fireEvent.click(screen.getByRole("button", { name: "Copy error" }));
  await screen.findByText("Could not copy. Select and copy the details below.");
});
