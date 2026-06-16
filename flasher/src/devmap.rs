//! RY5088 device map — embedded at compile time from `data/ry5088_devices.json`. dev_id is the only safe
//! model discriminator: USB PID 0x5030 is shared across many models, so it cannot identify a variant.

use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[allow(dead_code)] // pid/company kept for completeness / future use (lookup is by dev_id)
pub struct DeviceRec {
    pub dev_id: u16,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "displayName", default)]
    pub display_name: String,
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub platform: String,
}

impl DeviceRec {
    /// True if this board is on the RY5088 (AT32F405) platform this flasher supports.
    /// Sibling-platform boards (YiChip YC3121/YC500/…, mice) are in the catalog for
    /// recognition only; their firmware is not an AT32F405 keyboard image.
    pub fn is_ry5088(&self) -> bool {
        self.platform.is_empty() || self.platform == "ry5088"
    }
}

#[derive(Deserialize)]
struct DevFile {
    devices: Vec<DeviceRec>,
}

const RAW: &str = include_str!("../data/ry5088_devices.json");

fn all() -> Vec<DeviceRec> {
    serde_json::from_str::<DevFile>(RAW).map(|f| f.devices).unwrap_or_default()
}

/// Catalog record for a dev_id, or None if the id is not in the device map. The record may be a
/// sibling-platform board (YiChip, mouse, …) — check `DeviceRec::is_ry5088()` before flashing.
pub fn lookup(dev_id: u16) -> Option<DeviceRec> {
    all().into_iter().find(|d| d.dev_id == dev_id)
}

/// Human label for a dev_id (falls back gracefully for unknown ids; flags non-RY5088 boards).
pub fn label(dev_id: u16) -> String {
    match lookup(dev_id) {
        Some(d) if d.is_ry5088() => format!("{} ({})", d.display_name, d.name),
        Some(d) => format!("{} — {} platform, NOT an RY5088 board (not flashable here)",
                           d.display_name, d.platform),
        None => format!("unknown dev_id {dev_id}"),
    }
}
