import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { constants, homedir, setPriority } from "node:os";
import { join } from "node:path";
import { downloadUrl, latestJson, uploadName } from "./release-assets.mjs";

const ROOT = join(import.meta.dirname, "..");
const REPO = "pedrosegato/Achievement-Watcher";
const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const notesAt = args.indexOf("--notes");
const notesFile = notesAt >= 0 ? args[notesAt + 1] : null;

const fail = (message) => {
  console.error(`release: ${message}`);
  process.exit(1);
};

const run = (command, commandArgs, options = {}) => {
  const result = spawnSync(command, commandArgs, { cwd: ROOT, stdio: "inherit", ...options });
  if (result.status !== 0) fail(`${command} ${commandArgs[0]} failed`);
};

const read = (command, commandArgs) =>
  (spawnSync(command, commandArgs, { cwd: ROOT, encoding: "utf8" }).stdout ?? "").trim();

const conf = JSON.parse(readFileSync(join(ROOT, "crates", "aw-app", "tauri.conf.json"), "utf8"));
const { version, productName } = conf;

if (read("git", ["status", "--porcelain"])) fail("commit or stash your changes first");
if (!dryRun) {
  run("git", ["fetch", "origin", "main", "--tags"]);
  const head = read("git", ["rev-parse", "HEAD"]);
  if (head !== read("git", ["rev-parse", "origin/main"]))
    fail("check out the commit on origin/main");
  if (read("git", ["ls-remote", "--tags", "origin", version]))
    fail(`tag ${version} already exists`);
}

setPriority(0, constants.priority.PRIORITY_BELOW_NORMAL);
const env = { ...process.env };
env.TAURI_SIGNING_PRIVATE_KEY ??= join(homedir(), ".tauri", "achievement-watcher.key");
env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= "";
const cli = join(ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js");
const config = JSON.stringify({ bundle: { createUpdaterArtifacts: true } });
run(process.execPath, [cli, "build", "--config", config], { env });

const installer = `${productName}_${version}_x64-setup.exe`;
const bundle = join(ROOT, "target", "release", "bundle", "nsis");
const upload = join(ROOT, "target", "release", "upload");
const name = uploadName(installer);
rmSync(upload, { recursive: true, force: true });
mkdirSync(upload, { recursive: true });
copyFileSync(join(bundle, installer), join(upload, name));
copyFileSync(join(bundle, `${installer}.sig`), join(upload, `${name}.sig`));
const latest = latestJson({
  version,
  notes: `${productName} ${version}`,
  signature: readFileSync(join(upload, `${name}.sig`), "utf8"),
  url: downloadUrl(REPO, version, name),
  date: new Date(),
});
writeFileSync(join(upload, "latest.json"), `${JSON.stringify(latest, null, 2)}\n`);

if (dryRun) {
  console.log(`release: dry run, assets in ${upload}`);
  process.exit(0);
}

const notes = notesFile ? ["--notes-file", notesFile] : ["--generate-notes"];
run("gh", [
  "release",
  "create",
  version,
  "--repo",
  REPO,
  "--target",
  read("git", ["rev-parse", "HEAD"]),
  "--title",
  `${productName} ${version}`,
  ...notes,
  "--latest",
  join(upload, name),
  join(upload, `${name}.sig`),
  join(upload, "latest.json"),
]);
console.log(`release: ${productName} ${version} published`);
