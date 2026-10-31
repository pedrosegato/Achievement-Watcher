import { describe, expect, it } from "vitest";
import type { UpdateStatus } from "../types/domain";
import { updatePercent, updateView } from "./update";

function status(partial: Partial<UpdateStatus>): UpdateStatus {
  return {
    current: "3.1.0",
    available: null,
    notes: null,
    phase: "idle",
    downloaded: 0,
    total: null,
    error: null,
    checkedAt: null,
    ...partial,
  };
}

describe("updateView", () => {
  it("offers a check before anything was checked", () => {
    expect(updateView(status({}))).toEqual({
      busy: false,
      action: { key: "settings.checkUpdate" },
      note: { key: "settings.version", vars: { version: "3.1.0" } },
      percent: null,
    });
  });

  it("shows that a check is running", () => {
    const view = updateView(status({ phase: "checking" }));
    expect(view.busy).toBe(true);
    expect(view.action.key).toBe("settings.updateChecking");
  });

  it("says the app is up to date after a check found nothing", () => {
    expect(updateView(status({ checkedAt: 1 })).note?.key).toBe("settings.versionCurrent");
  });

  it("offers the found version", () => {
    const view = updateView(status({ available: "3.2.0", checkedAt: 1 }));
    expect(view.action).toEqual({ key: "settings.installUpdate", vars: { version: "3.2.0" } });
    expect(view.note).toEqual({
      key: "settings.updateAvailable",
      vars: { version: "3.1.0", next: "3.2.0" },
    });
  });

  it("puts a failed check in the note", () => {
    const view = updateView(status({ error: "offline", checkedAt: 1 }));
    expect(view.busy).toBe(false);
    expect(view.note).toEqual({ key: "settings.updateError", vars: { error: "offline" } });
  });

  it("reports download progress in whole percent", () => {
    const view = updateView(
      status({ phase: "downloading", available: "3.2.0", downloaded: 333, total: 1000 }),
    );
    expect(view.action).toEqual({ key: "settings.updateDownloading", vars: { pct: 33 } });
    expect(view.percent).toBe(33);
  });

  it("starts the download without a size", () => {
    const view = updateView(status({ phase: "downloading", available: "3.2.0" }));
    expect(view.action.key).toBe("settings.updateDownloadingStart");
    expect(view.percent).toBeNull();
  });

  it("warns that installing restarts the app", () => {
    const view = updateView(status({ phase: "installing" }));
    expect(view.note?.key).toBe("settings.updateRestarting");
    expect(view.percent).toBe(100);
  });
});

describe("updatePercent", () => {
  it("never goes past 100", () => {
    expect(updatePercent(status({ phase: "downloading", downloaded: 1200, total: 1000 }))).toBe(
      100,
    );
  });
});
