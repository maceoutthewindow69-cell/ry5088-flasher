//! depth_snoop — safe all-interface Hall report diagnostic for X65 bring-up.
//!
//! No bootloader, calibration, erase, config write, or firmware flash commands
//! are used. The only SET is volatile SET_MAGNETISM_REPORT (0x1B), which is
//! disabled again before exit.

use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::thread::sleep;
use std::time::{Duration, Instant};

const VID: u16 = 0x3151;
const PID_X65: u16 = 0x502D;
const PID_LEGACY: u16 = 0x5030;
const X65_DEV_ID: u16 = 2268;

#[derive(Debug, Clone, Serialize)]
struct IfaceMeta {
    slot: usize,
    pid: u16,
    usage_page: u16,
    usage: u16,
    interface_number: i32,
    path: String,
    open_ok: bool,
    open_error: Option<String>,
}

struct OpenIface {
    slot: usize,
    dev: HidDevice,
}

#[derive(Debug, Default, Serialize)]
struct RxStats {
    packets: u64,
    read_errors: u64,
    first_error: Option<String>,
    samples: Vec<String>,
}

fn is_normal_pid(pid: u16) -> bool {
    pid == PID_X65 || pid == PID_LEGACY
}

fn is_vendor_page(page: u16) -> bool {
    page >= 0xFF00
}

fn bit7(payload: &[u8]) -> [u8; 64] {
    let mut r = [0u8; 64];
    let n = payload.len().min(7);
    r[..n].copy_from_slice(&payload[..n]);
    let sum = r[..7].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    r[7] = 0xFFu8.wrapping_sub(sum);
    r
}

fn send_feature(d: &HidDevice, frame: &[u8; 64]) -> Result<(), String> {
    let mut out = [0u8; 65];
    out[1..].copy_from_slice(frame);
    d.send_feature_report(&out).map(|_| ()).map_err(|e| e.to_string())
}

fn recv_feature(d: &HidDevice) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 65];
    buf[0] = 0;
    let n = d.get_feature_report(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf[..n].to_vec())
}

fn parse_infor(r: &[u8]) -> Option<u16> {
    let p = if r.first() == Some(&0) && r.len() > 1 { &r[1..] } else { r };
    if p.len() < 3 || p[0] != 0x8F { return None; }
    Some(u16::from_le_bytes([p[1], p[2]]))
}

fn find_feature(api: &HidApi) -> Option<HidDevice> {
    for info in api.device_list() {
        if info.vendor_id() != VID || !is_normal_pid(info.product_id()) || !is_vendor_page(info.usage_page()) {
            continue;
        }
        if let Ok(d) = info.open_device(api) {
            if send_feature(&d, &bit7(&[0x8F])).is_ok() {
                sleep(Duration::from_millis(40));
                if recv_feature(&d).ok().and_then(|r| parse_infor(&r)) == Some(X65_DEV_ID) {
                    return Some(d);
                }
            }
        }
    }
    None
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(12).clamp(3, 60);

    let api = match HidApi::new() {
        Ok(x) => x,
        Err(e) => {
            println!("{}", json!({"ok":false,"error":"hidapi","message":e.to_string()}));
            return;
        }
    };

    let feature = match find_feature(&api) {
        Some(d) => d,
        None => {
            println!("{}", json!({"ok":false,"error":"no_feature_interface","message":"No X65 interface answered read-only GET_INFOR."}));
            return;
        }
    };

    // Enumerate/open every X65 normal-mode HID collection, not just a guessed
    // vendor-input usage. This is the whole point of the diagnostic.
    let infos: Vec<_> = api.device_list()
        .filter(|i| i.vendor_id() == VID && is_normal_pid(i.product_id()))
        .cloned()
        .collect();

    let mut meta = Vec::new();
    let mut opened = Vec::new();
    for (slot, info) in infos.iter().enumerate() {
        match info.open_device(&api) {
            Ok(dev) => {
                meta.push(IfaceMeta {
                    slot,
                    pid: info.product_id(),
                    usage_page: info.usage_page(),
                    usage: info.usage(),
                    interface_number: info.interface_number(),
                    path: info.path().to_string_lossy().into_owned(),
                    open_ok: true,
                    open_error: None,
                });
                opened.push(OpenIface { slot, dev });
            }
            Err(e) => meta.push(IfaceMeta {
                slot,
                pid: info.product_id(),
                usage_page: info.usage_page(),
                usage: info.usage(),
                interface_number: info.interface_number(),
                path: info.path().to_string_lossy().into_owned(),
                open_ok: false,
                open_error: Some(e.to_string()),
            }),
        }
    }

    if let Err(e) = send_feature(&feature, &bit7(&[0x1B, 1])) {
        println!("{}", json!({"ok":false,"error":"monitor_enable","message":e,"interfaces":meta}));
        return;
    }

    eprintln!("Raw X65 HID snoop active for {seconds}s. Press A fully + release, D fully + release, then both fully + release.");
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut stats: BTreeMap<usize, RxStats> = BTreeMap::new();
    let mut buf = [0u8; 128];

    while Instant::now() < deadline {
        let mut got_any = false;
        for h in &opened {
            match h.dev.read_timeout(&mut buf, 0) {
                Ok(n) if n > 0 => {
                    got_any = true;
                    let s = stats.entry(h.slot).or_default();
                    s.packets += 1;
                    if s.samples.len() < 24 {
                        s.samples.push(hex(&buf[..n]));
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    let s = stats.entry(h.slot).or_default();
                    s.read_errors += 1;
                    if s.first_error.is_none() { s.first_error = Some(e.to_string()); }
                }
            }
        }
        if !got_any { sleep(Duration::from_millis(1)); }
    }

    let disable_error = send_feature(&feature, &bit7(&[0x1B, 0])).err();

    println!("{}", json!({
        "ok": true,
        "dev_id": X65_DEV_ID,
        "seconds": seconds,
        "interfaces": meta,
        "rx": stats,
        "disable_error": disable_error,
        "note": "Only volatile SET_MAGNETISM_REPORT 0x1B was toggled. No calibration, erase, bootloader, config write, or flash command was used."
    }));
}
