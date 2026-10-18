import type { CatalogTable, MapRelationship } from './workbench-model';

export interface MongoLink { from: string; field: string; to: string; targetField: string }
const stem = (name: string) => name.replace(/[^a-z0-9]/gi, '').toLowerCase().replace(/ies$/, 'y').replace(/s$/, '');
const identity = (link: MongoLink) => JSON.stringify([link.from, link.field, link.to, link.targetField]);
export function validMongoLink(link: MongoLink, tables: CatalogTable[]): boolean {
  return !!tables.find(t => t.id === link.from)?.table.columns.some(c => c.name === link.field)
    && !!tables.find(t => t.id === link.to)?.table.columns.some(c => c.name === link.targetField);
}
/** Metadata-only hints: no document values are read and no referential integrity is asserted. */
export function mongoRelationships(tables: CatalogTable[], manual: MongoLink[], infer = true): MapRelationship[] {
  const links = manual.filter(link => validMongoLink(link, tables));
  const result: MapRelationship[] = links.map(link => ({id:`manual:${identity(link)}`,from:link.from,to:link.to,columns:[link.field],targetColumns:[link.targetField],name:'Manual reference',origin:'manual',evidence:'User-defined map link; not a database constraint.'}));
  if (!infer) return result;
  for (const source of tables) for (const column of source.table.columns) {
    if (column.name === '_id' || links.some(link => link.from === source.id && link.field === column.name)) continue;
    // Require an explicit id suffix and an eligible sampled type. A matching name alone is not proof.
    const base = column.name.replace(/(?:_?ids?)$/i, '');
    if (!base || base === column.name || !/objectid|string|int|long|double|decimal|array/i.test(column.native_type)) continue;
    const candidates = tables.filter(target => target.schema === source.schema && stem(target.table.name) === stem(base) && target.table.columns.some(c => c.name === '_id'));
    if (candidates.length !== 1) continue;
    const target = candidates[0]!;
    result.push({id:`inferred:${JSON.stringify([source.id,column.name,target.id])}`,from:source.id,to:target.id,columns:[column.name],targetColumns:['_id'],name:'Inferred reference',origin:'inferred',evidence:`Field name and sampled type (${column.native_type}) suggest this link. Document values have not been verified.`});
  }
  return result;
}
export function readMongoLinks(key: string): MongoLink[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key) ?? '[]');
    if (!Array.isArray(value)) return [];
    return value.filter((item): item is MongoLink => item !== null && typeof item === 'object' && ['from','field','to','targetField'].every(key => typeof item[key] === 'string')).slice(0, 1000);
  } catch { return []; }
}
export function saveMongoLinks(key: string, links: MongoLink[]) {
  localStorage.setItem(key, JSON.stringify(links));
}
