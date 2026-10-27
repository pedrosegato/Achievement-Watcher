import { describe, expect, it } from "vitest";
import type { CloudGameView } from "../types/domain";
import {
  cloudAttention,
  cloudErrorKey,
  cloudFactKey,
  cloudFolderFor,
  elidedPath,
  gamesSavedKey,
  lastActionKey,
  panelTitleKey,
  playtimeLabel,
  savePathAction,
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

describe("lastActionKey", () => {
  it("has copy for every reason", () => {
    expect(lastActionKey("restore")).toBe("cloud.card.lastAction.restore");
    expect(lastActionKey("undo")).toBe("cloud.card.lastAction.undo");
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

  it("reports where the last snapshot is", () => {
    expect(cloudFactKey(VIEW, true)).toBe("cloud.fact.inFolder");
    expect(cloudFactKey({ ...VIEW, state: "inCloud" }, true)).toBe("cloud.fact.inCloud");
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
    expect(panelTitleKey(VIEW)).toBe("cloud.panel.title.inFolder");
    expect(panelTitleKey({ ...VIEW, state: "inCloud" })).toBe("cloud.panel.title.inCloud");
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
