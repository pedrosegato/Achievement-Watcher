import { Segmented } from "../../../components/Segmented";
import { VIDEO_FPS, VIDEO_QUALITIES } from "../../../constants";
import { QUALITY_HINTS, QUALITY_LABELS } from "../../../i18n/labels";
import { useI18n } from "../../../i18n/useI18n";
import type { VideoQuality } from "../../../types/domain";
import type { SectionProps } from "../../../types/ui";
import { ClipTimeline } from "../ClipTimeline";
import { PathField } from "../controls";
import { FfmpegRow } from "../FfmpegRow";
import { SettingBlock, SettingRow, SwitchRow } from "../SettingRow";

const isQuality = (value: string): value is VideoQuality =>
  (VIDEO_QUALITIES as readonly string[]).includes(value);

export function Captures({ settings, update }: SectionProps) {
  const { t } = useI18n();
  const fpsOptions = VIDEO_FPS.map((fps) => ({
    value: String(fps),
    label: t("settings.fps", { n: fps }),
  }));
  const qualityOptions = VIDEO_QUALITIES.map((quality) => ({
    value: quality,
    label: t(QUALITY_LABELS[quality]),
  }));
  const saveWindow = (before: number, after: number) => {
    if (before !== settings.videoBefore) void update("videoBefore", before);
    if (after !== settings.videoAfter) void update("videoAfter", after);
  };

  return (
    <>
      <SwitchRow
        label={t("settings.screenshot")}
        checked={settings.screenshot}
        onChange={(value) => void update("screenshot", value)}
      />
      <SettingRow label={t("settings.screenshotDir")} hint={t("settings.perGame")}>
        <PathField
          label={t("settings.screenshotDir")}
          value={settings.screenshotDir}
          fallback={t("settings.defaultShots")}
          onCommit={(value) => void update("screenshotDir", value)}
        />
      </SettingRow>
      <FfmpegRow settings={settings} update={update} />
      {settings.video && (
        <>
          <SettingBlock label={t("settings.clipLength")}>
            <ClipTimeline
              before={settings.videoBefore}
              after={settings.videoAfter}
              fps={settings.videoFps}
              quality={settings.videoQuality}
              onCommit={saveWindow}
            />
          </SettingBlock>
          <SettingRow label={t("settings.videoFps")}>
            <Segmented
              label={t("settings.videoFps")}
              options={fpsOptions}
              value={String(settings.videoFps)}
              onChange={(value) => void update("videoFps", Number(value))}
            />
          </SettingRow>
          <SettingRow
            label={t("settings.videoQuality")}
            hint={t(QUALITY_HINTS[settings.videoQuality])}
          >
            <Segmented
              label={t("settings.videoQuality")}
              options={qualityOptions}
              value={settings.videoQuality}
              onChange={(value) => isQuality(value) && void update("videoQuality", value)}
            />
          </SettingRow>
        </>
      )}
      <SettingRow label={t("settings.videoDir")} hint={t("settings.perGame")}>
        <PathField
          label={t("settings.videoDir")}
          value={settings.videoDir}
          fallback={t("settings.defaultVideos")}
          onCommit={(value) => void update("videoDir", value)}
        />
      </SettingRow>
    </>
  );
}
