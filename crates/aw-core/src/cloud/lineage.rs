use std::collections::HashSet;

use crate::cloud::snapshot::SnapshotMeta;

pub fn has_children(metas: &[SnapshotMeta], id: &str) -> bool {
    metas.iter().any(|meta| meta.parent.as_deref() == Some(id))
}

pub fn hidden(metas: &[SnapshotMeta], principal: Option<&str>) -> HashSet<String> {
    metas
        .iter()
        .filter_map(|meta| {
            let replaced = meta.replaces.as_deref()?;
            let only_child = metas
                .iter()
                .filter(|other| other.parent.as_deref() == Some(replaced))
                .all(|other| other.id == meta.id);
            (only_child && Some(replaced) != principal).then(|| replaced.to_owned())
        })
        .collect()
}

pub fn history(metas: &[SnapshotMeta], principal: Option<&str>) -> Vec<SnapshotMeta> {
    let hidden = hidden(metas, principal);
    let parent_of = |id: &str| {
        metas
            .iter()
            .find(|meta| meta.id == id)
            .and_then(|meta| meta.parent.clone())
    };
    metas
        .iter()
        .filter(|meta| !hidden.contains(&meta.id))
        .map(|meta| {
            let mut shown = meta.clone();
            let mut seen = HashSet::new();
            while let Some(parent) = shown.parent.clone()
                && hidden.contains(&parent)
                && seen.insert(parent.clone())
            {
                shown.parent = parent_of(&parent);
            }
            shown
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::snapshot::{Contents, FORMAT_VERSION};

    fn snap(id: &str, parent: Option<&str>, replaces: Option<&str>) -> SnapshotMeta {
        SnapshotMeta {
            format: FORMAT_VERSION,
            id: id.into(),
            game_key: "7".into(),
            game_name: "G".into(),
            device_id: "d".into(),
            device_name: "D".into(),
            created_at: 0,
            session_ended_at: 0,
            parent: parent.map(str::to_owned),
            found_on_device: false,
            kept: false,
            contents: Contents::default(),
            files: Vec::new(),
            digest: id.into(),
            playtime_seconds: None,
            unlocked: None,
            local_app_id: 7,
            state_file: None,
            replaces: replaces.map(str::to_owned),
        }
    }

    fn ids(metas: &[SnapshotMeta]) -> Vec<(&str, Option<&str>)> {
        metas
            .iter()
            .map(|meta| (meta.id.as_str(), meta.parent.as_deref()))
            .collect()
    }

    #[test]
    fn a_replaced_tip_is_hidden_and_its_line_skips_it() {
        let metas = [
            snap("a", None, None),
            snap("b", Some("a"), Some("a")),
            snap("c", Some("b"), Some("b")),
        ];
        assert_eq!(
            hidden(&metas, Some("c")),
            HashSet::from(["a".to_owned(), "b".to_owned()])
        );
        assert_eq!(ids(&history(&metas, Some("c"))), vec![("c", None)]);
    }

    #[test]
    fn the_current_version_always_shows() {
        let metas = [snap("a", None, None), snap("b", Some("a"), Some("a"))];
        assert_eq!(
            ids(&history(&metas, Some("a"))),
            vec![("a", None), ("b", Some("a"))]
        );
    }

    #[test]
    fn a_version_with_another_child_stays_as_the_fork_point() {
        let metas = [
            snap("a", None, None),
            snap("b", Some("a"), Some("a")),
            snap("c", Some("a"), None),
        ];
        assert!(hidden(&metas, Some("c")).is_empty());
        assert_eq!(
            ids(&history(&metas, Some("c"))),
            vec![("a", None), ("b", Some("a")), ("c", Some("a"))]
        );
    }

    #[test]
    fn a_new_line_links_to_the_version_it_left_from() {
        let metas = [
            snap("root", None, None),
            snap("a", Some("root"), Some("root")),
            snap("b", Some("a"), Some("a")),
            snap("x", Some("a"), None),
        ];
        assert_eq!(
            ids(&history(&metas, Some("x"))),
            vec![("a", None), ("b", Some("a")), ("x", Some("a"))]
        );
    }

    #[test]
    fn children_are_found_by_parent() {
        let metas = [snap("a", None, None), snap("b", Some("a"), None)];
        assert!(has_children(&metas, "a"));
        assert!(!has_children(&metas, "b"));
    }
}
