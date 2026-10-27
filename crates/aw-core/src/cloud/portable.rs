use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::discovery::folders::KnownFolders;

const SEPARATOR: char = '/';
const BACKSLASH: char = '\u{5c}';
const WILDCARD: char = '*';

const TOKENS: [&str; 10] = [
    "<winAppData>",
    "<winLocalAppData>",
    "<winLocalAppDataLow>",
    "<winDocuments>",
    "<winPublic>",
    "<winProgramData>",
    "<winDir>",
    "<home>",
    "<base>",
    "<steamRoot>",
];

#[derive(Debug, Clone, Default)]
pub struct Placeholders {
    pub folders: Option<KnownFolders>,
    pub install_dir: Option<PathBuf>,
    pub steam_root: Option<PathBuf>,
}

impl Placeholders {
    fn resolve(&self, token: &str) -> Option<&Path> {
        let folders = self.folders.as_ref();
        match token {
            "<winAppData>" => folders?.roaming_app_data.as_deref(),
            "<winLocalAppData>" => folders?.local_app_data.as_deref(),
            "<winLocalAppDataLow>" => folders?.local_app_data_low.as_deref(),
            "<winDocuments>" => folders?.documents.as_deref(),
            "<winPublic>" => folders?.public.as_deref(),
            "<winProgramData>" => folders?.program_data.as_deref(),
            "<winDir>" => folders?.windows.as_deref(),
            "<home>" => folders?.profile.as_deref(),
            "<base>" => self.install_dir.as_deref(),
            "<steamRoot>" => self.steam_root.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct PortablePath(String);

impl From<PortablePath> for String {
    fn from(path: PortablePath) -> String {
        path.0
    }
}

impl TryFrom<String> for PortablePath {
    type Error = String;

    fn try_from(text: String) -> Result<PortablePath, String> {
        PortablePath::new(&text).ok_or_else(|| format!("invalid portable path: {text}"))
    }
}

const ACCOUNT_TOKENS: [&str; 2] = ["<storeUserId>", "<storeGameId>"];

fn valid_segment(segment: &str) -> bool {
    if ACCOUNT_TOKENS.contains(&segment) {
        return true;
    }
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.contains([':', '<', '>', '|', '"', '?'])
        && !segment.chars().any(char::is_control)
}

impl PortablePath {
    pub fn new(text: &str) -> Option<PortablePath> {
        let text = forward_slashes(text);
        let text = text.trim_end_matches(SEPARATOR);
        let mut segments = text.split(SEPARATOR);
        let token = segments.next()?;
        (TOKENS.contains(&token) && segments.all(valid_segment))
            .then(|| PortablePath(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn from_absolute(path: &Path, ph: &Placeholders) -> Option<PortablePath> {
        let parts = components(path);
        let (token, depth) = TOKENS
            .iter()
            .filter_map(|token| {
                let base = components(ph.resolve(token)?);
                let inside = !base.is_empty()
                    && base.len() <= parts.len()
                    && base
                        .iter()
                        .zip(&parts)
                        .all(|(a, b)| a.to_lowercase() == b.to_lowercase());
                inside.then_some((*token, base.len()))
            })
            .max_by_key(|(_, depth)| *depth)?;
        let tail = parts[depth..].join("/");
        let text = if tail.is_empty() {
            token.to_owned()
        } else {
            format!("{token}/{tail}")
        };
        Some(PortablePath(text))
    }

    pub fn to_absolute(&self, ph: &Placeholders) -> Option<PathBuf> {
        let mut parts = self.0.split(SEPARATOR);
        let base = ph.resolve(parts.next()?)?;
        Some(
            parts
                .filter(|part| !part.is_empty())
                .fold(base.to_path_buf(), |dir, part| dir.join(part)),
        )
    }

    pub fn join(&self, tail: &str) -> PortablePath {
        let tail = forward_slashes(tail);
        let tail = tail.trim_matches(SEPARATOR);
        if tail.is_empty() {
            return self.clone();
        }
        PortablePath::new(&format!("{}/{tail}", self.0)).unwrap_or_else(|| self.clone())
    }

    pub fn has_wildcard(&self) -> bool {
        self.0.contains(WILDCARD) || ACCOUNT_TOKENS.iter().any(|token| self.0.contains(token))
    }
}

fn components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|part| match part {
            Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().into_owned()),
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            Component::RootDir | Component::CurDir | Component::ParentDir => None,
        })
        .collect()
}

fn forward_slashes(text: &str) -> String {
    text.replace(BACKSLASH, "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(root: &Path) -> Placeholders {
        Placeholders {
            folders: Some(KnownFolders::under(root)),
            ..Default::default()
        }
    }

    #[test]
    fn most_specific_token_wins() {
        let root = tempfile::tempdir().unwrap();
        let ph = placeholders(root.path());
        let abs = root
            .path()
            .join("RoamingAppData")
            .join("DarkSoulsIII")
            .join("0110000100000001");
        let p = PortablePath::from_absolute(&abs, &ph).unwrap();
        assert_eq!(p.as_str(), "<winAppData>/DarkSoulsIII/0110000100000001");
        assert_eq!(p.to_absolute(&ph).unwrap(), abs);
    }

    #[test]
    fn longest_prefix_beats_home() {
        let root = tempfile::tempdir().unwrap();
        let mut folders = KnownFolders::under(root.path());
        folders.profile = Some(root.path().to_path_buf());
        let ph = Placeholders {
            folders: Some(folders),
            ..Default::default()
        };
        let abs = root
            .path()
            .join("LocalAppDataLow")
            .join("Game")
            .join("s.dat");
        assert_eq!(
            PortablePath::from_absolute(&abs, &ph).unwrap().as_str(),
            "<winLocalAppDataLow>/Game/s.dat"
        );
        let other = root.path().join("Saved Games").join("x");
        assert_eq!(
            PortablePath::from_absolute(&other, &ph).unwrap().as_str(),
            "<home>/Saved Games/x"
        );
    }

    #[test]
    fn portable_round_trips_accents_and_spaces() {
        let root = tempfile::tempdir().unwrap();
        let ph = placeholders(root.path());
        let p = PortablePath::new("<winDocuments>/Meus Jogos/João Save").unwrap();
        let abs = p.to_absolute(&ph).unwrap();
        assert_eq!(
            abs,
            root.path()
                .join("Documents")
                .join("Meus Jogos")
                .join("João Save")
        );
        assert_eq!(PortablePath::from_absolute(&abs, &ph).unwrap(), p);
    }

    #[test]
    fn comparison_ignores_case() {
        let root = tempfile::tempdir().unwrap();
        let ph = placeholders(root.path());
        let upper = PathBuf::from(
            root.path()
                .join("ROAMINGAPPDATA")
                .join("Game")
                .to_string_lossy()
                .into_owned(),
        );
        assert_eq!(
            PortablePath::from_absolute(&upper, &ph).unwrap().as_str(),
            "<winAppData>/Game"
        );
    }

    #[test]
    fn install_dir_and_steam_root_tokens() {
        let root = tempfile::tempdir().unwrap();
        let ph = Placeholders {
            folders: None,
            install_dir: Some(root.path().join("Games").join("DS3")),
            steam_root: Some(root.path().join("Steam")),
        };
        let save = root.path().join("Games").join("DS3").join("save.dat");
        assert_eq!(
            PortablePath::from_absolute(&save, &ph).unwrap().as_str(),
            "<base>/save.dat"
        );
        let remote = PortablePath::new("<steamRoot>/userdata/1/374320/remote").unwrap();
        assert_eq!(
            remote.to_absolute(&ph).unwrap(),
            root.path()
                .join("Steam")
                .join("userdata")
                .join("1")
                .join("374320")
                .join("remote")
        );
    }

    #[test]
    fn missing_token_target_is_none() {
        let p = PortablePath::new("<base>/x").unwrap();
        assert_eq!(p.to_absolute(&Placeholders::default()), None);
    }

    #[test]
    fn outside_every_token_is_none() {
        let root = tempfile::tempdir().unwrap();
        let ph = placeholders(&root.path().join("pc"));
        assert_eq!(
            PortablePath::from_absolute(&root.path().join("Elsewhere").join("x"), &ph),
            None
        );
    }

    #[test]
    fn unknown_token_is_rejected() {
        assert!(PortablePath::new("<nope>/x").is_none());
        assert!(PortablePath::new("C:/x").is_none());
        assert!(PortablePath::new("<winAppData>").is_some());
    }

    #[test]
    fn join_and_wildcard() {
        let p = PortablePath::new("<winAppData>/DarkSoulsIII")
            .unwrap()
            .join("*");
        assert_eq!(p.as_str(), "<winAppData>/DarkSoulsIII/*");
        assert!(p.has_wildcard());
        assert!(!PortablePath::new("<winAppData>/x").unwrap().has_wildcard());
    }

    #[test]
    fn traversal_and_drive_segments_are_rejected() {
        assert!(PortablePath::new("<winAppData>/../Startup/x.exe").is_none());
        assert!(PortablePath::new("<winAppData>/a/./b").is_none());
        assert!(PortablePath::new("<winAppData>/C:/x").is_none());
        assert!(PortablePath::new("<winAppData>/a/<osUserName>").is_none());
        assert!(PortablePath::new("<winAppData>/a/<storeUserId>/b").is_some());
        assert!(serde_json::from_str::<PortablePath>("\"<winAppData>/../x\"").is_err());
        assert!(serde_json::from_str::<PortablePath>("\"<winAppData>/ok\"").is_ok());
    }
}
