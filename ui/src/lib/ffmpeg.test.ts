import { describe, expect, it } from "vitest";
import type { FfmpegStatus } from "../types/domain";
import { ffmpegNote } from "./ffmpeg";

function status(partial: Partial<FfmpegStatus>): FfmpegStatus {
  return {
    available: true,
    path: "ffmpeg.exe",
    downloading: false,
    error: null,
    downloaded: 0,
    total: null,
    ...partial,
  };
}

describe("ffmpegNote", () => {
  it("says nothing while the status is unknown or ffmpeg is ready", () => {
    expect(ffmpegNote(null)).toBeNull();
    expect(ffmpegNote(status({}))).toBeNull();
  });

  it("warns that the first use downloads ffmpeg", () => {
    expect(ffmpegNote(status({ available: false, path: null }))).toEqual({
      key: "settings.videoFirst",
    });
  });

  it("reports download progress in whole megabytes", () => {
    expect(
      ffmpegNote(
        status({ available: false, downloading: true, downloaded: 41_600_000, total: 83_200_000 }),
      ),
    ).toEqual({ key: "settings.ffmpegProgress", vars: { got: 42, total: 83, pct: 50 } });
  });

  it("reports a download that has not sized itself yet", () => {
    expect(ffmpegNote(status({ available: false, downloading: true }))).toEqual({
      key: "settings.ffmpegStarting",
    });
  });

  it("shows the download error", () => {
    expect(ffmpegNote(status({ available: false, error: "timed out" }))).toEqual({
      key: "settings.ffmpegError",
      vars: { error: "timed out" },
    });
  });
});
