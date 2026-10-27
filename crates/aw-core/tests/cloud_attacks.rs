use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use aw_core::cloud::capture::{HashCache, capture};
use aw_core::cloud::device::{DeviceId, LocalDevice};
use aw_core::cloud::heads::{HeadChoice, HeadReason, read_heads, write_head};
use aw_core::cloud::key::{CloudKey, GameKind};
use aw_core::cloud::keyed::{KeyedDoc, Stamped, apply as keyed_apply, flatten, merge};
use aw_core::cloud::layout::CloudLayout;
use aw_core::cloud::locate::{Category, SaveRoot};
use aw_core::cloud::portable::{Placeholders, PortablePath};
use aw_core::cloud::snapshot::{SNAPSHOT_FILE, SnapshotMeta, list_snapshots};
use aw_core::cloud::sync::{
    CloudContext, GameTarget, SyncEvent, WAIT_CAP_SECS, comparable_digest, reconcile,
    set_principal, undo,
};
use aw_core::cloud::{CloudError, CloudResult};
use aw_core::discovery::folders::KnownFolders;
use aw_core::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;

const APP: u32 = 374_320;
const T0: i64 = 1_800_000_000;

struct Pc {
    root: TempDir,
    store: Store,
    device: LocalDevice,
    cache: HashCache,
    kind: GameKind,
    seen: std::cell::RefCell<Vec<String>>,
}

impl Pc {
    fn new(id: &str, name: &str, kind: GameKind) -> Pc {
        Pc {
            root: tempfile::tempdir().unwrap(),
            store: Store::in_memory().unwrap(),
            device: LocalDevice::new(DeviceId(id.into()), name),
            cache: HashCache::default(),
            kind,
            seen: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn app_data(&self) -> PathBuf {
        self.root.path().join("RoamingAppData")
    }

    fn placeholders(&self) -> Placeholders {
        Placeholders {
            folders: Some(KnownFolders::under(self.root.path())),
            install_dir: None,
            steam_root: Some(self.root.path().join("Steam")),
        }
    }

    fn target(&self) -> GameTarget {
        let mut roots = vec![root(
            Category::Save,
            "<winAppData>/DarkSoulsIII/<storeUserId>",
        )];
        if self.kind == GameKind::Emulator {
            roots.push(root(Category::Save, "<winAppData>/GSE Saves/374320/remote"));
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
            placeholders: self.placeholders(),
        }
    }

    fn save_dir(&self, account: &str) -> PathBuf {
        self.app_data().join("DarkSoulsIII").join(account)
    }

    fn save_path(&self, account: &str) -> PathBuf {
        self.save_dir(account).join("DS30000.sl2")
    }

    fn write_save(&self, account: &str, text: &str) {
        put(&self.save_path(account), text);
    }

    fn read_save(&self, account: &str) -> String {
        std::fs::read_to_string(self.save_path(account)).unwrap()
    }

    fn record_digest_for(&self, target: &GameTarget) {
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
        if !local.files.is_empty() {
            self.seen.borrow_mut().push(local.digest);
        }
    }

    fn ctx<'a>(&'a self, cloud: &'a CloudLayout, now: i64) -> CloudContext<'a> {
        CloudContext {
            layout: cloud,
            device: &self.device,
            store: &self.store,
            cache: &self.cache,
            now,
        }
    }

    fn try_reconcile(&self, cloud: &CloudLayout, now: i64) -> CloudResult<SyncEvent> {
        let target = self.target();
        self.record_digest_for(&target);
        reconcile(&self.ctx(cloud, now), &target, now)
    }

    fn reconcile(&self, cloud: &CloudLayout, now: i64) -> SyncEvent {
        self.try_reconcile(cloud, now).unwrap()
    }
}

fn root(category: Category, pattern: &str) -> SaveRoot {
    SaveRoot {
        category,
        pattern: PortablePath::new(pattern).unwrap(),
        single_file: false,
    }
}

fn put(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn cloud() -> (TempDir, CloudLayout) {
    let dir = tempfile::tempdir().unwrap();
    let layout = CloudLayout::new(dir.path().join("AchievementWatcher"));
    std::fs::create_dir_all(layout.root()).unwrap();
    (dir, layout)
}

fn snapshots(cloud: &CloudLayout) -> Vec<(PathBuf, SnapshotMeta)> {
    list_snapshots(&cloud.game_dirs(&CloudKey::Steam(APP)))
}

fn snapshot_dir(cloud: &CloudLayout, id: &str) -> PathBuf {
    snapshots(cloud)
        .into_iter()
        .find(|(_, m)| m.id == id)
        .unwrap()
        .0
}

fn game_dir(cloud: &CloudLayout) -> PathBuf {
    cloud.game_dirs(&CloudKey::Steam(APP)).remove(0).0
}

fn assert_nothing_lost(cloud: &CloudLayout, pcs: &[&Pc]) {
    let snaps = snapshots(cloud);
    for pc in pcs {
        for digest in pc.seen.borrow().iter() {
            assert!(
                snaps
                    .iter()
                    .any(|(_, meta)| &comparable_digest(meta, pc.kind) == digest),
                "a local state of {} was never snapshotted",
                pc.device.name
            );
        }
    }
}

fn all_files(dir: &Path) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(d) = pending.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            out.insert(path);
        }
    }
    out
}

fn rewrite_meta(dir: &Path, edit: impl FnOnce(&mut Value)) {
    let path = dir.join(SNAPSHOT_FILE);
    let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    edit(&mut value);
    std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn head(snapshot: &str, previous: Option<&str>, at: i64, device: &str) -> HeadChoice {
    HeadChoice {
        snapshot_id: snapshot.into(),
        previous: previous.map(str::to_owned),
        reason: HeadReason::Manual,
        chosen_at: at,
        device_id: device.into(),
        device_name: device.to_uppercase(),
    }
}

fn pushed(event: SyncEvent) -> String {
    match event {
        SyncEvent::Pushed { snapshot } => snapshot,
        other => panic!("expected push, got {other:?}"),
    }
}

#[test]
fn same_size_corrupted_principal_waits_instead_of_failing() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "AAAA-v1");
    a.reconcile(&cloud, T0);
    b.reconcile(&cloud, T0 + 10);
    a.write_save("1", "AAAA-v2");
    let v2 = pushed(a.reconcile(&cloud, T0 + 100));
    let dir = snapshot_dir(&cloud, &v2);
    let meta = snapshots(&cloud)
        .into_iter()
        .find(|(_, m)| m.id == v2)
        .unwrap()
        .1;
    std::fs::write(dir.join(&meta.files[0].stored), "ZZZZ-v2").unwrap();

    let result = b.try_reconcile(&cloud, T0 + 200);
    assert!(
        matches!(result, Ok(SyncEvent::Waiting)),
        "expected Waiting, got {result:?}"
    );
    assert_eq!(b.read_save("1"), "AAAA-v1");
}

#[test]
fn failed_restore_pins_the_pc_to_the_broken_snapshot_forever() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "AAAA-v1");
    let v1 = pushed(a.reconcile(&cloud, T0));
    b.reconcile(&cloud, T0 + 10);
    a.write_save("1", "AAAA-v2");
    let v2 = pushed(a.reconcile(&cloud, T0 + 100));
    let dir = snapshot_dir(&cloud, &v2);
    let meta = snapshots(&cloud)
        .into_iter()
        .find(|(_, m)| m.id == v2)
        .unwrap()
        .1;
    std::fs::write(dir.join(&meta.files[0].stored), "ZZZZ-v2").unwrap();
    let _ = b.try_reconcile(&cloud, T0 + 200);

    a.write_save("1", "AAAA-v3");
    let v3 = pushed(a.reconcile(&cloud, T0 + 300));
    let later = b.try_reconcile(&cloud, T0 + 400);
    let older = match &later {
        Ok(SyncEvent::Restored {
            kept_local: Some(kept),
            ..
        }) => kept.clone(),
        _ => v1.clone(),
    };
    let picked = set_principal(
        &b.ctx(&cloud, T0 + 500),
        &b.target(),
        &older,
        HeadReason::Manual,
    );
    let undone = undo(&b.ctx(&cloud, T0 + 600), &b.target());
    assert!(
        matches!(&later, Ok(SyncEvent::Restored { snapshot, .. }) if *snapshot == v3)
            && picked.is_ok(),
        "B should move on to v3 ({v3}) and the user should be able to pick an older version;          got reconcile={later:?} set_principal={picked:?} undo={undone:?}"
    );
}

#[test]
fn zero_byte_state_json_waits_instead_of_failing() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::Emulator);
    let b = Pc::new("b", "Notebook", GameKind::Emulator);
    a.write_save("1", "v1");
    a.store.record_unlock(APP, "ACH_1", 1, 1, "gse").unwrap();
    let v1 = pushed(a.reconcile(&cloud, T0));
    std::fs::write(snapshot_dir(&cloud, &v1).join("state.json"), "").unwrap();
    let result = b.try_reconcile(&cloud, T0 + 100);
    assert!(
        matches!(result, Ok(SyncEvent::Waiting)),
        "expected Waiting, got {result:?}"
    );
}

#[test]
fn steam_userdata_with_an_unrelated_second_account_still_restores() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::Emulator);
    put(
        &a.app_data()
            .join("GSE Saves")
            .join("374320")
            .join("remote")
            .join("slot.dat"),
        "cloud",
    );
    a.store.record_unlock(APP, "A", 1, 1, "gse").unwrap();
    a.reconcile(&cloud, T0);

    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    let userdata = b.root.path().join("Steam").join("userdata");
    put(
        &userdata
            .join("222")
            .join("374320")
            .join("remote")
            .join("slot.dat"),
        "local",
    );
    std::fs::create_dir_all(userdata.join("111").join("7")).unwrap();
    let target = GameTarget {
        key: CloudKey::Steam(APP),
        name: "Dark Souls III".into(),
        app_id: APP,
        kind: GameKind::SteamOriginal,
        roots: vec![root(
            Category::Save,
            "<steamRoot>/userdata/<storeUserId>/374320/remote",
        )],
        placeholders: b.placeholders(),
    };
    let event = reconcile(&b.ctx(&cloud, T0 + 100), &target, T0 + 100);
    assert!(
        matches!(event, Ok(SyncEvent::Restored { .. })),
        "expected restore into userdata/222, got {event:?}"
    );
    assert_eq!(
        std::fs::read_to_string(userdata.join("222/374320/remote/slot.dat")).unwrap(),
        "cloud"
    );
}

#[test]
fn stale_pending_marker_restores_an_old_snapshot_when_it_reappears() {
    let (dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, T0);
    b.reconcile(&cloud, T0 + 10);
    a.write_save("1", "v2");
    let v2 = pushed(a.reconcile(&cloud, T0 + 100));
    b.store.meta_set("cloud_pending:374320", &v2).unwrap();
    let v2_dir = snapshot_dir(&cloud, &v2);
    let parked = dir.path().join("parked");
    std::fs::rename(&v2_dir, &parked).unwrap();

    b.write_save("1", "b plays on");
    let first = b.reconcile(&cloud, T0 + 5000);
    assert!(matches!(first, SyncEvent::Pushed { .. }), "{first:?}");
    b.write_save("1", "b plays more");
    assert!(matches!(
        b.reconcile(&cloud, T0 + 6000),
        SyncEvent::Pushed { .. }
    ));

    std::fs::rename(&parked, &v2_dir).unwrap();
    let event = b.reconcile(&cloud, T0 + 7000);
    assert_eq!(
        event,
        SyncEvent::Nothing,
        "B's own pushed save is principal; a stale marker must not restore v2"
    );
    assert_eq!(b.read_save("1"), "b plays more");
}

#[test]
fn pc_with_clock_ahead_reverts_the_other_pcs_new_progress() {
    let (_dir, cloud) = cloud();
    let ahead = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let honest = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    let skew = 86_400;
    ahead.write_save("1", "v1");
    ahead.reconcile(&cloud, T0 + skew);
    honest.reconcile(&cloud, T0 + 100);
    assert_eq!(honest.read_save("1"), "v1");

    honest.write_save("1", "new progress");
    let mine = pushed(honest.reconcile(&cloud, T0 + 200));
    let next = honest.reconcile(&cloud, T0 + 300);
    assert_eq!(
        next,
        SyncEvent::Nothing,
        "honest PC's own push {mine} should stay principal"
    );
    assert_eq!(honest.read_save("1"), "new progress");
}

#[test]
fn future_head_for_missing_snapshot_does_not_wait_forever() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, T0);
    write_head(
        &aw_core::cloud::layout::GamePaths(game_dir(&cloud)),
        &DeviceId("z".into()),
        &head("deadbeefcafe", None, T0 + 365 * 86_400, "z"),
    )
    .unwrap();
    let later = T0 + WAIT_CAP_SECS + 3600;
    a.write_save("1", "v2");
    let event = a.reconcile(&cloud, later);
    assert_ne!(event, SyncEvent::Waiting);
}

/// (debug: overflow panic in `now - latest.chosen_at`; release: wraps and waits
#[test]
fn head_with_minimum_timestamp_does_not_crash_or_wait() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, T0);
    let heads = game_dir(&cloud).join("heads");
    for entry in std::fs::read_dir(&heads).unwrap().flatten() {
        std::fs::remove_file(entry.path()).unwrap();
    }
    std::fs::write(
        heads.join("evil.json"),
        serde_json::to_vec(&head("nope", None, i64::MIN, "evil")).unwrap(),
    )
    .unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        a.write_save("1", "v2");
        a.try_reconcile(&cloud, T0 + 100)
    }));
    let Ok(result) = outcome else {
        panic!("reconcile panicked on a garbage head");
    };
    assert!(
        matches!(result, Ok(SyncEvent::Pushed { .. })),
        "expected the change to be pushed, got {result:?}"
    );
}

#[test]
fn huge_playtime_in_cloud_state_does_not_crash() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::Emulator);
    let b = Pc::new("b", "Notebook", GameKind::Emulator);
    a.write_save("1", "v1");
    a.store.record_unlock(APP, "ACH_1", 1, 1, "gse").unwrap();
    a.reconcile(&cloud, T0);
    b.reconcile(&cloud, T0 + 10);
    b.store.add_playtime(APP, 500, T0 + 20).unwrap();
    a.write_save("1", "v2");
    let v2 = pushed(a.reconcile(&cloud, T0 + 100));
    let state_path = snapshot_dir(&cloud, &v2).join("state.json");
    let mut state: Value = serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
    state["playtimeSeconds"] = json!(i64::MAX);
    std::fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        b.try_reconcile(&cloud, T0 + 200)
    }));
    assert!(outcome.is_ok(), "reconcile panicked on a huge playtime");
    assert!(b.store.aw_playtime_seconds(APP).unwrap() > 0);
}

#[test]
fn restore_over_file_with_different_name_case_keeps_the_file() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    put(&a.save_dir("1").join("Slot.sav"), "from desktop");
    a.reconcile(&cloud, T0);
    put(&b.save_dir("1").join("slot.sav"), "from notebook");
    let event = b.reconcile(&cloud, T0 + 100);
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    let names: Vec<String> = std::fs::read_dir(b.save_dir("1"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let again = b.reconcile(&cloud, T0 + 200);
    assert_eq!(
        names.len(),
        1,
        "files left after restore: {names:?}; next reconcile: {again:?}, still empty: {}",
        std::fs::read_dir(b.save_dir("1")).unwrap().count() == 0
    );
    assert_eq!(
        std::fs::read_to_string(b.save_dir("1").join(&names[0])).unwrap(),
        "from desktop"
    );
    assert_nothing_lost(&cloud, &[&a, &b]);
}

#[test]
fn read_only_destination_does_not_leave_a_half_applied_restore() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    put(&a.save_dir("1").join("a.sav"), "A1");
    put(&a.save_dir("1").join("b.sav"), "B1");
    a.reconcile(&cloud, T0);
    b.reconcile(&cloud, T0 + 10);
    let locked = b.save_dir("1").join("b.sav");
    let mut perms = std::fs::metadata(&locked).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&locked, perms).unwrap();

    put(&a.save_dir("1").join("a.sav"), "A2");
    put(&a.save_dir("1").join("b.sav"), "B2");
    a.reconcile(&cloud, T0 + 100);
    let result = b.try_reconcile(&cloud, T0 + 200);
    let a_now = std::fs::read_to_string(b.save_dir("1").join("a.sav")).unwrap();
    let b_now = std::fs::read_to_string(&locked).unwrap();
    let mut perms = std::fs::metadata(&locked).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&locked, perms).unwrap();
    assert!(
        (a_now == "A1" && b_now == "B1") || (a_now == "A2" && b_now == "B2"),
        "mixed restore: a.sav={a_now} b.sav={b_now} result={result:?}"
    );
}

#[test]
fn restore_does_not_wipe_a_save_root_the_snapshot_does_not_cover() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::Emulator);
    put(
        &a.app_data()
            .join("GSE Saves")
            .join("374320")
            .join("remote")
            .join("slot.dat"),
        "emu",
    );
    a.store.record_unlock(APP, "A", 1, 1, "gse").unwrap();
    a.reconcile(&cloud, T0);

    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    b.write_save("1111", "my only save");
    let event = b.reconcile(&cloud, T0 + 100);
    assert!(
        b.save_path("1111").exists(),
        "B's save was deleted and nothing was written (event {event:?})"
    );
    assert_nothing_lost(&cloud, &[&a, &b]);
}

fn hostile_original_restore(original: &str) -> (Pc, BTreeSet<PathBuf>, CloudResult<SyncEvent>) {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1", "evil");
    let id = pushed(a.reconcile(&cloud, T0));
    let dir = snapshot_dir(&cloud, &id);
    rewrite_meta(&dir, |meta| {
        meta["files"][0]["original"] = json!(original);
    });
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    std::fs::create_dir_all(b.save_dir("1")).unwrap();
    std::fs::create_dir_all(b.root.path().join("Windows")).unwrap();
    let before = all_files(b.root.path());
    let result = reconcile(&b.ctx(&cloud, T0 + 100), &b.target(), T0 + 100);
    (b, before, result)
}

#[test]
fn hostile_originals_never_write_outside_the_save_root() {
    let hostile = [
        "<winAppData>/DarkSoulsIII/1/../../../Windows/evil.dll",
        "<winAppData>/DarkSoulsIII/1/.. /.. /.. /Windows/evil.dll",
        "<winAppData>/DarkSoulsIII/1/... /Windows/evil.dll",
        "<winAppData>/DarkSoulsIII/1/..\u{20}\u{20}/..\u{20}/evil.dll",
        "<winAppData>/DarkSoulsIII/1/..\\..\\..\\evil.dll",
        "<winAppData>/DarkSoulsIII/1/C:/evil.dll",
        "<winDir>/System32/evil.dll",
        "<winAppData>/DarkSoulsIII/<storeUserId>/../evil.dll",
        "<winAppData>/DarkSoulsIII/1/x\u{0}y",
        "<winAppData>/DarkSoulsIII",
        "<winAppData>/DarkSoulsIII/1/foo/..",
        "<winAppData>/DarkSoulsIII/1/....",
        "<winAppData>/DarkSoulsIII/1/.. .",
    ];
    for original in hostile {
        let (b, before, result) = hostile_original_restore(original);
        let save_root = b.app_data().join("DarkSoulsIII");
        let after = all_files(b.root.path());
        let escaped: Vec<_> = after
            .difference(&before)
            .filter(|p| !p.starts_with(&save_root))
            .collect();
        assert!(
            escaped.is_empty(),
            "{original:?} wrote outside the save root: {escaped:?} ({result:?})"
        );
    }
}

#[test]
fn hostile_stored_paths_are_never_read() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1", "evil");
    let id = pushed(a.reconcile(&cloud, T0));
    let dir = snapshot_dir(&cloud, &id);
    for stored in [
        "../../../secret",
        "C:/Windows/win.ini",
        "/etc/passwd",
        "",
        "files\\..\\..\\x",
    ] {
        rewrite_meta(&dir, |meta| {
            meta["files"][0]["stored"] = json!(stored);
        });
        assert!(
            snapshots(&cloud).is_empty(),
            "stored {stored:?} was accepted"
        );
    }
}

#[test]
fn garbage_everywhere_in_the_cloud_folder_is_ignored() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, T0);
    let game = game_dir(&cloud);
    let snaps = game.join("snapshots");
    put(&game.join("heads").join("junk.json"), "{\"snapshotId\": 7}");
    put(&game.join("heads").join("empty.json"), "");
    std::fs::create_dir_all(game.join("heads").join("dir.json")).unwrap();
    put(&game.join("heads").join("bin.json"), "\u{0}\u{1}\u{2}");
    put(&snaps.join("not-a-dir"), "x");
    put(
        &snaps.join("2026-01-01_00-00_x_abcdef").join(SNAPSHOT_FILE),
        "[]",
    );
    put(
        &snaps.join("foreign").join(SNAPSHOT_FILE),
        "{\"format\": 99}",
    );
    put(&snaps.join(".tmp-123456789012").join(SNAPSHOT_FILE), "{");
    std::fs::create_dir_all(snaps.join("emptydir")).unwrap();
    put(&cloud.root().join("games").join("374320 - file"), "x");
    std::fs::create_dir_all(cloud.root().join("games").join("374320 - Other Name")).unwrap();
    put(
        &cloud.root().join("games").join("garbage name").join("x"),
        "x",
    );

    let event = b.reconcile(&cloud, T0 + 100);
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    assert_eq!(b.read_save("1"), "v1");
    assert_eq!(b.reconcile(&cloud, T0 + 200), SyncEvent::Nothing);
    assert_eq!(a.reconcile(&cloud, T0 + 300), SyncEvent::Nothing);
}

#[test]
fn snapshot_json_with_extreme_values_is_tolerated() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    let id = pushed(a.reconcile(&cloud, T0));
    let dir = snapshot_dir(&cloud, &id);
    let copy = dir.with_file_name("copy (1)");
    std::fs::create_dir_all(copy.join("files").join("save")).unwrap();
    for entry in all_files(&dir) {
        let rel = entry.strip_prefix(&dir).unwrap();
        if entry.is_file() {
            std::fs::create_dir_all(copy.join(rel).parent().unwrap()).unwrap();
            std::fs::copy(&entry, copy.join(rel)).unwrap();
        }
    }
    rewrite_meta(&copy, |meta| {
        meta["id"] = json!("x".repeat(5000));
        meta["sessionEndedAt"] = json!(i64::MIN);
        meta["createdAt"] = json!(i64::MAX);
        meta["parent"] = json!(id.clone());
        meta["deviceName"] = json!("\u{202e}evil\u{0}");
    });
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    let event = b.try_reconcile(&cloud, T0 + 100);
    assert!(event.is_ok(), "{event:?}");
    assert_eq!(b.read_save("1"), "v1");
}

#[test]
fn three_pcs_diverging_converge_and_go_quiet() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    let c = Pc::new("c", "Deck", GameKind::SteamOriginal);
    a.write_save("1", "base");
    a.reconcile(&cloud, T0);
    b.reconcile(&cloud, T0 + 1);
    c.reconcile(&cloud, T0 + 2);
    a.write_save("1", "a line");
    b.write_save("1", "b line");
    c.write_save("1", "c line");
    a.reconcile(&cloud, T0 + 100);
    b.reconcile(&cloud, T0 + 110);
    c.reconcile(&cloud, T0 + 120);
    let mut cloud_files_before = all_files(cloud.root());
    for round in 0..4 {
        for (i, pc) in [&a, &b, &c].iter().enumerate() {
            pc.reconcile(&cloud, T0 + 1000 + round * 100 + i as i64);
        }
        let now = all_files(cloud.root());
        let removed: Vec<_> = cloud_files_before.difference(&now).collect();
        assert!(removed.is_empty(), "cloud files disappeared: {removed:?}");
        cloud_files_before = now;
    }
    let content = a.read_save("1");
    assert_eq!(b.read_save("1"), content);
    assert_eq!(c.read_save("1"), content);
    let count = snapshots(&cloud).len();
    for pc in [&a, &b, &c] {
        assert_eq!(pc.reconcile(&cloud, T0 + 5000), SyncEvent::Nothing);
    }
    assert_eq!(snapshots(&cloud).len(), count);
    assert_nothing_lost(&cloud, &[&a, &b, &c]);
}

#[test]
fn simultaneous_pushes_with_tied_timestamps_converge() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "base");
    a.reconcile(&cloud, T0);
    b.reconcile(&cloud, T0 + 1);
    a.write_save("1", "a");
    b.write_save("1", "b");
    a.reconcile(&cloud, T0 + 100);
    let heads_dir = game_dir(&cloud).join("heads");
    let a_head = std::fs::read(heads_dir.join("a.json")).unwrap();
    std::fs::remove_file(heads_dir.join("a.json")).unwrap();
    let b_snaps_before = snapshots(&cloud).len();
    b.reconcile(&cloud, T0 + 100);
    std::fs::write(heads_dir.join("a.json"), a_head).unwrap();
    assert!(snapshots(&cloud).len() > b_snaps_before);
    for t in 0..3 {
        a.reconcile(&cloud, T0 + 200 + t * 10);
        b.reconcile(&cloud, T0 + 205 + t * 10);
    }
    assert_eq!(a.read_save("1"), b.read_save("1"));
    assert_eq!(a.reconcile(&cloud, T0 + 400), SyncEvent::Nothing);
    assert_eq!(b.reconcile(&cloud, T0 + 410), SyncEvent::Nothing);
    assert_nothing_lost(&cloud, &[&a, &b]);
}

#[test]
fn set_principal_to_old_then_play_then_undo_keeps_everything() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    let v1 = pushed(a.reconcile(&cloud, T0));
    a.write_save("1", "v2");
    a.reconcile(&cloud, T0 + 100);
    b.reconcile(&cloud, T0 + 210);
    set_principal(
        &b.ctx(&cloud, T0 + 300),
        &b.target(),
        &v1,
        HeadReason::Manual,
    )
    .unwrap();
    assert_eq!(b.read_save("1"), "v1");
    b.write_save("1", "v1 then more");
    assert!(matches!(
        b.reconcile(&cloud, T0 + 400),
        SyncEvent::Pushed { .. }
    ));
    assert!(matches!(
        a.reconcile(&cloud, T0 + 500),
        SyncEvent::Restored { .. }
    ));
    assert_eq!(a.read_save("1"), "v1 then more");
    undo(&b.ctx(&cloud, T0 + 600), &b.target()).unwrap();
    a.reconcile(&cloud, T0 + 700);
    b.reconcile(&cloud, T0 + 710);
    assert_eq!(a.read_save("1"), b.read_save("1"));
    assert_eq!(a.reconcile(&cloud, T0 + 800), SyncEvent::Nothing);
    assert_eq!(b.reconcile(&cloud, T0 + 810), SyncEvent::Nothing);
    assert_nothing_lost(&cloud, &[&a, &b]);
}

#[test]
fn user_deleting_all_heads_and_principal_folder_recovers() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, T0);
    b.reconcile(&cloud, T0 + 10);
    a.write_save("1", "v2");
    let v2 = pushed(a.reconcile(&cloud, T0 + 100));
    std::fs::remove_dir_all(game_dir(&cloud).join("heads")).unwrap();
    std::fs::remove_dir_all(snapshot_dir(&cloud, &v2)).unwrap();
    let ea = a.try_reconcile(&cloud, T0 + 5000);
    let eb = b.try_reconcile(&cloud, T0 + 5010);
    assert!(ea.is_ok() && eb.is_ok(), "{ea:?} {eb:?}");
    a.reconcile(&cloud, T0 + 5100);
    b.reconcile(&cloud, T0 + 5110);
    assert_eq!(a.read_save("1"), b.read_save("1"));
    assert_eq!(a.reconcile(&cloud, T0 + 5200), SyncEvent::Nothing);
    assert_eq!(b.reconcile(&cloud, T0 + 5210), SyncEvent::Nothing);
    assert!(
        snapshots(&cloud)
            .iter()
            .any(|(_, m)| comparable_digest(m, GameKind::SteamOriginal)
                == *a.seen.borrow().last().unwrap())
    );
}

#[test]
fn interrupted_restore_leftovers_are_not_captured_or_pushed() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, T0);
    put(&a.save_dir("1").join("DS30000.sl2.awtmp"), "half staged");
    assert_eq!(a.reconcile(&cloud, T0 + 100), SyncEvent::Nothing);
    put(
        &game_dir(&cloud)
            .join("snapshots")
            .join(".tmp-aaaaaaaaaaaa")
            .join("files/save/0-x"),
        "x",
    );
    assert_eq!(a.reconcile(&cloud, T0 + 200), SyncEvent::Nothing);
}

#[test]
fn unicode_and_long_names_round_trip() {
    let (_dir, cloud) = cloud();
    let a = Pc::new(
        "a",
        "Área de Trabalho do João 🎮 ".repeat(6).as_str(),
        GameKind::SteamOriginal,
    );
    let b = Pc::new("b", "ノートPC", GameKind::SteamOriginal);
    let long = "セーブ".repeat(30);
    let nested = a
        .save_dir("1")
        .join("Ünïcödé dir")
        .join(format!("{long}.sav"));
    put(&nested, "unicode");
    a.reconcile(&cloud, T0);
    b.reconcile(&cloud, T0 + 10);
    assert_eq!(
        std::fs::read_to_string(
            b.save_dir("1")
                .join("Ünïcödé dir")
                .join(format!("{long}.sav"))
        )
        .unwrap(),
        "unicode"
    );
    assert_eq!(b.reconcile(&cloud, T0 + 20), SyncEvent::Nothing);
}

#[test]
fn different_account_folders_per_pc_never_cross() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("AAAA", "a1");
    a.reconcile(&cloud, T0);
    b.write_save("BBBB", "b own");
    b.reconcile(&cloud, T0 + 10);
    assert_eq!(b.read_save("AAAA"), "a1");
    assert_eq!(b.read_save("BBBB"), "b own");
    b.write_save("AAAA", "b2");
    b.reconcile(&cloud, T0 + 20);
    a.reconcile(&cloud, T0 + 30);
    assert_eq!(a.read_save("AAAA"), "b2");
    assert_eq!(a.read_save("BBBB"), "b own");
    assert_eq!(a.reconcile(&cloud, T0 + 40), SyncEvent::Nothing);
    assert_eq!(b.reconcile(&cloud, T0 + 50), SyncEvent::Nothing);
}

#[test]
fn conflict_copy_of_own_head_file_does_not_flip_principal() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, T0);
    let heads = game_dir(&cloud).join("heads");
    std::fs::copy(
        heads.join("a.json"),
        heads.join("a (Notebook's conflicted copy).json"),
    )
    .unwrap();
    a.write_save("1", "v2");
    a.reconcile(&cloud, T0 + 100);
    b.reconcile(&cloud, T0 + 200);
    assert_eq!(b.read_save("1"), "v2");
    assert_eq!(read_heads(&cloud.game_dirs(&CloudKey::Steam(APP))).len(), 2);
    assert_eq!(b.reconcile(&cloud, T0 + 300), SyncEvent::Nothing);
}

fn doc(device: &str, keys: &[(&str, Value, i64)]) -> KeyedDoc {
    KeyedDoc {
        device_id: device.into(),
        keys: keys
            .iter()
            .map(|(k, v, at)| {
                (
                    (*k).to_owned(),
                    Stamped {
                        value: v.clone(),
                        at: *at,
                    },
                )
            })
            .collect(),
    }
}

#[test]
fn keyed_merge_is_order_independent_with_conflicts() {
    let docs = [
        doc(
            "a",
            &[
                ("preset", json!("x"), 5),
                ("gameSync.7", json!({"enabled": true}), 9),
            ],
        ),
        doc(
            "b",
            &[("preset", json!("y"), 5), ("gameSync.7", Value::Null, 9)],
        ),
        doc("a", &[("preset", json!("z"), 5)]),
        doc(
            "c",
            &[("sound", json!(false), i64::MIN), ("preset", json!("w"), 4)],
        ),
    ];
    let reference = merge(&docs);
    let mut permuted = docs.to_vec();
    permuted.reverse();
    assert_eq!(merge(&permuted), reference);
    permuted.rotate_left(1);
    assert_eq!(merge(&permuted), reference);
}

#[test]
fn keyed_apply_keeps_local_only_keys_and_round_trips() {
    let local_keys = ["cloudFolder", "extraGameDirs"];
    let theirs =
        json!({"preset": "x", "cloudFolder": "Z:/evil", "gameSync": {"7": {"enabled": true}}});
    let mut mine = json!({"preset": "old", "cloudFolder": "D:/mine", "gameSync": "garbage"});
    let flat = flatten(&theirs, &local_keys);
    keyed_apply(&mut mine, &flat);
    assert_eq!(mine["cloudFolder"], json!("D:/mine"));
    assert_eq!(mine["gameSync"], json!({"7": {"enabled": true}}));
    assert_eq!(flatten(&mine, &local_keys), flat);
}

#[test]
fn portable_paths_reject_every_escape() {
    for bad in [
        "<winAppData>/..",
        "<winAppData>/a/../b",
        "<winAppData>\\..\\b",
        "<winAppData>/a/./b",
        "<winAppData>//b",
        "<winAppData>/C:",
        "<winAppData>/\\\\server\\share",
        "<winAppData>/a\u{0}",
        "<winAppData>/a?b",
        "../<winAppData>",
        "<winAppData>x/a",
        "",
    ] {
        assert!(PortablePath::new(bad).is_none(), "{bad:?} accepted");
    }
}

#[allow(dead_code)]
fn _unused(_: CloudError) {}
