use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cloud::layout::{read_json, write_json_atomic};
use crate::cloud::random_id;

const ID_LEN: usize = 16;
const FALLBACK_NAME: &str = "PC";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalDevice {
    pub id: DeviceId,
    pub name: String,
}

impl LocalDevice {
    pub fn load_or_create(path: &Path, default_name: &str) -> io::Result<LocalDevice> {
        if let Some(device) = read_json::<LocalDevice>(path)
            && !device.id.0.is_empty()
        {
            return Ok(device);
        }
        let device = LocalDevice {
            id: DeviceId(random_id(ID_LEN)),
            name: default_name.to_owned(),
        };
        device.save(path)?;
        Ok(device)
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
        let first = LocalDevice::load_or_create(&path, "Desktop").unwrap();
        let second = LocalDevice::load_or_create(&path, "Other").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.name, "Desktop");
        assert_eq!(first.id.0.len(), 16);
        assert!(first.id.0.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn default_name_is_not_empty() {
        assert!(!default_device_name().trim().is_empty());
    }
}
