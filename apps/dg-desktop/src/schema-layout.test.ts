import { describe, expect, it } from 'vitest';
import { layoutSchemaFlow, flowColumns } from './schema-layout';
import { relationships, tableId, type CatalogTable } from './workbench-model';

function table(name: string, parents: string[] = []): CatalogTable {
  return { id: tableId('main', name), schema: 'main', table: { name, kind: 'Table',
    columns: ['id', ...parents.map((_, i) => `parent_${i}`), 'label', 'value'].map(name => ({ name, native_type: 'INTEGER', nullable: false })),
    constraints: [{ name: `${name}_pk`, kind: 'PrimaryKey', columns: ['id'], definition: '', reference: null }, ...parents.map((parent, i) => ({ name: `${name}_${i}`, kind: 'ForeignKey' as const, columns: [`parent_${i}`], definition: '', reference: { schema: 'main', table: parent, columns: ['id'] } }))],
  } };
}
const points = (path: string) => path.split(/[ML]/).map(value => value.trim()).filter(Boolean).map(value => { const [x, y] = value.split(/\s+/).map(Number); return { x: x!, y: y! }; });

describe('schema flow layout', () => {
  it('ranks a dependency chain by foreign keys rather than alphabetical grid position', async () => {
    const flow = await layoutSchemaFlow([table('a_root'), table('z_child', ['a_root']), table('m_leaf', ['z_child'])]);
    const root = flow.nodes.find(n => n.table.name === 'a_root')!;
    const child = flow.nodes.find(n => n.table.name === 'z_child')!;
    const leaf = flow.nodes.find(n => n.table.name === 'm_leaf')!;
    expect(leaf.x).toBeLessThan(child.x); expect(child.x).toBeLessThan(root.x);
    expect(flow.edges).toHaveLength(2);
  });
  it('routes orthogonal connectors around unrelated nodes without overlapping table cards', async () => {
    const flow = await layoutSchemaFlow([table('root'), table('child', ['root']), table('branch', ['root']), table('leaf', ['root', 'child']), table('island')]);
    for (const a of flow.nodes) for (const b of flow.nodes) if (a.id !== b.id) {
      expect(a.x + a.width <= b.x || b.x + b.width <= a.x || a.y + a.height <= b.y || b.y + b.height <= a.y).toBe(true);
    }
    for (const edge of flow.edges) for (const path of edge.paths) {
      const vertices = points(path); expect(vertices.length).toBeGreaterThanOrEqual(2);
      for (let i = 1; i < vertices.length; i++) {
        const a = vertices[i - 1]!; const b = vertices[i]!;
        expect(a.x === b.x || a.y === b.y).toBe(true);
        for (const node of flow.nodes.filter(node => node.id !== edge.from && node.id !== edge.to)) {
          const crosses = a.x === b.x ? a.x > node.x && a.x < node.x + node.width && Math.max(a.y, b.y) > node.y && Math.min(a.y, b.y) < node.y + node.height
            : a.y > node.y && a.y < node.y + node.height && Math.max(a.x, b.x) > node.x && Math.min(a.x, b.x) < node.x + node.width;
          expect(crosses, `edge ${edge.name} crosses ${node.table.name}`).toBe(false);
        }
      }
    }
  });
  it('handles cyclic and self-referencing keys and keeps their endpoints visible', async () => {
    const tables = [table('one', ['two', 'one']), table('two', ['one'])];
    const flow = await layoutSchemaFlow(tables);
    expect(flow.edges).toHaveLength(3);
    expect(flow.edges.every(edge => edge.paths.length > 0)).toBe(true);
    const many = table('many', Array.from({ length: 9 }, () => 'one'));
    const columns = flowColumns(many, relationships([many, ...tables]));
    expect(columns).toHaveLength(10);
    expect(columns.map(column => column.name)).toContain('parent_8');
  });
  it('returns an empty flow for an empty catalog', async () => {
    expect(await layoutSchemaFlow([])).toEqual({ nodes: [], edges: [], width: 0, height: 0 });
  });
  it('routes supplied MongoDB references and retains their provenance', async () => {
    const source = table('posts'); const target = table('users');
    const edge = {id:'mongo-link',from:source.id,to:target.id,columns:['label'],targetColumns:['id'],name:'Inferred reference',origin:'inferred' as const,evidence:'Unverified name hint'};
    const flow = await layoutSchemaFlow([source,target],[edge]);
    expect(flow.edges).toHaveLength(1);
    expect(flow.edges[0]).toMatchObject({origin:'inferred',evidence:'Unverified name hint'});
    expect(flow.edges[0]!.paths.length).toBeGreaterThan(0);
    expect((await layoutSchemaFlow([source],[edge])).edges).toEqual([]);
  });
});
