import { beforeAll, describe, expect, it } from "vitest";
import layout from "../../../crates/aw-app/resources/presets/xbox/layout.json";
import playerSource from "../../../crates/aw-app/resources/presets/xbox/player.js?raw";
import preset from "../../../crates/aw-app/resources/presets/xbox/preset.json";
import spec from "../../../crates/aw-app/resources/presets/xbox/toasts.json";
import golden from "./fixtures/xbox-golden.json";

type Values = Record<string, Record<string, number>>;
interface Board {
  tracks: { el: string }[];
}
interface LayoutNode {
  name?: string;
  style?: Record<string, string | undefined>;
  children?: LayoutNode[];
}
interface Timeline {
  step: (t: number) => Values;
}
interface LaurelPose {
  stem: { scale: number; op: number };
  leaves: { rot: number; scale: number; op: number }[];
}
interface Engine {
  measureWidth: (widths: number[], toast: typeof layout.toast) => number;
  accentShades: (hex: string, shades: typeof layout.accent) => Record<string, string>;
  createTimeline: (spec: unknown, layout: unknown, kind: string, width: number) => Timeline;
  laurelPose: (
    leafSet: unknown,
    ease: unknown,
    start: number,
    hide: number,
    t: number,
  ) => LaurelPose;
  frameIndex: (flipbook: Record<string, unknown>, elapsed: number) => number;
}

const FRAME = 1 / 30;
const TEXT_EXIT = 0.267;
const KINDS = ["normal", "rare", "mythic"] as const;
type Kind = (typeof KINDS)[number];
const REFERENCE_HIDE: Record<Kind, number> = { normal: 6.2, rare: 6.8, mythic: 8.6 };
const SWOOSH_HIT: Record<Kind, number> = { normal: 9.85, rare: 9.85, mythic: 11.85 };
const POPUP_SECONDS: Record<Kind, [keyof typeof preset.durations, number]> = {
  normal: ["unlock", 10.8],
  rare: ["rare", 10.8],
  mythic: ["platinum", 12.8],
};
const referenceLayout = structuredClone(layout);
for (const kind of KINDS) referenceLayout.panels[kind].seq.hide = REFERENCE_HIDE[kind];
const TOLERANCE: Record<string, number> = {
  tx: 0.6,
  ty: 0.6,
  rot: 0.6,
  sx: 0.02,
  sy: 0.02,
  op: 0.02,
  vis: 0,
};

let engine: Engine;

beforeAll(() => {
  new Function(playerSource)();
  engine = (globalThis as unknown as { AwXbox: Engine }).AwXbox;
});

function sampleAt(kind: string, width: number, at: number, source: unknown = layout): Values {
  const timeline = engine.createTimeline(spec, source, kind, width);
  let values = timeline.step(0);
  for (let t = FRAME; t < at; t += FRAME) values = timeline.step(t);
  return at > 0 ? timeline.step(at) : values;
}

describe("xbox engine", () => {
  it("measures the toast width like Game Bar", () => {
    expect(engine.measureWidth([10, 5], layout.toast)).toBe(300);
    expect(engine.measureWidth([284.83, 267.15], layout.toast)).toBe(361);
    expect(engine.measureWidth([900, 10], layout.toast)).toBe(530);
  });

  it("sizes every text block from the measured font size", () => {
    const sizes: string[] = [];
    const collect = (node: LayoutNode) => {
      if (node.name === "FrontText" || node.name === "BackText") {
        sizes.push(node.style?.["font-size"] ?? "");
      }
      for (const child of node.children ?? []) collect(child);
    };
    for (const panel of Object.values(layout.panels)) collect(panel.toast);
    expect(layout.toast.fontSize).toBe(22);
    expect(sizes).toHaveLength(6);
    expect(new Set(sizes)).toEqual(new Set(["22px"]));
  });

  it("derives Light1 and Dark1 from the accent", () => {
    expect(engine.accentShades("#107C10", layout.accent)).toEqual({
      accent: "rgb(16, 124, 16)",
      light: "rgb(23, 178, 23)",
      dark: "rgb(10, 79, 10)",
    });
    expect(engine.accentShades("#0078D4", layout.accent)).toEqual({
      accent: "rgb(0, 120, 212)",
      light: "rgb(18, 152, 255)",
      dark: "rgb(0, 91, 161)",
    });
  });

  it("plays flipbooks with holds, loops and boomerang", () => {
    const intro = { FrameCount: "43", DesiredFrameRate: "30", hold: [1, 36] };
    expect(engine.frameIndex(intro, 0)).toBe(0);
    expect(engine.frameIndex(intro, 1 / 30 + 0.001)).toBe(1);
    expect(engine.frameIndex(intro, 30 / 30 + 0.001)).toBe(1);
    expect(engine.frameIndex(intro, 38 / 30 + 0.001)).toBe(2);
    expect(engine.frameIndex(intro, 10)).toBe(42);
    const aurora = { FrameCount: "60", DesiredFrameRate: "8", Boomerang: "True", rep: -1 };
    expect(engine.frameIndex(aurora, 59 / 8 + 0.01)).toBe(59);
    expect(engine.frameIndex(aurora, 60 / 8 + 0.01)).toBe(58);
    const loop = { FrameCount: "20", DesiredFrameRate: "30", rep: 10 };
    expect(engine.frameIndex(loop, 21 / 30 + 0.001)).toBe(1);
  });

  for (const kind of KINDS) {
    it(`matches the reference timeline for ${kind}`, () => {
      const reference = golden.kinds[kind];
      const props = golden.props;
      const hide = REFERENCE_HIDE[kind];
      const states = (spec as unknown as Record<string, { states: Record<string, Board> }>)[
        layout.panels[kind].panel
      ].states;
      const delayed = new Set([...states.Show.tracks, ...states.Hide.tracks].map((t) => t.el));
      const misses: string[] = [];
      reference.times.forEach((at, index) => {
        const values = sampleAt(kind, reference.W, at, referenceLayout);
        const late =
          at >= hide ? sampleAt(kind, reference.W, at + TEXT_EXIT, referenceLayout) : values;
        for (const [name, rows] of Object.entries(reference.els)) {
          if (name === "BackText" && at >= hide) continue;
          const source = delayed.has(name) ? late : values;
          const expected = rows[index];
          props.forEach((prop, slot) => {
            const actual = source[name]?.[prop];
            const want = expected?.[slot];
            if (actual === undefined || want === undefined) return;
            if (Math.abs(actual - want) > (TOLERANCE[prop] ?? 0)) {
              misses.push(`${at}s ${name}.${prop}: ${actual.toFixed(3)} vs ${want}`);
            }
          });
        }
      });
      expect(misses.slice(0, 12)).toEqual([]);
    });
  }

  for (const kind of KINDS) {
    it(`rotates the text out before the pill shrinks for ${kind}`, () => {
      const hide = layout.panels[kind].seq.hide;
      const width = golden.kinds[kind].W;
      const start = sampleAt(kind, width, hide);
      const sliding = sampleAt(kind, width, hide + TEXT_EXIT / 2);
      const gone = sampleAt(kind, width, hide + TEXT_EXIT);
      const closed = sampleAt(kind, width, hide + TEXT_EXIT + 0.7);
      expect(start.BackText?.ty).toBeCloseTo(0, 1);
      expect(sliding.TextGrid?.op).toBeCloseTo(1, 2);
      expect(sliding.ToastBackground?.sx).toBeCloseTo(1, 2);
      expect(gone.BackText?.ty).toBeCloseTo(-100, 0);
      expect(gone.ToastBackground?.sx).toBeCloseTo(1, 2);
      expect(closed.ToastBackground?.sx).toBeCloseTo(0, 2);
    });
  }

  for (const kind of KINDS) {
    it(`closes the pill on the sound's swoosh for ${kind}`, () => {
      const width = golden.kinds[kind].W;
      const hit = SWOOSH_HIT[kind];
      const [key, seconds] = POPUP_SECONDS[kind];
      expect(sampleAt(kind, width, hit - 0.95).BackText?.ty).toBeCloseTo(0, 1);
      expect(sampleAt(kind, width, hit - 0.68).ToastBackground?.sx).toBeCloseTo(1, 2);
      expect(sampleAt(kind, width, hit - 0.1).ToastBackground?.sx).toBeGreaterThan(0.1);
      expect(sampleAt(kind, width, hit + 0.02).ToastBackground?.sx).toBeCloseTo(0, 2);
      expect(preset.durations[key]).toBe(seconds);
    });
  }

  for (const kind of ["rare", "mythic"] as const) {
    it(`matches the reference laurel for ${kind}`, () => {
      const reference = golden.kinds[kind];
      const panel = layout.panels[kind];
      const leafSet = (spec as unknown as { laurel: Record<string, unknown> }).laurel[
        panel.laurel ?? ""
      ];
      const misses: string[] = [];
      reference.times.forEach((at, index) => {
        const expected = reference.laurel?.[index];
        if (!expected) return;
        const pose = engine.laurelPose(
          leafSet,
          layout.laurelEase,
          panel.seq.laurel ?? 0,
          REFERENCE_HIDE[kind],
          at,
        );
        const parts = [{ rot: 0, ...pose.stem }, ...pose.leaves];
        expected.forEach(([rot, scale, op], slot) => {
          const part = parts[slot];
          if (!part) return;
          if (slot > 0 && Math.abs(part.rot - rot) > 0.6)
            misses.push(`${at}s leaf ${slot} rot ${part.rot} vs ${rot}`);
          if (Math.abs(part.scale - scale) > 0.012)
            misses.push(`${at}s part ${slot} scale ${part.scale} vs ${scale}`);
          if (part.op !== op) misses.push(`${at}s part ${slot} op ${part.op} vs ${op}`);
        });
      });
      expect(misses.slice(0, 12)).toEqual([]);
    });
  }
});
