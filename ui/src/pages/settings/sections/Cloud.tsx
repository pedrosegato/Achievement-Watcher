import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { ErrorAlert } from "../../../components/ErrorAlert";
import { Icon } from "../../../components/Icon";
import { Button } from "../../../components/ui/button";
import { EVENTS } from "../../../constants";
import { useCloudAction } from "../../../hooks/useCloudAction";
import { useTauriEvent } from "../../../hooks/useTauriEvent";
import { useI18n } from "../../../i18n/useI18n";
import { cloudErrorKey, cloudFolderFor, elidedPath, gamesSavedKey } from "../../../lib/cloud";
import { api } from "../../../lib/commands";
import { pickFolder } from "../../../lib/dialog";
import { formatBytes } from "../../../lib/format";
import { queries } from "../../../lib/queries";
import type { CloudOverview } from "../../../types/domain";
import { SettingBlock, SettingRow } from "../SettingRow";

const INPUT =
  "w-[220px] rounded-card border border-line bg-surface px-2.5 py-1.5 text-[13px] outline-none focus-visible:border-text";

const PATH_LINK =
  "min-w-0 cursor-pointer truncate whitespace-nowrap text-left font-semibold text-[14px] underline-offset-[3px] outline-none hover:underline focus-visible:underline";

export function Cloud() {
  const { t } = useI18n();
  const { data: overview } = useQuery(queries.cloudOverview());
  const { refresh } = useCloudAction();
  useTauriEvent(EVENTS.cloudChanged, () => void refresh());

  if (!overview) return null;
  const workerError = overview.lastError ? cloudErrorKey(overview.lastError) : null;

  return (
    <>
      <ErrorAlert message={workerError ? t(workerError) : null} />
      <FolderField overview={overview} />
      <DeviceNameRow current={overview.deviceName} />
      {overview.folder && !overview.folderMissing && <SpaceBlock overview={overview} />}
    </>
  );
}

interface OverviewProps {
  overview: CloudOverview;
}

function FolderField({ overview }: OverviewProps) {
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
                onClick={() => void api.openPath(folder)}
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

function DeviceNameRow({ current }: { current: string }) {
  const { t } = useI18n();
  const { error, run } = useCloudAction();
  const [name, setName] = useState<string | null>(null);
  const changed = name !== null && name !== current;

  return (
    <SettingRow label={t("cloud.deviceName")} hint={error ?? t("cloud.deviceNameHint")}>
      {changed && (
        <Button
          size="sm"
          variant="primary"
          onClick={() =>
            void run(async () => {
              await api.setDeviceName(name);
              setName(null);
            })
          }
        >
          {t("cloud.save")}
        </Button>
      )}
      <input
        aria-label={t("cloud.deviceName")}
        value={name ?? current}
        maxLength={40}
        onChange={(event) => setName(event.target.value)}
        className={INPUT}
      />
    </SettingRow>
  );
}

function SpaceBlock({ overview }: OverviewProps) {
  const { t, locale } = useI18n();
  const folder = overview.folder ?? "";

  return (
    <div className="flex flex-col gap-3 py-3.5">
      <div className="flex items-baseline justify-between gap-4">
        <span className="font-semibold text-[14px]">{t("cloud.space")}</span>
        <span className="font-semibold text-[14px]">
          {formatBytes(overview.totalBytes, locale)}
        </span>
      </div>
      {overview.games.length > 0 ? (
        <ul className="flex flex-col rounded-card border border-line bg-surface">
          {overview.games.map((game) => (
            <li
              key={game.key}
              className="flex items-center justify-between gap-4 border-line border-b px-3.5 py-2.5 text-[13px] last:border-b-0"
            >
              <span className="truncate">{game.name}</span>
              <span className="flex-none text-dim">{formatBytes(game.bytes, locale)}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="rounded-card border border-line border-dashed p-3 text-[13px] text-dim">
          {t("cloud.noGames")}
        </p>
      )}
      <div className="flex items-center justify-between gap-4">
        <p className="text-[12.5px] text-dim">{t("cloud.cleanup")}</p>
        <Button size="sm" className="flex-none" onClick={() => void api.openPath(folder)}>
          <Icon name="folder-open" size={16} />
          {t("cloud.openFolder")}
        </Button>
      </div>
    </div>
  );
}
