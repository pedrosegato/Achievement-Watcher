import { describe, expect, it } from "vitest";
import { layoutGraph } from "./graph";

const node = (id: string, parent: string | null, at: number) => ({ id, parent, createdAt: at });

describe("layoutGraph", () => {
  it("puts a straight history in one lane, newest first", () => {
    const rows = layoutGraph([node("a", null, 1), node("b", "a", 2), node("c", "b", 3)]);
    expect(rows.map((r) => [r.id, r.lane])).toEqual([
      ["c", 0],
      ["b", 0],
      ["a", 0],
    ]);
    expect(rows[0]?.lanesAbove).toEqual([]);
    expect(rows[0]?.continues).toBe(true);
    expect(rows[2]?.continues).toBe(false);
    expect(rows[0]?.incoming).toBe(false);
    expect(rows[1]?.incoming).toBe(true);
  });

  it("opens a second lane when two versions share a parent", () => {
    const rows = layoutGraph([node("a", null, 1), node("b", "a", 2), node("c", "a", 3)]);
    expect(rows.map((r) => [r.id, r.lane])).toEqual([
      ["c", 0],
      ["b", 1],
      ["a", 0],
    ]);
    expect(rows[1]?.lanesAbove).toEqual([0]);
    expect(rows[2]?.merges).toEqual([1]);
  });

  it("starts a new lane for a version whose parent is gone", () => {
    const rows = layoutGraph([node("a", null, 1), node("b", "deleted", 2)]);
    expect(rows.map((r) => [r.id, r.lane])).toEqual([
      ["b", 0],
      ["a", 0],
    ]);
    expect(rows[0]?.continues).toBe(false);
  });

  it("reports how many lanes the drawing needs", () => {
    const rows = layoutGraph([node("a", null, 1), node("b", "a", 2), node("c", "a", 3)]);
    expect(Math.max(...rows.map((r) => r.width))).toBe(2);
  });

  it("survives a duplicated snapshot folder", () => {
    const rows = layoutGraph([
      node("a", null, 1),
      node("b", "a", 2),
      node("b", "a", 2),
      node("c", "b", 3),
    ]);
    expect(rows).toHaveLength(4);
    for (const row of rows) expect(row.width).toBeGreaterThan(row.lane);
  });

  it("keeps every line closed for random histories, skewed clocks and garbage", () => {
    let seed = 7;
    const random = (n: number) => {
      seed = (seed * 1103515245 + 12345) % 2147483648;
      return seed % n;
    };
    for (let round = 0; round < 500; round++) {
      const nodes: { id: string; parent: string | null; createdAt: number }[] = [];
      const count = 1 + random(25);
      for (let i = 0; i < count; i++) {
        const pick = random(10);
        const parent =
          pick === 0
            ? null
            : pick === 1
              ? "deleted"
              : pick === 2
                ? `n${i}`
                : nodes.length > 0
                  ? (nodes[random(nodes.length)]?.id ?? null)
                  : null;
        nodes.push(node(`n${i}`, parent, i * 100 + random(400) - 200));
      }
      const rows = layoutGraph(nodes);
      expect(rows).toHaveLength(nodes.length);
      rows.forEach((row, index) => {
        expect(row.lane).toBeGreaterThanOrEqual(0);
        expect(row.width).toBeGreaterThan(row.lane);
        for (const other of [...row.lanesAbove, ...row.merges])
          expect(row.width).toBeGreaterThan(other);
        if (row.continues) {
          const parent = nodes.find((n) => n.id === row.id)?.parent;
          const below = rows.slice(index + 1).some((r) => r.id === parent && r.incoming);
          expect(below, `${row.id} draws a line to ${parent} that never arrives`).toBe(true);
        }
      });
    }
  });
});
