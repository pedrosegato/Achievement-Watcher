import { useState } from "react";
import { Collapsible } from "../../../components/Collapsible";
import { Segmented } from "../../../components/Segmented";
import { Button } from "../../../components/ui/button";
import { VIDEO_FORMATS, VIDEO_FPS, VIDEO_QUALITIES } from "../../../constants";
import { FORMAT_HINTS, FORMAT_LABELS, QUALITY_HINTS, QUALITY_LABELS } from "../../../i18n/labels";
import { useI18n } from "../../../i18n/useI18n";
import { clipSizeMb } from "../../../lib/video";
import type { VideoFormat, VideoQuality } from "../../../types/domain";
import type { SectionProps } from "../../../types/ui";
import { ClipTimeline } from "../ClipTimeline";
import { PathField } from "../controls";
import { SettingGroup } from "../SettingGroup";
import { SettingRow } from "../SettingRow";
import { useFfmpeg } from "../useFfmpeg";

const isQuality = (value: string): value is VideoQuality =>
  (VIDEO_QUALITIES as readonly string[]).includes(value);

const isFormat = (value: string): value is VideoFormat =>
  (VIDEO_FORMATS as readonly string[]).includes(value);

const CHIP = "rounded-card bg-surface-2 px-2 py-0.5 font-semibold text-[12px] text-muted";

export function Captures({ settings, update }: SectionProps) {
  const { t } = useI18n();
  const ffmpeg = useFfmpeg(update);
  const [more, setMore] = useState(false);
  const fpsOptions = VIDEO_FPS.map((fps) => ({
    value: String(fps),
    label: t("settings.fps", { n: fps }),
  }));
  const qualityOptions = VIDEO_QUALITIES.map((quality) => ({
    value: quality,
    label: t(QUALITY_LABELS[quality]),
  }));
  const formatOptions = VIDEO_FORMATS.map((format) => ({
    value: format,
    label: t(FORMAT_LABELS[format]),
  }));
  const saveWindow = (before: number, after: number) => {
    if (before !== settings.videoBefore) void update("videoBefore", before);
    if (after !== settings.videoAfter) void update("videoAfter", after);
  };
  const size = clipSizeMb(
    settings.videoBefore + settings.videoAfter,
    settings.videoFps,
    settings.videoQuality,
    settings.videoFormat,
  );

  return (
    <>
      <SettingGroup
        icon="image"
        title={t("settings.screenshot")}
        description={t("settings.screenshotDescription")}
        checked={settings.screenshot}
        onCheckedChange={(value) => void update("screenshot", value)}
      >
        <SettingRow label={t("settings.saveTo")} hint={t("settings.perGame")}>
          <PathField
            label={t("settings.screenshotDir")}
            value={settings.screenshotDir}
            fallback={t("settings.defaultShots")}
            onCommit={(value) => void update("screenshotDir", value)}
          />
        </SettingRow>
      </SettingGroup>
      <SettingGroup
        icon="video"
        title={t("settings.video")}
        description={t("settings.videoDescription")}
        note={ffmpeg.note && t(ffmpeg.note.key, ffmpeg.note.vars)}
        action={
          ffmpeg.downloading && (
            <Button size="sm" onClick={() => ffmpeg.setVideo(false)}>
              {t("settings.cancel")}
            </Button>
          )
        }
        checked={settings.video}
        onCheckedChange={ffmpeg.setVideo}
        footer={t("settings.clipSize", { mb: size })}
      >
        <ClipTimeline
          before={settings.videoBefore}
          after={settings.videoAfter}
          onCommit={saveWindow}
        />
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
        <SettingRow label={t("settings.saveTo")} hint={t("settings.perGame")}>
          <PathField
            label={t("settings.videoDir")}
            value={settings.videoDir}
            fallback={t("settings.defaultVideos")}
            onCommit={(value) => void update("videoDir", value)}
          />
        </SettingRow>
        <Collapsible
          open={more}
          onToggle={() => setMore((open) => !open)}
          header={t("settings.moreOptions")}
          peek={[
            <span key="fps" className={CHIP}>
              {t("settings.fps", { n: settings.videoFps })}
            </span>,
            <span key="format" className={CHIP}>
              {t(FORMAT_LABELS[settings.videoFormat])}
            </span>,
          ]}
        >
          <SettingRow label={t("settings.videoFps")}>
            <Segmented
              label={t("settings.videoFps")}
              options={fpsOptions}
              value={String(settings.videoFps)}
              onChange={(value) => void update("videoFps", Number(value))}
            />
          </SettingRow>
          <SettingRow
            label={t("settings.videoFormat")}
            hint={t(FORMAT_HINTS[settings.videoFormat])}
          >
            <Segmented
              label={t("settings.videoFormat")}
              options={formatOptions}
              value={settings.videoFormat}
              onChange={(value) => isFormat(value) && void update("videoFormat", value)}
            />
          </SettingRow>
        </Collapsible>
      </SettingGroup>
    </>
  );
}
