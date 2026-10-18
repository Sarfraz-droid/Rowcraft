import { databaseProfile } from "../database-profile";
import { useRef, useState } from "react";
import type { Catalog, TableInfo } from "../api";
import { Icon } from "./Icon";

interface Props { dialect?: string; catalog: Catalog; sourceName: string; busy?: boolean; onRefresh(): void; onOpenTable(schema: string, table: string): void; onOpenSchema?(schema: string): void }
const KIND_LABEL: Record<TableInfo["kind"], string> = { Table: "table", PartitionedTable: "partitioned", View: "view", MaterializedView: "mat. view", ForeignTable: "foreign" };
const schemaKey = (name: string) => JSON.stringify([name]);
const tableKey = (schema: string, name: string) => JSON.stringify([schema, name]);

export function Explorer({ dialect, catalog, sourceName, busy = false, onRefresh, onOpenTable, onOpenSchema }: Props) {
  const profile = databaseProfile(dialect);
  const disclosure = useRef<HTMLDetailsElement>(null);
  const [search, setSearch] = useState("");
  const [open, setOpen] = useState<Set<string>>(() => new Set(catalog.schemas.length === 1 ? [schemaKey(catalog.schemas[0]!.name)] : []));
  const needle = search.trim().toLocaleLowerCase();
  const matches = (name: string) => name.toLocaleLowerCase().includes(needle);
  const visible = catalog.schemas.flatMap(schema => {
    const schemaMatches = matches(schema.name);
    const tables = schema.tables.filter(table => schemaMatches || matches(table.name) || table.columns.some(column => matches(column.name)));
    return tables.length || schemaMatches ? [{ ...schema, tables }] : [];
  });
  const toggle = (key: string) => setOpen(previous => { const next = new Set(previous); if (!next.delete(key)) next.add(key); return next; });
  const expandAll = () => {
    if (disclosure.current) disclosure.current.open = true;
    setOpen(new Set(catalog.schemas.flatMap(schema => [schemaKey(schema.name), ...schema.tables.map(table => tableKey(schema.name, table.name))])));
  };
  return <section className="source-explorer source-tables-section" aria-label="Database object explorer">
    <details className="source-tables" ref={disclosure}>
    <summary><Icon name="chevronRight" size={12} className="tables-disclosure" /><Icon name="table" size={14} /><span className="tables-label" title={profile.objects}>{profile.objects}</span><span className="tables-count tw:ml-auto tw:text-meta tw:font-normal tw:text-muted-foreground">{catalog.schemas.reduce((count, schema) => count + schema.tables.length, 0)}</span></summary>
    <div className="schema-explorer">
    <label className="explorer-search"><Icon name="search" /><input aria-label="Filter schemas, tables, and columns" placeholder={`Find ${profile.object} or ${profile.fields.toLowerCase()}…`} value={search} onChange={event => setSearch(event.target.value)} />{search && <button type="button" aria-label="Clear object filter" onClick={() => setSearch("")}><Icon name="close" size={12} /></button>}</label>
    {needle && <p className="explorer-match-count" role="status">{visible.reduce((count, schema) => count + schema.tables.length, 0)} matches</p>}
    {!catalog.schemas.length ? <p className="sidebar-empty tw:[padding:0_16px] tw:text-muted-foreground tw:leading-[1.6]">No schemas found.</p> : !visible.length ? <p className="sidebar-empty tw:[padding:0_16px] tw:text-muted-foreground tw:leading-[1.6]">No objects match “{search}”.</p> : <ul className="object-tree">
      {visible.map(schema => {
        const skey = schemaKey(schema.name);
        const schemaOpen = Boolean(needle) || open.has(skey);
        return <li key={skey}>
          <div className="object-schema-row"><button type="button" className="object-node schema-node" onClick={() => toggle(skey)} aria-expanded={schemaOpen} title={schema.name}><Icon name={schemaOpen ? "chevronDown" : "chevronRight"} size={12} /><Icon name="database" /><span className="object-name tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap">{schema.name}</span><span className="object-count tw:text-muted-foreground tw:ml-auto tw:text-meta tw:pl-[4px]">{schema.tables.length}</span></button>{onOpenSchema && <button type="button" className="object-preview" onClick={() => onOpenSchema(schema.name)} aria-label={`Open ${schema.name} schema in a tab`} title="Open schema in a tab"><Icon name="arrow" size={13} /></button>}</div>
          {schemaOpen && <ul>{schema.tables.map(table => {
            const tkey = tableKey(schema.name, table.name);
            const tableMatches = matches(schema.name) || matches(table.name);
            const columnMatches = Boolean(needle) && !tableMatches;
            const tableOpen = open.has(tkey) || columnMatches;
            return <li key={tkey}>
              <div className="object-table-row"><button type="button" className="object-node table-node" onClick={() => toggle(tkey)} onDoubleClick={() => onOpenTable(schema.name, table.name)} aria-expanded={tableOpen} title={`${schema.name}.${table.name} · ${profile.relationships ? KIND_LABEL[table.kind] : profile.object}`}><Icon name={tableOpen ? "chevronDown" : "chevronRight"} size={12} /><Icon name="table" size={13} /><span className="object-name tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap">{table.name}</span><span className="object-count tw:text-muted-foreground tw:ml-auto tw:text-meta tw:pl-[4px]">{table.columns.length}</span></button><button type="button" className="object-preview" onClick={() => onOpenTable(schema.name, table.name)} aria-label={`Preview ${schema.name}.${table.name}`} title="Preview rows"><Icon name="play" size={12} /></button></div>
              {tableOpen && <ul className="object-columns">{table.columns.filter(column => !columnMatches || matches(column.name)).map(column => {
                const primary = table.constraints?.some(constraint => constraint.kind === "PrimaryKey" && constraint.columns.includes(column.name));
                const foreign = table.constraints?.some(constraint => constraint.kind === "ForeignKey" && constraint.columns.includes(column.name));
                return <li key={column.name} className="object-column" title={`${column.name}: ${column.native_type}${column.nullable ? " · nullable" : " · not null"}${primary ? " · primary key" : ""}${foreign ? " · foreign key" : ""}`}><span className="object-column-name tw:[flex:1] tw:min-w-[60px] tw:[overflow-wrap:anywhere]">{column.name}</span>{primary && <span className="column-key" aria-label="Primary key">PK</span>}{foreign && <span className="column-key foreign" aria-label="Foreign key">FK</span>}<span className="object-type">{column.native_type}</span>{!column.nullable && <span className="column-required tw:text-muted-foreground tw:text-caption tw:w-[5px]" aria-label="Not null" title="Not null">*</span>}</li>;
              })}{!table.columns.length && <li className="object-column no-columns">No field metadata</li>}</ul>}
            </li>;
          })}</ul>}
        </li>;
      })}
    </ul>}
    </div>
    </details>
    <div className="tables-actions" role="group" aria-label="Table tree controls">
      <button type="button" className="icon-button" aria-label="Expand all" title="Expand all schemas and columns" disabled={!catalog.schemas.length} onClick={expandAll}><Icon name="expandAll" size={13} /></button>
      <button type="button" className="icon-button" aria-label="Collapse all" title="Collapse all schemas and columns" disabled={!open.size || !!needle} onClick={() => setOpen(new Set())}><Icon name="collapseAll" size={13} /></button>
      <button type="button" className="icon-button" disabled={busy} onClick={onRefresh} aria-label={`Refresh tables in ${sourceName}`} title={`Refresh ${profile.objects.toLowerCase()}`}><Icon name="refresh" size={13} /></button>
    </div>
  </section>;
}
