//! depth_monitor — temporary Hall-depth capture for X65 bring-up.
//!
//! This helper does NOT enter the bootloader, erase config, calibrate sensors,
//! or write firmware. It verifies GET_INFOR == dev_id 2268, enables the stock
//! firmware's volatile magnetism-report stream (SET 0x1B = 1), records vendor
//! input events for a short interval, then disables the stream (SET 0x1B = 0).

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
const A_INDEX: u8 = 9;
const D_INDEX: u8 = 21;

#[derive(Clone, Debug, Default, Serialize)]
struct Stats {
    count: u64,
    min: Option<u16>,
    max: Option<u16>,
    last: Option<u16>,
}

impl Stats {
    fn add(&mut self, v: u16) {
        self.count += 1;
        self.min = Some(self.min.map_or(v, |x| x.min(v)));
        self.max = Some(self.max.map_or(v, |x| x.max(v)));
        self.last = Some(v);
    }
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
    d.send_feature_report(&out)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn recv_feature(d: &HidDevice) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 65];
    buf[0] = 0;
    let n = d.get_feature_report(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf[..n].to_vec())
}

fn parse_infor(r: &[u8]) -> Option<u16> {
    let p = if r.first() == Some(&0) && r.len() > 1 { &r[1..] } else { r };
    if p.len() < 3 || p[0] != 0x8F {
        return None;
    }
    Some(u16::from_le_bytes([p[1], p[2]]))
}

fn find_feature(api: &HidApi) -> Option<HidDevice> {
    // Known protocol collection first: vendor page, usage 2.
    for info in api.device_list() {
        if info.vendor_id() == VID
            && is_normal_pid(info.product_id())
            && is_vendor_page(info.usage_page())
            && info.usage() == 2
        {
            if let Ok(d) = info.open_device(api) {
                if send_feature(&d, &bit7(&[0x8F])).is_ok() {
                    sleep(Duration::from_millis(40));
                    if recv_feature(&d).ok().and_then(|r| parse_infor(&r)).is_some() {
                        return Some(d);
                    }
                }
            }
        }
    }

    // Fallback to any vendor-defined collection that answers GET_INFOR.
    for info in api.device_list() {
        if info.vendor_id() != VID || !is_normal_pid(info.product_id()) || !is_vendor_page(info.usage_page()) {
            continue;
        }
        if let Ok(d) = info.open_device(api) {
            if send_feature(&d, &bit7(&[0x8F])).is_ok() {
                sleep(Duration::from_millis(40));
                if recv_feature(&d).ok().and_then(|r| parse_infor(&r)).is_some() {
                    return Some(d);
                }
            }
        }
    }
    None
}

fn find_input(api: &HidApi) -> Option<HidDevice> {
    // Stock RY gen2 vendor-input collection: usage page FFFF-ish, usage 1.
    for info in api.device_list() {
        if info.vendor_id() == VID
            && is_normal_pid(info.product_id())
            && is_vendor_page(info.usage_page())
            && info.usage() == 1
        {
            if let Ok(d) = info.open_device(api) {
                return Some(d);
            }
        }
    }
    None
}

fn parse_depth_event(data: &[u8]) -> Option<(u8, u16)> {
    if data.is_empty() {
        return None;
    }

    // Windows hidapi normally returns report ID 0x05 first. Some backends may
    // omit it, so accept both [05,1B,lo,hi,idx] and [1B,lo,hi,idx].
    let p = if data[0] == 0x05 && data.len() >= 5 {
        &data[1..]
    } else {
        data
    };
    if p.len() < 4 || p[0] != 0x1B {
        return None;
    }
    let depth = u16::from_le_bytes([p[1], p[2]]);
    let index = p[3];
    Some((index, depth))
}

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(12)
        .clamp(3, 60);

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
            println!("{}", json!({"ok":false,"error":"no_feature_interface","message":"Close Sharkfin/qmk.top/vendor-app tabs, reconnect the X65 by USB, then retry."}));
            return;
        }
    };

    if let Err(e) = send_feature(&feature, &bit7(&[0x8F])) {
        println!("{}", json!({"ok":false,"error":"get_infor_send","message":e}));
        return;
    }
    sleep(Duration::from_millis(40));
    let dev_id = match recv_feature(&feature).ok().and_then(|r| parse_infor(&r)) {
        Some(x) => x,
        None => {
            println!("{}", json!({"ok":false,"error":"get_infor_reply","message":"Keyboard did not return a valid GET_INFOR reply."}));
            return;
        }
    };
    if dev_id != X65_DEV_ID {
        println!("{}", json!({"ok":false,"error":"wrong_device","dev_id":dev_id,"expected":X65_DEV_ID}));
        return;
    }

    let input = match find_input(&api) {
        Some(d) => d,
        None => {
            println!("{}", json!({"ok":false,"error":"no_input_interface","message":"Could not open the vendor input collection (usage 1). Close apps/tabs using the keyboard and retry."}));
            return;
        }
    };

    // Volatile monitor toggle only. This is NOT calibration (0x1C/0x1E),
    // bootloader entry, config erase, or firmware write.
    if let Err(e) = send_feature(&feature, &bit7(&[0x1B, 1])) {
        println!("{}", json!({"ok":false,"error":"monitor_enable","message":e}));
        return;
    }

    eprintln!("Depth capture active for {seconds}s. Press A fully + release, D fully + release, then both fully + release.");
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut stats: BTreeMap<u8, Stats> = BTreeMap::new();
    let mut read_error: Option<String> = None;
    let mut buf = [0u8; 64];

    while Instant::now() < deadline {
        match input.read_timeout(&mut buf, 50) {
            Ok(n) if n > 0 => {
                if let Some((idx, depth)) = parse_depth_event(&buf[..n]) {
                    stats.entry(idx).or_default().add(depth);
                }
            }
            Ok(_) => {}
            Err(e) => {
                read_error = Some(e.to_string());
                break;
            }
        }
    }

    let disable_error = send_feature(&feature, &bit7(&[0x1B, 0])).err();
    let a = stats.get(&A_INDEX).cloned().unwrap_or_default();
    let d = stats.get(&D_INDEX).cloned().unwrap_or_default();

    println!("{}", json!({
        "ok": read_error.is_none(),
        "dev_id": dev_id,
        "seconds": seconds,
        "a_index": A_INDEX,
        "d_index": D_INDEX,
        "a": a,
        "d": d,
        "observed_keys": stats,
        "read_error": read_error,
        "disable_error": disable_error,
        "note": "No bootloader, calibration, erase, or flash command is used. SET_MAGNETISM_REPORT is disabled before exit."
    }));
}
