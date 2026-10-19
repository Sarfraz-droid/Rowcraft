import { useState } from 'react';
import type { CatalogTable } from '../workbench-model';
import { type MongoLink, validMongoLink } from '../mongo-relationships';
import { SelectField } from './SelectField';

export function MongoReferences({tables, links, onChange, error}: {tables: CatalogTable[]; links: MongoLink[]; onChange(links: MongoLink[]): void; error: string}) {
  const [from, setFrom] = useState('');
  const [field, setField] = useState('');
  const [to, setTo] = useState('');
  const [targetField, setTargetField] = useState('');
  const source = tables.find(table => table.id === from);
  const target = tables.find(table => table.id === to);
  const link = {from, field, to, targetField};
  const duplicate = links.some(item => item.from === from && item.field === field && item.to === to && item.targetField === targetField);
  const objects = [{value:'',label:'Choose collection'}, ...tables.map(table => ({value:table.id,label:`${table.schema}.${table.table.name}`}))];
  const fields = (table?: CatalogTable) => [{value:'',label:'Choose field'}, ...(table?.table.columns.map(column => ({value:column.name,label:column.name})) ?? [])];
  const label = (id: string) => { const table = tables.find(table => table.id === id); if (table) return `${table.schema}.${table.table.name}`; try { const names: unknown = JSON.parse(id); return Array.isArray(names) ? names.join('.') : id; } catch { return id; } };
  return <details className="mongo-references">
    <summary>Manage references · {links.length} manual</summary>
    <p>Inferred links use field names and sampled types; document values are unverified. Manual links describe your model and do not create MongoDB constraints. A manual link overrides suggestions for that field.</p>
    <form onSubmit={event => {event.preventDefault(); if (validMongoLink(link, tables) && !duplicate) onChange([...links, link]);}}>
      <label>From collection<SelectField aria-label="Reference source collection" value={from} onValueChange={value => {setFrom(value);setField('');}} options={objects} /></label>
      <label>Reference field<SelectField aria-label="Reference source field" value={field} onValueChange={setField} options={fields(source)} /></label>
      <span aria-hidden="true">→</span>
      <label>To collection<SelectField aria-label="Reference target collection" value={to} onValueChange={value => {setTo(value);setTargetField(tables.find(table => table.id === value)?.table.columns.some(column => column.name === '_id') ? '_id' : '');}} options={objects} /></label>
      <label>Target field<SelectField aria-label="Reference target field" value={targetField} onValueChange={setTargetField} options={fields(target)} /></label>
      <button className="sv-button" type="submit" disabled={!validMongoLink(link,tables) || duplicate || links.length >= 1000}>Add reference</button>
    </form>
    {error && <p role="alert">{error}</p>}
    {links.length > 0 && <ul>{links.map((item,index) => <li key={JSON.stringify(item)}><span>{label(item.from)}.{item.field} → {label(item.to)}.{item.targetField}{!validMongoLink(item,tables) && ' · Not in current metadata'}</span><button type="button" className="sv-text-button" aria-label={`Remove reference ${label(item.from)}.${item.field} to ${label(item.to)}.${item.targetField}`} onClick={() => onChange(links.filter((_,at) => at !== index))}>Remove</button></li>)}</ul>}
  </details>;
}
