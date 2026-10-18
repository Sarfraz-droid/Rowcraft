import { CompletionContext } from "@codemirror/autocomplete";
import { keywordCompletionSource, schemaCompletionSource, sql } from "@codemirror/lang-sql";
import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import type { Catalog } from "./api";
import { editorSqlConfig } from "./editor-completions";

const catalog: Catalog = { database: "example", default_schema: "main", schemas: [
  { name: "main", tables: [{ name: "people", kind: "Table", columns: [{ name: "person_id", native_type: "INTEGER", nullable: false }, { name: "email", native_type: "TEXT", nullable: true }] }] },
  { name: "audit", tables: [{ name: "events", kind: "Table", columns: [{ name: "event_id", native_type: "BIGINT", nullable: false }] }] },
] };

async function schemaOptions(doc: string, schema?: string) {
  const config = editorSqlConfig("mysql", catalog, schema);
  const state = EditorState.create({ doc, extensions: [sql(config)] });
  const result = await schemaCompletionSource(config)(new CompletionContext(state, doc.length, true));
  return result?.options ?? [];
}

describe("console SQL suggestions", () => {
  it("suggests tables from the console's selected schema", async () => {
    const options = await schemaOptions("select * from ", "audit");
    expect(options.some(option => option.label === "events" && option.detail === "table")).toBe(true);
    expect(options.some(option => option.label === "people")).toBe(false);
  });

  it("suggests column names with their database types after a qualified table", async () => {
    const options = await schemaOptions("select main.people.");
    expect(options).toEqual(expect.arrayContaining([
      expect.objectContaining({ label: "person_id", detail: "INTEGER", type: "property" }),
      expect.objectContaining({ label: "email", detail: "TEXT", type: "property" }),
    ]));
  });

  it("uses MySQL keyword suggestions and readable uppercase labels", async () => {
    const config = editorSqlConfig("mysql", null);
    const doc = "show t";
    const state = EditorState.create({ doc, extensions: [sql(config)] });
    const result = await keywordCompletionSource(config.dialect!, config.upperCaseKeywords)(new CompletionContext(state, doc.length, true));
    expect(result?.options.some(option => option.label === "TABLES")).toBe(true);
    expect(result?.options.some(option => option.label === "SHOW")).toBe(true);
  });
});
