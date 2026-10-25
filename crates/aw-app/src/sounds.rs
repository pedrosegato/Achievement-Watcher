use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PACK: &str = "achievement-watcher";
const EXTENSIONS: [&str; 4] = ["wav", "mp3", "ogg", "flac"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoundEvent {
    Unlock,
    Rare,
    Platinum,
}

impl SoundEvent {
    fn file_stem(self) -> &'static str {
        match self {
            SoundEvent::Unlock => "unlock",
            SoundEvent::Rare => "rare",
            SoundEvent::Platinum => "platinum",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundPack {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub has_rare: bool,
    pub has_platinum: bool,
    #[serde(skip)]
    pub unlock: Option<PathBuf>,
    #[serde(skip)]
    pub rare: Option<PathBuf>,
    #[serde(skip)]
    pub platinum: Option<PathBuf>,
}

impl SoundPack {
    pub fn source(&self, event: SoundEvent) -> Option<PathBuf> {
        match event {
            SoundEvent::Unlock => self.unlock.clone(),
            SoundEvent::Rare => self.rare.clone().or_else(|| self.unlock.clone()),
            SoundEvent::Platinum => self.platinum.clone().or_else(|| self.unlock.clone()),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct PackJson {
    name: Option<String>,
}

pub fn catalog(builtin_dir: &Path, user_dir: &Path) -> Vec<SoundPack> {
    let mut by_id: BTreeMap<String, SoundPack> = BTreeMap::new();
    for (dir, builtin) in [(builtin_dir, true), (user_dir, false)] {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let pack = match entry.file_type() {
                Ok(t) if t.is_dir() => read_pack_dir(&path, builtin),
                Ok(t) if t.is_file() && is_audio(&path) => single_file_pack(&path, builtin),
                _ => None,
            };
            if let Some(pack) = pack {
                by_id.insert(pack.id.to_lowercase(), pack);
            }
        }
    }
    let mut packs: Vec<SoundPack> = by_id.into_values().collect();
    packs.sort_by_key(|p| (!p.builtin, p.name.to_lowercase()));
    packs
}

pub fn find(builtin_dir: &Path, user_dir: &Path, id: &str) -> Option<SoundPack> {
    catalog(builtin_dir, user_dir)
        .into_iter()
        .find(|p| p.id.eq_ignore_ascii_case(id))
}

fn read_pack_dir(dir: &Path, builtin: bool) -> Option<SoundPack> {
    let id = dir.file_name()?.to_string_lossy().into_owned();
    let files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_audio(p))
        .collect();
    let named = |event: SoundEvent| {
        files
            .iter()
            .find(|p| {
                p.file_stem()
                    .is_some_and(|s| s.eq_ignore_ascii_case(event.file_stem()))
            })
            .cloned()
    };
    let unlock = named(SoundEvent::Unlock).or_else(|| {
        let mut sorted = files.clone();
        sorted.sort();
        sorted.into_iter().next()
    })?;
    let rare = named(SoundEvent::Rare);
    let platinum = named(SoundEvent::Platinum);
    let name = std::fs::read(dir.join("pack.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<PackJson>(&b).ok())
        .and_then(|j| j.name)
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| id.clone());
    Some(SoundPack {
        id,
        name,
        builtin,
        has_rare: rare.is_some(),
        has_platinum: platinum.is_some(),
        unlock: Some(unlock),
        rare,
        platinum,
    })
}

fn single_file_pack(path: &Path, builtin: bool) -> Option<SoundPack> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    Some(SoundPack {
        id: stem.clone(),
        name: stem,
        builtin,
        has_rare: false,
        has_platinum: false,
        unlock: Some(path.to_path_buf()),
        rare: None,
        platinum: None,
    })
}

fn is_audio(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        EXTENSIONS
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(dir: &Path, names: &[&str]) {
        std::fs::create_dir_all(dir).unwrap();
        for name in names {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
    }

    #[test]
    fn packs_from_folders_and_loose_files() {
        let tmp = tempfile::tempdir().unwrap();
        let builtin = tmp.path().join("builtin");
        let user = tmp.path().join("user");
        files(&builtin.join("xbox"), &["unlock.wav", "rare.wav"]);
        std::fs::write(builtin.join("xbox").join("pack.json"), r#"{"name":"Xbox"}"#).unwrap();
        files(&builtin.join("ps5"), &["unlock.wav", "platinum.wav"]);
        files(&builtin.join("empty"), &["readme.txt"]);
        files(&user, &["mine.mp3", "notes.md"]);
        files(&user.join("pack"), &["Ding.OGG"]);
        let packs = catalog(&builtin, &user);
        let ids: Vec<(&str, &str, bool, bool, bool)> = packs
            .iter()
            .map(|p| {
                (
                    p.id.as_str(),
                    p.name.as_str(),
                    p.builtin,
                    p.has_rare,
                    p.has_platinum,
                )
            })
            .collect();
        assert_eq!(
            ids,
            vec![
                ("ps5", "ps5", true, false, true),
                ("xbox", "Xbox", true, true, false),
                ("mine", "mine", false, false, false),
                ("pack", "pack", false, false, false),
            ]
        );
    }

    #[test]
    fn events_fall_back_to_unlock_sound() {
        let tmp = tempfile::tempdir().unwrap();
        files(&tmp.path().join("xbox"), &["unlock.wav", "rare.wav"]);
        let pack = find(tmp.path(), &tmp.path().join("u"), "XBOX").unwrap();
        let dir = tmp.path().join("xbox");
        assert_eq!(pack.source(SoundEvent::Rare), Some(dir.join("rare.wav")));
        assert_eq!(
            pack.source(SoundEvent::Platinum),
            Some(dir.join("unlock.wav"))
        );
    }

    #[test]
    fn user_pack_overrides_builtin_with_same_id() {
        let tmp = tempfile::tempdir().unwrap();
        let builtin = tmp.path().join("b");
        let user = tmp.path().join("u");
        files(&builtin.join("xbox"), &["unlock.wav"]);
        files(&user.join("Xbox"), &["unlock.ogg"]);
        let pack = find(&builtin, &user, "xbox").unwrap();
        assert!(!pack.builtin);
    }

    #[test]
    fn real_builtin_packs_have_expected_events() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("sounds");
        let packs = catalog(&dir, &dir.join("none"));
        for id in [DEFAULT_PACK, "xbox"] {
            let pack = packs.iter().find(|p| p.id == id).unwrap();
            assert!(pack.has_rare && pack.has_platinum, "{id}");
        }
        let ps5 = packs.iter().find(|p| p.id == "playstation-5").unwrap();
        assert!(ps5.has_platinum);
        for gone in ["aw-chime", "android", "windows-8"] {
            assert!(packs.iter().all(|p| p.id != gone), "{gone}");
        }
        assert_eq!(packs.len(), 8);
    }
}
