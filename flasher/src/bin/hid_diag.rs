use hidapi::HidApi;
use serde_json::json;
use std::thread::sleep;
use std::time::Duration;

const VID: u16 = 0x3151;
const PID: u16 = 0x5030;

fn get_infor_frame() -> [u8; 64] {
    let mut r = [0u8; 64];
    r[0] = 0x8F;
    let s = r[..7].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    r[7] = 0xFFu8.wrapping_sub(s);
    r
}

fn hex(v: &[u8]) -> String {
    v.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

fn parse(resp: &[u8]) -> Option<(u16, String)> {
    let r = if resp.first() == Some(&0) && resp.len() > 1 { &resp[1..] } else { resp };
    if r.len() < 9 || r[0] != 0x8F { return None; }
    let id = r[1] as u16 | ((r[2] as u16) << 8);
    Some((id, format!("v{}{:02}", r[8], r[7])))
}

fn main() {
    let api = match HidApi::new() {
        Ok(x) => x,
        Err(e) => {
            println!("{}", json!({"ok":false,"error":"hidapi","message":e.to_string()}));
            return;
        }
    };

    let mut interfaces = Vec::new();
    for info in api.device_list() {
        if info.vendor_id() != VID || info.product_id() != PID { continue; }

        let mut open_ok = false;
        let mut send_ok = false;
        let mut recv_hex = None;
        let mut parsed_dev_id = None;
        let mut parsed_version = None;
        let mut error = None;

        match info.open_device(&api) {
            Err(e) => error = Some(format!("open: {e}")),
            Ok(d) => {
                open_ok = true;
                let frame = get_infor_frame();
                let mut out = [0u8; 65];
                out[1..].copy_from_slice(&frame);
                match d.send_feature_report(&out) {
                    Err(e) => error = Some(format!("send_feature_report: {e}")),
                    Ok(_) => {
                        send_ok = true;
                        sleep(Duration::from_millis(75));
                        let mut input = [0u8; 65];
                        input[0] = 0;
                        match d.get_feature_report(&mut input) {
                            Err(e) => error = Some(format!("get_feature_report: {e}")),
                            Ok(0) => error = Some("get_feature_report returned 0 bytes".into()),
                            Ok(n) => {
                                let r = &input[..n];
                                recv_hex = Some(hex(r));
                                if let Some((id, ver)) = parse(r) {
                                    parsed_dev_id = Some(id);
                                    parsed_version = Some(ver);
                                }
                            }
                        }
                    }
                }
            }
        }

        interfaces.push(json!({
            "usage_page": format!("0x{:04X}", info.usage_page()),
            "usage": format!("0x{:04X}", info.usage()),
            "interface_number": info.interface_number(),
            "path": info.path().to_string_lossy(),
            "open_ok": open_ok,
            "send_ok": send_ok,
            "recv_hex": recv_hex,
            "parsed_dev_id": parsed_dev_id,
            "parsed_version": parsed_version,
            "error": error
        }));
    }

    println!("{}", json!({
        "ok": true,
        "vid": "0x3151",
        "pid": "0x5030",
        "count": interfaces.len(),
        "interfaces": interfaces
    }));
}
