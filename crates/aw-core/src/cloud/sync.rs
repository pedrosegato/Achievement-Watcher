use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::cloud::capture::{Capture, HashCache, capture, digest};
use crate::cloud::decide::{DecideInput, Decision, PrincipalView, decide};
use crate::cloud::device::LocalDevice;
use crate::cloud::heads::{HeadChoice, HeadReason, principal, read_heads, write_head};
use crate::cloud::key::{CloudKey, GameKind};
use crate::cloud::layout::{CloudLayout, GamePaths};
use crate::cloud::locate::{Category, SaveRoot};
use crate::cloud::portable::Placeholders;
use crate::cloud::restore::{RestoreTarget, restore};
use crate::cloud::snapshot::{
    NewSnapshot, SnapshotMeta, SnapshotRead, Verify, list_snapshots, read_snapshot, write_snapshot,
};
use crate::cloud::{CancelGate, CloudError, CloudResult};
use crate::store::{CloudBase, Store};

const RECENT_HEAD_SECS: i64 = 600;
pub const WAIT_CAP_SECS: i64 = 6 * 60 * 60;
const SHORT_ID_LEN: usize = 6;

pub struct CloudContext<'a> {
    pub layout: &'a CloudLayout,
    pub device: &'a LocalDevice,
    pub store: &'a Store,
    pub cache: &'a HashCache,
    pub now: i64,
}

#[derive(Debug, Clone)]
pub struct GameTarget {
    pub key: CloudKey,
    pub name: String,
    pub app_id: u32,
    pub kind: GameKind,
    pub roots: Vec<SaveRoot>,
    pub placeholders: Placeholders,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SyncEvent {
    Nothing,
    Waiting,
    Pushed {
        snapshot: String,
    },
    Adopted {
        snapshot: String,
    },
    #[serde(rename_all = "camelCase")]
    Restored {
        snapshot: String,
        playtime_seconds: Option<i64>,
        kept_local: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Branched {
        principal: String,
        principal_device: String,
        other: String,
        local_is_principal: bool,
    },
}

struct Cloud {
    games: Vec<GamePaths>,
    snapshots: Vec<(PathBuf, SnapshotMeta)>,
    heads: Vec<HeadChoice>,
}

impl Cloud {
    fn load(layout: &CloudLayout, key: &CloudKey, excluded: &HashSet<String>) -> Cloud {
        let games = layout.game_dirs(key);
        let mut snapshots = list_snapshots(&games);
        snapshots.retain(|(_, meta)| !excluded.contains(&meta.id));
        Cloud {
            snapshots,
            heads: read_heads(&games),
            games,
        }
    }

    fn metas(&self) -> Vec<SnapshotMeta> {
        self.snapshots
            .iter()
            .map(|(_, meta)| meta.clone())
            .collect()
    }

    fn find(&self, id: &str) -> Option<&(PathBuf, SnapshotMeta)> {
        self.snapshots.iter().find(|(_, meta)| meta.id == id)
    }

    fn waiting(&self, now: i64) -> bool {
        let Some(latest) = self
            .heads
            .iter()
            .max_by(|a, b| (a.chosen_at, &a.device_id).cmp(&(b.chosen_at, &b.device_id)))
        else {
            return false;
        };
        let age = now.saturating_sub(latest.chosen_at).saturating_abs();
        if self.find(&latest.snapshot_id).is_some() || age > WAIT_CAP_SECS {
            return false;
        }
        let short: String = latest.snapshot_id.chars().take(SHORT_ID_LEN).collect();
        let folder_exists = self.games.iter().any(|game| {
            std::fs::read_dir(game.snapshots_dir())
                .into_iter()
                .flatten()
                .flatten()
                .any(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.ends_with(&format!("_{short}")) || name.ends_with(&latest.snapshot_id)
                })
        });
        folder_exists || age < RECENT_HEAD_SECS
    }
}

pub fn comparable_digest(meta: &SnapshotMeta, kind: GameKind) -> String {
    if kind == GameKind::Emulator && meta.contents.state {
        return meta.digest.clone();
    }
    let saves: Vec<_> = meta
        .files
        .iter()
        .filter(|file| file.category == Category::Save)
        .map(|file| (file.category, file.original.clone(), file.sha256.clone()))
        .collect();
    digest(&saves, None)
}

pub fn profile(game: &GameTarget) -> String {
    let mut roots: Vec<String> = game
        .roots
        .iter()
        .map(|root| format!("{:?}:{}", root.category, root.pattern.as_str()))
        .collect();
    roots.sort();
    format!("{:?}|{}", game.kind, roots.join(";"))
}

/// The base only describes this PC while the watched folders stay the same.
/// After they change, a save that still matches the base snapshot is treated
/// as never synced (the cloud fills in what is missing, e.g. achievements),
/// while a save that differs is local progress made since the base.
fn rebase(base: CloudBase, game: &GameTarget, local: &Capture, cloud: &Cloud) -> Option<CloudBase> {
    if base.profile == profile(game) {
        return Some(base);
    }
    let (_, synced) = cloud.find(&base.snapshot_id)?;
    let synced_saves = comparable_digest(synced, GameKind::SteamOriginal);
    (local.has_save() && saves_digest(local) != synced_saves).then(|| CloudBase {
        digest: String::new(),
        ..base
    })
}

fn saves_digest(capture: &Capture) -> String {
    let saves: Vec<_> = capture
        .files
        .iter()
        .filter(|file| file.category == Category::Save)
        .map(|file| (file.category, file.original.clone(), file.sha256.clone()))
        .collect();
    digest(&saves, None)
}

fn has_content(capture: &Capture) -> bool {
    !capture.files.is_empty()
        || capture
            .state
            .as_ref()
            .is_some_and(|s| !s.unlocks.is_empty() || !s.progress.is_empty())
}

fn local_capture(ctx: &CloudContext, game: &GameTarget) -> CloudResult<Capture> {
    let state = match game.kind {
        GameKind::Emulator => Some(ctx.store.game_state(game.app_id)?),
        GameKind::SteamOriginal => None,
    };
    Ok(capture(&game.roots, &game.placeholders, state, ctx.cache)?)
}

fn set_base(
    ctx: &CloudContext,
    game: &GameTarget,
    snapshot: &str,
    local: &Capture,
) -> CloudResult<()> {
    ctx.store.set_cloud_base(&CloudBase {
        game_key: game.key.as_folder_key(),
        snapshot_id: snapshot.to_owned(),
        digest: local.digest.clone(),
        playtime_seconds: local.state.as_ref().map(|s| s.playtime_seconds),
        synced_at: ctx.now,
        profile: profile(game),
    })?;
    Ok(())
}

fn choose(
    ctx: &CloudContext,
    game: &GameTarget,
    snapshot: &str,
    previous: Option<String>,
    reason: HeadReason,
) -> CloudResult<()> {
    let dir = ctx.layout.game_dir_for_write(&game.key, &game.name)?;
    let latest_seen = read_heads(&ctx.layout.game_dirs(&game.key))
        .iter()
        .map(|head| head.chosen_at)
        .max();
    write_head(
        &dir,
        &ctx.device.id,
        &HeadChoice {
            snapshot_id: snapshot.to_owned(),
            previous,
            reason,
            chosen_at: latest_seen.map_or(ctx.now, |seen| ctx.now.max(seen.saturating_add(1))),
            device_id: ctx.device.id.0.clone(),
            device_name: ctx.device.name.clone(),
        },
    )?;
    Ok(())
}

fn push(
    ctx: &CloudContext,
    game: &GameTarget,
    local: &Capture,
    parent: Option<String>,
    ended: i64,
    found_on_device: bool,
    kept: bool,
) -> CloudResult<SnapshotMeta> {
    let dir = ctx.layout.game_dir_for_write(&game.key, &game.name)?;
    write_snapshot(&NewSnapshot {
        game: &dir,
        game_key: &game.key.as_folder_key(),
        game_name: &game.name,
        device: ctx.device,
        now: ctx.now,
        session_ended_at: ended,
        parent,
        found_on_device,
        kept,
        local_app_id: game.app_id,
        capture: local,
    })
}

fn preserve(
    ctx: &CloudContext,
    game: &GameTarget,
    local: &Capture,
    cloud: &Cloud,
    base: Option<&CloudBase>,
    ended: i64,
    kept: bool,
) -> CloudResult<Option<String>> {
    let unchanged_since_sync =
        base.is_some_and(|b| b.digest == local.digest && cloud.find(&b.snapshot_id).is_some());
    if !has_content(local)
        || unchanged_since_sync
        || cloud
            .snapshots
            .iter()
            .any(|(_, meta)| comparable_digest(meta, game.kind) == local.digest)
    {
        return Ok(None);
    }
    let kept = push(
        ctx,
        game,
        local,
        base.map(|b| b.snapshot_id.clone()),
        ended,
        base.is_none(),
        kept,
    )?
    .id;
    Ok(Some(kept))
}

fn apply(
    ctx: &CloudContext,
    game: &GameTarget,
    entry: &(PathBuf, SnapshotMeta),
    local: &Capture,
    base: Option<&CloudBase>,
    cloud: &Cloud,
    cancel: &CancelGate,
) -> CloudResult<()> {
    let (dir, meta) = entry;
    let inherited = (game.kind == GameKind::Emulator && !meta.contents.state)
        .then(|| last_cracked(cloud))
        .flatten();
    if let Some((donor_dir, donor)) = inherited
        && !matches!(
            read_snapshot(donor_dir, Verify::Hashes),
            SnapshotRead::Ready(_)
        )
    {
        return Err(CloudError::Incomplete(donor.id.clone()));
    }
    ctx.store.meta_set(&pending_key(game), &meta.id)?;
    let restored = restore(
        dir,
        meta,
        &RestoreTarget {
            app_id: game.app_id,
            kind: game.kind,
            roots: &game.roots,
            placeholders: &game.placeholders,
            local,
            base_playtime: base.and_then(|b| b.playtime_seconds),
            inherited_state: inherited.map(|(donor_dir, _)| donor_dir.as_path()),
            cancel: Some(cancel),
        },
        ctx.store,
    );
    if let Err(error) = restored {
        clear_pending(ctx, game)?;
        return Err(error);
    }
    let after = local_capture(ctx, game)?;
    set_base(ctx, game, &meta.id, &after)?;
    clear_pending(ctx, game)
}

/// The most recent snapshot that carries achievements (pushed by a cracked
/// copy), skipping copies kept only as a backup before an overwrite. Every PC
/// sees the same list, so cracked PCs agree on it.
fn last_cracked(cloud: &Cloud) -> Option<&(PathBuf, SnapshotMeta)> {
    cloud
        .snapshots
        .iter()
        .rev()
        .find(|(_, meta)| meta.contents.state && !meta.kept)
}

fn clear_pending(ctx: &CloudContext, game: &GameTarget) -> CloudResult<()> {
    ctx.store.meta_set(&pending_key(game), "")?;
    Ok(())
}

fn pending_key(game: &GameTarget) -> String {
    format!("cloud_pending:{}", game.key.as_folder_key())
}

/// A snapshot whose files fail verification is treated as still downloading:
/// it is left out and the decision is made again without it.
pub fn reconcile(
    ctx: &CloudContext,
    game: &GameTarget,
    session_ended_at: i64,
) -> CloudResult<SyncEvent> {
    reconcile_cancellable(ctx, game, session_ended_at, &CancelGate::default())
}

/// Like `reconcile`, but a restore stops before touching local files once
/// `cancel` is set, returning `CloudError::Cancelled`.
pub fn reconcile_cancellable(
    ctx: &CloudContext,
    game: &GameTarget,
    session_ended_at: i64,
    cancel: &CancelGate,
) -> CloudResult<SyncEvent> {
    let mut excluded = HashSet::new();
    loop {
        match reconcile_once(ctx, game, session_ended_at, &excluded, cancel) {
            Err(CloudError::Incomplete(id)) if !excluded.contains(&id) => {
                excluded.insert(id);
            }
            result => return result,
        }
    }
}

fn reconcile_once(
    ctx: &CloudContext,
    game: &GameTarget,
    session_ended_at: i64,
    excluded: &HashSet<String>,
    cancel: &CancelGate,
) -> CloudResult<SyncEvent> {
    if !ctx.layout.exists() {
        return Err(CloudError::MissingRoot);
    }
    let local = local_capture(ctx, game)?;
    let cloud = Cloud::load(ctx.layout, &game.key, excluded);
    if cloud.waiting(ctx.now) {
        let base = ctx.store.cloud_base(&game.key.as_folder_key())?;
        preserve(
            ctx,
            game,
            &local,
            &cloud,
            base.as_ref(),
            session_ended_at,
            false,
        )?;
        return Ok(SyncEvent::Waiting);
    }
    let current = principal(&cloud.heads, &cloud.metas());
    let current_entry = current.as_ref().and_then(|p| cloud.find(&p.snapshot_id));
    let base = ctx
        .store
        .cloud_base(&game.key.as_folder_key())?
        .and_then(|base| rebase(base, game, &local, &cloud));
    let principal_digest = current_entry.map(|(_, meta)| comparable_digest(meta, game.kind));
    let pending = ctx
        .store
        .meta_get(&pending_key(game))?
        .filter(|id| cloud.find(id).is_some());
    if let Some(pending) = pending {
        let snapshot = current_entry.map_or(pending, |(_, meta)| meta.id.clone());
        let entry = cloud
            .find(&snapshot)
            .ok_or_else(|| CloudError::Incomplete(snapshot.clone()))?;
        let kept = preserve(
            ctx,
            game,
            &local,
            &cloud,
            base.as_ref(),
            session_ended_at,
            true,
        )?;
        apply(ctx, game, entry, &local, base.as_ref(), &cloud, cancel)?;
        return Ok(SyncEvent::Restored {
            snapshot,
            playtime_seconds: entry.1.playtime_seconds,
            kept_local: kept,
        });
    }
    let settled_empty = !has_content(&local)
        && base.as_ref().is_some_and(|b| {
            b.digest == local.digest
                && current_entry.is_some_and(|(_, meta)| meta.id == b.snapshot_id)
        });
    if settled_empty {
        return Ok(SyncEvent::Nothing);
    }
    let decision = decide(&DecideInput {
        local: has_content(&local).then_some(local.digest.as_str()),
        base: base.as_ref(),
        principal: current_entry
            .zip(principal_digest.as_deref())
            .map(|((_, meta), digest)| PrincipalView {
                id: &meta.id,
                digest,
            }),
    });
    let current_id = current_entry.map(|(_, meta)| meta.id.clone());
    match decision {
        Decision::Nothing => Ok(SyncEvent::Nothing),
        Decision::Adopt { snapshot } => {
            set_base(ctx, game, &snapshot, &local)?;
            clear_pending(ctx, game)?;
            Ok(SyncEvent::Adopted { snapshot })
        }
        Decision::Push { parent } => {
            let meta = push(ctx, game, &local, parent, session_ended_at, false, false)?;
            choose(ctx, game, &meta.id, current_id, HeadReason::Push)?;
            set_base(ctx, game, &meta.id, &local)?;
            clear_pending(ctx, game)?;
            Ok(SyncEvent::Pushed { snapshot: meta.id })
        }
        Decision::Pull { snapshot } | Decision::AdoptCloud { snapshot } => {
            let entry = cloud
                .find(&snapshot)
                .ok_or_else(|| CloudError::Incomplete(snapshot.clone()))?;
            let kept = preserve(
                ctx,
                game,
                &local,
                &cloud,
                base.as_ref(),
                session_ended_at,
                true,
            )?;
            apply(ctx, game, entry, &local, base.as_ref(), &cloud, cancel)?;
            if let Some(kept) = &kept {
                choose(
                    ctx,
                    game,
                    &snapshot,
                    Some(kept.clone()),
                    HeadReason::Restore,
                )?;
            }
            Ok(SyncEvent::Restored {
                snapshot,
                playtime_seconds: entry.1.playtime_seconds,
                kept_local: kept,
            })
        }
        Decision::Branch { parent } => {
            let entry = current_entry.ok_or_else(|| CloudError::Incomplete(parent.clone()))?;
            let mine = push(
                ctx,
                game,
                &local,
                Some(parent),
                session_ended_at,
                false,
                false,
            )?;
            if session_ended_at >= entry.1.session_ended_at {
                choose(
                    ctx,
                    game,
                    &mine.id,
                    Some(entry.1.id.clone()),
                    HeadReason::Branch,
                )?;
                set_base(ctx, game, &mine.id, &local)?;
                clear_pending(ctx, game)?;
                Ok(SyncEvent::Branched {
                    principal: mine.id,
                    principal_device: ctx.device.name.clone(),
                    other: entry.1.id.clone(),
                    local_is_principal: true,
                })
            } else {
                apply(ctx, game, entry, &local, base.as_ref(), &cloud, cancel)?;
                Ok(SyncEvent::Branched {
                    principal: entry.1.id.clone(),
                    principal_device: entry.1.device_name.clone(),
                    other: mine.id,
                    local_is_principal: false,
                })
            }
        }
    }
}

pub fn set_principal(
    ctx: &CloudContext,
    game: &GameTarget,
    snapshot_id: &str,
    reason: HeadReason,
) -> CloudResult<SyncEvent> {
    if reconcile(ctx, game, ctx.now)? == SyncEvent::Waiting {
        return Ok(SyncEvent::Waiting);
    }
    let cloud = Cloud::load(ctx.layout, &game.key, &HashSet::new());
    if cloud.find(snapshot_id).is_none() {
        return Err(CloudError::Incomplete(snapshot_id.to_owned()));
    }
    let current = principal(&cloud.heads, &cloud.metas()).map(|p| p.snapshot_id);
    choose(ctx, game, snapshot_id, current, reason)?;
    reconcile(ctx, game, ctx.now)
}

pub fn undo(ctx: &CloudContext, game: &GameTarget) -> CloudResult<SyncEvent> {
    let games = ctx.layout.game_dirs(&game.key);
    let previous = read_heads(&games)
        .into_iter()
        .filter(|choice| choice.device_id == ctx.device.id.0)
        .max_by_key(|choice| choice.chosen_at)
        .and_then(|choice| choice.previous);
    match previous {
        Some(previous) => set_principal(ctx, game, &previous, HeadReason::Undo),
        None => Ok(SyncEvent::Nothing),
    }
}

pub fn last_choice(layout: &CloudLayout, key: &CloudKey) -> Option<HeadChoice> {
    read_heads(&layout.game_dirs(key))
        .into_iter()
        .max_by(|a, b| (a.chosen_at, &a.device_id).cmp(&(b.chosen_at, &b.device_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::device::DeviceId;
    use crate::cloud::portable::PortablePath;
    use crate::discovery::folders::KnownFolders;

    struct Pc {
        dir: tempfile::TempDir,
        layout: CloudLayout,
        device: LocalDevice,
        store: Store,
        cache: HashCache,
    }

    fn pc() -> Pc {
        let dir = tempfile::tempdir().unwrap();
        let layout = CloudLayout::new(dir.path().join("cloud"));
        std::fs::create_dir_all(layout.root()).unwrap();
        Pc {
            dir,
            layout,
            device: LocalDevice {
                id: DeviceId("desk".into()),
                name: "Desktop".into(),
            },
            store: Store::in_memory().unwrap(),
            cache: HashCache::default(),
        }
    }

    fn ctx(pc: &Pc, now: i64) -> CloudContext<'_> {
        CloudContext {
            layout: &pc.layout,
            device: &pc.device,
            store: &pc.store,
            cache: &pc.cache,
            now,
        }
    }

    fn target(pc: &Pc, kind: GameKind) -> GameTarget {
        GameTarget {
            key: CloudKey::Steam(7),
            name: "Game".into(),
            app_id: 7,
            kind,
            roots: vec![SaveRoot {
                category: Category::Save,
                pattern: PortablePath::new("<winAppData>/Game").unwrap(),
                single_file: false,
            }],
            placeholders: Placeholders {
                folders: Some(KnownFolders::under(&pc.dir.path().join("pc"))),
                ..Default::default()
            },
        }
    }

    fn save(pc: &Pc, text: &str) {
        let path = pc
            .dir
            .path()
            .join("pc")
            .join("RoamingAppData")
            .join("Game")
            .join("slot.sav");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read_save(pc: &Pc) -> String {
        std::fs::read_to_string(
            pc.dir
                .path()
                .join("pc")
                .join("RoamingAppData")
                .join("Game")
                .join("slot.sav"),
        )
        .unwrap()
    }

    fn snapshots(pc: &Pc) -> Vec<SnapshotMeta> {
        list_snapshots(&pc.layout.game_dirs(&CloudKey::Steam(7)))
            .into_iter()
            .map(|(_, meta)| meta)
            .collect()
    }

    #[test]
    fn first_session_pushes_and_sets_base() {
        let pc = pc();
        save(&pc, "v1");
        let game = target(&pc, GameKind::SteamOriginal);
        let event = reconcile(&ctx(&pc, 100), &game, 100).unwrap();
        let SyncEvent::Pushed { snapshot } = event else {
            panic!("expected push, got {event:?}");
        };
        let base = pc.store.cloud_base("7").unwrap().unwrap();
        assert_eq!(base.snapshot_id, snapshot);
        assert_eq!(snapshots(&pc).len(), 1);
    }

    #[test]
    fn unchanged_second_run_does_nothing() {
        let pc = pc();
        save(&pc, "v1");
        let game = target(&pc, GameKind::SteamOriginal);
        reconcile(&ctx(&pc, 100), &game, 100).unwrap();
        assert_eq!(
            reconcile(&ctx(&pc, 200), &game, 200).unwrap(),
            SyncEvent::Nothing
        );
        save(&pc, "v2");
        assert!(matches!(
            reconcile(&ctx(&pc, 300), &game, 300).unwrap(),
            SyncEvent::Pushed { .. }
        ));
        assert_eq!(snapshots(&pc).len(), 2);
        assert_eq!(
            snapshots(&pc)[1].parent.as_deref(),
            Some(snapshots(&pc)[0].id.as_str())
        );
    }

    #[test]
    fn playtime_only_change_creates_no_snapshot() {
        let pc = pc();
        save(&pc, "v1");
        pc.store.record_unlock(7, "A", 1, 1, "gse").unwrap();
        let game = target(&pc, GameKind::Emulator);
        reconcile(&ctx(&pc, 100), &game, 100).unwrap();
        pc.store.add_playtime(7, 120, 150).unwrap();
        assert_eq!(
            reconcile(&ctx(&pc, 200), &game, 200).unwrap(),
            SyncEvent::Nothing
        );
        pc.store.record_unlock(7, "B", 2, 2, "gse").unwrap();
        assert!(matches!(
            reconcile(&ctx(&pc, 300), &game, 300).unwrap(),
            SyncEvent::Pushed { .. }
        ));
    }

    #[test]
    fn incomplete_principal_waits() {
        let pc = pc();
        save(&pc, "v1");
        let game = target(&pc, GameKind::SteamOriginal);
        reconcile(&ctx(&pc, 100), &game, 100).unwrap();
        save(&pc, "v2");
        reconcile(&ctx(&pc, 200), &game, 200).unwrap();
        let (dir, meta) = list_snapshots(&pc.layout.game_dirs(&CloudKey::Steam(7)))
            .into_iter()
            .last()
            .unwrap();
        std::fs::remove_file(dir.join(&meta.files[0].stored)).unwrap();
        save(&pc, "v3");
        assert_eq!(
            reconcile(&ctx(&pc, 300), &game, 300).unwrap(),
            SyncEvent::Waiting
        );
        assert_eq!(read_save(&pc), "v3");
    }

    #[test]
    fn set_principal_restores_that_snapshot() {
        let pc = pc();
        save(&pc, "v1");
        let game = target(&pc, GameKind::SteamOriginal);
        let SyncEvent::Pushed { snapshot: first } = reconcile(&ctx(&pc, 100), &game, 100).unwrap()
        else {
            panic!()
        };
        save(&pc, "v2");
        reconcile(&ctx(&pc, 200), &game, 200).unwrap();
        let event = set_principal(&ctx(&pc, 300), &game, &first, HeadReason::Manual).unwrap();
        assert!(matches!(event, SyncEvent::Restored { .. }));
        assert_eq!(read_save(&pc), "v1");
        assert_eq!(
            reconcile(&ctx(&pc, 400), &game, 400).unwrap(),
            SyncEvent::Nothing
        );
    }

    #[test]
    fn undo_returns_to_previous_choice() {
        let pc = pc();
        save(&pc, "v1");
        let game = target(&pc, GameKind::SteamOriginal);
        let SyncEvent::Pushed { snapshot: first } = reconcile(&ctx(&pc, 100), &game, 100).unwrap()
        else {
            panic!()
        };
        save(&pc, "v2");
        let SyncEvent::Pushed { snapshot: second } = reconcile(&ctx(&pc, 200), &game, 200).unwrap()
        else {
            panic!()
        };
        set_principal(&ctx(&pc, 300), &game, &first, HeadReason::Manual).unwrap();
        assert_eq!(read_save(&pc), "v1");
        undo(&ctx(&pc, 400), &game).unwrap();
        assert_eq!(read_save(&pc), "v2");
        let last = last_choice(&pc.layout, &CloudKey::Steam(7)).unwrap();
        assert_eq!(last.snapshot_id, second);
        assert_eq!(last.reason, HeadReason::Undo);
    }

    #[test]
    fn missing_root_errors() {
        let pc = pc();
        std::fs::remove_dir_all(pc.layout.root()).unwrap();
        let game = target(&pc, GameKind::SteamOriginal);
        assert!(matches!(
            reconcile(&ctx(&pc, 1), &game, 1),
            Err(CloudError::MissingRoot)
        ));
    }

    fn break_latest(pc: &Pc) {
        let (dir, meta) = list_snapshots(&pc.layout.game_dirs(&CloudKey::Steam(7)))
            .into_iter()
            .last()
            .unwrap();
        std::fs::remove_file(dir.join(&meta.files[0].stored)).unwrap();
    }

    #[test]
    fn waiting_still_keeps_local_sessions() {
        let pc = pc();
        save(&pc, "v1");
        let game = target(&pc, GameKind::SteamOriginal);
        reconcile(&ctx(&pc, 100), &game, 100).unwrap();
        save(&pc, "v2");
        reconcile(&ctx(&pc, 200), &game, 200).unwrap();
        break_latest(&pc);
        let before = snapshots(&pc).len();
        save(&pc, "v3");
        assert_eq!(
            reconcile(&ctx(&pc, 300), &game, 300).unwrap(),
            SyncEvent::Waiting
        );
        assert_eq!(snapshots(&pc).len(), before + 1);
        assert_eq!(
            reconcile(&ctx(&pc, 310), &game, 310).unwrap(),
            SyncEvent::Waiting
        );
        assert_eq!(snapshots(&pc).len(), before + 1);
    }

    #[test]
    fn waiting_gives_up_after_the_cap() {
        let pc = pc();
        save(&pc, "v1");
        let game = target(&pc, GameKind::SteamOriginal);
        reconcile(&ctx(&pc, 100), &game, 100).unwrap();
        save(&pc, "v2");
        reconcile(&ctx(&pc, 200), &game, 200).unwrap();
        break_latest(&pc);
        let later = 200 + WAIT_CAP_SECS + 1;
        assert_ne!(
            reconcile(&ctx(&pc, later), &game, later).unwrap(),
            SyncEvent::Waiting
        );
    }
}
