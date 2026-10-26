import { useRef } from "react";
import { useElementWidth } from "../../hooks/useElementWidth";
import { useI18n } from "../../i18n/useI18n";
import { presetName, previewUrl } from "../../lib/presets";
import type { PresetInfo } from "../../types/domain";

interface PresetPickerProps {
  presets: PresetInfo[];
  selected: string | undefined;
  onSelect: (id: string) => void;
}

const THUMB = { width: 560, height: 190 };

function PresetThumb({ preset, name }: { preset: PresetInfo; name: string }) {
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

export function PresetPicker({ presets, selected, onSelect }: PresetPickerProps) {
  const { t } = useI18n();
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(128px,1fr))] gap-2">
      {presets
        .filter((preset) => preset.valid)
        .map((preset) => {
          const name = presetName(preset, t);
          return (
            <button
              key={preset.id}
              type="button"
              aria-pressed={preset.id === selected}
              onClick={() => onSelect(preset.id)}
              className="flex cursor-pointer flex-col gap-[7px] rounded-card border border-line bg-surface p-1.5 text-left transition-colors duration-(--dur-fast) ease-(--ease-out) hover:bg-surface-2 aria-pressed:border-text aria-pressed:shadow-[inset_0_0_0_1px_var(--text)]"
            >
              <PresetThumb preset={preset} name={name} />
              <span className="truncate px-0.5 font-semibold text-[12.5px]">{name}</span>
            </button>
          );
        })}
    </div>
  );
}
