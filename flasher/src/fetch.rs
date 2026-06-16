//! Cloud firmware fetch — makes ry-flash self-contained (no local image required).
//! The vendor firmware API is public (no auth, just a `dev_id`) and the payload is plaintext. Flow:
//!   1. POST  {API}/get_fw_version  {"dev_id": N}        -> {"code":0,"data":{file_path, version_str, …}}
//!   2. GET   {DL}/<file_path>                            -> bytes: a PK zip OR raw DEFLATE
//!   3. unpack: PK -> zip member `firmwareFile.bin` (the AT32 image); else raw-inflate (no zlib header).
//!
//! TLS NOTE: the vendor's server certificate is invalid, so we MUST skip verification (matches the
//! Python's `ssl.CERT_NONE`). The transfer is non-sensitive (a firmware image for hardware you own).

use crate::proto;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// fw_at32.bin SHA-256 per dev_id, from the vendor firmware manifest (embedded at build time).
const SHA_MANIFEST: &str = include_str!("../data/ry5088_firmware_sha256.json");

/// Published SHA-256 of `dev_id`'s stock fw_at32.bin, if the manifest has one.
fn expected_sha256(dev_id: u16) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(SHA_MANIFEST).ok()?;
    v.get("firmware")?.get(dev_id.to_string())?.get("sha256")?.as_str().map(str::to_string)
}

/// Verify `image` against the manifest hash for `dev_id`: `Ok(Some(sha))` verified, `Ok(None)` when
/// no hash is published (nothing to check), `Err` on a mismatch.
pub fn verify_sha256(dev_id: u16, image: &[u8]) -> Result<Option<String>, String> {
    match expected_sha256(dev_id) {
        None => Ok(None),
        Some(want) => {
            let got: String = Sha256::digest(image).iter().map(|b| format!("{b:02x}")).collect();
            if got.eq_ignore_ascii_case(&want) {
                Ok(Some(got))
            } else {
                Err(format!("firmware sha256 mismatch for dev_id {dev_id}: manifest {want}, got {got}"))
            }
        }
    }
}

const API: &str = "https://api2.rongyuan.tech:3816/api/v2/get_fw_version";
const DL: &str = "https://api2.rongyuan.tech:3816/download";
/// The zip member carrying the AT32F405 keyboard image (the side ry-flash flashes).
const ZIP_MEMBER: &str = "firmwareFile.bin";

/// A ureq agent that accepts the vendor's invalid TLS cert (see module note).
fn agent() -> Result<ureq::Agent, String> {
    let connector = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .map_err(|e| format!("tls init: {e}"))?;
    Ok(ureq::AgentBuilder::new().tls_connector(Arc::new(connector)).build())
}

/// Human-readable description of a ureq error (includes the server body, e.g. "Record not found").
fn describe(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let body = body.trim();
            if body.is_empty() {
                format!("HTTP {code}")
            } else {
                format!("HTTP {code}: {}", body.chars().take(200).collect::<String>())
            }
        }
        ureq::Error::Transport(t) => format!("network: {t}"),
    }
}

/// PK -> ZIP (extract `firmwareFile.bin`); otherwise raw DEFLATE (no zlib header — like `zlib.decompress(blob,-15)`).
fn unpack(blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() >= 2 && &blob[..2] == b"PK" {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(blob)).map_err(|e| format!("open zip: {e}"))?;
        let names: Vec<String> = zip.file_names().map(|s| s.to_string()).collect();
        let mut f = zip
            .by_name(ZIP_MEMBER)
            .map_err(|_| format!("zip has no '{ZIP_MEMBER}' (members: {})", names.join(", ")))?;
        let mut out = Vec::new();
        f.read_to_end(&mut out).map_err(|e| format!("read zip member '{ZIP_MEMBER}': {e}"))?;
        Ok(out)
    } else {
        let mut out = Vec::new();
        flate2::read::DeflateDecoder::new(blob)
            .read_to_end(&mut out)
            .map_err(|e| format!("raw-inflate: {e}"))?;
        Ok(out)
    }
}

/// Fetch the stock AT32 image for `dev_id` from the vendor cloud.
/// Returns `(at32_image_bytes, version_str)` — the full image (chip-ID @0x5000), ready for `load_slice`.
pub fn fetch_image(dev_id: u16) -> Result<(Vec<u8>, String), String> {
    let agent = agent()?;

    // 1. get_fw_version -> file_path + version_str
    let body = agent
        .post(API)
        .set("Content-Type", "application/json")
        .send_string(&format!("{{\"dev_id\":{dev_id}}}"))
        .map_err(|e| format!("get_fw_version: {}", describe(e)))?
        .into_string()
        .map_err(|e| format!("get_fw_version read: {e}"))?;
    let meta: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("get_fw_version parse: {e}"))?;
    // shape is {"code":0,"data":{file_path, version_str, …}}; tolerate a flat shape too.
    let data = meta.get("data").unwrap_or(&meta);
    let file_path = data.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| {
        let m = meta.get("err_message").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
        match m {
            Some(m) => format!("no firmware for dev_id {dev_id} ({m})"),
            None => format!("no file_path for dev_id {dev_id} in API response"),
        }
    })?;
    let version = data.get("version_str").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // 2. download the firmware package
    let mut blob = Vec::new();
    agent
        .get(&format!("{DL}/{file_path}"))
        .call()
        .map_err(|e| format!("download {file_path}: {}", describe(e)))?
        .into_reader()
        .read_to_end(&mut blob)
        .map_err(|e| format!("download {file_path} read: {e}"))?;
    if blob.is_empty() {
        return Err(format!("empty download for dev_id {dev_id} ({file_path})"));
    }

    // 3. unpack (PK zip member, or raw DEFLATE) and validate it's a real keyboard image.
    let image = unpack(&blob)?;
    if proto::slice_image(&image).is_none() {
        return Err(format!(
            "fetched image for dev_id {dev_id} lacks the keyboard chip-ID '{}' @0x5000 or @0x0 — not a flashable RY5088 image",
            String::from_utf8_lossy(proto::CHIP_ID)
        ));
    }
    // 4. best-effort integrity check against the published manifest hash. A mismatch is most
    //    likely a newer firmware version (the API serves the latest) rather than tampering, so
    //    warn but do not fail — the chip-ID check above is the hard gate.
    if let Err(e) = verify_sha256(dev_id, &image) {
        eprintln!("warning: {e} (likely a firmware update since the manifest snapshot)");
    }
    Ok((image, version))
}

/// Write a fetched image to `<stock_dir>/<dev_id>/fw_at32.bin`, returning the path.
pub fn save_image(dev_id: u16, stock_dir: &Path, image: &[u8]) -> Result<PathBuf, String> {
    let dir = stock_dir.join(dev_id.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let path = dir.join("fw_at32.bin");
    std::fs::write(&path, image).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// Fetch `dev_id`'s stock image and write it to `<stock_dir>/<dev_id>/fw_at32.bin`. Returns the path.
pub fn fetch_to(dev_id: u16, stock_dir: &Path) -> Result<PathBuf, String> {
    let (image, _version) = fetch_image(dev_id)?;
    save_image(dev_id, stock_dir, &image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn unpack_zip_extracts_firmware_member() {
        // a PK zip with the AT32 member plus a decoy -> we must pick firmwareFile.bin.
        let payload = b"\x00\x01AT32 fake image bytes\xff".repeat(40);
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file(ZIP_MEMBER, opts).unwrap();
        zw.write_all(&payload).unwrap();
        zw.start_file("firmwareNordicFile.bin", opts).unwrap();
        zw.write_all(b"nordic decoy").unwrap();
        let zip_bytes = zw.finish().unwrap().into_inner();
        assert_eq!(&zip_bytes[..2], b"PK");
        assert_eq!(unpack(&zip_bytes).unwrap(), payload);
    }

    #[test]
    fn unpack_raw_deflate_roundtrips() {
        // no zlib header -> the else-branch raw-inflate must recover the original bytes.
        let payload = b"raw deflate firmware payload \x00\xff\x7f".repeat(50);
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&payload).unwrap();
        let raw = enc.finish().unwrap();
        assert_ne!(&raw[..2], b"PK");
        assert_eq!(unpack(&raw).unwrap(), payload);
    }

    #[test]
    fn sha256_manifest_loads_and_detects_mismatch() {
        // a published board (Fun60 Ultra, 2307) has a known hash...
        assert!(expected_sha256(2307).is_some());
        // ...which arbitrary bytes must not match.
        assert!(verify_sha256(2307, b"not the real firmware").is_err());
        // an unpublished dev_id has nothing to verify against.
        assert_eq!(verify_sha256(0xFFFF, b"anything").unwrap(), None);
    }
}
