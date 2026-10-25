import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { Icon } from "../../components/Icon";
import { TIMINGS } from "../../constants";
import { useWindowFocus } from "../../hooks/useWindowFocus";
import { SOUND_EVENT_LABELS } from "../../i18n/labels";
import { useI18n } from "../../i18n/useI18n";
import { cn } from "../../lib/cn";
import { api } from "../../lib/commands";
import { queries } from "../../lib/queries";
import type { SoundEvent, SoundPack } from "../../types/domain";

interface SoundPacksProps {
  selected: string;
  onSelect: (id: string) => void;
}

function packEvents(pack: SoundPack): SoundEvent[] {
  const events: SoundEvent[] = ["unlock"];
  if (pack.hasRare) events.push("rare");
  if (pack.hasPlatinum) events.push("platinum");
  return events;
}

export function SoundPacks({ selected, onSelect }: SoundPacksProps) {
  const { t } = useI18n();
  const { data: packs, refetch } = useQuery(queries.sounds());
  useWindowFocus(() => void refetch());
  const [playing, setPlaying] = useState<string | null>(null);
  const timer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  const play = (id: string, event: SoundEvent) => {
    const tag = `${id}:${event}`;
    setPlaying(tag);
    api
      .previewSound(id, event)
      .then((duration) => {
        if (timer.current !== null) window.clearTimeout(timer.current);
        timer.current = window.setTimeout(
          () => setPlaying((current) => (current === tag ? null : current)),
          duration ?? TIMINGS.soundFallbackMs,
        );
      })
      .catch(() => setPlaying(null));
  };

  const current = selected.toLowerCase();
  return (
    <div role="radiogroup" aria-label={t("settings.soundPack")} className="flex flex-col gap-0.5">
      {(packs ?? []).map((pack) => (
        <div
          key={pack.id}
          className="pack-row relative flex items-center gap-3 rounded-card px-2.5 py-2 transition-colors duration-(--dur-fast) ease-(--ease-out) hover:bg-surface"
        >
          <label className="flex flex-1 cursor-pointer items-center gap-3 after:absolute after:inset-0 after:content-['']">
            <input
              type="radio"
              name="sound-pack"
              className="peer sr-only"
              checked={pack.id.toLowerCase() === current}
              readOnly
              onClick={() => {
                onSelect(pack.id);
                play(pack.id, "unlock");
              }}
            />
            <span className="grid size-4 place-items-center rounded-full border-[1.5px] border-dim peer-checked:border-text peer-checked:after:size-2 peer-checked:after:rounded-full peer-checked:after:bg-text peer-checked:after:content-['']" />
            <span className="font-semibold text-[13.5px]">
              {pack.builtin ? pack.name : t("settings.customPack", { name: pack.name })}
            </span>
          </label>
          <span className="relative z-1 flex gap-1">
            {packEvents(pack).map((event) => {
              const active = playing === `${pack.id}:${event}`;
              return (
                <button
                  key={event}
                  type="button"
                  onClick={() => play(pack.id, event)}
                  className={cn(
                    "inline-flex cursor-pointer items-center gap-1.5 rounded-card border border-line px-[9px] py-1 font-semibold text-[12px] text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:bg-surface-2",
                    active && "border-text bg-text text-bg hover:bg-text",
                  )}
                >
                  <Icon name="play" size={14} className={active ? "sound-beat" : undefined} />
                  {t(SOUND_EVENT_LABELS[event])}
                </button>
              );
            })}
          </span>
        </div>
      ))}
    </div>
  );
}
