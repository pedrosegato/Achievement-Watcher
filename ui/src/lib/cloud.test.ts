import { describe, expect, it } from "vitest";
import type { CloudGameView } from "../types/domain";
import {
  cloudAttention,
  cloudErrorKey,
  cloudFactKey,
  cloudFolderFor,
  cloudMood,
  elidedPath,
  gamesSavedKey,
  panelTitleKey,
  playtimeLabel,
  savePathAction,
  statusFact,
  statusSummary,
} from "./cloud";

describe("cloudErrorKey", () => {
  it("maps known worker codes to copy", () => {
    expect(cloudErrorKey("missingRoot")).toBe("cloud.error.missingRoot");
    expect(cloudErrorKey("pathNotPortable")).toBe("cloud.error.pathNotPortable");
  });

  it("leaves unknown errors alone", () => {
    expect(cloudErrorKey("disk full")).toBeNull();
    expect(cloudErrorKey("toString")).toBeNull();
  });
});

const VIEW: CloudGameView = {
  key: "1245620",
  enabled: true,
  default: "on",
  located: "found",
  paths: [],
  oversize: false,
  state: "inFolder",
  lastSnapshotAt: 100,
  openElsewhere: null,
  lastAction: null,
  snapshots: [],
  status: { type: "idle" },
};

describe("cloudFactKey", () => {
  it("asks to set up when there is no cloud folder", () => {
    expect(cloudFactKey(VIEW, false)).toBe("cloud.fact.setup");
  });

  it("says off when sync is disabled", () => {
    expect(cloudFactKey({ ...VIEW, enabled: false }, true)).toBe("cloud.fact.off");
  });

  it("puts another PC first", () => {
    expect(cloudFactKey({ ...VIEW, openElsewhere: "Notebook" }, true)).toBe(
      "cloud.card.openElsewhere",
    );
  });

  it("says synced once a version exists, wherever it is", () => {
    expect(cloudFactKey(VIEW, true)).toBe("cloud.status.synced");
    expect(cloudFactKey({ ...VIEW, state: "inCloud" }, true)).toBe("cloud.status.synced");
    expect(cloudFactKey({ ...VIEW, state: null, lastSnapshotAt: null }, true)).toBe(
      "cloud.fact.notSynced",
    );
  });

  it("explains a save it cannot find", () => {
    expect(cloudFactKey({ ...VIEW, located: "notFound", state: null }, true)).toBe(
      "cloud.fact.notFound",
    );
  });
});

describe("savePathAction", () => {
  it("only offers a quiet change when the save was found", () => {
    expect(savePathAction("found")).toBe("change");
  });

  it("asks to choose a folder when nothing was found", () => {
    expect(savePathAction("notFound")).toBe("choose");
  });

  it("offers nothing while loading or when folders cannot help", () => {
    expect(savePathAction("waitingManifest")).toBeNull();
    expect(savePathAction("registry")).toBeNull();
    expect(savePathAction("unsupported")).toBeNull();
  });
});

describe("cloudFolderFor", () => {
  it("keeps the app inside its own folder", () => {
    expect(cloudFolderFor(String.raw`C:\Users\me\OneDrive`)).toBe(
      String.raw`C:\Users\me\OneDrive\AchievementWatcher`,
    );
    expect(cloudFolderFor(String.raw`G:\My Drive\ `.trim())).toBe(
      String.raw`G:\My Drive\AchievementWatcher`,
    );
  });

  it("accepts a folder that already is the app's", () => {
    expect(cloudFolderFor(String.raw`D:\Dropbox\achievementwatcher`)).toBe(
      String.raw`D:\Dropbox\achievementwatcher`,
    );
  });
});

describe("elidedPath", () => {
  it("keeps the last two folders behind an ellipsis", () => {
    expect(
      elidedPath(String.raw`C:\Users\Administrator\AppData\Roaming\EldenRing\76561199048130839`),
    ).toBe(String.raw`...\EldenRing\76561199048130839`);
    expect(elidedPath("D:/Games/Save/Slot")).toBe(String.raw`...\Save\Slot`);
  });

  it("shows short paths whole", () => {
    expect(elidedPath(String.raw`D:\Saves`)).toBe(String.raw`D:\Saves`);
    expect(elidedPath("Saves")).toBe("Saves");
  });
});

describe("panelTitleKey", () => {
  it("leads with the state of the save", () => {
    expect(panelTitleKey(VIEW)).toBe("cloud.status.synced");
    expect(panelTitleKey({ ...VIEW, state: null, lastSnapshotAt: null })).toBe(
      "cloud.fact.notSynced",
    );
    expect(panelTitleKey({ ...VIEW, enabled: false })).toBe("cloud.panel.title.off");
    expect(panelTitleKey({ ...VIEW, openElsewhere: "Notebook" })).toBe("cloud.card.openElsewhere");
    expect(panelTitleKey({ ...VIEW, located: "notFound", state: null })).toBe(
      "cloud.panel.title.notFound",
    );
  });
});

describe("gamesSavedKey", () => {
  it("handles none, one and many", () => {
    expect(gamesSavedKey(0)).toBe("cloud.noGames");
    expect(gamesSavedKey(1)).toBe("cloud.gamesSavedOne");
    expect(gamesSavedKey(3)).toBe("cloud.gamesSaved");
  });
});

describe("cloudAttention", () => {
  const action = (reason: "branch" | "restore" | "push", at: number) => ({
    ...VIEW,
    lastAction: { reason, at, deviceName: "Notebook", canUndo: true },
  });

  it("flags a split or a restore the user has not seen", () => {
    expect(cloudAttention(action("branch", 200), 100)).toBe("branched");
    expect(cloudAttention(action("restore", 200), null)).toBe("restored");
  });

  it("stays quiet once seen or for ordinary sends", () => {
    expect(cloudAttention(action("branch", 200), 250)).toBeNull();
    expect(cloudAttention(action("push", 200), null)).toBeNull();
    expect(cloudAttention(VIEW, null)).toBeNull();
  });
});

describe("playtimeLabel", () => {
  it("names zero hours instead of hiding them", () => {
    expect(playtimeLabel(0)).toEqual({ key: "cloud.history.noHours" });
    expect(playtimeLabel(10_800)).toEqual({ key: "cloud.history.hours", seconds: 10_800 });
    expect(playtimeLabel(null)).toBeNull();
  });
});

describe("statusFact", () => {
  it("keeps the setup and off messages", () => {
    expect(statusFact(VIEW, false, false)).toEqual({
      value: { key: "cloud.fact.setup", vars: { device: "" } },
      at: null,
      busy: false,
      alert: false,
    });
    expect(statusFact({ ...VIEW, enabled: false }, true, true).value).toEqual({
      key: "cloud.fact.off",
      vars: { device: "" },
    });
  });

  it("shows a sync in progress, live or from the backend", () => {
    expect(statusFact(VIEW, true, true)).toEqual({
      value: { key: "cloud.status.syncing" },
      at: null,
      busy: true,
      alert: false,
    });
    expect(statusFact({ ...VIEW, status: { type: "syncing" } }, true, false).busy).toBe(true);
  });

  it("flags a failed sync", () => {
    expect(statusFact({ ...VIEW, status: { type: "failed", code: "x" } }, true, false)).toEqual({
      value: { key: "cloud.status.failed" },
      at: null,
      busy: false,
      alert: true,
    });
  });

  it("says synced with the time, whoever sent it and wherever it sits", () => {
    const synced = { value: { key: "cloud.status.synced" }, busy: false, alert: false };
    expect(
      statusFact({ ...VIEW, state: "inCloud", status: { type: "sent", at: 50 } }, true, false),
    ).toEqual({ ...synced, at: 50 });
    expect(
      statusFact({ ...VIEW, state: "inFolder", status: { type: "sent", at: 50 } }, true, false),
    ).toEqual({ ...synced, at: 50 });
    expect(
      statusFact(
        { ...VIEW, status: { type: "received", at: 60, device: "Notebook" } },
        true,
        false,
      ),
    ).toEqual({ ...synced, at: 60 });
  });

  it("keeps waiting, playing and outdated to one word", () => {
    expect(statusFact({ ...VIEW, status: { type: "waiting" } }, true, false).value).toEqual({
      key: "cloud.status.waiting",
    });
    expect(statusFact({ ...VIEW, status: { type: "playing" } }, true, false).value).toEqual({
      key: "cloud.status.playing",
    });
    expect(
      statusFact({ ...VIEW, status: { type: "behind", device: "Notebook" } }, true, false).value,
    ).toEqual({ key: "cloud.status.behind" });
  });

  it("shows where the game is open before older news", () => {
    expect(
      statusFact(
        { ...VIEW, openElsewhere: "Notebook", status: { type: "sent", at: 50 } },
        true,
        false,
      ).value,
    ).toEqual({ key: "cloud.card.openElsewhere", vars: { device: "Notebook" } });
  });
});

describe("statusSummary", () => {
  it("says the same as the strip", () => {
    expect(statusSummary({ ...VIEW, state: "inFolder", status: { type: "sent", at: 50 } })).toEqual(
      { title: { key: "cloud.status.synced" }, detail: null, at: 50, retry: false },
    );
    expect(
      statusSummary({ ...VIEW, status: { type: "received", at: 60, device: "Notebook" } }),
    ).toEqual({ title: { key: "cloud.status.synced" }, detail: null, at: 60, retry: false });
    expect(statusSummary({ ...VIEW, status: { type: "waiting" } })).toEqual({
      title: { key: "cloud.status.waiting" },
      detail: null,
      at: null,
      retry: false,
    });
    expect(
      statusSummary({ ...VIEW, status: { type: "behind", device: "Notebook" } })?.title,
    ).toEqual({ key: "cloud.status.behind" });
    expect(statusSummary({ ...VIEW, status: { type: "syncing" } })?.title).toEqual({
      key: "cloud.status.syncing",
    });
  });

  it("leaves idle games to the usual title", () => {
    expect(statusSummary(VIEW)).toBeNull();
  });

  it("names the reason of a failed sync and offers a retry", () => {
    expect(statusSummary({ ...VIEW, status: { type: "failed", code: "missingRoot" } })).toEqual({
      title: { key: "cloud.status.failed" },
      detail: { key: "cloud.error.missingRoot" },
      at: null,
      retry: true,
    });
    expect(
      statusSummary({ ...VIEW, status: { type: "failed", code: "os error 5" } })?.detail,
    ).toEqual({ key: "cloud.status.failedDetail" });
  });
});

describe("cloudMood", () => {
  it("stays plain without a folder and crossed out when off", () => {
    expect(cloudMood(VIEW, false, false)).toBe("idle");
    expect(cloudMood({ ...VIEW, enabled: false }, true, false)).toBe("off");
  });

  it("follows the live sync first", () => {
    expect(cloudMood(VIEW, true, true)).toBe("syncing");
    expect(cloudMood({ ...VIEW, status: { type: "syncing" } }, true, false)).toBe("syncing");
    expect(cloudMood({ ...VIEW, status: { type: "failed", code: "x" } }, true, true)).toBe(
      "syncing",
    );
  });

  it("maps each settled status to its mood", () => {
    const mood = (status: CloudGameView["status"]) => cloudMood({ ...VIEW, status }, true, false);
    expect(mood({ type: "failed", code: "x" })).toBe("failed");
    expect(mood({ type: "sent", at: 1 })).toBe("synced");
    expect(mood({ type: "received", at: 1, device: "Notebook" })).toBe("synced");
    expect(mood({ type: "behind", device: "Notebook" })).toBe("pulling");
    expect(mood({ type: "waiting" })).toBe("pulling");
    expect(mood({ type: "playing" })).toBe("playing");
  });

  it("dozes while the game is open elsewhere and rests when idle", () => {
    expect(cloudMood({ ...VIEW, openElsewhere: "Notebook" }, true, false)).toBe("playing");
    expect(cloudMood(VIEW, true, false)).toBe("synced");
    expect(cloudMood({ ...VIEW, state: null, lastSnapshotAt: null }, true, false)).toBe("idle");
  });
});
