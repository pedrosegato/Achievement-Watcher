import { useRef } from "react";
import { useElementWidth } from "../../hooks/useElementWidth";
import { previewUrl } from "../../lib/presets";
import type { PresetInfo } from "../../types/domain";

const THUMB = { width: 560, height: 190 };

export function PresetThumb({ preset, name }: { preset: PresetInfo; name: string }) {
  const frame = useRef<HTMLSpanElement>(null);
  const scale = useElementWidth(frame) / THUMB.width;
  const x = ((THUMB.width - preset.width) / 2) * scale;
  const y = ((THUMB.height - preset.height) / 2) * scale;
  return (
    <span
      ref={frame}
      className="relative block aspect-[56/19] overflow-hidden rounded-[3px] bg-surface-2"
    >
      {scale > 0 && (
        <iframe
          title={name}
          tabIndex={-1}
          sandbox="allow-scripts"
          src={previewUrl(preset.id, "unlock", "middle-center")}
          className="pointer-events-none absolute top-0 left-0 origin-top-left border-0 bg-transparent"
          style={{
            width: preset.width,
            height: preset.height,
            colorScheme: "normal",
            transform: `translate(${x}px, ${y}px) scale(${scale})`,
          }}
        />
      )}
    </span>
  );
}
