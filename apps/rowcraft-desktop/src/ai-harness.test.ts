import { afterEach, expect, it, vi } from "vitest";
import { api } from "./api";
import { runSqlAssistance, type AssistanceContext } from "./ai-harness";

const context: AssistanceContext = { action: "generate", prompt: "Count people", sql: "", source: "local", dialect: "sqlite", schema: "main", catalog: null, includeSchema: false, allowTools: true };
afterEach(() => vi.restoreAllMocks());

it("feeds read-only database results back to the model and records visible tool activity", async () => {
  const complete = vi.spyOn(api, "aiComplete")
    .mockResolvedValueOnce({ content: "", model: "route", tool_calls: [{ id: "query-1", type: "function", function: { name: "run_readonly_query", arguments: JSON.stringify({ sql: "SELECT count(*) FROM people" }) } }] })
    .mockResolvedValueOnce({ content: "```sql\nSELECT count(*) FROM people;\n```", model: "route", tool_calls: [] });
  const query = vi.spyOn(api, "aiReadonlyQuery").mockResolvedValue({ columns: ["count"], rows: [[40]], summary: "1 row" });
  const trace = vi.fn();
  const reply = await runSqlAssistance({ ...context, model: "chosen-model" }, trace);
  expect(complete.mock.calls.every(call => call[2] === "chosen-model")).toBe(true);
  expect(reply.content).toContain("SELECT count(*)");
  expect(query).toHaveBeenCalledWith("local", "SELECT count(*) FROM people", "main");
  expect(complete.mock.calls[1]?.[0]).toEqual(expect.arrayContaining([expect.objectContaining({ role: "tool", tool_call_id: "query-1", content: expect.stringContaining("40") })]));
  expect(trace).toHaveBeenLastCalledWith([expect.objectContaining({ status: "done", name: "run_readonly_query" })]);
});

it("does not execute model-requested tools when database tools are turned off", async () => {
  vi.spyOn(api, "aiComplete").mockResolvedValue({ content: "", model: "route", tool_calls: [{ id: "bad", type: "function", function: { name: "run_readonly_query", arguments: '{"sql":"SELECT 1"}' } }] });
  const query = vi.spyOn(api, "aiReadonlyQuery");
  await expect(runSqlAssistance({ ...context, allowTools: false }, vi.fn())).rejects.toThrow("tool limit");
  expect(query).not.toHaveBeenCalled();
});

it("returns tool failures to the model so it can explain a failed query instead of claiming success", async () => {
  const complete = vi.spyOn(api, "aiComplete")
    .mockResolvedValueOnce({ content: "", model: "route", tool_calls: [{ id: "bad", type: "function", function: { name: "run_readonly_query", arguments: '{"sql":"DELETE FROM people"}' } }] })
    .mockResolvedValueOnce({ content: "I could not run that query because changes are blocked.", model: "route" });
  vi.spyOn(api, "aiReadonlyQuery").mockRejectedValue(new Error("Changes are blocked"));
  const result = await runSqlAssistance(context, vi.fn());
  expect(result.content).toContain("blocked");
  expect(complete.mock.calls[1]?.[0]).toEqual(expect.arrayContaining([expect.objectContaining({ role: "tool", content: "Changes are blocked" })]));
});
