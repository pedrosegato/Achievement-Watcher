use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use aw_core::cloud::capture::{HashCache, capture};
use aw_core::cloud::device::{DeviceId, LocalDevice};
use aw_core::cloud::heads::HeadReason;
use aw_core::cloud::key::{CloudKey, GameKind};
use aw_core::cloud::layout::CloudLayout;
use aw_core::cloud::locate::{Category, SaveRoot};
use aw_core::cloud::portable::{Placeholders, PortablePath};
use aw_core::cloud::snapshot::{SnapshotMeta, list_snapshots, load_state};
use aw_core::cloud::sync::{CloudContext, GameTarget, SyncEvent, reconcile, set_principal, undo};
use aw_core::discovery::folders::KnownFolders;
use aw_core::store::Store;
use tempfile::TempDir;

const APP: u32 = 374_320;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

struct Pc {
    name: String,
    generation: u32,
    root: TempDir,
    mirror: PathBuf,
    layout: CloudLayout,
    store: Store,
    device: LocalDevice,
    cache: HashCache,
    kind: GameKind,
    account: String,
    skew: i64,
    plays: u32,
}

impl Pc {
    fn target(&self) -> GameTarget {
        let mut roots = vec![root(
            Category::Save,
            "<winAppData>/DarkSoulsIII/<storeUserId>",
        )];
        if self.kind == GameKind::Emulator {
            roots.push(root(
                Category::EmulatorFiles,
                "<winAppData>/GSE Saves/374320",
            ));
        }
        GameTarget {
            key: CloudKey::Steam(APP),
            name: "Dark Souls III".into(),
            app_id: APP,
            kind: self.kind,
            roots,
            placeholders: Placeholders {
                folders: Some(KnownFolders::under(self.root.path())),
                ..Default::default()
            },
        }
    }

    fn saves_dir(&self) -> PathBuf {
        self.root.path().join("RoamingAppData").join("DarkSoulsIII")
    }

    fn ctx(&self, now: i64) -> CloudContext<'_> {
        CloudContext {
            layout: &self.layout,
            device: &self.device,
            store: &self.store,
            cache: &self.cache,
            now: now + self.skew,
        }
    }

    fn digest(&self) -> Option<String> {
        let target = self.target();
        let state = match self.kind {
            GameKind::Emulator => Some(self.store.game_state(APP).unwrap()),
            GameKind::SteamOriginal => None,
        };
        let local = capture(
            &target.roots,
            &target.placeholders,
            state,
            &HashCache::default(),
        )
        .unwrap();
        (!local.files.is_empty()).then_some(local.digest)
    }

    fn save_contents(&self) -> BTreeSet<String> {
        walk(&self.saves_dir())
            .iter()
            .map(|file| std::fs::read_to_string(file).unwrap())
            .collect()
    }

    fn visible_save(&self) -> Option<String> {
        std::fs::read_to_string(self.saves_dir().join(&self.account).join("DS30000.sl2")).ok()
    }

    fn unlocks(&self) -> BTreeSet<String> {
        self.store
            .game_state(APP)
            .unwrap()
            .unlocks
            .into_iter()
            .map(|u| u.api_name)
            .collect()
    }
}

fn root(category: Category, pattern: &str) -> SaveRoot {
    SaveRoot {
        category,
        pattern: PortablePath::new(pattern).unwrap(),
        single_file: false,
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn relative_files(dir: &Path) -> BTreeSet<PathBuf> {
    walk(dir)
        .into_iter()
        .map(|p| p.strip_prefix(dir).unwrap().to_path_buf())
        .collect()
}

fn owner(rel: &Path) -> Option<String> {
    let parent = rel.parent()?.file_name()?.to_str()?;
    if !matches!(parent, "heads" | "locks" | "settings" | "devices") {
        return None;
    }
    Some(rel.file_stem()?.to_str()?.to_owned())
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(from, to).unwrap();
}

fn same(a: &Path, b: &Path) -> bool {
    std::fs::read(a).ok() == std::fs::read(b).ok()
}

struct Sim {
    seed: u64,
    rng: Rng,
    _dir: TempDir,
    server: PathBuf,
    pcs: Vec<Pc>,
    now: i64,
    history: Vec<String>,
    seen: Vec<(String, Fingerprint)>,
    observed: HashMap<String, (Option<String>, Fingerprint)>,
    mirror_files: Vec<BTreeSet<PathBuf>>,
    same_account: bool,
}

impl Sim {
    fn new(seed: u64) -> Sim {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let dir = tempfile::tempdir().unwrap();
        let server = dir.path().join("server");
        std::fs::create_dir_all(&server).unwrap();
        let count = 2 + rng.below(2) as usize;
        let same_account = rng.chance(60);
        let skewed = rng.chance(50);
        let mut pcs = Vec::new();
        for i in 0..count {
            let kind = if rng.chance(50) {
                GameKind::Emulator
            } else {
                GameKind::SteamOriginal
            };
            let account = if same_account {
                "76561198000000001".to_owned()
            } else {
                format!("7656119800000000{i}")
            };
            let skew = if skewed {
                rng.below(4000) as i64 - 2000
            } else {
                0
            };
            pcs.push(new_pc(&dir, i, 0, kind, account, skew));
        }
        let mirror_files = pcs.iter().map(|pc| relative_files(&pc.mirror)).collect();
        let mut sim = Sim {
            seed,
            rng,
            _dir: dir,
            server,
            pcs,
            now: 1_000_000,
            history: Vec::new(),
            seen: Vec::new(),
            observed: HashMap::new(),
            mirror_files,
            same_account,
        };
        let kinds: Vec<String> = sim
            .pcs
            .iter()
            .map(|p| format!("{}:{:?}:{}:skew{}", p.name, p.kind, p.account, p.skew))
            .collect();
        sim.log(format!("pcs {}", kinds.join(", ")));
        sim
    }

    fn log(&mut self, line: String) {
        self.history.push(format!("t={} {line}", self.now));
    }

    fn fail(&self, message: &str) -> ! {
        panic!(
            "seed {}: {message}\n--- history ---\n{}",
            self.seed,
            self.history.join("\n")
        );
    }

    fn record(&mut self, i: usize) {
        let pc = &self.pcs[i];
        if pc.digest().is_some() {
            let print = Fingerprint {
                saves: pc.save_contents(),
                unlocks: (pc.kind == GameKind::Emulator).then(|| pc.unlocks()),
            };
            let label = format!("{} at step {}", pc.name, self.history.len());
            self.seen.push((label, print));
        }
    }

    fn reconcile(&mut self, i: usize) -> SyncEvent {
        self.record(i);
        let pc = &self.pcs[i];
        let now = self.now;
        let result = reconcile(&pc.ctx(now), &pc.target(), now + pc.skew);
        let name = pc.name.clone();
        match result {
            Ok(event) => {
                self.log(format!("{name} reconcile -> {event:?}"));
                event
            }
            Err(error) => {
                self.log(format!("{name} reconcile -> ERROR {error}"));
                self.fail(&format!("reconcile failed on {name}: {error}"))
            }
        }
    }

    fn play(&mut self, i: usize) {
        let pc = &mut self.pcs[i];
        pc.plays += 1;
        let text = format!("{}-g{}-play{:04}", pc.name, pc.generation, pc.plays);
        let path = pc.saves_dir().join(&pc.account).join("DS30000.sl2");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
        if pc.kind == GameKind::Emulator {
            let gse = pc
                .root
                .path()
                .join("RoamingAppData")
                .join("GSE Saves")
                .join("374320")
                .join("achievements.json");
            std::fs::create_dir_all(gse.parent().unwrap()).unwrap();
            std::fs::write(&gse, format!("{{\"{text}\":true}}")).unwrap();
            let ach = format!("ACH_{}_{}", pc.name, pc.plays);
            pc.store
                .record_unlock(APP, &ach, self.now, self.now, "gse")
                .unwrap();
            pc.store.add_playtime(APP, 600, self.now).unwrap();
        }
        let name = pc.name.clone();
        self.log(format!("{name} plays -> {text}"));
        self.reconcile(i);
    }

    fn upload(&mut self, i: usize, percent: u64) {
        let pc = &self.pcs[i];
        let device = pc.device.id.0.clone();
        let mut moved = 0;
        for rel in relative_files(&pc.mirror) {
            let local = pc.mirror.join(&rel);
            let remote = self.server.join(&rel);
            let newer_own =
                owner(&rel).as_deref() == Some(device.as_str()) && !same(&local, &remote);
            if (!remote.exists() || newer_own) && self.rng.chance(percent) {
                copy(&local, &remote);
                moved += 1;
            }
        }
        let name = self.pcs[i].name.clone();
        self.log(format!("{name} uploads {percent}% -> {moved} files"));
    }

    fn download(&mut self, i: usize, percent: u64) {
        let device = self.pcs[i].device.id.0.clone();
        let mirror = self.pcs[i].mirror.clone();
        let mut moved = 0;
        for rel in relative_files(&self.server) {
            let remote = self.server.join(&rel);
            let local = mirror.join(&rel);
            let theirs = owner(&rel).as_deref() != Some(device.as_str());
            let stale = theirs && local.exists() && !same(&local, &remote);
            if (!local.exists() || stale) && self.rng.chance(percent) {
                copy(&remote, &local);
                moved += 1;
            }
        }
        let name = self.pcs[i].name.clone();
        self.log(format!("{name} downloads {percent}% -> {moved} files"));
    }

    fn set_principal(&mut self, i: usize) {
        let pc = &self.pcs[i];
        let snaps = list_snapshots(&pc.layout.game_dirs(&CloudKey::Steam(APP)));
        if snaps.is_empty() {
            return;
        }
        let pick = snaps[self.rng.below(snaps.len() as u64) as usize]
            .1
            .id
            .clone();
        self.record(i);
        let pc = &self.pcs[i];
        let result = set_principal(&pc.ctx(self.now), &pc.target(), &pick, HeadReason::Manual);
        let name = pc.name.clone();
        match result {
            Ok(event) => self.log(format!("{name} picks {pick} -> {event:?}")),
            Err(error) => self.fail(&format!("{name} set_principal {pick}: {error}")),
        }
    }

    fn undo(&mut self, i: usize) {
        self.record(i);
        let pc = &self.pcs[i];
        let result = undo(&pc.ctx(self.now), &pc.target());
        let name = pc.name.clone();
        match result {
            Ok(event) => self.log(format!("{name} undo -> {event:?}")),
            Err(error) => self.fail(&format!("{name} undo: {error}")),
        }
    }

    fn format(&mut self, i: usize) {
        self.upload(i, 100);
        self.record(i);
        let old = &self.pcs[i];
        let (index, generation, kind, account, skew) = (
            old.name[2..].parse::<usize>().unwrap(),
            old.generation + 1,
            old.kind,
            old.account.clone(),
            old.skew,
        );
        let fresh = new_pc(&self._dir, index, generation, kind, account, skew);
        self.pcs[i] = fresh;
        self.mirror_files[i] = BTreeSet::new();
        let name = self.pcs[i].name.clone();
        self.log(format!("{name} formatted (generation {generation})"));
        self.download(i, 100);
    }

    fn corrupt_download(&mut self, i: usize) {
        let mirror = self.pcs[i].mirror.clone();
        let downloaded: Vec<PathBuf> = relative_files(&mirror)
            .into_iter()
            .filter(|rel| rel.components().any(|c| c.as_os_str() == "files"))
            .filter(|rel| {
                let remote = self.server.join(rel);
                remote.exists() && same(&remote, &mirror.join(rel))
            })
            .collect();
        if downloaded.is_empty() {
            return;
        }
        let rel = downloaded[self.rng.below(downloaded.len() as u64) as usize].clone();
        let path = mirror.join(&rel);
        let size = std::fs::metadata(&path).unwrap().len() as usize;
        std::fs::write(&path, "#".repeat(size)).unwrap();
        let name = self.pcs[i].name.clone();
        self.log(format!(
            "{name} gets a corrupted download of {}",
            rel.display()
        ));
    }

    fn observe(&mut self) {
        let roots: Vec<PathBuf> = std::iter::once(self.server.clone())
            .chain(self.pcs.iter().map(|pc| pc.mirror.clone()))
            .collect();
        for root in roots {
            let layout = CloudLayout::new(root);
            for (dir, meta) in list_snapshots(&layout.game_dirs(&CloudKey::Steam(APP))) {
                if self.observed.contains_key(&meta.id) {
                    continue;
                }
                if let Some(print) = try_fingerprint(&dir, &meta) {
                    self.observed
                        .insert(meta.id.clone(), (meta.replaces.clone(), print));
                }
            }
        }
    }

    fn pruned_by_design(&self, id_prefix: &str) -> bool {
        self.observed.iter().any(|(replacer, (replaced, _))| {
            replaced
                .as_deref()
                .is_some_and(|x| x.starts_with(id_prefix))
                && self
                    .observed
                    .values()
                    .any(|(next, _)| next.as_deref() == Some(replacer.as_str()))
        })
    }

    fn check_mirrors_only_grow(&mut self) {
        self.observe();
        for i in 0..self.pcs.len() {
            let now = relative_files(&self.pcs[i].mirror);
            let lost: Vec<_> = self.mirror_files[i]
                .difference(&now)
                .filter(|rel| !rel.components().any(|c| c.as_os_str() == "locks"))
                .filter(|rel| !snapshot_short_id(rel).is_some_and(|id| self.pruned_by_design(&id)))
                .cloned()
                .collect();
            if !lost.is_empty() {
                self.fail(&format!(
                    "{} cloud folder lost files: {lost:?}",
                    self.pcs[i].name
                ));
            }
            self.mirror_files[i] = now;
        }
    }

    fn step(&mut self) {
        let i = self.rng.below(self.pcs.len() as u64) as usize;
        match self.rng.below(100) {
            0..=29 => self.play(i),
            30..=39 => {
                self.reconcile(i);
            }
            40..=54 => {
                let p = if self.rng.chance(50) { 100 } else { 40 };
                self.upload(i, p);
            }
            55..=69 => {
                let p = if self.rng.chance(50) { 100 } else { 40 };
                self.download(i, p);
            }
            70..=77 => self.set_principal(i),
            78..=82 => self.undo(i),
            83..=85 => self.format(i),
            86..=88 => self.corrupt_download(i),
            _ => {
                let jump = if self.rng.chance(10) {
                    7 * 60 * 60
                } else {
                    self.rng.below(1200) as i64
                };
                self.now += jump;
                self.log(format!("time +{jump}s"));
            }
        }
        self.now += 1 + self.rng.below(60) as i64;
        self.check_mirrors_only_grow();
    }

    fn full_sync(&mut self) {
        for i in 0..self.pcs.len() {
            self.upload(i, 100);
        }
        for i in 0..self.pcs.len() {
            self.download(i, 100);
        }
    }

    fn settle(&mut self) {
        self.now += 7 * 60 * 60;
        let mut quiet_rounds = 0;
        for _ in 0..12 {
            self.full_sync();
            let before = relative_files(&self.server).len();
            let mut quiet = true;
            for i in 0..self.pcs.len() {
                self.now += 30;
                let event = self.reconcile(i);
                if !matches!(event, SyncEvent::Nothing | SyncEvent::Adopted { .. }) {
                    quiet = false;
                }
                self.upload(i, 100);
            }
            self.full_sync();
            if quiet && relative_files(&self.server).len() == before {
                quiet_rounds += 1;
                if quiet_rounds == 2 {
                    return;
                }
            } else {
                quiet_rounds = 0;
            }
        }
        self.fail("never settled: reconcile keeps doing work with no new play");
    }

    fn check_converged(&self) {
        if !self.same_account {
            return;
        }
        let saves: Vec<_> = self.pcs.iter().map(Pc::visible_save).collect();
        if let Some(reference) = saves.iter().flatten().next() {
            for (pc, content) in self.pcs.iter().zip(&saves) {
                if content.as_ref().is_some_and(|c| c != reference) {
                    self.fail(&format!(
                        "{} ended with {content:?}, others with {reference:?}",
                        pc.name
                    ));
                }
            }
        }
        let emulators: Vec<_> = self
            .pcs
            .iter()
            .filter(|pc| pc.kind == GameKind::Emulator)
            .collect();
        if let Some(first) = emulators.first() {
            for pc in &emulators {
                if pc.unlocks() != first.unlocks() {
                    self.fail(&format!(
                        "{} unlocks {:?} differ from {} {:?}",
                        pc.name,
                        pc.unlocks(),
                        first.name,
                        first.unlocks()
                    ));
                }
            }
        }
    }

    fn check_nothing_lost(&self) {
        let layout = CloudLayout::new(self.server.clone());
        let prints: Vec<Fingerprint> = list_snapshots(&layout.game_dirs(&CloudKey::Steam(APP)))
            .iter()
            .map(|(dir, meta)| snapshot_fingerprint(dir, meta))
            .collect();
        for (label, print) in &self.seen {
            let pruned = |keeps: &dyn Fn(&Fingerprint) -> bool| {
                self.observed
                    .iter()
                    .any(|(id, (_, kept))| keeps(kept) && self.pruned_by_design(id))
            };
            let saves_kept = print.saves.iter().all(|save| {
                prints.iter().any(|snap| snap.saves.contains(save))
                    || pruned(&|kept| kept.saves.contains(save))
            });
            let unlocks_kept = print.unlocks.as_ref().is_none_or(|u| {
                u.is_empty()
                    || prints.iter().any(|snap| snap.unlocks.as_ref() == Some(u))
                    || pruned(&|kept| kept.unlocks.as_ref() == Some(u))
            });
            let found = saves_kept && unlocks_kept;
            if !found {
                self.fail(&format!(
                    "a local state of {label} was never snapshotted: {print:?}"
                ));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Fingerprint {
    saves: BTreeSet<String>,
    unlocks: Option<BTreeSet<String>>,
}

fn snapshot_short_id(rel: &Path) -> Option<String> {
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let at = parts.iter().position(|part| part == "snapshots")?;
    let folder = parts.get(at + 1)?;
    Some(folder.rsplit('_').next()?.to_owned())
}

fn try_fingerprint(dir: &Path, meta: &SnapshotMeta) -> Option<Fingerprint> {
    let saves = meta
        .files
        .iter()
        .filter(|f| f.category == Category::Save)
        .map(|f| std::fs::read_to_string(dir.join(&f.stored)).ok())
        .collect::<Option<BTreeSet<String>>>()?;
    let unlocks = load_state(dir)
        .ok()?
        .map(|state| state.unlocks.into_iter().map(|u| u.api_name).collect());
    Some(Fingerprint { saves, unlocks })
}

fn snapshot_fingerprint(dir: &Path, meta: &SnapshotMeta) -> Fingerprint {
    let saves = meta
        .files
        .iter()
        .filter(|f| f.category == Category::Save)
        .map(|f| std::fs::read_to_string(dir.join(&f.stored)).unwrap())
        .collect();
    let unlocks = load_state(dir)
        .unwrap()
        .map(|state| state.unlocks.into_iter().map(|u| u.api_name).collect());
    Fingerprint { saves, unlocks }
}

fn new_pc(
    dir: &TempDir,
    index: usize,
    generation: u32,
    kind: GameKind,
    account: String,
    skew: i64,
) -> Pc {
    let name = format!("pc{index}");
    let mirror = dir.path().join(format!("{name}-g{generation}-cloud"));
    std::fs::create_dir_all(&mirror).unwrap();
    Pc {
        device: LocalDevice::new(DeviceId(format!("{name}g{generation}")), name.clone()),
        name,
        generation,
        root: tempfile::tempdir().unwrap(),
        layout: CloudLayout::new(mirror.clone()),
        mirror,
        store: Store::in_memory().unwrap(),
        cache: HashCache::default(),
        kind,
        account,
        skew,
        plays: 0,
    }
}

fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
fn random_multi_pc_histories_never_lose_saves_and_converge() {
    let seeds = env_number("CLOUD_FUZZ_SEEDS", 40);
    let steps = env_number("CLOUD_FUZZ_STEPS", 40);
    let first = env_number("CLOUD_FUZZ_FIRST", 1);
    for seed in first..first + seeds {
        let mut sim = Sim::new(seed);
        for _ in 0..steps {
            sim.step();
        }
        sim.settle();
        sim.check_converged();
        sim.check_nothing_lost();
    }
}
