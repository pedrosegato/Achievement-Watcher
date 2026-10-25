import { useRef, useState } from "react";
import { Icon } from "../../components/Icon";
import { Segmented } from "../../components/Segmented";
import { Button } from "../../components/ui/button";
import { POSITIONS, PREVIEW_KINDS, SCREEN } from "../../constants";
import { useElementWidth } from "../../hooks/useElementWidth";
import { POSITION_LABELS, PREVIEW_LABELS } from "../../i18n/labels";
import { useI18n } from "../../i18n/useI18n";
import { cn } from "../../lib/cn";
import { api } from "../../lib/commands";
import { popupFrame, presetName, previewUrl } from "../../lib/presets";
import type { Position, PresetInfo, PreviewKind } from "../../types/domain";

interface NotificationPreviewProps {
  preset: PresetInfo | undefined;
  position: Position;
  scale: number;
  onPosition: (position: Position) => void;
}

const SPOTS: Record<Position, string> = {
  "top-left": "row-start-1 col-start-1",
  "top-center": "row-start-1 col-start-2",
  "top-right": "row-start-1 col-start-3",
  "middle-left": "row-start-2 col-start-1",
  "middle-right": "row-start-2 col-start-3",
  "bottom-left": "row-start-3 col-start-1",
  "bottom-center": "row-start-3 col-start-2",
  "bottom-right": "row-start-3 col-start-3",
};

export function NotificationPreview({
  preset,
  position,
  scale,
  onPosition,
}: NotificationPreviewProps) {
  const { t } = useI18n();
  const [kind, setKind] = useState<PreviewKind>("unlock");
  const screen = useRef<HTMLDivElement>(null);
  const ratio = useElementWidth(screen) / SCREEN.width;
  const frame = preset && popupFrame(preset, position, scale);
  const kinds = PREVIEW_KINDS.map((value) => ({ value, label: t(PREVIEW_LABELS[value]) }));

  return (
    <div className="flex flex-col gap-2.5">
      <div
        ref={screen}
        className="relative aspect-video overflow-hidden rounded-card border border-line bg-surface-2"
      >
        <fieldset
          aria-label={t("settings.position")}
          className="absolute inset-0 grid min-w-0 grid-cols-3 grid-rows-3 gap-1.5 p-1.5"
        >
          {POSITIONS.map((spot) => (
            <button
              key={spot}
              type="button"
              aria-label={t(POSITION_LABELS[spot])}
              aria-pressed={spot === position}
              onClick={() => onPosition(spot)}
              className={cn(
                "cursor-pointer rounded-tag border border-line border-dashed transition-colors duration-(--dur-fast) ease-(--ease-out) hover:border-dim hover:bg-surface aria-pressed:border-dim aria-pressed:border-solid",
                SPOTS[spot],
              )}
            />
          ))}
        </fieldset>
        {preset && frame && ratio > 0 && (
          <iframe
            key={`${preset.id}:${kind}:${position}`}
            title={presetName(preset, t)}
            tabIndex={-1}
            sandbox="allow-scripts"
            src={previewUrl(preset.id, kind, position)}
            className="pointer-events-none absolute top-0 left-0 origin-top-left border-0 bg-transparent"
            style={{
              width: preset.width,
              height: preset.height,
              colorScheme: "normal",
              transform: `translate(${frame.x * ratio}px, ${frame.y * ratio}px) scale(${ratio * scale})`,
            }}
          />
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Segmented
          label={t("settings.previewKind")}
          options={kinds}
          value={kind}
          onChange={setKind}
        />
        {preset && (
          <Button
            size="sm"
            className="ml-auto"
            onClick={() => {
              api.previewPreset(preset.id, kind).catch(() => undefined);
            }}
          >
            <Icon name="play" size={15} />
            {t("settings.showOnScreen")}
          </Button>
        )}
      </div>
    </div>
  );
}
