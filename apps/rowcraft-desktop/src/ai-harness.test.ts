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
  await expect(runSqlAssistance({ ...context, allowTools: false }, vi.fn())).rejects.toThrow("database tools are turned off");
  expect(query).not.toHaveBeenCalled();
});

const queryCall = (id: string) => ({ id, type: "function" as const, function: { name: "run_readonly_query", arguments: JSON.stringify({ sql: `SELECT ${id}` }) } });

it("asks for an answer once the tool budget is used up instead of failing", async () => {
  const complete = vi.spyOn(api, "aiComplete").mockImplementation(async (_messages, toolsEnabled) => toolsEnabled
    ? { content: "", model: "route", tool_calls: [queryCall(crypto.randomUUID())] }
    : { content: "Here is what I found.", model: "route", tool_calls: [queryCall("ignored")] });
  const query = vi.spyOn(api, "aiReadonlyQuery").mockResolvedValue({ columns: ["n"], rows: [[1]], summary: "1 row" });
  const reply = await runSqlAssistance(context, vi.fn());
  expect(reply).toEqual({ content: "Here is what I found.", model: "route", tool_calls: [] });
  expect(query).toHaveBeenCalledTimes(6);
  const [final, toolsEnabled] = complete.mock.calls.at(-1)!;
  expect(toolsEnabled).toBe(false);
  // The last request carries no tool messages, which providers reject without tool definitions.
  expect(final.some(message => message.role === "tool" || message.tool_calls)).toBe(false);
  expect(final.at(-1)?.content).toContain("budget");
});

it("skips tool calls beyond the per-turn limit but still answers each one", async () => {
  const calls = ["a", "b", "c", "d", "e", "f"].map(queryCall);
  const complete = vi.spyOn(api, "aiComplete")
    .mockResolvedValueOnce({ content: "", model: "route", tool_calls: calls })
    .mockResolvedValueOnce({ content: "Done.", model: "route" });
  const query = vi.spyOn(api, "aiReadonlyQuery").mockResolvedValue({ columns: ["n"], rows: [[1]], summary: "1 row" });
  expect((await runSqlAssistance(context, vi.fn())).content).toBe("Done.");
  expect(query).toHaveBeenCalledTimes(4);
  const answered = complete.mock.calls[1]![0].filter(message => message.role === "tool");
  expect(answered.map(message => message.tool_call_id)).toEqual(["a", "b", "c", "d", "e", "f"]);
  expect(answered.at(-1)?.content).toContain("Skipped");
});

it("sends only the most recent chat history", async () => {
  const complete = vi.spyOn(api, "aiComplete").mockResolvedValue({ content: "ok", model: "route" });
  const history = Array.from({ length: 40 }, (_, i) => ({ role: i % 2 ? "assistant" as const : "user" as const, content: `turn ${i}` }));
  await runSqlAssistance({ ...context, action: "chat", allowTools: false, history }, vi.fn());
  const sent = complete.mock.calls[0]![0];
  expect(sent.some(message => message.content === "turn 27")).toBe(false);
  expect(sent.some(message => message.content === "turn 39")).toBe(true);
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
