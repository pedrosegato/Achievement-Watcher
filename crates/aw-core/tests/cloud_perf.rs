use std::path::PathBuf;
use std::time::{Duration, Instant};

use aw_core::cloud::capture::HashCache;
use aw_core::cloud::device::{DeviceId, LocalDevice};
use aw_core::cloud::key::{CloudKey, GameKind};
use aw_core::cloud::layout::CloudLayout;
use aw_core::cloud::locate::{Category, SaveRoot};
use aw_core::cloud::portable::{Placeholders, PortablePath};
use aw_core::cloud::snapshot::list_snapshots;
use aw_core::cloud::sync::{CloudContext, GameTarget, SyncEvent, reconcile};
use aw_core::discovery::folders::KnownFolders;
use aw_core::store::Store;

const SAVE_BYTES: usize = 30 * 1024 * 1024;
const VERSIONS: usize = 300;
const GAMES: u32 = 40;

fn target(root: &std::path::Path, app_id: u32) -> GameTarget {
    GameTarget {
        key: CloudKey::Steam(app_id),
        name: format!("Game {app_id}"),
        app_id,
        kind: GameKind::SteamOriginal,
        roots: vec![SaveRoot {
            category: Category::Save,
            pattern: PortablePath::new(&format!("<winAppData>/Game{app_id}")).unwrap(),
            single_file: false,
        }],
        placeholders: Placeholders {
            folders: Some(KnownFolders::under(root)),
            ..Default::default()
        },
    }
}

fn save_dir(root: &std::path::Path, app_id: u32) -> PathBuf {
    root.join("RoamingAppData").join(format!("Game{app_id}"))
}

fn time<T>(label: &str, work: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let out = work();
    let took = started.elapsed();
    eprintln!("{label}: {took:?}");
    (out, took)
}

#[test]
#[ignore = "performance measurement, run with --ignored --nocapture"]
fn sync_costs_with_many_versions_and_games() {
    let dir = tempfile::tempdir().unwrap();
    let pc = dir.path().join("pc");
    let layout = CloudLayout::new(dir.path().join("cloud"));
    std::fs::create_dir_all(layout.root()).unwrap();
    let store = Store::in_memory().unwrap();
    let cache = HashCache::default();
    let device = LocalDevice::new(DeviceId("desk".into()), "Desktop");
    let ctx = |now| CloudContext {
        layout: &layout,
        device: &device,
        store: &store,
        cache: &cache,
        now,
    };

    let big = target(&pc, 1);
    std::fs::create_dir_all(save_dir(&pc, 1)).unwrap();
    let mut blob = vec![7u8; SAVE_BYTES];
    for version in 0..VERSIONS {
        blob[0] = (version % 251) as u8;
        blob[1] = (version / 251) as u8;
        std::fs::write(save_dir(&pc, 1).join("slot.sav"), &blob).unwrap();
        std::fs::write(save_dir(&pc, 1).join("small.cfg"), format!("v{version}")).unwrap();
        let now = 1_000 + version as i64 * 100;
        if version == VERSIONS - 1 {
            time("push of a 30 MB save (hash + copy + verify)", || {
                reconcile(&ctx(now), &big, now).unwrap()
            });
        } else {
            reconcile(&ctx(now), &big, now).unwrap();
        }
    }
    let games = layout.game_dirs(&CloudKey::Steam(1));
    let (snaps, _) = time("list 300 versions", || list_snapshots(&games));
    assert_eq!(snaps.len(), VERSIONS);

    let later = 1_000_000;
    let (event, idle) = time("idle reconcile, 300 versions, warm hash cache", || {
        reconcile(&ctx(later), &big, later).unwrap()
    });
    assert_eq!(event, SyncEvent::Nothing);

    for app_id in 2..2 + GAMES {
        std::fs::create_dir_all(save_dir(&pc, app_id)).unwrap();
        std::fs::write(
            save_dir(&pc, app_id).join("save.dat"),
            vec![1u8; 512 * 1024],
        )
        .unwrap();
        reconcile(&ctx(later), &target(&pc, app_id), later).unwrap();
    }
    let (_, tick) = time("idle tick over 41 games", || {
        for app_id in 1..2 + GAMES {
            reconcile(&ctx(later + 10), &target(&pc, app_id), later + 10).unwrap();
        }
    });
    eprintln!("per game: {:?}", tick / (GAMES + 1));
    assert!(
        idle < Duration::from_secs(2),
        "idle reconcile took {idle:?}"
    );
}
