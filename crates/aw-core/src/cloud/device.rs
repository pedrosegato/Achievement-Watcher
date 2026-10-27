use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cloud::layout::{read_json, write_json_atomic};
use crate::cloud::{hex, random_id};

const ID_LEN: usize = 16;
const FALLBACK_NAME: &str = "PC";
const MACHINE_SALT: &str = "achievement-watcher/device/";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDevice {
    pub id: DeviceId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub former_ids: Vec<DeviceId>,
}

impl LocalDevice {
    pub fn new(id: DeviceId, name: impl Into<String>) -> LocalDevice {
        LocalDevice {
            id,
            name: name.into(),
            former_ids: Vec::new(),
        }
    }

    pub fn load(
        path: &Path,
        default_name: &str,
        machine: Option<DeviceId>,
    ) -> io::Result<LocalDevice> {
        let stored = read_json::<LocalDevice>(path).filter(|device| !device.id.0.is_empty());
        let device = match (stored, machine) {
            (Some(device), None) => return Ok(device),
            (Some(device), Some(machine)) if device.id == machine => return Ok(device),
            (Some(mut device), Some(machine)) => {
                let old = std::mem::replace(&mut device.id, machine);
                if !device.former_ids.contains(&old) {
                    device.former_ids.push(old);
                }
                device
            }
            (None, machine) => LocalDevice::new(
                machine.unwrap_or_else(|| DeviceId(random_id(ID_LEN))),
                default_name,
            ),
        };
        device.save(path)?;
        Ok(device)
    }

    pub fn is_mine(&self, id: &str) -> bool {
        self.id.0 == id || self.former_ids.iter().any(|former| former.0 == id)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        write_json_atomic(path, self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRecord {
    pub name: String,
    pub last_seen: i64,
}

pub fn machine_device_id(machine_guid: &str) -> DeviceId {
    let digest = Sha256::digest(format!(
        "{MACHINE_SALT}{}",
        machine_guid.trim().to_lowercase()
    ));
    DeviceId(hex(&digest)[..ID_LEN].to_owned())
}

pub fn this_machine_id() -> Option<DeviceId> {
    crate::system::machine_guid().map(|guid| machine_device_id(&guid))
}

pub fn default_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| FALLBACK_NAME.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_is_created_once_and_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device.json");
        let first = LocalDevice::load(&path, "Desktop", None).unwrap();
        let second = LocalDevice::load(&path, "Other", None).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.name, "Desktop");
        assert_eq!(first.id.0.len(), 16);
        assert!(first.id.0.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn machine_id_is_stable_hex_and_hides_the_guid() {
        let id = machine_device_id("1B2C3D4E-AAAA-BBBB-CCCC-0123456789AB");
        assert_eq!(
            id,
            machine_device_id(" 1b2c3d4e-aaaa-bbbb-cccc-0123456789ab ")
        );
        assert_ne!(
            id,
            machine_device_id("1B2C3D4E-AAAA-BBBB-CCCC-0123456789AC")
        );
        assert_eq!(id.0.len(), 16);
        assert!(id.0.bytes().all(|c| c.is_ascii_hexdigit()));
        assert!(!"1b2c3d4eaaaabbbbcccc0123456789ab".contains(&id.0));
    }

    #[test]
    fn first_run_takes_the_machine_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device.json");
        let machine = DeviceId("feedfacefeedface".into());
        let device = LocalDevice::load(&path, "Desktop", Some(machine.clone())).unwrap();
        assert_eq!(device.id, machine);
        assert!(device.former_ids.is_empty());
        assert_eq!(read_json::<LocalDevice>(&path), Some(device));
    }

    #[test]
    fn stored_random_id_becomes_a_former_id_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device.json");
        std::fs::write(&path, r#"{"id":"e28b3f4a2d0840a1","name":"pedro pc"}"#).unwrap();
        let machine = DeviceId("feedfacefeedface".into());
        let device = LocalDevice::load(&path, "Desktop", Some(machine.clone())).unwrap();
        assert_eq!(device.id, machine);
        assert_eq!(device.name, "pedro pc");
        assert_eq!(device.former_ids, vec![DeviceId("e28b3f4a2d0840a1".into())]);
        let again = LocalDevice::load(&path, "Desktop", Some(machine)).unwrap();
        assert_eq!(again, device);
    }

    #[test]
    fn without_a_machine_id_the_stored_id_stays() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device.json");
        let first = LocalDevice::load(&path, "Desktop", None).unwrap();
        assert_eq!(first.id.0.len(), 16);
        assert_eq!(LocalDevice::load(&path, "Other", None).unwrap(), first);
    }

    #[test]
    fn former_ids_still_count_as_this_pc() {
        let mut device = LocalDevice::new(DeviceId("new".into()), "Desktop");
        device.former_ids.push(DeviceId("old".into()));
        assert!(device.is_mine("new"));
        assert!(device.is_mine("old"));
        assert!(!device.is_mine("other"));
    }

    #[test]
    fn default_name_is_not_empty() {
        assert!(!default_device_name().trim().is_empty());
    }
}
