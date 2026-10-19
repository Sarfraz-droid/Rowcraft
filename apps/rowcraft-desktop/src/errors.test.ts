import { describe, expect, it } from "vitest";
import { errorGuidance, errorText } from "./errors";

describe("error diagnostics", () => {
  it("retains nested causes and structured driver fields", () => {
    expect(errorText(new Error("Query failed", { cause: { message: "syntax error", sqlState: "42601", position: 12 } }))).toBe("Query failed\nCaused by: message: syntax error\nsqlState: 42601\nposition: 12");
  });
  it("handles circular and empty rejections", () => {
    const error: Record<string, unknown> = { message: "failed" };
    error.cause = error;
    expect(errorText(error)).toContain("[Circular error reference]");
    expect(errorText(null)).toBe("No error details were provided.");
  });
  it("offers relevant recovery without guessing an unknown cause", () => {
    expect(errorGuidance("connection refused")).toContain("host and port");
    expect(errorGuidance("password authentication failed")).toContain("username, password");
    expect(errorGuidance("syntax error at position 12")).toContain("reported token or position");
    expect(errorGuidance("something unusual")).toContain("reported details");
  });
});
