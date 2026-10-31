use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::cloud::capture::{Capture, HashCache, capture, digest};
use crate::cloud::decide::{DecideInput, Decision, PrincipalView, decide};
use crate::cloud::device::LocalDevice;
use crate::cloud::heads::{HeadChoice, HeadReason, principal, read_heads, write_head};
use crate::cloud::key::{CloudKey, GameKind};
use crate::cloud::layout::{CloudLayout, GamePaths};
use crate::cloud::lineage::has_children;
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

struct Lineage {
    parent: Option<String>,
    replaces: Option<String>,
}

impl From<Option<String>> for Lineage {
    fn from(parent: Option<String>) -> Lineage {
        Lineage {
            parent,
            replaces: None,
        }
    }
}

fn push(
    ctx: &CloudContext,
    game: &GameTarget,
    local: &Capture,
    lineage: Lineage,
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
        parent: lineage.parent,
        replaces: lineage.replaces,
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
        base.map(|b| b.snapshot_id.clone()).into(),
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
    let donor = inherited.map_or("", |(_, donor)| donor.id.as_str());
    ctx.store.meta_set(&inherited_key(game), donor)?;
    clear_pending(ctx, game)
}

fn lost_files(meta: &SnapshotMeta, local: &Capture) -> bool {
    let present: HashSet<_> = local
        .files
        .iter()
        .filter(|file| file.category == Category::Save)
        .map(|file| &file.original)
        .collect();
    meta.files
        .iter()
        .filter(|file| file.category == Category::Save)
        .any(|file| !present.contains(&file.original))
}

fn prune_replaced(cloud: &Cloud, replaced: &SnapshotMeta) {
    let Some(older) = replaced.replaces.as_deref() else {
        return;
    };
    let Some((dir, _)) = cloud.find(older) else {
        return;
    };
    let only_child = cloud
        .snapshots
        .iter()
        .filter(|(_, meta)| meta.parent.as_deref() == Some(older))
        .all(|(_, meta)| meta.id == replaced.id);
    let chosen = cloud.heads.iter().any(|head| head.snapshot_id == older);
    if only_child && !chosen {
        let _ = std::fs::remove_dir_all(dir);
    }
}

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

fn inherited_key(game: &GameTarget) -> String {
    format!("cloud_inherited:{}", game.key.as_folder_key())
}

fn newer_cracked_state(
    ctx: &CloudContext,
    game: &GameTarget,
    current: &SnapshotMeta,
    cloud: &Cloud,
) -> CloudResult<bool> {
    if game.kind != GameKind::Emulator || current.contents.state {
        return Ok(false);
    }
    let Some((_, donor)) = last_cracked(cloud) else {
        return Ok(false);
    };
    let inherited = ctx.store.meta_get(&inherited_key(game))?;
    Ok(inherited.as_deref() != Some(donor.id.as_str()))
}

fn pending_key(game: &GameTarget) -> String {
    format!("cloud_pending:{}", game.key.as_folder_key())
}

pub fn reconcile(
    ctx: &CloudContext,
    game: &GameTarget,
    session_ended_at: i64,
) -> CloudResult<SyncEvent> {
    reconcile_cancellable(ctx, game, session_ended_at, &CancelGate::default())
}

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
                if cancel.is_cancelled() {
                    return Err(CloudError::Cancelled);
                }
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
    let stored_base = ctx.store.cloud_base(&game.key.as_folder_key())?;
    let waiting = cloud.waiting(ctx.now);
    let base = if waiting {
        stored_base
    } else {
        stored_base.and_then(|base| rebase(base, game, &local, &cloud))
    };
    let pass = Pass {
        ctx,
        game,
        local,
        cloud,
        base,
        session_ended_at,
        cancel,
    };
    if waiting {
        pass.preserve(false)?;
        return Ok(SyncEvent::Waiting);
    }
    pass.run()
}

type SnapshotEntry = (PathBuf, SnapshotMeta);

struct Pass<'a> {
    ctx: &'a CloudContext<'a>,
    game: &'a GameTarget,
    local: Capture,
    cloud: Cloud,
    base: Option<CloudBase>,
    session_ended_at: i64,
    cancel: &'a CancelGate,
}

impl Pass<'_> {
    fn run(&self) -> CloudResult<SyncEvent> {
        let current = principal(&self.cloud.heads, &self.cloud.metas());
        let current_entry = current
            .as_ref()
            .and_then(|p| self.cloud.find(&p.snapshot_id));
        if let Some(pending) = self.pending()? {
            let snapshot = current_entry.map_or(pending, |(_, meta)| meta.id.clone());
            let (entry, kept) = self.restore_keeping_local(&snapshot)?;
            return Ok(restored(entry, kept));
        }
        if self.is_settled_empty(current_entry) {
            return Ok(SyncEvent::Nothing);
        }
        let principal_digest =
            current_entry.map(|(_, meta)| comparable_digest(meta, self.game.kind));
        let decision = decide(&DecideInput {
            local: has_content(&self.local).then_some(self.local.digest.as_str()),
            base: self.base.as_ref(),
            principal: current_entry
                .zip(principal_digest.as_deref())
                .map(|((_, meta), digest)| PrincipalView {
                    id: &meta.id,
                    digest,
                }),
        });
        match decision {
            Decision::Nothing => self.refresh_cracked_state(current_entry),
            Decision::Adopt { snapshot } => {
                self.settle(&snapshot)?;
                Ok(SyncEvent::Adopted { snapshot })
            }
            Decision::Push { parent } => {
                let current_id = current_entry.map(|(_, meta)| meta.id.clone());
                self.push_session(parent, current_id)
            }
            Decision::Pull { snapshot } | Decision::AdoptCloud { snapshot } => self.pull(&snapshot),
            Decision::Branch { parent } => self.branch(parent, current_entry),
        }
    }

    fn pending(&self) -> CloudResult<Option<String>> {
        Ok(self
            .ctx
            .store
            .meta_get(&pending_key(self.game))?
            .filter(|id| self.cloud.find(id).is_some()))
    }

    fn is_settled_empty(&self, current: Option<&SnapshotEntry>) -> bool {
        !has_content(&self.local)
            && self.base.as_ref().is_some_and(|base| {
                base.digest == self.local.digest
                    && current.is_some_and(|(_, meta)| meta.id == base.snapshot_id)
            })
    }

    fn refresh_cracked_state(&self, current: Option<&SnapshotEntry>) -> CloudResult<SyncEvent> {
        match current {
            Some(entry) if newer_cracked_state(self.ctx, self.game, &entry.1, &self.cloud)? => {
                self.apply(entry)?;
                Ok(restored(entry, None))
            }
            _ => Ok(SyncEvent::Nothing),
        }
    }

    fn push_session(
        &self,
        parent: Option<String>,
        current_id: Option<String>,
    ) -> CloudResult<SyncEvent> {
        let played_from = parent.as_deref().and_then(|id| self.cloud.find(id));
        if let Some(entry) = played_from
            && lost_files(&entry.1, &self.local)
        {
            return self.keep_cloud_line(entry, parent);
        }
        let replaces = played_from
            .filter(|(_, meta)| !has_children(&self.cloud.metas(), &meta.id))
            .map(|(_, meta)| meta.id.clone());
        let meta = self.push(Lineage { parent, replaces })?;
        self.choose(&meta.id, current_id, HeadReason::Push)?;
        self.settle(&meta.id)?;
        if let Some((_, replaced)) = played_from
            && meta.replaces.is_some()
        {
            prune_replaced(&self.cloud, replaced);
        }
        Ok(SyncEvent::Pushed { snapshot: meta.id })
    }

    fn keep_cloud_line(
        &self,
        entry: &SnapshotEntry,
        parent: Option<String>,
    ) -> CloudResult<SyncEvent> {
        let stray = self.push(parent.into())?;
        self.apply(entry)?;
        self.choose(&entry.1.id, Some(stray.id.clone()), HeadReason::Branch)?;
        Ok(branched_to_cloud(entry, stray.id))
    }

    fn pull(&self, snapshot: &str) -> CloudResult<SyncEvent> {
        let (entry, kept) = self.restore_keeping_local(snapshot)?;
        if let Some(kept) = &kept {
            self.choose(snapshot, Some(kept.clone()), HeadReason::Restore)?;
        }
        Ok(restored(entry, kept))
    }

    fn branch(&self, parent: String, current: Option<&SnapshotEntry>) -> CloudResult<SyncEvent> {
        let entry = current.ok_or_else(|| CloudError::Incomplete(parent.clone()))?;
        let mine = self.push(Some(parent).into())?;
        if self.session_ended_at < entry.1.session_ended_at {
            self.apply(entry)?;
            return Ok(branched_to_cloud(entry, mine.id));
        }
        self.choose(&mine.id, Some(entry.1.id.clone()), HeadReason::Branch)?;
        self.settle(&mine.id)?;
        Ok(SyncEvent::Branched {
            principal: mine.id,
            principal_device: self.ctx.device.name.clone(),
            other: entry.1.id.clone(),
            local_is_principal: true,
        })
    }

    fn restore_keeping_local(
        &self,
        snapshot: &str,
    ) -> CloudResult<(&SnapshotEntry, Option<String>)> {
        let entry = self
            .cloud
            .find(snapshot)
            .ok_or_else(|| CloudError::Incomplete(snapshot.to_owned()))?;
        let kept = self.preserve(true)?;
        self.apply(entry)?;
        Ok((entry, kept))
    }

    fn preserve(&self, kept: bool) -> CloudResult<Option<String>> {
        preserve(
            self.ctx,
            self.game,
            &self.local,
            &self.cloud,
            self.base.as_ref(),
            self.session_ended_at,
            kept,
        )
    }

    fn apply(&self, entry: &SnapshotEntry) -> CloudResult<()> {
        apply(
            self.ctx,
            self.game,
            entry,
            &self.local,
            self.base.as_ref(),
            &self.cloud,
            self.cancel,
        )
    }

    fn push(&self, lineage: Lineage) -> CloudResult<SnapshotMeta> {
        push(
            self.ctx,
            self.game,
            &self.local,
            lineage,
            self.session_ended_at,
            false,
            false,
        )
    }

    fn choose(
        &self,
        snapshot: &str,
        previous: Option<String>,
        reason: HeadReason,
    ) -> CloudResult<()> {
        choose(self.ctx, self.game, snapshot, previous, reason)
    }

    fn settle(&self, snapshot: &str) -> CloudResult<()> {
        set_base(self.ctx, self.game, snapshot, &self.local)?;
        clear_pending(self.ctx, self.game)
    }
}

fn restored(entry: &SnapshotEntry, kept_local: Option<String>) -> SyncEvent {
    SyncEvent::Restored {
        snapshot: entry.1.id.clone(),
        playtime_seconds: entry.1.playtime_seconds,
        kept_local,
    }
}

fn branched_to_cloud(entry: &SnapshotEntry, other: String) -> SyncEvent {
    SyncEvent::Branched {
        principal: entry.1.id.clone(),
        principal_device: entry.1.device_name.clone(),
        other,
        local_is_principal: false,
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
        .filter(|choice| ctx.device.is_mine(&choice.device_id))
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
    use crate::cloud::lineage::history;
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
            device: LocalDevice::new(DeviceId("desk".into()), "Desktop"),
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

    fn save_named(pc: &Pc, name: &str, text: &str) {
        let path = pc
            .dir
            .path()
            .join("pc")
            .join("RoamingAppData")
            .join("Game")
            .join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn pushed(event: SyncEvent) -> String {
        match event {
            SyncEvent::Pushed { snapshot } => snapshot,
            other => panic!("expected push, got {other:?}"),
        }
    }

    fn find(pc: &Pc, id: &str) -> SnapshotMeta {
        snapshots(pc)
            .into_iter()
            .find(|meta| meta.id == id)
            .unwrap()
    }

    #[test]
    fn playing_on_the_latest_version_replaces_it() {
        let pc = pc();
        let game = target(&pc, GameKind::SteamOriginal);
        save(&pc, "v1");
        let first = pushed(reconcile(&ctx(&pc, 100), &game, 100).unwrap());
        save(&pc, "v2");
        let second = pushed(reconcile(&ctx(&pc, 200), &game, 200).unwrap());
        assert_eq!(find(&pc, &second).replaces.as_deref(), Some(first.as_str()));
        let shown = history(&snapshots(&pc), Some(&second));
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].id, second);
    }

    #[test]
    fn only_one_hidden_previous_is_kept() {
        let pc = pc();
        let game = target(&pc, GameKind::SteamOriginal);
        save(&pc, "v1");
        let first = pushed(reconcile(&ctx(&pc, 100), &game, 100).unwrap());
        save(&pc, "v2");
        let second = pushed(reconcile(&ctx(&pc, 200), &game, 200).unwrap());
        save(&pc, "v3");
        let third = pushed(reconcile(&ctx(&pc, 300), &game, 300).unwrap());
        let ids: Vec<String> = snapshots(&pc).into_iter().map(|meta| meta.id).collect();
        assert_eq!(ids, vec![second.clone(), third.clone()]);
        assert!(!ids.contains(&first));
    }

    #[test]
    fn playing_from_an_older_version_starts_a_new_line() {
        let pc = pc();
        let game = target(&pc, GameKind::SteamOriginal);
        save(&pc, "v1");
        let first = pushed(reconcile(&ctx(&pc, 100), &game, 100).unwrap());
        save(&pc, "v2");
        let second = pushed(reconcile(&ctx(&pc, 200), &game, 200).unwrap());
        set_principal(&ctx(&pc, 300), &game, &first, HeadReason::Manual).unwrap();
        assert_eq!(read_save(&pc), "v1");
        save(&pc, "v1-other");
        let other = pushed(reconcile(&ctx(&pc, 400), &game, 400).unwrap());
        let meta = find(&pc, &other);
        assert_eq!(meta.parent.as_deref(), Some(first.as_str()));
        assert_eq!(meta.replaces, None);
        let shown: Vec<(String, Option<String>)> = history(&snapshots(&pc), Some(&other))
            .into_iter()
            .map(|meta| (meta.id, meta.parent))
            .collect();
        assert_eq!(
            shown,
            vec![
                (first.clone(), None),
                (second, Some(first.clone())),
                (other, Some(first))
            ]
        );
    }

    #[test]
    fn undo_after_a_session_returns_to_the_replaced_version() {
        let pc = pc();
        let game = target(&pc, GameKind::SteamOriginal);
        save(&pc, "good");
        let first = pushed(reconcile(&ctx(&pc, 100), &game, 100).unwrap());
        save(&pc, "bad session");
        pushed(reconcile(&ctx(&pc, 200), &game, 200).unwrap());
        undo(&ctx(&pc, 300), &game).unwrap();
        assert_eq!(read_save(&pc), "good");
        let last = last_choice(&pc.layout, &CloudKey::Steam(7)).unwrap();
        assert_eq!(last.snapshot_id, first);
    }

    #[test]
    fn a_save_missing_files_becomes_a_parallel_line_instead_of_replacing() {
        let pc = pc();
        let game = target(&pc, GameKind::SteamOriginal);
        save(&pc, "characters");
        save_named(&pc, "slot.bak", "characters backup");
        let first = pushed(reconcile(&ctx(&pc, 100), &game, 100).unwrap());
        std::fs::remove_file(
            pc.dir
                .path()
                .join("pc")
                .join("RoamingAppData")
                .join("Game")
                .join("slot.bak"),
        )
        .unwrap();
        save(&pc, "fresh empty save");
        let event = reconcile(&ctx(&pc, 200), &game, 200).unwrap();
        let SyncEvent::Branched {
            principal,
            other,
            local_is_principal,
            ..
        } = event
        else {
            panic!("expected a split, got {event:?}");
        };
        assert_eq!(principal, first);
        assert!(!local_is_principal);
        assert_eq!(read_save(&pc), "characters");
        let stray = find(&pc, &other);
        assert_eq!(stray.replaces, None);
        assert_eq!(stray.parent.as_deref(), Some(first.as_str()));
        let last = last_choice(&pc.layout, &CloudKey::Steam(7)).unwrap();
        assert_eq!(last.reason, HeadReason::Branch);
        assert_eq!(last.previous.as_deref(), Some(other.as_str()));
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
