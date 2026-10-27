use std::path::{Path, PathBuf};

use crate::cloud::CloudResult;
use crate::cloud::capture::file_sha256;
use crate::cloud::layout::{CloudLayout, sanitize_name};
use crate::cloud::utc_stamp;
use crate::discovery::folders::{Base, KnownFolders};
use crate::store::Store;

pub struct IdentityDef {
    pub emulator: &'static str,
    pub base: Base,
    pub dir: &'static str,
    pub files: &'static [&'static str],
}

pub const IDENTITIES: &[IdentityDef] = &[
    IdentityDef {
        emulator: "goldberg",
        base: Base::RoamingAppData,
        dir: "Goldberg SteamEmu Saves/settings",
        files: &["user_steam_id.txt", "account_name.txt"],
    },
    IdentityDef {
        emulator: "gse",
        base: Base::RoamingAppData,
        dir: "GSE Saves/settings",
        files: &["configs.user.ini"],
    },
];

const REPLACED: &str = "replaced";

fn local_path(def: &IdentityDef, folders: &KnownFolders, file: &str) -> Option<PathBuf> {
    let base = folders.resolve(def.base)?;
    Some(
        def.dir
            .split('/')
            .fold(base.to_path_buf(), |dir, part| dir.join(part))
            .join(file),
    )
}

fn replaced_stamp(device_name: &str, now: i64) -> String {
    format!(
        "{}_{}",
        utc_stamp(now),
        sanitize_name(device_name).to_lowercase().replace(' ', "-")
    )
}

fn meta_key(def: &IdentityDef, file: &str) -> String {
    format!("cloud_identity:{}:{file}", def.emulator)
}

fn hash(path: &Path) -> Option<String> {
    path.is_file().then(|| file_sha256(path).ok()).flatten()
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(from, to).map(|_| ())
}

pub fn restore_identities(
    layout: &CloudLayout,
    folders: &KnownFolders,
    store: &Store,
    device_name: &str,
    now: i64,
) -> CloudResult<Vec<&'static str>> {
    let mut restored = Vec::new();
    for def in IDENTITIES {
        let cloud_dir = layout.emulator_dir(def.emulator);
        let mut changed = false;
        for file in def.files {
            let cloud = cloud_dir.join(file);
            let (Some(cloud_hash), Some(local)) = (hash(&cloud), local_path(def, folders, file))
            else {
                continue;
            };
            let local_hash = hash(&local);
            if local_hash.as_deref() != Some(cloud_hash.as_str()) {
                if local_hash.is_some() {
                    copy(
                        &local,
                        &cloud_dir
                            .join(REPLACED)
                            .join(replaced_stamp(device_name, now))
                            .join(file),
                    )?;
                }
                copy(&cloud, &local)?;
                changed = true;
            }
            store.meta_set(&meta_key(def, file), &cloud_hash)?;
        }
        if changed {
            restored.push(def.emulator);
        }
    }
    Ok(restored)
}

pub fn push_identities(
    layout: &CloudLayout,
    folders: &KnownFolders,
    store: &Store,
    device_name: &str,
    now: i64,
) -> CloudResult<Vec<&'static str>> {
    let mut pushed = Vec::new();
    for def in IDENTITIES {
        let cloud_dir = layout.emulator_dir(def.emulator);
        let mut changed = false;
        for file in def.files {
            let Some(local) = local_path(def, folders, file) else {
                continue;
            };
            let Some(local_hash) = hash(&local) else {
                continue;
            };
            let cloud = cloud_dir.join(file);
            let last = store.meta_get(&meta_key(def, file))?;
            if !cloud.is_file() || last.as_deref() != Some(local_hash.as_str()) {
                if hash(&cloud).is_some_and(|existing| existing != local_hash) {
                    copy(
                        &cloud,
                        &cloud_dir
                            .join(REPLACED)
                            .join(replaced_stamp(device_name, now))
                            .join(file),
                    )?;
                }
                copy(&local, &cloud)?;
                store.meta_set(&meta_key(def, file), &local_hash)?;
                changed = true;
            }
        }
        if changed {
            pushed.push(def.emulator);
        }
    }
    Ok(pushed)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        dir: tempfile::TempDir,
        layout: CloudLayout,
        folders: KnownFolders,
        store: Store,
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let layout = CloudLayout::new(dir.path().join("cloud"));
        let folders = KnownFolders::under(&dir.path().join("pc"));
        World {
            dir,
            layout,
            folders,
            store: Store::in_memory().unwrap(),
        }
    }

    fn local_file(w: &World) -> PathBuf {
        w.dir
            .path()
            .join("pc")
            .join("RoamingAppData")
            .join("Goldberg SteamEmu Saves")
            .join("settings")
            .join("user_steam_id.txt")
    }

    fn cloud_file(w: &World) -> PathBuf {
        w.layout.emulator_dir("goldberg").join("user_steam_id.txt")
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn new_pc_gets_the_old_identity_and_keeps_its_own_copy() {
        let w = world();
        put(&cloud_file(&w), "76561197960287930");
        put(&local_file(&w), "76561197960000000");
        let restored =
            restore_identities(&w.layout, &w.folders, &w.store, "Notebook", 1_793_050_200).unwrap();
        assert_eq!(restored, vec!["goldberg"]);
        assert_eq!(read(&local_file(&w)), "76561197960287930");
        let backup = w
            .layout
            .emulator_dir("goldberg")
            .join("replaced")
            .join("2026-10-26_21-30_notebook")
            .join("user_steam_id.txt");
        assert_eq!(read(&backup), "76561197960000000");
    }

    #[test]
    fn identical_identity_is_not_backed_up() {
        let w = world();
        put(&cloud_file(&w), "same");
        put(&local_file(&w), "same");
        let restored = restore_identities(&w.layout, &w.folders, &w.store, "PC", 1).unwrap();
        assert!(restored.is_empty());
        assert!(!w.layout.emulator_dir("goldberg").join("replaced").exists());
    }

    #[test]
    fn missing_local_identity_is_created() {
        let w = world();
        put(&cloud_file(&w), "id");
        restore_identities(&w.layout, &w.folders, &w.store, "PC", 1).unwrap();
        assert_eq!(read(&local_file(&w)), "id");
    }

    #[test]
    fn push_only_when_cloud_is_empty_or_local_changed() {
        let w = world();
        put(&local_file(&w), "A");
        assert_eq!(
            push_identities(&w.layout, &w.folders, &w.store, "PC", 1).unwrap(),
            vec!["goldberg"]
        );
        assert_eq!(read(&cloud_file(&w)), "A");
        assert!(
            push_identities(&w.layout, &w.folders, &w.store, "PC", 1)
                .unwrap()
                .is_empty()
        );
        put(&cloud_file(&w), "Z");
        assert!(
            push_identities(&w.layout, &w.folders, &w.store, "PC", 1)
                .unwrap()
                .is_empty()
        );
        assert_eq!(read(&cloud_file(&w)), "Z");
        put(&local_file(&w), "C");
        assert_eq!(
            push_identities(&w.layout, &w.folders, &w.store, "PC", 1).unwrap(),
            vec!["goldberg"]
        );
        assert_eq!(read(&cloud_file(&w)), "C");
    }

    #[test]
    fn restore_does_not_bounce_back_on_next_push() {
        let w = world();
        put(&cloud_file(&w), "cloud-id");
        put(&local_file(&w), "local-id");
        restore_identities(&w.layout, &w.folders, &w.store, "PC", 1).unwrap();
        assert!(
            push_identities(&w.layout, &w.folders, &w.store, "PC", 1)
                .unwrap()
                .is_empty()
        );
        assert_eq!(read(&cloud_file(&w)), "cloud-id");
    }

    #[test]
    fn push_keeps_the_identity_it_replaces() {
        let w = world();
        put(&local_file(&w), "A");
        push_identities(&w.layout, &w.folders, &w.store, "Desktop", 1).unwrap();
        put(&cloud_file(&w), "Z");
        put(&local_file(&w), "C");
        push_identities(&w.layout, &w.folders, &w.store, "Desktop", 1_793_050_200).unwrap();
        assert_eq!(read(&cloud_file(&w)), "C");
        let backup = w
            .layout
            .emulator_dir("goldberg")
            .join("replaced")
            .join("2026-10-26_21-30_desktop")
            .join("user_steam_id.txt");
        assert_eq!(read(&backup), "Z");
    }
}
