import { DEFAULT_PRESET, POPUP_MARGIN, PROTOCOL_ORIGIN, SCREEN } from "../constants";
import type { Position, PresetInfo, PreviewKind } from "../types/domain";
import type { Translate } from "../types/ui";

interface Frame {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function presetName(preset: PresetInfo, t: Translate): string {
  return preset.builtin && preset.id === DEFAULT_PRESET ? t("settings.defaultStyle") : preset.name;
}

export function effectivePreset(presets: PresetInfo[], wanted: string): PresetInfo | undefined {
  const valid = presets.filter((preset) => preset.valid);
  const id = wanted.toLowerCase();
  return (
    valid.find((preset) => preset.id.toLowerCase() === id) ??
    valid.find((preset) => preset.id === DEFAULT_PRESET)
  );
}

export function previewUrl(id: string, kind: PreviewKind, position: Position | "middle-center") {
  return `${PROTOCOL_ORIGIN}/preset/${encodeURIComponent(id)}/index.html?preview=loop&kind=${kind}&position=${position}`;
}

export function popupFrame(preset: PresetInfo, position: Position, scale: number): Frame {
  const width = preset.width * scale;
  const height = preset.height * scale;
  const insetX = POPUP_MARGIN - preset.bleed[0] * scale;
  const insetY = POPUP_MARGIN - preset.bleed[1] * scale;
  const [row, column] = position.split("-");
  const freeWidth = SCREEN.width - width;
  const freeHeight = SCREEN.height - height;
  const xs: Record<string, number> = { left: insetX, right: freeWidth - insetX };
  const ys: Record<string, number> = { top: insetY, bottom: freeHeight - insetY };
  return {
    x: xs[column ?? ""] ?? freeWidth / 2,
    y: ys[row ?? ""] ?? freeHeight / 2,
    width,
    height,
  };
}
