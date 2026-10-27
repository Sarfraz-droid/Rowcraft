// Pure helpers behind editing rows in the result grid: which table a result came from, which of
// its columns can be written, and the change set the backend applies (`apply_row_changes`).

import type { Catalog, Cell, RowChanges, TableInfo } from "./api";

/**
 * The columns that identify one row: the primary key, else the first unique key whose columns are
 * all NOT NULL (NULLs never compare equal). Mirrors `row_key` in the workbench.
 */
export function rowKey(table: TableInfo): string[] | null {
  const constraints = table.constraints ?? [];
  const primary = constraints.find(item => item.kind === "PrimaryKey" && item.columns.length > 0);
  if (primary) return primary.columns;
  const unique = constraints.find(item => item.kind === "Unique" && item.columns.length > 0
    && item.columns.every(name => table.columns.some(column => column.name === name && !column.nullable)));
  return unique?.columns ?? null;
}

const IDENT = String.raw`(?:"(?:[^"]|"")+"|\x60(?:[^\x60]|\x60\x60)+\x60|\[[^\]]+\]|[\p{L}_][\p{L}\p{N}_$#@]*)`;
const NAME = new RegExp(String.raw`^(${IDENT})(?:\s*\.\s*(${IDENT}))?(?:\s*\.\s*(${IDENT}))?$`, "u");
const CLAUSE = /\b(where|order\s+by|limit|offset|fetch|for\s+update|for\s+share)\b/i;

function unquote(name: string): string {
  if (name.startsWith('"')) return name.slice(1, -1).replace(/""/g, '"');
  if (name.startsWith("`")) return name.slice(1, -1).replace(/``/g, "`");
  if (name.startsWith("[")) return name.slice(1, -1);
  return name;
}

/** Splits on commas outside parentheses and quotes. */
function splitTopLevel(text: string): string[] {
  const parts: string[] = [];
  let depth = 0, quote = "", start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) { if (c === quote) quote = ""; continue; }
    if (c === "'" || c === '"' || c === "`") quote = c;
    else if (c === "[") quote = "]";
    else if (c === "(") depth++;
    else if (c === ")") depth--;
    else if (c === "," && depth === 0) { parts.push(text.slice(start, i)); start = i + 1; }
  }
  parts.push(text.slice(start));
  return parts.map(part => part.trim());
}

/**
 * `sql` as the outermost query sees it: comments dropped, string literals emptied and the inside of
 * parentheses removed, so words in strings or subqueries (`like '%join%'`, `in (select …)`) are
 * not taken for clauses of the query itself. Quoted identifiers are kept.
 */
function outerLevel(sql: string): string {
  let out = "", depth = 0;
  for (let i = 0; i < sql.length; i++) {
    const c = sql[i]!, next = sql[i + 1];
    if (c === "-" && next === "-" || c === "#" && (next === undefined || /\s/.test(next))) {
      const end = sql.indexOf("\n", i); i = end < 0 ? sql.length : end; out += depth ? "" : " "; continue;
    }
    if (c === "/" && next === "*") {
      const end = sql.indexOf("*/", i + 2); i = end < 0 ? sql.length : end + 1; out += depth ? "" : " "; continue;
    }
    const close = c === "'" ? "'" : c === '"' ? '"' : c === "`" ? "`" : c === "[" ? "]" : "";
    if (close) {
      // A doubled closing quote is an escaped one and stays inside the literal.
      let end = i + 1;
      for (; end < sql.length; end++) if (sql[end] === close) { if (close !== "]" && sql[end + 1] === close) end++; else break; }
      if (!depth) out += c === "'" ? "''" : sql.slice(i, end + 1);
      i = end; continue;
    }
    if (c === "(") { if (!depth++) out += "("; continue; }
    if (c === ")") { if (depth && !--depth) out += ")"; continue; }
    if (!depth) out += c;
  }
  return out;
}

/** MySQL index hints (`force index (…)`) say how to read the table, not which rows it returns. */
const INDEX_HINT = /\s+(?:use|force|ignore)\s+(?:index|key)(?:\s+for\s+(?:join|order\s+by|group\s+by))?\s*\(\)/gi;

export interface SourceTable {
  schema?: string;
  table: string;
  /** Result columns that are plain table columns, or `"*"` when the select list has a star. */
  columns: "*" | string[];
}

/**
 * The single table a plain `select … from table [alias] [where …]` reads, or null for anything
 * else (joins, grouping, unions, derived tables, CTEs), whose rows do not map back to one table
 * row. Subqueries in the where clause are fine: the rows still come from the one table.
 */
export function sourceTable(statement: string): SourceTable | null {
  const sql = outerLevel(statement).replace(INDEX_HINT, "").trim().replace(/;\s*$/, "");
  const match = /^select\s+(?:top\s+(?:\d+|\(\))\s+)?([\s\S]+?)\s+from\s+([\s\S]+)$/i.exec(sql);
  if (!match || /\b(join|group\s+by|having|union|intersect|except|distinct|with)\b/i.test(sql)) return null;
  const [, list = "", rest = ""] = match;
  const clause = CLAUSE.exec(rest);
  const from = (clause ? rest.slice(0, clause.index) : rest).trim();
  if (from.includes(",")) return null;
  // Drop a trailing alias: `table t` or `table as t`.
  const target = from.replace(new RegExp(String.raw`\s+(?:as\s+)?${IDENT}$`, "iu"), "");
  const name = NAME.exec(target);
  if (!name) return null;
  const parts = name.slice(1).flatMap(part => part ? [unquote(part)] : []);
  const table = parts.at(-1);
  if (!table) return null;
  const schema = parts.at(-2);
  const items = splitTopLevel(list);
  if (items.some(item => /^(?:[\s\S]+\.)?\*$/.test(item))) return { schema, table, columns: "*" };
  const plain = new RegExp(String.raw`^(?:${IDENT}\s*\.\s*)?(${IDENT})$`, "u");
  return { schema, table, columns: items.flatMap(item => { const column = plain.exec(item); return column?.[1] ? [unquote(column[1])] : []; }) };
}

/** Table names are matched exactly first, then without regard to case (unquoted identifiers). */
export function findTable(catalog: Catalog | null, source: SourceTable, defaultSchema?: string): { schema: string; table: TableInfo } | null {
  if (!catalog) return null;
  const wanted = source.schema ?? defaultSchema ?? catalog.default_schema ?? catalog.database ?? undefined;
  const fold = (a: string, b: string) => a === b || a.toLowerCase() === b.toLowerCase();
  const schemas = wanted ? catalog.schemas.filter(item => fold(item.name, wanted)) : catalog.schemas.length === 1 ? catalog.schemas : [];
  for (const schema of schemas) {
    const table = schema.tables.find(item => item.name === source.table) ?? schema.tables.find(item => fold(item.name, source.table));
    if (table) return { schema: schema.name, table };
  }
  return null;
}

export interface EditSupport {
  /** Per result column: whether its cells may be written. */
  editable: boolean[];
  /** Per result column: whether it is part of the row key (left out when duplicating a row). */
  key: boolean[];
  /** Why existing rows cannot be changed or deleted; new rows can still be added. */
  locked?: string;
}

/** What the grid may change in a result of `columns` read from `table`. */
export function editSupport(columns: string[], table: TableInfo, selected: "*" | string[] = "*"): EditSupport {
  const counts = new Map<string, number>();
  for (const name of columns) counts.set(name, (counts.get(name) ?? 0) + 1);
  const editable = columns.map(name => counts.get(name) === 1 && table.columns.some(column => column.name === name) && (selected === "*" || selected.includes(name)));
  const keys = rowKey(table);
  const key = columns.map(name => !!keys?.includes(name));
  const missing = keys?.find(name => !columns.includes(name));
  const locked = !keys ? `${table.name} has no primary key, so its rows cannot be matched for editing.`
    : missing ? `The result does not include the key column ${missing}; select it to edit rows.` : undefined;
  return { editable, key, locked };
}

/** Edits waiting to be saved: changed cells of read rows, rows to delete, and new rows. */
export interface PendingEdits {
  /** Row index → column index → new value. */
  updates: Map<number, Map<number, Cell>>;
  deletes: Set<number>;
  /** Column index → value; columns not set get their default. */
  inserts: Map<number, Cell>[];
}

export const noEdits = (): PendingEdits => ({ updates: new Map(), deletes: new Set(), inserts: [] });

export function pendingCount(edits: PendingEdits): number {
  let updated = 0;
  for (const [row, cells] of edits.updates) if (cells.size && !edits.deletes.has(row)) updated++;
  return updated + edits.deletes.size + edits.inserts.length;
}

export function buildChanges(columns: string[], rows: Cell[][], edits: PendingEdits): RowChanges {
  const named = (cells: Map<number, Cell>) => Object.fromEntries([...cells].sort(([a], [b]) => a - b).map(([column, value]) => [columns[column], value]));
  return {
    deletes: [...edits.deletes].sort((a, b) => a - b).flatMap(row => rows[row] ? [rows[row]] : []),
    updates: [...edits.updates].filter(([row, cells]) => cells.size && !edits.deletes.has(row)).sort(([a], [b]) => a - b).flatMap(([row, cells]) => rows[row] ? [{ row: rows[row], values: named(cells) }] : []),
    inserts: edits.inserts.map(named),
  };
}

/** The text a cell starts with in the editor; NULL starts empty. */
export function editText(cell: Cell): string {
  if (cell === null || typeof cell === "object") return "";
  return String(cell);
}
