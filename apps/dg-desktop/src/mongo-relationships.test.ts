// @vitest-environment happy-dom
import { afterEach, expect, it } from 'vitest';
import { mongoRelationships, readMongoLinks, saveMongoLinks } from './mongo-relationships';
import { tableId, type CatalogTable } from './workbench-model';
const table = (schema: string, name: string, fields = ['_id']): CatalogTable => ({id:tableId(schema,name),schema,table:{name,kind:'Table',columns:fields.map(name => ({name,native_type:'objectId',nullable:false}))}});
afterEach(() => localStorage.clear());
it('infers camelCase, snake_case and plural array references without creating FK constraints', () => {
  const users = table('db','users'), communities = table('db','communities');
  const posts = table('db','posts',['_id','userId','community_id','user_ids','title']);
  posts.table.columns[3]!.native_type = 'array';
  const links = mongoRelationships([users,communities,posts],[]);
  expect(links.map(link => [link.columns[0],link.to,link.origin])).toEqual([['userId',users.id,'inferred'],['community_id',communities.id,'inferred'],['user_ids',users.id,'inferred']]);
  expect(posts.table.constraints).toBeUndefined();
  expect(links.every(link => link.evidence?.includes('not been verified'))).toBe(true);
});
it('does not guess across databases, ambiguous collection names, or missing target fields', () => {
  expect(mongoRelationships([table('db','posts',['userId']),table('other','users')],[])).toEqual([]);
  expect(mongoRelationships([table('db','posts',['userId']),table('db','users'),table('db','user')],[])).toEqual([]);
  expect(mongoRelationships([table('db','posts',['userId']),table('db','users',['name'])],[])).toEqual([]);
});
it('manual links override name hints, support custom fields, and remain when inference is disabled', () => {
  const posts = table('db','posts',['userId']), users = table('db','users'), authors = table('db','authors',['externalId']);
  const manual = [{from:posts.id,field:'userId',to:authors.id,targetField:'externalId'}];
  for (const enabled of [true,false]) {
    const links = mongoRelationships([posts,users,authors],manual,enabled);
    expect(links).toHaveLength(1);expect(links[0]).toMatchObject({origin:'manual',to:authors.id,targetColumns:['externalId']});
  }
  expect(mongoRelationships([posts,users],manual,false)).toEqual([]);
});
it('persists per-source links and tolerates malformed or obsolete local data', () => {
  const links = [{from:'posts',field:'authorId',to:'users',targetField:'_id'}];
  saveMongoLinks('source-a',links);
  expect(readMongoLinks('source-a')).toEqual(links);
  expect(readMongoLinks('source-b')).toEqual([]);
  localStorage.setItem('source-a','[null,{},4]');expect(readMongoLinks('source-a')).toEqual([]);
  localStorage.setItem('source-a','broken');expect(readMongoLinks('source-a')).toEqual([]);
});
