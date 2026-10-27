import { ErrorAlert } from "../../../../components/ErrorAlert";
import { Icon } from "../../../../components/Icon";
import { Button } from "../../../../components/ui/button";
import { useCloudAction } from "../../../../hooks/useCloudAction";
import { useI18n } from "../../../../i18n/useI18n";
import { cloudFolderFor, elidedPath, gamesSavedKey } from "../../../../lib/cloud";
import { api } from "../../../../lib/commands";
import { pickFolder } from "../../../../lib/dialog";
import type { CloudOverview } from "../../../../types/domain";
import { SettingBlock } from "../../SettingBlock";

const PATH_LINK =
  "min-w-0 cursor-pointer truncate whitespace-nowrap text-left font-semibold text-[14px] underline-offset-[3px] hover:underline";

export function FolderField({ overview }: { overview: CloudOverview }) {
  const { t } = useI18n();
  const { error, run } = useCloudAction();
  const setFolder = (path: string | null) => void run(() => api.setCloudFolder(path));
  const browse = () =>
    void run(async () => {
      const picked = await pickFolder(t("cloud.folder"), overview.folder ?? undefined);
      if (picked) await api.setCloudFolder(cloudFolderFor(picked));
    });
  const folder = overview.folder;
  const count = overview.games.length;

  return (
    <SettingBlock label={t("cloud.folder")} hint={t("cloud.folderHint")}>
      <ErrorAlert message={error} />
      {folder ? (
        <>
          <div className="flex items-center gap-3 rounded-card border border-line bg-surface px-3.5 py-3">
            <span className="grid size-9 flex-none place-items-center rounded-card bg-surface-2">
              <Icon name="cloud" size={18} />
            </span>
            <div className="flex min-w-0 flex-1 flex-col gap-0.5">
              <button
                type="button"
                title={`${t("cloud.openFolder")}: ${folder}`}
                onClick={() => void run(() => api.openPath(folder))}
                className={PATH_LINK}
              >
                {elidedPath(folder)}
              </button>
              <span className="text-[12.5px] text-dim">{t(gamesSavedKey(count), { count })}</span>
            </div>
            <Button size="sm" onClick={browse}>
              {t("cloud.change")}
            </Button>
          </div>
          <Button
            size="sm"
            variant="ghost"
            className="w-max text-dim hover:text-text"
            onClick={() => setFolder(null)}
          >
            {t("cloud.disconnect")}
          </Button>
        </>
      ) : (
        <>
          <Button size="sm" variant="primary" className="w-max" onClick={browse}>
            <Icon name="folder-open" size={16} />
            {t("cloud.choose")}
          </Button>
          {overview.suggestions.length > 0 && (
            <div className="flex flex-col gap-1.5">
              <span className="text-[12.5px] text-dim">{t("cloud.suggestions")}</span>
              {overview.suggestions.map((path) => (
                <Button
                  key={path}
                  size="sm"
                  variant="link"
                  className="w-max"
                  onClick={() => setFolder(path)}
                >
                  <Icon name="folder" size={16} />
                  {path}
                </Button>
              ))}
            </div>
          )}
        </>
      )}
    </SettingBlock>
  );
}
