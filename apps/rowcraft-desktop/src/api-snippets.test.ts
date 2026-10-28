import { describe, expect, it } from "vitest";
import { apiSnippets, shellQuote, sourcePath } from "./api-snippets";

describe("HTTP API snippets", () => {
  it("quotes SQL and source names so the command runs as shown", () => {
    expect(shellQuote("it's")).toBe(`'it'\\''s'`);
    expect(sourcePath("my db/1")).toBe("/datasources/my%20db%2F1");
    const query = apiSnippets({ source: "my db", sql: "select 'a' as x" }).find(s => s.id === "query")!.command;
    expect(query).toContain("'http://127.0.0.1:7878/datasources/my%20db/query'");
    expect(query).toContain(`-d '{"sql":"select '\\''a'\\'' as x","max_rows":100}'`);
  });

  it("falls back to a trivial statement and follows a custom port", () => {
    const snippets = apiSnippets({ source: "pg", sql: "  ", port: 9000 });
    expect(snippets[0]!.command).toBe("rowcraft serve --listen 127.0.0.1:9000");
    expect(snippets.find(s => s.id === "csv")!.command).toContain("--data-binary 'SELECT 1'");
    expect(snippets.find(s => s.id === "list")!.command).toBe("curl -s http://127.0.0.1:9000/datasources");
  });
});
