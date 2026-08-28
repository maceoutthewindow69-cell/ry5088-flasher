//! HID transport for the RY5088 flasher: enumerate/open, GET_INFOR, enter-bootloader, and the flash stream.
//! All the byte-level protocol lives in `proto`; this module is the I/O + sequencing around it.

use crate::proto;
use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub const VID: u16 = 0x3151;
pub const PID_NORMAL: u16 = 0x5030;
pub const PID_X65_NORMAL: u16 = 0x502D;
pub const PID_BOOT: u16 = 0x502A;
const UP_NORMAL: u16 = 0xFFFF;
const UP_BOOT: u16 = 0xFF01;
const NORMAL_PIDS: &[u16] = &[PID_X65_NORMAL, PID_NORMAL];

pub struct Dev {
    api: HidApi,
}

#[derive(Debug, Serialize)]
pub struct HidProbe {
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub interface_number: i32,
    pub path: String,
    pub open_ok: bool,
    pub send_ok: bool,
    pub recv_hex: Option<String>,
    pub parsed_dev_id: Option<u16>,
    pub parsed_version: Option<String>,
    pub error: Option<String>,
}

fn ms(n: u64) -> Duration { Duration::from_millis(n) }

fn is_normal_pid(pid: u16) -> bool {
    NORMAL_PIDS.contains(&pid)
}

fn is_vendor_page(page: u16) -> bool {
    page >= 0xFF00
}

/// send_feature_report wants [report_id=0, ...64 payload bytes].
fn send(d: &HidDevice, frame: &[u8; 64]) -> Result<(), String> {
    let mut buf = [0u8; 65];
    buf[1..].copy_from_slice(frame);
    d.send_feature_report(&buf).map_err(|e| e.to_string())
}

/// Read a feature report; normalise away a possible leading report-id 0 so the first byte is the opcode echo.
fn recv(d: &HidDevice) -> Option<Vec<u8>> {
    let mut buf = [0u8; 65];
    buf[0] = 0;
    let n = d.get_feature_report(&mut buf).ok()?;
    if n == 0 { return None; }
    Some(buf[..n].to_vec())
}

fn parse_infor_reply(r: &[u8]) -> Option<(u16, String)> {
    proto::parse_infor(r).or_else(|| if r.len() > 1 { proto::parse_infor(&r[1..]) } else { None })
}

fn hex_bytes(v: &[u8]) -> String {
    v.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

impl Dev {
    pub fn new() -> Result<Self, String> {
        HidApi::new().map(|api| Dev { api }).map_err(|e| e.to_string())
    }

    pub fn refresh(&mut self) {
        let _ = self.api.refresh_devices();
    }

    fn open(&self, pid: u16, up: u16, usage: Option<u16>) -> Option<HidDevice> {
        for info in self.api.device_list() {
            if info.vendor_id() == VID
                && info.product_id() == pid
                && info.usage_page() == up
                && usage.map_or(true, |u| info.usage() == u)
            {
                if let Ok(d) = info.open_device(&self.api) {
                    return Some(d);
                }
            }
        }
        None
    }

    fn present(&self, pid: u16, up: u16) -> bool {
        self.api.device_list().any(|i| i.vendor_id() == VID && i.product_id() == pid && i.usage_page() == up)
    }

    pub fn normal_present(&self) -> bool {
        self.api.device_list().any(|i| i.vendor_id() == VID && is_normal_pid(i.product_id()))
    }

    pub fn boot_present(&self) -> bool { self.present(PID_BOOT, UP_BOOT) }

    /// Enumerate every candidate normal-mode HID interface and issue only GET_INFOR.
    /// This is a read-only diagnostic: it does not send ISP_PREPARE, enter the
    /// bootloader, erase config, or write flash.
    pub fn diagnostic_normal_interfaces(&self) -> Vec<HidProbe> {
        let mut out = Vec::new();
        for info in self.api.device_list() {
            if info.vendor_id() != VID || !is_normal_pid(info.product_id()) {
                continue;
            }

            let mut p = HidProbe {
                product_id: info.product_id(),
                usage_page: info.usage_page(),
                usage: info.usage(),
                interface_number: info.interface_number(),
                path: info.path().to_string_lossy().into_owned(),
                open_ok: false,
                send_ok: false,
                recv_hex: None,
                parsed_dev_id: None,
                parsed_version: None,
                error: None,
            };

            match info.open_device(&self.api) {
                Err(e) => p.error = Some(format!("open: {e}")),
                Ok(d) => {
                    p.open_ok = true;
                    match send(&d, &proto::get_infor()) {
                        Err(e) => p.error = Some(format!("send_feature_report: {e}")),
                        Ok(_) => {
                            p.send_ok = true;
                            sleep(ms(50));
                            let mut buf = [0u8; 65];
                            buf[0] = 0;
                            match d.get_feature_report(&mut buf) {
                                Err(e) => p.error = Some(format!("get_feature_report: {e}")),
                                Ok(0) => p.error = Some("get_feature_report returned 0 bytes".into()),
                                Ok(n) => {
                                    let r = &buf[..n];
                                    p.recv_hex = Some(hex_bytes(r));
                                    if let Some((id, ver)) = parse_infor_reply(r) {
                                        p.parsed_dev_id = Some(id);
                                        p.parsed_version = Some(ver);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            out.push(p);
        }
        out
    }

    /// Try GET_INFOR on one already-open normal-mode vendor HID interface.
    fn read_infor_on(&self, d: &HidDevice) -> Option<(u16, String)> {
        send(d, &proto::get_infor()).ok()?;
        sleep(ms(50));
        let r = recv(d)?;
        parse_infor_reply(&r)
    }

    /// Find the normal-mode vendor HID interface that actually speaks the RongYuan
    /// protocol. Most known boards use 3151:5030 / vendor page FFFF. The X65 seen
    /// in the field enumerates as 3151:502D, so probe both known normal-mode PIDs.
    /// GET_INFOR is read-only.
    fn open_normal_command(&self) -> Option<HidDevice> {
        // Preserve the known/common usage-2 path first on both normal PIDs.
        for &pid in NORMAL_PIDS {
            if let Some(d) = self.open(pid, UP_NORMAL, Some(2)) {
                if self.read_infor_on(&d).is_some() {
                    return Some(d);
                }
            }
        }

        // Fall back to any vendor-defined HID page for either normal PID. This
        // covers X65 layouts where the command interface is a separate MI_02
        // vendor-defined collection rather than the legacy FFFF/usage-2 shape.
        for info in self.api.device_list() {
            if info.vendor_id() != VID
                || !is_normal_pid(info.product_id())
                || !is_vendor_page(info.usage_page())
            {
                continue;
            }
            if let Ok(d) = info.open_device(&self.api) {
                if self.read_infor_on(&d).is_some() {
                    return Some(d);
                }
            }
        }
        None
    }

    /// Read GET_INFOR from the connected keyboard (normal mode). Returns (dev_id, version).
    pub fn read_infor(&self) -> Option<(u16, String)> {
        // Try the historical usage-2 interface first for each accepted PID.
        for &pid in NORMAL_PIDS {
            if let Some(d) = self.open(pid, UP_NORMAL, Some(2)) {
                if let Some(info) = self.read_infor_on(&d) {
                    return Some(info);
                }
            }
        }

        // Then probe every vendor-defined collection for 3151:502D/5030.
        for info in self.api.device_list() {
            if info.vendor_id() != VID
                || !is_normal_pid(info.product_id())
                || !is_vendor_page(info.usage_page())
            {
                continue;
            }
            if let Ok(d) = info.open_device(&self.api) {
                if let Some(reply) = self.read_infor_on(&d) {
                    return Some(reply);
                }
            }
        }
        None
    }

    /// Open the normal-mode command interface and read its USB strings.
    pub fn usb_strings(&self) -> (Option<String>, Option<String>, Option<String>) {
        match self.open_normal_command() {
            Some(d) => (
                d.get_manufacturer_string().ok().flatten(),
                d.get_product_string().ok().flatten(),
                d.get_serial_number_string().ok().flatten(),
            ),
            None => (None, None, None),
        }
    }

    /// Send the enter-bootloader sequence (WIPES config) and wait up to ~20s for re-enumeration to 502A.
    pub fn enter_bootloader(&mut self) -> Result<(), String> {
        let d = self.open_normal_command().ok_or("normal device found, but no vendor HID interface answered GET_INFOR")?;
        send(&d, &proto::isp_prepare()).map_err(|e| format!("enter-bootloader (ISP_PREPARE) failed: {e}"))?;
        sleep(ms(100));
        // This command resets the device, so its transfer may report an error as the port drops — that is
        // expected; the authoritative signal is re-enumeration to the bootloader, polled below.
        let _ = send(&d, &proto::enter_bootloader());
        drop(d);
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(20) {
            sleep(ms(500));
            self.refresh();
            if self.boot_present() {
                return Ok(());
            }
        }
        Err("did not re-enumerate to bootloader (502A) within 20s".into())
    }

    /// Flash a 0x5000-slice while in bootloader mode. `progress(done, total)` per chunk. Returns the ACK
    /// (None if the device reset before ACK — treat success via re-enumeration, see `wait_normal`).
    pub fn flash_slice<F: FnMut(usize, usize)>(&self, slice: &[u8], mut progress: F) -> Result<Option<bool>, String> {
        let d = self.open(PID_BOOT, UP_BOOT, None).ok_or("not in bootloader mode (3151:502A)")?;
        let chunks = proto::chunkify(slice);
        if chunks.len() > u16::MAX as usize {
            return Err(format!("image too large: {} chunks exceeds the 16-bit transfer limit", chunks.len()));
        }
        let cc = chunks.len() as u16;
        let cks = proto::fw_checksum(slice);
        let size = slice.len() as u32;

        send(&d, &proto::build_start(cc, size))?;
        sleep(ms(30));
        for (i, ch) in chunks.iter().enumerate() {
            send(&d, ch)?;
            progress(i + 1, chunks.len());
            sleep(ms(2));
        }
        sleep(ms(30));
        send(&d, &proto::build_complete(cc, cks))?;
        sleep(ms(60));
        // best-effort ACK: a successful flash resets the device immediately, so this may legitimately fail.
        match recv(&d) {
            Some(a) => {
                let ok = proto::ack_ok(&a) || (a.len() > 1 && proto::ack_ok(&a[1..]));
                let fail = (a.len() > 4 && a[0] == 0xAB && a[4] == 0xAA)
                    || (a.len() > 5 && a[1] == 0xAB && a[5] == 0xAA);
                Ok(if ok { Some(true) } else if fail { Some(false) } else { None })
            }
            None => Ok(None), // device likely reset -> verify by re-enumeration
        }
    }

    /// Wait up to ~20s for return to normal mode (the authoritative success signal after a flash).
    pub fn wait_normal(&mut self) -> bool {
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(20) {
            sleep(ms(500));
            self.refresh();
            if self.normal_present() {
                return true;
            }
        }
        false
    }
}
