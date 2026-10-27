use aw_core::cloud::device::LocalDevice;
use aw_core::cloud::sync::SyncEvent;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Synced(SyncEvent),
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct Activity {
    pub syncing: bool,
    pub last: Option<Outcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SyncStatus {
    Idle,
    Syncing,
    Waiting,
    Failed { code: String },
    Playing,
    Behind { device: String },
    Sent { at: i64 },
    Received { at: i64, device: String },
}

pub struct Current<'a> {
    pub id: &'a str,
    pub device_id: &'a str,
    pub device_name: &'a str,
    pub created_at: i64,
}

pub struct StatusInput<'a> {
    pub activity: &'a Activity,
    pub running: bool,
    pub current: Option<Current<'a>>,
    pub base: Option<(&'a str, i64)>,
    pub own: &'a LocalDevice,
}

pub const CLOUD_ACTIVITY: &str = "cloud-activity";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub app_id: u32,
    pub syncing: bool,
}

fn kind(outcome: Option<&Outcome>) -> Option<std::mem::Discriminant<SyncEvent>> {
    match outcome {
        Some(Outcome::Synced(event)) => Some(std::mem::discriminant(event)),
        _ => None,
    }
}

pub fn outcome_changed(before: Option<&Outcome>, after: Option<&Outcome>) -> bool {
    let failed = |outcome: Option<&Outcome>| match outcome {
        Some(Outcome::Failed(code)) => Some(code.clone()),
        _ => None,
    };
    kind(before) != kind(after) || failed(before) != failed(after)
}

pub fn sync_status(input: &StatusInput) -> SyncStatus {
    if input.activity.syncing {
        return SyncStatus::Syncing;
    }
    match &input.activity.last {
        Some(Outcome::Failed(code)) => return SyncStatus::Failed { code: code.clone() },
        Some(Outcome::Synced(SyncEvent::Waiting)) => return SyncStatus::Waiting,
        _ => {}
    }
    if input.running {
        return SyncStatus::Playing;
    }
    let Some(current) = &input.current else {
        return SyncStatus::Idle;
    };
    match input.base {
        Some((id, _)) if id == current.id && input.own.is_mine(current.device_id) => {
            SyncStatus::Sent {
                at: current.created_at,
            }
        }
        Some((id, synced_at)) if id == current.id => SyncStatus::Received {
            at: synced_at,
            device: current.device_name.to_owned(),
        },
        _ => SyncStatus::Behind {
            device: current.device_name.to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_core::cloud::device::DeviceId;

    fn current<'a>(id: &'a str, device: &'a str) -> Current<'a> {
        Current {
            id,
            device_id: device,
            device_name: if device == "desk" {
                "Desktop"
            } else {
                "Notebook"
            },
            created_at: 100,
        }
    }

    fn status(
        activity: &Activity,
        running: bool,
        current: Option<Current>,
        base: Option<(&str, i64)>,
    ) -> SyncStatus {
        sync_status(&StatusInput {
            activity,
            running,
            current,
            base,
            own: &own(),
        })
    }

    fn own() -> LocalDevice {
        let mut device = LocalDevice::new(DeviceId("desk".into()), "Desktop");
        device.former_ids.push(DeviceId("old-desk".into()));
        device
    }

    #[test]
    fn a_version_pushed_under_a_former_id_is_still_sent() {
        assert_eq!(
            status(
                &Activity::default(),
                false,
                Some(current("v1", "old-desk")),
                Some(("v1", 100))
            ),
            SyncStatus::Sent { at: 100 }
        );
    }

    #[test]
    fn a_running_sync_shows_first() {
        let activity = Activity {
            syncing: true,
            last: Some(Outcome::Failed("boom".into())),
        };
        assert_eq!(
            status(
                &activity,
                true,
                Some(current("v1", "desk")),
                Some(("v1", 100))
            ),
            SyncStatus::Syncing
        );
    }

    #[test]
    fn errors_and_waiting_come_from_the_last_sync() {
        let failed = Activity {
            syncing: false,
            last: Some(Outcome::Failed("missingRoot".into())),
        };
        assert_eq!(
            status(&failed, false, None, None),
            SyncStatus::Failed {
                code: "missingRoot".into()
            }
        );
        let waiting = Activity {
            syncing: false,
            last: Some(Outcome::Synced(SyncEvent::Waiting)),
        };
        assert_eq!(status(&waiting, false, None, None), SyncStatus::Waiting);
    }

    #[test]
    fn a_running_game_waits_for_its_close() {
        assert_eq!(
            status(
                &Activity::default(),
                true,
                Some(current("v1", "desk")),
                Some(("v1", 100))
            ),
            SyncStatus::Playing
        );
    }

    #[test]
    fn a_newer_cloud_version_is_behind() {
        assert_eq!(
            status(
                &Activity::default(),
                false,
                Some(current("v2", "nb")),
                Some(("v1", 100))
            ),
            SyncStatus::Behind {
                device: "Notebook".into()
            }
        );
        assert_eq!(
            status(&Activity::default(), false, Some(current("v2", "nb")), None),
            SyncStatus::Behind {
                device: "Notebook".into()
            }
        );
    }

    #[test]
    fn in_sync_says_who_made_the_version() {
        assert_eq!(
            status(
                &Activity::default(),
                false,
                Some(current("v1", "desk")),
                Some(("v1", 300))
            ),
            SyncStatus::Sent { at: 100 }
        );
        assert_eq!(
            status(
                &Activity::default(),
                false,
                Some(current("v1", "nb")),
                Some(("v1", 300))
            ),
            SyncStatus::Received {
                at: 300,
                device: "Notebook".into()
            }
        );
    }

    #[test]
    fn nothing_in_the_cloud_is_idle() {
        assert_eq!(
            status(&Activity::default(), false, None, None),
            SyncStatus::Idle
        );
    }

    #[test]
    fn quiet_syncs_do_not_reload_the_screen() {
        let nothing = Outcome::Synced(SyncEvent::Nothing);
        let waiting = Outcome::Synced(SyncEvent::Waiting);
        let failed = Outcome::Failed("missingRoot".into());
        assert!(!outcome_changed(Some(&nothing), Some(&nothing)));
        assert!(outcome_changed(Some(&waiting), Some(&nothing)));
        assert!(outcome_changed(Some(&nothing), Some(&failed)));
        assert!(!outcome_changed(Some(&failed), Some(&failed)));
        assert!(outcome_changed(
            Some(&failed),
            Some(&Outcome::Failed("other".into()))
        ));
        assert!(outcome_changed(None, Some(&nothing)));
    }
}
