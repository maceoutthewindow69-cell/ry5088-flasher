//! Resolving + slicing the stock image for a dev_id.

use crate::proto;
use std::path::{Path, PathBuf};

/// Local image for dev_id: `<stock_dir>/<dev_id>/fw_at32.bin` (full image or already-sliced — both work).
pub fn local_image(stock_dir: &Path, dev_id: u16) -> Option<PathBuf> {
    let p = stock_dir.join(dev_id.to_string()).join("fw_at32.bin");
    p.is_file().then_some(p)
}

/// Extract the embedded chip-ID string (e.g. "AT32F405 8KMKB") from a full image, if present. Used by
/// `--probe` to confirm a fetched image is a keyboard build before a contributor analyses it.
pub fn chip_id(bytes: &[u8]) -> Option<String> {
    let needle = b"AT32F405";
    let pos = bytes.windows(needle.len()).position(|w| w == needle)?;
    let end = bytes[pos..]
        .iter()
        .position(|&b| !(0x20..=0x7e).contains(&b))
        .map_or(bytes.len(), |e| pos + e);
    Some(String::from_utf8_lossy(&bytes[pos..end]).trim().to_string())
}

/// Read an image and return the 0x5000-slice the RY-DFU writes (verifies the keyboard chip-ID).
pub fn load_slice(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    proto::slice_image(&bytes).ok_or_else(|| {
        format!(
            "{}: no keyboard chip-ID 'AT32F405 8KMKB' @0x5000 or @0x0 — not a flashable RY5088 keyboard image",
            path.display()
        )
    })
}
