import { MongoReferences } from './MongoReferences';
import { mongoRelationships, readMongoLinks, saveMongoLinks } from '../mongo-relationships';
import { databaseProfile } from "../database-profile";
import { SelectField } from "./SelectField";
import { memo, useEffect, useId, useMemo, useState } from 'react';
import { catalogTables, filterTables, relationships } from '../workbench-model';
import type { SourceViewProps } from '../workbench-model';
import { layoutSchemaFlow, type SchemaFlow } from '../schema-layout';
import { useMapCamera } from '../use-map-camera';
import { Icon } from './Icon';

const FlowCanvas = memo(function FlowCanvas({ flow, marker, selected, onSelect, connected, busy, onOpenTable }: {
  flow: SchemaFlow; marker: string; selected: string | null; onSelect(id: string | null): void;
  connected: boolean; busy: boolean; onOpenTable(schema: string, table: string): void;
}) {
  const adjacent = new Set(flow.edges.filter(edge => edge.from === selected || edge.to === selected).flatMap(edge => [edge.from, edge.to]));
  return <>
    <svg className="sv-flow-edges tw:absolute tw:left-0 tw:top-0 tw:overflow-visible tw:pointer-events-none" width={flow.width} height={flow.height} aria-label="Relationship flow">
      <defs><marker id={marker} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" fill="var(--primary)" /></marker></defs>
      {flow.edges.map(edge => {
        const active = !selected || edge.from === selected || edge.to === selected;
        return <g key={edge.id} className={`sv-flow-relationship${active ? ' is-emphasized' : ' is-muted'}`}>
          {edge.paths.map((path, i) => <path key={i} d={path} fill="none" stroke="var(--primary)" strokeWidth="1.5" strokeDasharray={edge.origin === "inferred" ? "6 4" : edge.origin === "manual" ? "2 3" : undefined} markerEnd={`url(#${marker})`}><title>{edge.columns.join(', ')} → {edge.targetColumns.join(', ')}{edge.name ? ` (${edge.name})` : ''}{edge.evidence ? ` · ${edge.evidence}` : ''}</title></path>)}
          {edge.label && <g className="sv-flow-edge-label"><rect x={edge.label.x - 4} y={edge.label.y - 2} width={edge.label.width + 8} height={19} rx={3} /><text x={edge.label.x} y={edge.label.y + 11}>{edge.label.text}</text></g>}
        </g>;
      })}
    </svg>
    {flow.nodes.map(node => {
      const primary = new Set(node.table.constraints?.filter(key => key.kind === 'PrimaryKey').flatMap(key => key.columns));
      const references = new Set(flow.edges.filter(edge => edge.origin && edge.from === node.id).flatMap(edge => edge.columns));
      const foreign = new Set(node.table.constraints?.filter(key => key.kind === 'ForeignKey').flatMap(key => key.columns));
      const incident = flow.edges.filter(edge => edge.from === node.id || edge.to === node.id).length;
      return <article key={node.id} className={`sv-flow-node${selected === node.id ? ' is-selected' : ''}${selected && selected !== node.id && !adjacent.has(node.id) ? ' is-muted' : ''}`} style={{ left: node.x, top: node.y, width: node.width, height: node.height }}>
        <header><Icon name="table" size={13} /><button className="sv-flow-node-name" aria-pressed={selected === node.id} onClick={() => onSelect(selected === node.id ? null : node.id)} title={`Highlight relationships for ${node.schema}.${node.table.name}`}>{node.table.name}</button><button className="icon-button" disabled={!connected || busy} onClick={() => onOpenTable(node.schema, node.table.name)} aria-label={`Preview ${node.schema}.${node.table.name}`} title="Open table data"><Icon name="play" size={12} /></button></header>
        <div className="sv-flow-columns tw:[padding:6px_0]">{node.columns.map(column => <div className="sv-flow-column" key={column.name}><span className="sv-key-mark tw:text-micro tw:min-w-[15px] tw:text-action" title={primary.has(column.name) ? 'Primary key' : foreign.has(column.name) ? 'Foreign key' : references.has(column.name) ? 'Reference field (map link)' : ''}>{primary.has(column.name) ? 'PK' : foreign.has(column.name) ? 'FK' : references.has(column.name) ? 'REF' : ''}</span><span title={column.name}>{column.name}</span><small title={column.native_type}>{column.native_type}</small></div>)}</div>
        <footer><span title={node.schema}>{node.schema}</span><span>{node.table.columns.length} cols{node.table.columns.length > node.columns.length ? ` · +${node.table.columns.length - node.columns.length} hidden` : ''}</span><span title="Visible relationships">{incident} links</span></footer>
      </article>;
    })}
  </>;
});

export function SchemaMap(props: SourceViewProps) {
  return <SchemaMapContent key={JSON.stringify([props.source?.name, props.source?.host, props.source?.dialect])} {...props} />;
}
function SchemaMapContent({ catalog, source, connected, busy, onOpenTable, onRefresh }: SourceViewProps) {
  const mongo = source?.dialect === 'mongo';
  const storageKey = `rowcraft.mongo-references.v1:${JSON.stringify([source?.name, source?.host])}`;
  const [manual, setManual] = useState(() => mongo ? readMongoLinks(storageKey) : []);
  const [referenceError, setReferenceError] = useState('');
  const [infer, setInfer] = useState(true);
  const catalogObjects = useMemo(() => catalogTables(catalog), [catalog]);
  const [schema, setSchema] = useState('');
  const profile = databaseProfile(source?.dialect);
  const [search, setSearch] = useState('');
  const [includeUnlinked, setIncludeUnlinked] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [flow, setFlow] = useState<SchemaFlow | null>(null);
  const [layoutError, setLayoutError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const marker = useId().replace(/:/g, '');
  const schemaTables = useMemo(() => filterTables(catalogTables(catalog), schema, ''), [catalog, schema]);
  const catalogEdges = useMemo(() => mongo ? mongoRelationships(catalogObjects, manual, infer) : relationships(catalogObjects), [mongo, catalogObjects, manual, infer]);
  const allEdges = useMemo(() => {const ids = new Set(schemaTables.map(table => table.id)); return catalogEdges.filter(edge => ids.has(edge.from) && ids.has(edge.to));}, [catalogEdges, schemaTables]);
  const linked = useMemo(() => new Set(allEdges.flatMap(edge => [edge.from, edge.to])), [allEdges]);
  const tables = useMemo(() => {
    if (search.trim()) {
      const matches = new Set(filterTables(schemaTables, '', search).map(table => table.id));
      const neighbors = allEdges.filter(edge => matches.has(edge.from) || matches.has(edge.to)).flatMap(edge => [edge.from, edge.to]);
      return schemaTables.filter(table => matches.has(table.id) || neighbors.includes(table.id));
    }
    return includeUnlinked || !allEdges.length ? schemaTables : schemaTables.filter(table => linked.has(table.id));
  }, [schemaTables, allEdges, linked, search, includeUnlinked]);
  const { viewport, layer, percent, fit, zoom, actualSize } = useMapCamera(flow?.width ?? 0, flow?.height ?? 0);
  useEffect(() => {
    let current = true; setFlow(null); setLayoutError(''); setSelected(null);
    void layoutSchemaFlow(tables, allEdges).then(next => { if (current) setFlow(next); }).catch((error: unknown) => {
      if (current) setLayoutError(error instanceof Error ? error.message : String(error));
    });
    return () => { current = false; };
  }, [tables, allEdges, attempt]);
  const canvas = useMemo(() => flow && <FlowCanvas flow={flow} marker={marker} selected={selected} onSelect={setSelected} connected={connected} busy={busy} onOpenTable={onOpenTable} />, [flow, marker, selected, connected, busy, onOpenTable]);
  return <section className="schema-map-view schema-flow-view" aria-label="Schema map">
    <header className="sv-map-toolbar"><div><h1>{profile.relationships ? "Schema map" : "Object map"}</h1><span className="sv-subtle">{source?.name ?? 'Select a source'} · {tables.length} of {schemaTables.length} {profile.objects.toLowerCase()} · {flow?.edges.length ?? 0} {mongo ? "reference links" : "relationships"}</span></div><div className="sv-map-filters"><input aria-label="Find tables on schema map" placeholder={`Find ${profile.object}…`} value={search} onChange={event => setSearch(event.target.value)} /><SelectField aria-label="Filter map by schema" value={schema} onValueChange={setSchema} options={[{ value: "", label: `All ${profile.namespace.toLowerCase()}s` }, ...(catalog?.schemas.map(item => ({ value: item.name, label: item.name })) ?? [])]} />{mongo && <label className="sv-flow-unlinked"><input type="checkbox" checked={infer} onChange={event => setInfer(event.target.checked)} />Infer references</label>}<label className="sv-flow-unlinked"><input type="checkbox" checked={includeUnlinked} disabled={!allEdges.length} onChange={event => setIncludeUnlinked(event.target.checked)} />All {profile.objects.toLowerCase()}</label><button className="sv-button" disabled={!connected || busy} onClick={onRefresh}>{busy ? 'Refreshing…' : 'Refresh'}</button></div></header>
    {mongo && <MongoReferences tables={catalogObjects} links={manual} error={referenceError} onChange={links => {
      try {saveMongoLinks(storageKey, links);setManual(links);setReferenceError('');}
      catch {setReferenceError('Could not save references locally. Check available storage and try again.');}
    }} />}
    <div className="sv-flow-viewport" ref={viewport} tabIndex={0} role="region" aria-label="Schema diagram. Pinch to zoom, scroll or drag to pan." onKeyDown={event => {
      if (event.target !== event.currentTarget) return;
      if (event.key === '+' || event.key === '=') { event.preventDefault(); zoom(1.2); }
      if (event.key === '-') { event.preventDefault(); zoom(1 / 1.2); }
      if (event.key === '0') { event.preventDefault(); fit(); }
      if (event.key === 'Escape') setSelected(null);
    }}>
      {flow?.nodes.length ? <div ref={layer} className="sv-flow-layer tw:absolute tw:left-0 tw:top-0 tw:[transform-origin:0_0] tw:[will-change:transform]" style={{ width: flow.width, height: flow.height }}>{canvas}</div> : <div className="sv-empty sv-flow-empty">{layoutError ? <><h2>Could not arrange schema</h2><p>{layoutError}</p><button className="sv-button" onClick={() => setAttempt(value => value + 1)}>Retry layout</button></> : !flow ? <><span className="spinner" /><h2>Arranging relationships…</h2></> : <><h2>{search || schema ? 'No matching tables' : 'Your schema, mapped'}</h2><p>{catalog ? 'Adjust the schema filter or table search.' : 'Connect to a data source to visualize its tables and foreign keys.'}</p></>}</div>}
    </div>
    <footer className="sv-map-footer"><span>{mongo ? "Dashed: inferred (unverified) · Dotted: manual reference" : profile.relationships ? "Foreign key → referenced key" : "Object metadata; relationships are not reported by this driver"} <span className="sv-flow-gesture-hint">· Pinch to zoom · Scroll or drag to pan</span></span><div className="sv-zoom-controls"><button aria-label="Fit schema to view" onClick={fit} disabled={!flow?.nodes.length}>Fit</button><button aria-label="Zoom out" disabled={percent <= 15} onClick={() => zoom(1 / 1.2)}><Icon name="minus" size={12} /></button><button aria-label="Reset zoom to 100 percent" onClick={actualSize}>{percent}%</button><button aria-label="Zoom in" disabled={percent >= 250} onClick={() => zoom(1.2)}><Icon name="plus" size={12} /></button></div></footer>
  </section>;
}
