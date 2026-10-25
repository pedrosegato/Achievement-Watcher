import { useState } from "react";
import { ArtImage } from "../../components/ArtImage";
import { Icon } from "../../components/Icon";
import { useI18n } from "../../i18n/useI18n";
import type { GameDetail } from "../../types/domain";
import { heroFallback, logoBox, logoObjectPosition } from "./logo";

interface GameHeroProps {
  detail: GameDetail;
  onBack: () => void;
}

export function GameHero({ detail, onBack }: GameHeroProps) {
  const { t } = useI18n();
  const [missing, setMissing] = useState<string[]>([]);
  const lose = (url: string) => () => setMissing((list) => [...list, url]);
  const mode = heroFallback(!missing.includes(detail.heroUrl), !missing.includes(detail.headerUrl));

  return (
    <header className="game-hero relative overflow-hidden bg-surface-2">
      {mode === "hero" && <ArtImage src={detail.heroUrl} onMissing={lose(detail.heroUrl)} />}
      {mode === "header-blur" && (
        <ArtImage
          src={detail.headerUrl}
          onMissing={lose(detail.headerUrl)}
          className="scale-110 opacity-70 blur-2xl"
        />
      )}
      <div className="game-hero-shade pointer-events-none absolute inset-0" />
      <button
        type="button"
        onClick={onBack}
        className="absolute top-4 left-5 z-1 flex cursor-pointer items-center gap-1 text-[13px] text-white/85 hover:text-white"
      >
        <Icon name="back" size={16} />
        {t("nav.games")}
      </button>
      <div className="hero-logo" style={logoBox(detail.logoPosition)}>
        <ArtImage
          src={detail.logoUrl}
          alt={detail.name}
          position={logoObjectPosition(detail.logoPosition)}
          className="object-contain"
          fallback={
            <span className="self-end font-bold text-[28px] text-white leading-tight">
              {detail.name}
            </span>
          }
        />
      </div>
    </header>
  );
}
