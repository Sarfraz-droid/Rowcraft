import { describe, expect, it } from 'vitest';
import type { CatalogTable } from './workbench-model';
import { columnTypes, filterTables, relationships, tableId } from './workbench-model';
const table = (schema: string, name: string, columns = 3): CatalogTable => ({ id: tableId(schema,name), schema, table: { name, kind: 'Table', columns: Array.from({length:columns}, (_,i)=>({name:`column_${i}`,native_type:i%2?'TEXT':'INTEGER',nullable:false})) } });
describe('schema map model', () => {
 it('uses collision-free identities for schema and table names',()=>{ expect(tableId('a.b','c')).not.toBe(tableId('a','b.c')); });
 it('only draws declared foreign keys whose targets are visible',()=>{ const users=table('public','users'); const posts=table('public','posts'); posts.table.constraints=[{name:'owner',kind:'ForeignKey',columns:['owner_id'],definition:'',reference:{schema:'public',table:'users',columns:['id']}},{name:'external',kind:'ForeignKey',columns:['team_id'],definition:'',reference:{schema:'private',table:'teams',columns:['id']}}]; expect(relationships([users,posts])).toEqual([{id:`${posts.id}:0`,from:posts.id,to:users.id,columns:['owner_id'],targetColumns:['id'],name:'owner'}]); expect(relationships([posts])).toEqual([]); });
 it('searches schema-qualified names and intersects the schema filter',()=>{const tables=[table('public','users'),table('audit','users')]; expect(filterTables(tables,'audit','USER')).toEqual([tables[1]]);expect(filterTables(tables,'public','audit')).toEqual([]);});
 it('aggregates actual column metadata and normalizes type lengths',()=>{const item=table('main','users',0);item.table.columns=[{name:'a',native_type:'varchar(40)',nullable:true},{name:'b',native_type:'VARCHAR(12)',nullable:true}];expect(columnTypes([item])).toEqual([{name:'VARCHAR',count:2}]);});
});
