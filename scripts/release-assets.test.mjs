import assert from "node:assert/strict";
import { test } from "node:test";
import { downloadUrl, latestJson, uploadName } from "./release-assets.mjs";

test("uploadName matches the name GitHub gives an asset with spaces", () => {
  assert.equal(
    uploadName("Achievement Watcher_3.0.1_x64-setup.exe"),
    "Achievement.Watcher_3.0.1_x64-setup.exe",
  );
});

test("downloadUrl points at the asset of the tagged release", () => {
  assert.equal(
    downloadUrl("owner/repo", "3.0.1", "Achievement.Watcher_3.0.1_x64-setup.exe"),
    "https://github.com/owner/repo/releases/download/3.0.1/Achievement.Watcher_3.0.1_x64-setup.exe",
  );
});

test("latestJson describes the installer for both Windows updater keys", () => {
  const latest = latestJson({
    version: "3.0.1",
    notes: "Fixes",
    signature: "c2lnbmF0dXJl\n",
    url: "https://example.com/setup.exe",
    date: new Date("2026-10-26T01:02:03.456Z"),
  });
  const entry = { signature: "c2lnbmF0dXJl", url: "https://example.com/setup.exe" };
  assert.deepEqual(latest, {
    version: "3.0.1",
    notes: "Fixes",
    pub_date: "2026-10-26T01:02:03Z",
    platforms: { "windows-x86_64": entry, "windows-x86_64-nsis": entry },
  });
});
