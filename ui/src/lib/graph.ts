export interface GraphNode {
  id: string;
  parent: string | null;
  createdAt: number;
}

export interface GraphRow {
  id: string;
  lane: number;
  lanesAbove: number[];
  merges: number[];
  continues: boolean;
  incoming: boolean;
  width: number;
}

export function layoutGraph(nodes: GraphNode[]): GraphRow[] {
  const sorted = [...nodes].sort((a, b) => b.createdAt - a.createdAt || b.id.localeCompare(a.id));
  const lastRow = new Map(sorted.map((node, index) => [node.id, index]));
  const lanes: (string | null)[] = [];
  return sorted.map((node, row) => {
    const matching = lanes.flatMap((expected, index) => (expected === node.id ? [index] : []));
    const free = lanes.indexOf(null);
    const lane = matching[0] ?? (free === -1 ? lanes.length : free);
    const merges = matching.slice(1);
    const merging = new Set(merges);
    const lanesAbove = lanes.flatMap((expected, index) =>
      expected !== null && index !== lane && !merging.has(index) ? [index] : [],
    );
    for (const index of merges) lanes[index] = null;
    const continues = node.parent !== null && (lastRow.get(node.parent) ?? -1) > row;
    lanes[lane] = continues ? node.parent : null;
    while (lanes.length > 0 && lanes[lanes.length - 1] === null) lanes.pop();
    const width = Math.max(lane, ...lanesAbove, ...merges) + 1;
    return {
      id: node.id,
      lane,
      lanesAbove,
      merges,
      continues,
      incoming: matching.length > 0,
      width,
    };
  });
}
