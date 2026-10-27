import { Icon } from "../../../components/Icon";
import { Button } from "../../../components/ui/button";
import { useI18n } from "../../../i18n/useI18n";
import { elidedPath, savePathAction } from "../../../lib/cloud";
import { api } from "../../../lib/commands";
import { pickFolder } from "../../../lib/dialog";
import type { CloudPanelProps } from "./types";

const LOCATED_KEYS = {
  notFound: "cloud.card.notFound",
  registry: "cloud.card.registry",
  unsupported: "cloud.card.unsupported",
  waitingManifest: "cloud.card.waitingManifest",
} as const;

export function SaveFolder({ appId, view, busy, run }: CloudPanelProps) {
  const { t } = useI18n();
  const action = savePathAction(view.located);
  const path = view.paths[0] ?? null;
  const browse = () =>
    void run(async () => {
      const picked = await pickFolder(t("cloud.panel.saveFolder"), path ?? undefined);
      if (picked) await api.setGameSavePath(appId, picked);
    });

  return (
    <div className="flex flex-col gap-1 border-line border-t px-6 py-3.5">
      {action === "change" && path ? (
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="text-[12px] text-muted">{t("cloud.panel.saveFolder")}</span>
            <button
              type="button"
              title={`${t("cloud.panel.openFolder")}: ${path}`}
              onClick={() => void run(() => api.openPath(path))}
              className="min-w-0 cursor-pointer truncate whitespace-nowrap text-left font-medium text-[13px] underline-offset-[3px] hover:underline"
            >
              {elidedPath(path)}
            </button>
          </div>
          <Button size="sm" disabled={busy} onClick={browse}>
            {t("cloud.change")}
          </Button>
        </div>
      ) : (
        <>
          <span className="text-[12px] text-muted">{t("cloud.panel.saveFolder")}</span>
          {view.located !== "found" && (
            <p className="text-[13px] text-muted">{t(LOCATED_KEYS[view.located])}</p>
          )}
          {action === "choose" && (
            <Button size="sm" variant="primary" className="w-max" disabled={busy} onClick={browse}>
              <Icon name="folder-open" size={16} />
              {t("cloud.card.chooseFolder")}
            </Button>
          )}
        </>
      )}
      {view.oversize && <p className="text-[12.5px] text-muted">{t("cloud.card.oversize")}</p>}
    </div>
  );
}
