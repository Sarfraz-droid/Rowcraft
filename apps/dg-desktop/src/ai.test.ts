import { describe, expect, it } from "vitest";
import type { Catalog } from "./api";
import { sqlMessages, suggestedSql } from "./ai";

const catalog: Catalog = { database: "private", schemas: [
  { name: "main", tables: [{ name: "people", kind: "Table", comment: "private table comment", columns: [{ name: "email", native_type: "TEXT", nullable: true, comment: "private column comment" }] }] },
  { name: "other", tables: [{ name: "hidden_table", kind: "Table", columns: [] }] },
] };

describe("SQL assistant context", () => {
  it("sends structure only after opting in, limits it to the active schema, and excludes comments", () => {
    const minimal = JSON.stringify(sqlMessages("generate", "Count people", "", "pg", "main", catalog, false));
    expect(minimal).not.toContain('"email"');
    const included = JSON.stringify(sqlMessages("generate", "Count people", "", "pg", "main", catalog, true));
    expect(included).toContain("email");
    expect(included).not.toContain("hidden_table");
    expect(included).not.toContain("private table comment");
    expect(included).not.toContain("private column comment");
  });

  it("provides the database error only for query repair", () => {
    const repaired = sqlMessages("fix", "", "SELECT missing FROM people", "pg", "main", catalog, false, "column missing does not exist");
    expect(repaired[1]?.content).toContain("column missing does not exist");
    expect(sqlMessages("explain", "", "SELECT 1", "pg", "main", catalog, false, "old error")[1]?.content).not.toContain("old error");
  });

  it("extracts only a fenced query for explicit insertion and rejects unfenced explanations", () => {
    expect(suggestedSql("Use this:\n```sql\nSELECT 1;\n```\nExplanation")).toBe("SELECT 1;");
    expect(suggestedSql("This query selects one value.")).toBeNull();
    expect(suggestedSql("```mongosh\ndb.getCollectionNames()\n```")).toBe("db.getCollectionNames()");
  });
});
