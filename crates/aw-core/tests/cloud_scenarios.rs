use std::path::{Path, PathBuf};

use aw_core::cloud::CloudError;
use aw_core::cloud::capture::{HashCache, capture};
use aw_core::cloud::device::{DeviceId, LocalDevice};
use aw_core::cloud::heads::HeadReason;
use aw_core::cloud::key::{CloudKey, GameKind};
use aw_core::cloud::layout::CloudLayout;
use aw_core::cloud::locate::{Category, SaveRoot};
use aw_core::cloud::lock::{
    LOCK_STALE_SECS, LockRecord, foreign_locks, own_lock, remove_own_lock, write_lock,
};
use aw_core::cloud::portable::{Placeholders, PortablePath};
use aw_core::cloud::snapshot::{SnapshotMeta, list_snapshots};
use aw_core::cloud::sync::{
    CloudContext, GameTarget, SyncEvent, comparable_digest, reconcile, reconcile_cancellable,
    set_principal, undo,
};
use aw_core::discovery::folders::KnownFolders;
use aw_core::store::Store;
use tempfile::TempDir;

const APP: u32 = 374_320;

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
            device: LocalDevice {
                id: DeviceId(id.into()),
                name: name.into(),
            },
            cache: HashCache::default(),
            kind,
            seen: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn app_data(&self) -> PathBuf {
        self.root.path().join("RoamingAppData")
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
            placeholders: Placeholders {
                folders: Some(KnownFolders::under(self.root.path())),
                ..Default::default()
            },
        }
    }

    fn save_path(&self, account: &str) -> PathBuf {
        self.app_data()
            .join("DarkSoulsIII")
            .join(account)
            .join("DS30000.sl2")
    }

    fn write_save(&self, account: &str, text: &str) {
        put(&self.save_path(account), text);
    }

    fn read_save(&self, account: &str) -> String {
        std::fs::read_to_string(self.save_path(account)).unwrap()
    }

    fn emulator_file(&self) -> PathBuf {
        self.app_data()
            .join("GSE Saves")
            .join("374320")
            .join("achievements.json")
    }

    fn record_digest(&self) {
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

    fn reconcile(&self, cloud: &CloudLayout, now: i64) -> SyncEvent {
        self.record_digest();
        reconcile(&self.ctx(cloud, now), &self.target(), now).unwrap()
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

#[test]
fn formatted_pc_restores_and_keeps_the_new_save() {
    let (_dir, cloud) = cloud();
    let desktop = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    desktop.write_save("1111", "87 hours");
    assert!(matches!(
        desktop.reconcile(&cloud, 100),
        SyncEvent::Pushed { .. }
    ));

    let formatted = Pc::new("b", "Desktop", GameKind::SteamOriginal);
    formatted.write_save("1111", "new game");
    let event = formatted.reconcile(&cloud, 200);
    let SyncEvent::Restored { kept_local, .. } = event else {
        panic!("expected restore, got {event:?}");
    };
    assert!(kept_local.is_some());
    assert_eq!(formatted.read_save("1111"), "87 hours");
    assert_eq!(formatted.reconcile(&cloud, 300), SyncEvent::Nothing);

    undo(&formatted.ctx(&cloud, 400), &formatted.target()).unwrap();
    assert_eq!(formatted.read_save("1111"), "new game");
    assert_nothing_lost(&cloud, &[&desktop, &formatted]);
}

#[test]
fn emulator_restore_brings_unlocks_hours_and_files_without_popups() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::Emulator);
    a.write_save("0110", "progress");
    put(&a.emulator_file(), r#"{"ACH_1":{"earned":true}}"#);
    a.store
        .record_unlock(APP, "ACH_1", 1000, 1001, "gse")
        .unwrap();
    a.store.update_progress(APP, "ACH_2", 3, 10).unwrap();
    a.store.add_playtime(APP, 313_200, 2000).unwrap();
    assert!(matches!(
        a.reconcile(&cloud, 3000),
        SyncEvent::Pushed { .. }
    ));

    let b = Pc::new("b", "Desktop", GameKind::Emulator);
    let event = b.reconcile(&cloud, 4000);
    assert!(
        matches!(
            event,
            SyncEvent::Restored {
                kept_local: None,
                ..
            }
        ),
        "{event:?}"
    );
    assert_eq!(
        b.store.game_state(APP).unwrap().unlocks,
        a.store.game_state(APP).unwrap().unlocks
    );
    assert_eq!(
        b.store.game_state(APP).unwrap().progress,
        a.store.game_state(APP).unwrap().progress
    );
    assert_eq!(b.store.aw_playtime_seconds(APP).unwrap(), 313_200);
    assert_eq!(
        std::fs::read(b.emulator_file()).unwrap(),
        std::fs::read(a.emulator_file()).unwrap()
    );
    assert_eq!(b.read_save("0110"), "progress");
    assert_eq!(b.reconcile(&cloud, 5000), SyncEvent::Nothing);
}

#[test]
fn two_pcs_diverge_into_branches_and_switch_back_and_forth() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "base");
    a.reconcile(&cloud, 100);
    b.reconcile(&cloud, 110);
    assert_eq!(b.read_save("1"), "base");

    a.write_save("1", "desktop line");
    let SyncEvent::Pushed {
        snapshot: desktop_line,
    } = a.reconcile(&cloud, 200)
    else {
        panic!()
    };
    b.write_save("1", "notebook line");
    let event = b.reconcile(&cloud, 250);
    let SyncEvent::Branched {
        principal: notebook_line,
        local_is_principal: true,
        ..
    } = event
    else {
        panic!("expected branch, got {event:?}");
    };
    let snaps = snapshots(&cloud);
    let parent_of = |id: &str| {
        snaps
            .iter()
            .find(|(_, m)| m.id == id)
            .and_then(|(_, m)| m.parent.clone())
    };
    assert_eq!(parent_of(&desktop_line), parent_of(&notebook_line));

    assert!(matches!(
        a.reconcile(&cloud, 300),
        SyncEvent::Restored { .. }
    ));
    assert_eq!(a.read_save("1"), "notebook line");

    set_principal(
        &a.ctx(&cloud, 400),
        &a.target(),
        &desktop_line,
        HeadReason::Manual,
    )
    .unwrap();
    assert_eq!(a.read_save("1"), "desktop line");
    assert!(matches!(
        b.reconcile(&cloud, 410),
        SyncEvent::Restored { .. }
    ));
    assert_eq!(b.read_save("1"), "desktop line");

    set_principal(
        &b.ctx(&cloud, 500),
        &b.target(),
        &notebook_line,
        HeadReason::Manual,
    )
    .unwrap();
    assert_eq!(b.read_save("1"), "notebook line");
    assert!(matches!(
        a.reconcile(&cloud, 510),
        SyncEvent::Restored { .. }
    ));
    assert_eq!(a.read_save("1"), "notebook line");
    assert_nothing_lost(&cloud, &[&a, &b]);
}

#[test]
fn crashed_session_is_pushed_on_next_start_and_stale_lock_is_ignored() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, 100);
    let game = cloud.game_dirs(&CloudKey::Steam(APP)).remove(0);
    write_lock(
        &game,
        &a.device.id,
        &LockRecord {
            device_id: "a".into(),
            device_name: "Desktop".into(),
            game_key: "374320".into(),
            started_at: 200,
            renewed_at: 200,
        },
    )
    .unwrap();
    a.write_save("1", "played then crashed");

    let games = cloud.game_dirs(&CloudKey::Steam(APP));
    assert_eq!(foreign_locks(&games, &b.device.id, 250).len(), 1);
    assert!(foreign_locks(&games, &b.device.id, 200 + LOCK_STALE_SECS + 1).is_empty());

    assert!(own_lock(&game, &a.device.id).is_some());
    assert!(matches!(
        a.reconcile(&cloud, 5000),
        SyncEvent::Pushed { .. }
    ));
    remove_own_lock(&game, &a.device.id).unwrap();
    b.reconcile(&cloud, 5100);
    assert_eq!(b.read_save("1"), "played then crashed");
}

#[test]
fn incomplete_snapshot_makes_both_pcs_wait() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, 100);
    b.reconcile(&cloud, 110);
    a.write_save("1", "v2");
    let SyncEvent::Pushed { snapshot } = a.reconcile(&cloud, 200) else {
        panic!()
    };
    let (dir, meta) = snapshots(&cloud)
        .into_iter()
        .find(|(_, m)| m.id == snapshot)
        .unwrap();
    std::fs::write(dir.join(&meta.files[0].stored), "half").unwrap();
    assert_eq!(b.reconcile(&cloud, 300), SyncEvent::Waiting);
    assert_eq!(b.read_save("1"), "v1");
    assert_eq!(a.reconcile(&cloud, 300), SyncEvent::Waiting);
}

#[test]
fn deleting_any_snapshot_folder_never_breaks_sync() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    for (i, text) in ["v1", "v2", "v3"].iter().enumerate() {
        a.write_save("1", text);
        a.reconcile(&cloud, 100 * (i as i64 + 1));
    }
    let snaps = snapshots(&cloud);
    std::fs::remove_dir_all(&snaps[1].0).unwrap();
    assert!(matches!(
        b.reconcile(&cloud, 1000),
        SyncEvent::Restored { .. }
    ));
    assert_eq!(b.read_save("1"), "v3");

    std::fs::remove_dir_all(&snaps[2].0).unwrap();
    let event = b.reconcile(&cloud, 5000);
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    let event = a.reconcile(&cloud, 5100);
    assert!(!matches!(event, SyncEvent::Waiting), "{event:?}");
    let latest = a.seen.borrow().last().cloned().unwrap();
    assert!(
        snapshots(&cloud)
            .iter()
            .any(|(_, meta)| comparable_digest(meta, a.kind) == latest)
    );
}

#[test]
fn steam_original_syncs_only_the_save() {
    let (_dir, cloud) = cloud();
    let cracked = Pc::new("a", "Desktop", GameKind::Emulator);
    cracked.write_save("0110", "save");
    put(&cracked.emulator_file(), "{}");
    cracked
        .store
        .record_unlock(APP, "ACH_1", 1, 1, "gse")
        .unwrap();
    cracked.reconcile(&cloud, 100);

    let original = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    original
        .store
        .record_unlock(APP, "ACH_9", 5, 5, "steam")
        .unwrap();
    original.reconcile(&cloud, 200);
    assert_eq!(original.read_save("0110"), "save");
    assert!(!original.emulator_file().exists());
    let unlocks = original.store.game_state(APP).unwrap().unlocks;
    assert_eq!(unlocks.len(), 1);
    assert_eq!(unlocks[0].api_name, "ACH_9");
}

#[test]
fn cracked_and_original_pcs_share_one_timeline() {
    let (_dir, cloud) = cloud();
    let cracked = Pc::new("a", "Desktop", GameKind::Emulator);
    let original = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    cracked.write_save("0110", "v1");
    put(&cracked.emulator_file(), "{}");
    cracked
        .store
        .record_unlock(APP, "ACH_1", 1, 1, "gse")
        .unwrap();
    cracked.reconcile(&cloud, 100);
    original.reconcile(&cloud, 200);
    assert_eq!(original.reconcile(&cloud, 210), SyncEvent::Nothing);

    original.write_save("0110", "v2 from steam");
    assert!(matches!(
        original.reconcile(&cloud, 300),
        SyncEvent::Pushed { .. }
    ));
    assert!(matches!(
        cracked.reconcile(&cloud, 400),
        SyncEvent::Restored { .. }
    ));
    assert_eq!(cracked.read_save("0110"), "v2 from steam");
    assert!(cracked.emulator_file().exists());
    assert!(cracked.store.is_unlocked(APP, "ACH_1").unwrap());
    assert_eq!(cracked.reconcile(&cloud, 500), SyncEvent::Nothing);
    assert_eq!(original.reconcile(&cloud, 600), SyncEvent::Nothing);
}

#[test]
fn no_local_state_disappears_without_a_snapshot() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "a1");
    a.reconcile(&cloud, 100);
    b.write_save("1", "b own");
    b.reconcile(&cloud, 200);
    a.write_save("1", "a2");
    a.reconcile(&cloud, 300);
    b.write_save("1", "b changed");
    b.reconcile(&cloud, 350);
    a.reconcile(&cloud, 400);
    undo(&b.ctx(&cloud, 500), &b.target()).unwrap();
    a.reconcile(&cloud, 600);
    b.reconcile(&cloud, 700);
    assert_nothing_lost(&cloud, &[&a, &b]);
}

#[test]
fn missing_cloud_root_is_reported_and_nothing_runs() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1", "keep me");
    std::fs::remove_dir_all(cloud.root()).unwrap();
    let result = reconcile(&a.ctx(&cloud, 1), &a.target(), 1);
    assert!(matches!(result, Err(CloudError::MissingRoot)));
    assert_eq!(a.read_save("1"), "keep me");
    assert!(!cloud.root().exists());
}

#[test]
fn ds3_with_loose_config_restores_on_a_formatted_pc() {
    let manifest =
        aw_core::cloud::manifest::parse_manifest(include_str!("ludusavi/manifest-sample.yaml"))
            .unwrap();
    let ds3 = manifest.games[&APP].clone();
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Desktop", GameKind::SteamOriginal);
    for pc in [&a, &b] {
        put(
            &pc.app_data()
                .join("DarkSoulsIII")
                .join("GraphicsConfig.xml"),
            "<config/>",
        );
    }
    let located = |pc: &Pc| {
        let mut target = pc.target();
        let input = aw_core::cloud::locate::LocateInput {
            app_id: APP,
            kind: GameKind::SteamOriginal,
            manifest: Some(&ds3),
            emulator_files: &[],
            sources: &[],
            manual: None,
            placeholders: &target.placeholders,
        };
        let aw_core::cloud::locate::Located::Roots(roots) = aw_core::cloud::locate::locate(&input)
        else {
            panic!("no roots");
        };
        target.roots = roots;
        target
    };
    a.write_save("0110AAAA", "87 hours");
    let ta = located(&a);
    assert!(matches!(
        reconcile(&a.ctx(&cloud, 100), &ta, 100).unwrap(),
        SyncEvent::Pushed { .. }
    ));
    let snaps = snapshots(&cloud);
    assert!(
        snaps[0]
            .1
            .files
            .iter()
            .all(|f| !f.original.as_str().ends_with("GraphicsConfig.xml"))
    );

    b.write_save("0110BBBB", "new game");
    let tb = located(&b);
    let event = reconcile(&b.ctx(&cloud, 200), &tb, 200).unwrap();
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    assert_eq!(b.read_save("0110AAAA"), "87 hours");
    assert_eq!(b.read_save("0110BBBB"), "new game");
    assert_eq!(
        reconcile(&b.ctx(&cloud, 300), &tb, 300).unwrap(),
        SyncEvent::Nothing
    );
    let leftovers: Vec<_> = std::fs::read_dir(b.app_data().join("DarkSoulsIII"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".awtmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn formatted_emulator_pc_pre_restored_as_steam_keeps_cloud_achievements() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::Emulator);
    a.write_save("0110", "50 hours");
    put(&a.emulator_file(), r#"{"A":{},"B":{}}"#);
    a.store.record_unlock(APP, "A", 1, 1, "gse").unwrap();
    a.store.record_unlock(APP, "B", 2, 2, "gse").unwrap();
    a.store.add_playtime(APP, 180_000, 3).unwrap();
    a.reconcile(&cloud, 100);

    let b = Pc::new("b", "Desktop", GameKind::SteamOriginal);
    assert!(matches!(
        b.reconcile(&cloud, 200),
        SyncEvent::Restored { .. }
    ));
    let b = Pc {
        kind: GameKind::Emulator,
        ..b
    };
    put(&b.emulator_file(), r#"{"C":{}}"#);
    b.store.record_unlock(APP, "C", 300, 300, "gse").unwrap();
    b.store.add_playtime(APP, 600, 300).unwrap();
    let event = b.reconcile(&cloud, 400);
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    let names: Vec<String> = b
        .store
        .game_state(APP)
        .unwrap()
        .unlocks
        .into_iter()
        .map(|u| u.api_name)
        .collect();
    assert_eq!(names, vec!["A", "B"]);
    assert_eq!(b.store.aw_playtime_seconds(APP).unwrap(), 180_000);
    assert_eq!(a.reconcile(&cloud, 500), SyncEvent::Nothing);
    assert_eq!(a.store.aw_playtime_seconds(APP).unwrap(), 180_000);
}

fn steam_remote_target(pc: &Pc) -> GameTarget {
    GameTarget {
        key: CloudKey::Steam(APP),
        name: "Dark Souls III".into(),
        app_id: APP,
        kind: GameKind::SteamOriginal,
        roots: vec![root(
            Category::Save,
            "<steamRoot>/userdata/<storeUserId>/374320/remote",
        )],
        placeholders: Placeholders {
            folders: Some(KnownFolders::under(pc.root.path())),
            install_dir: None,
            steam_root: Some(pc.root.path().join("Steam")),
        },
    }
}

#[test]
fn emulator_remote_save_lands_in_steam_userdata_without_looping() {
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
    a.reconcile(&cloud, 100);

    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    let account = b.root.path().join("Steam").join("userdata").join("999");
    std::fs::create_dir_all(&account).unwrap();
    let target = steam_remote_target(&b);
    let event = reconcile(&b.ctx(&cloud, 200), &target, 200).unwrap();
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    assert_eq!(
        std::fs::read_to_string(account.join("374320").join("remote").join("slot.dat")).unwrap(),
        "cloud"
    );
    assert!(!b.app_data().join("GSE Saves").exists());
    assert_eq!(
        reconcile(&b.ctx(&cloud, 300), &target, 300).unwrap(),
        SyncEvent::Nothing
    );
}

#[test]
fn snapshot_with_nothing_for_this_pc_does_not_restore_forever() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::Emulator);
    put(&a.emulator_file(), "{}");
    a.store.record_unlock(APP, "A", 1, 1, "gse").unwrap();
    a.reconcile(&cloud, 100);

    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    let target = b.target();
    reconcile(&b.ctx(&cloud, 200), &target, 200).unwrap();
    assert_eq!(
        reconcile(&b.ctx(&cloud, 300), &target, 300).unwrap(),
        SyncEvent::Nothing
    );
    assert_eq!(
        reconcile(&b.ctx(&cloud, 400), &target, 400).unwrap(),
        SyncEvent::Nothing
    );
}

#[test]
fn interrupted_restore_is_retried_instead_of_branching() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1", "v1");
    a.reconcile(&cloud, 100);
    b.reconcile(&cloud, 110);
    a.write_save("1", "v2");
    let SyncEvent::Pushed { snapshot } = a.reconcile(&cloud, 200) else {
        panic!()
    };
    b.store.meta_set("cloud_pending:374320", &snapshot).unwrap();
    b.write_save("1", "half written");
    let event = b.reconcile(&cloud, 300);
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    assert_eq!(b.read_save("1"), "v2");
    assert_eq!(b.reconcile(&cloud, 400), SyncEvent::Nothing);
}

#[test]
fn switching_versions_back_and_forth_creates_no_extra_snapshot() {
    let (_dir, cloud) = cloud();
    let desktop = Pc::new("a", "Desktop", GameKind::Emulator);
    put(&desktop.emulator_file(), "{}");
    desktop.write_save("0110", "base");
    desktop.reconcile(&cloud, 100);

    let notebook = Pc::new("b", "Notebook", GameKind::Emulator);
    notebook.reconcile(&cloud, 110);
    notebook.write_save("0110", "notebook");
    notebook.store.add_playtime(APP, 10_800, 120).unwrap();
    let SyncEvent::Pushed {
        snapshot: notebook_version,
    } = notebook.reconcile(&cloud, 130)
    else {
        panic!()
    };

    desktop.write_save("0110", "desktop");
    let SyncEvent::Branched {
        principal: desktop_version,
        ..
    } = desktop.reconcile(&cloud, 140)
    else {
        panic!()
    };
    let before = snapshots(&cloud).len();

    let ctx = desktop.ctx(&cloud, 200);
    let picked = set_principal(
        &ctx,
        &desktop.target(),
        &notebook_version,
        HeadReason::Manual,
    )
    .unwrap();
    assert!(matches!(picked, SyncEvent::Restored { .. }), "{picked:?}");
    assert_eq!(desktop.read_save("0110"), "notebook");
    assert_eq!(desktop.reconcile(&cloud, 210), SyncEvent::Nothing);

    let ctx = desktop.ctx(&cloud, 300);
    set_principal(
        &ctx,
        &desktop.target(),
        &desktop_version,
        HeadReason::Manual,
    )
    .unwrap();
    assert_eq!(desktop.read_save("0110"), "desktop");
    assert_eq!(snapshots(&cloud).len(), before);
}

#[test]
fn different_account_folders_do_not_spawn_a_version_on_every_pull() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1111", "v1");
    a.reconcile(&cloud, 100);
    b.write_save("2222", "fresh");
    b.reconcile(&cloud, 200);
    assert_eq!(b.read_save("1111"), "v1");

    let before = snapshots(&cloud).len();
    a.write_save("1111", "v2");
    a.reconcile(&cloud, 300);
    let event = b.reconcile(&cloud, 400);
    assert!(
        matches!(
            event,
            SyncEvent::Restored {
                kept_local: None,
                ..
            }
        ),
        "{event:?}"
    );
    assert_eq!(b.read_save("1111"), "v2");
    assert_eq!(b.read_save("2222"), "fresh");
    assert_eq!(snapshots(&cloud).len(), before + 1);
}

#[test]
fn restore_into_a_pc_with_two_other_accounts_touches_neither() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("3333", "desktop");
    a.reconcile(&cloud, 100);

    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    b.write_save("1111", "other user");
    let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(b.save_path("1111"))
        .unwrap()
        .set_modified(hour_ago)
        .unwrap();
    b.write_save("2222", "mine");
    let event = b.reconcile(&cloud, 200);
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    assert_eq!(b.read_save("3333"), "desktop");
    assert_eq!(b.read_save("2222"), "mine");
    assert_eq!(b.read_save("1111"), "other user");
    assert_nothing_lost(&cloud, &[&a, &b]);
}

#[test]
fn a_save_always_lands_in_its_own_account_folder() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1111", "account 1111 save");
    a.reconcile(&cloud, 100);

    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    b.write_save("2222", "account 2222 save");
    b.reconcile(&cloud, 200);
    assert_eq!(b.read_save("1111"), "account 1111 save");
    assert_eq!(b.read_save("2222"), "account 2222 save");
    assert_nothing_lost(&cloud, &[&a, &b]);
}

fn with_extra_root(pc: &Pc) -> GameTarget {
    let mut target = pc.target();
    target
        .roots
        .push(root(Category::Save, "<winDocuments>/Dark Souls III"));
    target
}

#[test]
fn changing_the_watched_folders_keeps_new_local_play() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1111", "chapter 7");
    a.reconcile(&cloud, 100);

    a.write_save("1111", "chapter 9");
    let event = reconcile(&a.ctx(&cloud, 200), &with_extra_root(&a), 200).unwrap();
    assert!(matches!(event, SyncEvent::Pushed { .. }), "{event:?}");
    assert_eq!(a.read_save("1111"), "chapter 9");
}

#[test]
fn changing_the_watched_folders_while_the_cloud_moved_branches() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1111", "chapter 7");
    a.reconcile(&cloud, 100);
    b.reconcile(&cloud, 110);
    b.write_save("1111", "notebook chapter 8");
    b.reconcile(&cloud, 150);

    a.write_save("1111", "desktop chapter 9");
    let event = reconcile(&a.ctx(&cloud, 200), &with_extra_root(&a), 200).unwrap();
    assert!(
        matches!(
            event,
            SyncEvent::Branched {
                local_is_principal: true,
                ..
            }
        ),
        "{event:?}"
    );
    assert_eq!(a.read_save("1111"), "desktop chapter 9");
}

#[test]
fn changing_the_watched_folders_with_an_unchanged_save_does_nothing_visible() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    a.write_save("1111", "chapter 7");
    a.reconcile(&cloud, 100);
    let before = snapshots(&cloud).len();

    reconcile(&a.ctx(&cloud, 200), &with_extra_root(&a), 200).unwrap();
    assert_eq!(a.read_save("1111"), "chapter 7");
    assert_eq!(snapshots(&cloud).len(), before);
    assert_eq!(
        reconcile(&a.ctx(&cloud, 300), &with_extra_root(&a), 300).unwrap(),
        SyncEvent::Nothing
    );
}

fn unlocks(pc: &Pc) -> Vec<String> {
    pc.store
        .game_state(APP)
        .unwrap()
        .unlocks
        .into_iter()
        .map(|u| u.api_name)
        .collect()
}

#[test]
fn cracked_pc_takes_the_originals_save_and_the_last_cracked_achievements() {
    let (_dir, cloud) = cloud();
    let pc2 = Pc::new("pc2", "Cracked 2", GameKind::Emulator);
    pc2.write_save("1111", "chapter 1");
    pc2.store.record_unlock(APP, "Z", 1, 1, "gse").unwrap();
    pc2.reconcile(&cloud, 100);

    let pc1 = Pc::new("pc1", "Cracked 1", GameKind::Emulator);
    pc1.reconcile(&cloud, 200);
    pc1.write_save("1111", "chapter 5");
    pc1.store.record_unlock(APP, "A", 2, 2, "gse").unwrap();
    pc1.store.record_unlock(APP, "B", 3, 3, "gse").unwrap();
    pc1.reconcile(&cloud, 300);

    let pc3 = Pc::new("pc3", "Original", GameKind::SteamOriginal);
    pc3.store
        .record_unlock(APP, "STEAM_ONLY", 4, 4, "steam")
        .unwrap();
    pc3.reconcile(&cloud, 400);
    assert_eq!(pc3.read_save("1111"), "chapter 5");
    pc3.write_save("1111", "chapter 7");
    assert!(matches!(
        pc3.reconcile(&cloud, 500),
        SyncEvent::Pushed { .. }
    ));
    assert_eq!(unlocks(&pc3), ["STEAM_ONLY"]);

    let event = pc2.reconcile(&cloud, 600);
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    assert_eq!(pc2.read_save("1111"), "chapter 7");
    assert_eq!(unlocks(&pc2), ["A", "B", "Z"]);
    assert_eq!(pc2.reconcile(&cloud, 700), SyncEvent::Nothing);
    assert_nothing_lost(&cloud, &[&pc1, &pc3]);
    let before_pull = pc2.seen.borrow()[0].clone();
    assert!(
        snapshots(&cloud)
            .iter()
            .any(|(_, meta)| comparable_digest(meta, GameKind::Emulator) == before_pull)
    );
}

#[test]
fn cracked_pc_keeps_its_achievements_when_no_cracked_version_exists() {
    let (_dir, cloud) = cloud();
    let original = Pc::new("o", "Original", GameKind::SteamOriginal);
    original.write_save("1111", "chapter 3");
    original.reconcile(&cloud, 100);

    let cracked = Pc::new("c", "Cracked", GameKind::Emulator);
    cracked.store.record_unlock(APP, "Z", 1, 1, "gse").unwrap();
    let event = cracked.reconcile(&cloud, 200);
    assert!(matches!(event, SyncEvent::Restored { .. }), "{event:?}");
    assert_eq!(cracked.read_save("1111"), "chapter 3");
    assert_eq!(unlocks(&cracked), ["Z"]);
    assert_eq!(cracked.reconcile(&cloud, 300), SyncEvent::Nothing);
}

#[test]
fn cracked_pcs_agree_on_achievements_even_when_the_originals_line_never_had_any() {
    let (_dir, cloud) = cloud();
    let original = Pc::new("o", "Original", GameKind::SteamOriginal);
    original.write_save("1111", "chapter 1");
    original.reconcile(&cloud, 100);

    let first = Pc::new("c1", "Cracked 1", GameKind::Emulator);
    first.store.record_unlock(APP, "A", 1, 1, "gse").unwrap();
    first.reconcile(&cloud, 200);
    first.write_save("1111", "cracked side line");
    first.store.record_unlock(APP, "B", 2, 2, "gse").unwrap();
    first.reconcile(&cloud, 300);

    original.write_save("1111", "chapter 2");
    let ctx = original.ctx(&cloud, 400);
    let first_original = snapshots(&cloud)[0].1.id.clone();
    set_principal(
        &ctx,
        &original.target(),
        &first_original,
        HeadReason::Manual,
    )
    .unwrap();
    original.write_save("1111", "chapter 2");
    original.reconcile(&cloud, 500);

    let second = Pc::new("c2", "Cracked 2", GameKind::Emulator);
    second.store.record_unlock(APP, "Z", 1, 1, "gse").unwrap();
    second.reconcile(&cloud, 600);
    first.reconcile(&cloud, 700);
    assert_eq!(first.read_save("1111"), "chapter 2");
    assert_eq!(second.read_save("1111"), "chapter 2");
    assert_eq!(unlocks(&first), ["A", "B"]);
    assert_eq!(unlocks(&second), ["A", "B"]);
}

#[test]
fn cancelled_pull_leaves_the_pc_as_it_was_and_retries_later() {
    let (_dir, cloud) = cloud();
    let a = Pc::new("a", "Desktop", GameKind::SteamOriginal);
    let b = Pc::new("b", "Notebook", GameKind::SteamOriginal);
    a.write_save("1111", "v1");
    a.reconcile(&cloud, 100);
    b.reconcile(&cloud, 110);
    a.write_save("1111", "v2");
    a.reconcile(&cloud, 200);

    let cancel = aw_core::cloud::CancelGate::default();
    cancel.cancel();
    let result = reconcile_cancellable(&b.ctx(&cloud, 300), &b.target(), 300, &cancel);
    assert!(matches!(result, Err(CloudError::Cancelled)), "{result:?}");
    assert_eq!(b.read_save("1111"), "v1");
    assert!(matches!(
        b.reconcile(&cloud, 400),
        SyncEvent::Restored { .. }
    ));
    assert_eq!(b.read_save("1111"), "v2");
}
