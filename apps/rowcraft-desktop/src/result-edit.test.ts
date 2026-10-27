import { expect, it } from "vitest";

import type { Catalog, TableInfo } from "./api";
import { buildChanges, editSupport, findTable, noEdits, pendingCount, rowKey, sourceTable } from "./result-edit";

const table = (constraints: TableInfo["constraints"], nullable = false): TableInfo => ({
  name: "Answers", kind: "Table", constraints,
  columns: [{ name: "Id", native_type: "int", nullable }, { name: "Answer", native_type: "text", nullable: true }],
});
const primary = table([{ name: "PRIMARY", kind: "PrimaryKey", columns: ["Id"], definition: "", reference: null }]);

it("finds the single table behind a plain select", () => {
  expect(sourceTable("select * from Answers where Id > 3 order by Id limit 10;")).toEqual({ schema: undefined, table: "Answers", columns: "*" });
  expect(sourceTable('SELECT a.* FROM "app"."Answers" AS a')).toEqual({ schema: "app", table: "Answers", columns: "*" });
  expect(sourceTable("select `Id`, upper(Answer) as Answer, t.Answer from `db`.`Answers` t")).toEqual({ schema: "db", table: "Answers", columns: ["Id", "Answer"] });
  expect(sourceTable("select top 5 * from [dbo].[Answers]")).toEqual({ schema: "dbo", table: "Answers", columns: "*" });
  expect(sourceTable("-- recent\nselect * from cat.sch.Answers")).toEqual({ schema: "sch", table: "Answers", columns: "*" });
});

it("ignores clause words inside strings, comments and where-clause subqueries", () => {
  const answers = { schema: undefined, table: "Answers", columns: "*" };
  for (const sql of [
    "select * from Answers where Answer like '%join%'",
    "select * from Answers where Answer = 'it''s with union'",
    "select * from Answers where Id in (select AnswerId from Votes group by AnswerId)",
    "select * from Answers where Id = (select max(Id) from Answers)",
    "select * from Answers force index (PRIMARY) where Id > 5",
    "select * from Answers # recent\nwhere Id > 5",
    "/* join */ select * from Answers",
  ]) expect(sourceTable(sql), sql).toEqual(answers);
  expect(sourceTable("select Id, (select count(*) from Votes) as n from Answers")).toEqual({ schema: undefined, table: "Answers", columns: ["Id"] });
});

it("refuses results that do not map back to one table row", () => {
  for (const sql of [
    "select * from a join b on a.id = b.id",
    "select * from a, b",
    "select count(*) from a group by x",
    "select distinct x from a",
    "select * from a union select * from b",
    "with t as (select 1) select * from t",
    "select * from (select * from a) s",
    "update a set x = 1",
    "show tables",
  ]) expect(sourceTable(sql), sql).toBeNull();
});

it("looks tables up in the given or default schema, ignoring case for unquoted names", () => {
  const catalog: Catalog = { database: "hr", default_schema: "hr", schemas: [{ name: "hr", tables: [primary] }, { name: "other", tables: [] }] };
  expect(findTable(catalog, { table: "answers", columns: "*" })?.schema).toBe("hr");
  expect(findTable(catalog, { schema: "other", table: "Answers", columns: "*" })).toBeNull();
  expect(findTable(null, { table: "Answers", columns: "*" })).toBeNull();
});

it("uses the primary key, else a NOT NULL unique key, to match rows", () => {
  expect(rowKey(primary)).toEqual(["Id"]);
  const unique = [{ name: "u", kind: "Unique" as const, columns: ["Id"], definition: "", reference: null }];
  expect(rowKey(table(unique))).toEqual(["Id"]);
  expect(rowKey(table(unique, true))).toBeNull();
  expect(editSupport(["Id", "Answer"], table([])).locked).toMatch(/no primary key/);
  expect(editSupport(["Answer"], primary).locked).toMatch(/key column Id/);
});

it("allows editing only real, unambiguous, plainly selected columns", () => {
  expect(editSupport(["Id", "Answer", "extra"], primary)).toEqual({ editable: [true, true, false], key: [true, false, false], locked: undefined });
  expect(editSupport(["Id", "Answer"], primary, ["Id"]).editable).toEqual([true, false]);
  expect(editSupport(["Id", "Answer", "Answer"], primary).editable).toEqual([true, false, false]);
});

it("turns pending edits into the change set, skipping edits of deleted rows", () => {
  const edits = noEdits();
  edits.updates.set(1, new Map([[1, "new"]]));
  edits.updates.set(2, new Map([[1, null]]));
  edits.deletes.add(2);
  edits.inserts.push(new Map([[1, "added"]]), new Map());
  expect(pendingCount(edits)).toBe(4);
  expect(buildChanges(["Id", "Answer"], [[1, "a"], [2, "b"], [3, "c"]], edits)).toEqual({
    deletes: [[3, "c"]],
    updates: [{ row: [2, "b"], values: { Answer: "new" } }],
    inserts: [{ Answer: "added" }, {}],
  });
});
