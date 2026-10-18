import type { ElkNode, ELK } from 'elkjs/lib/elk-api';
import workerUrl from 'elkjs/lib/elk-worker.min.js?url';
import { relationships, type CatalogTable, type MapRelationship } from './workbench-model';

export const FLOW_WIDTH = 250;
export const FLOW_HEADER = 38;
export const FLOW_ROW = 23;
export const FLOW_BODY_PADDING = 6;
export const FLOW_FOOTER = 26;
export interface FlowNode extends CatalogTable { x: number; y: number; width: number; height: number; columns: CatalogTable['table']['columns'] }
export interface FlowEdge extends MapRelationship { paths: string[]; label?: { text: string; x: number; y: number; width: number } }
export interface SchemaFlow { nodes: FlowNode[]; edges: FlowEdge[]; width: number; height: number }
let engine: Promise<ELK> | undefined;
async function getEngine() {
  engine ??= (typeof Worker === 'function' ? import('elkjs/lib/elk-api.js') : import('elkjs/lib/elk.bundled.js')).then(({ default: ELK }) => new ELK(typeof Worker === 'function' ? { workerUrl } : undefined));
  return engine;
}

/** Keys used by visible relationships stay visible so every connector reaches an actual column. */
export function flowColumns(item: CatalogTable, edges: ReturnType<typeof relationships>) {
  const keys = new Set((item.table.constraints ?? []).filter(key => key.kind === 'PrimaryKey' || key.kind === 'ForeignKey').flatMap(key => key.columns));
  for (const edge of edges) {
    if (edge.from === item.id) edge.columns.forEach(name => keys.add(name));
    if (edge.to === item.id) edge.targetColumns.forEach(name => keys.add(name));
  }
  const important = item.table.columns.filter(column => keys.has(column.name));
  return [...important, ...item.table.columns.filter(column => !keys.has(column.name))].slice(0, Math.max(5, important.length));
}

/** A layered dependency layout with orthogonal, obstacle-aware foreign-key routing. */
export async function layoutSchemaFlow(tables: CatalogTable[], suppliedEdges?: MapRelationship[]): Promise<SchemaFlow> {
  if (!tables.length) return { nodes: [], edges: [], width: 0, height: 0 };
  const ids = new Set(tables.map(table => table.id));
  const edges = (suppliedEdges ?? relationships(tables)).filter(edge => ids.has(edge.from) && ids.has(edge.to));
  const nodes = tables.map(item => {
    const columns = flowColumns(item, edges);
    return { ...item, columns, x: 0, y: 0, width: FLOW_WIDTH, height: FLOW_HEADER + FLOW_BODY_PADDING * 2 + FLOW_ROW * columns.length + FLOW_FOOTER };
  });
  const nodeMap = new Map(nodes.map(node => [node.id, node]));
  const ports = new Map<string, { id: string; width: number; height: number; x: number; y: number; layoutOptions: Record<string, string> }[]>();
  const port = (edge: typeof edges[number], from: boolean) => {
    const nodeId = from ? edge.from : edge.to;
    const node = nodeMap.get(nodeId)!;
    const column = (from ? edge.columns : edge.targetColumns)[0];
    const row = Math.max(0, node.columns.findIndex(item => item.name === column));
    const entry = {
      id: `${edge.id}:${from ? 'source' : 'target'}`, width: 0, height: 0,
      x: from ? FLOW_WIDTH : 0,
      y: FLOW_HEADER + FLOW_BODY_PADDING + row * FLOW_ROW + FLOW_ROW / 2,
      layoutOptions: { 'elk.port.side': from ? 'EAST' : 'WEST' },
    };
    ports.set(nodeId, [...ports.get(nodeId) ?? [], entry]);
    return entry.id;
  };
  const elkEdges = edges.map(edge => {
    const text = edge.columns.join(', ');
    return { id: edge.id, sources: [port(edge, true)], targets: [port(edge, false)], labels: [{ text, width: Math.min(150, text.length * 6 + 10), height: 16 }] };
  });
  const graph: ElkNode = {
    id: 'schema-flow',
    layoutOptions: {
      'elk.algorithm': 'layered', 'elk.direction': 'RIGHT', 'elk.edgeRouting': 'ORTHOGONAL',
      'elk.padding': '[top=36,left=36,bottom=36,right=36]',
      'elk.spacing.nodeNode': '44', 'elk.spacing.componentComponent': '80',
      'elk.layered.spacing.nodeNodeBetweenLayers': '130', 'elk.layered.spacing.edgeNodeBetweenLayers': '28',
      'elk.spacing.edgeNode': '22', 'elk.spacing.edgeEdge': '14',
      'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX',
      'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
      'elk.separateConnectedComponents': 'true', 'elk.aspectRatio': '1.5',
    },
    children: nodes.map(node => ({ id: node.id, width: node.width, height: node.height, ports: ports.get(node.id), layoutOptions: { 'elk.portConstraints': 'FIXED_POS' } })),
    edges: elkEdges,
  };
  const result = await (await getEngine()).layout(graph);
  const placed = new Map(result.children?.map(node => [node.id, node]));
  const routed = new Map(result.edges?.map(edge => [edge.id, edge]));
  return {
    nodes: nodes.map(node => ({ ...node, x: placed.get(node.id)?.x ?? 0, y: placed.get(node.id)?.y ?? 0 })),
    edges: edges.map(edge => {
      const route = routed.get(edge.id);
      const label = route?.labels?.[0];
      return { ...edge,
        paths: (route?.sections ?? []).map(section => [section.startPoint, ...section.bendPoints ?? [], section.endPoint].map((point, i) => `${i ? 'L' : 'M'} ${point.x} ${point.y}`).join(' ')),
        label: label?.x !== undefined && label.y !== undefined ? { text: label.text ?? '', x: label.x, y: label.y, width: label.width ?? 80 } : undefined,
      };
    }),
    width: result.width ?? FLOW_WIDTH + 72, height: result.height ?? 300,
  };
}
