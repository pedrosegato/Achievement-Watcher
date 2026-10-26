import { Icon } from "../../components/Icon";
import { Button } from "../../components/ui/button";
import { useI18n } from "../../i18n/useI18n";

interface LibraryEmptyProps {
  onRescan: () => void;
  scanning: number | null;
}

export function LibraryEmpty({ onRescan, scanning }: LibraryEmptyProps) {
  const { t } = useI18n();
  const busy = scanning !== null;
  return (
    <div className="flex flex-col items-center gap-3 rounded-card border border-line bg-surface px-6 py-14 text-center">
      <p className="font-semibold text-[15px]">{t("library.emptyTitle")}</p>
      <Button variant="primary" onClick={busy ? undefined : onRescan} aria-busy={busy}>
        <Icon name="sync" size={16} className={busy ? "spinning" : undefined} />
        {busy ? t("library.scanning", { pct: scanning }) : t("library.rescan")}
      </Button>
    </div>
  );
}
