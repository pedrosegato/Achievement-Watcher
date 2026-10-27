import { useI18n } from "../../i18n/useI18n";
import { presetName } from "../../lib/presets";
import type { PresetInfo } from "../../types/domain";
import { PresetThumb } from "./PresetThumb";

interface PresetPickerProps {
  presets: PresetInfo[];
  selected: string | undefined;
  onSelect: (id: string) => void;
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
