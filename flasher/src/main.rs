//! ry-flash — generic stock-firmware flasher + TUI for RongYuan RY5088 (AT32F405) magnetic keyboards.
//! Speaks the RY HID-DFU protocol; auto-detects the connected dev_id and flashes its matching stock image
//! (PID is shared across models, so dev_id is the only safe discriminator). Default = TUI; headless flags below.
//!
//! AGENT/SCRIPT MODE: pass `--json` to any headless command for structured stdout (progress goes to stderr).
//! Exit codes: 0 ok · 1 error · 2 no device · 3 no image · 4 flash failed.

mod device;
mod devmap;
mod fetch;
mod firmware;
mod proto;
mod tui;

use serde_json::json;
use std::path::PathBuf;
use std::process::ExitCode;

const EXIT_OK: u8 = 0;
const EXIT_ERR: u8 = 1;
const EXIT_NO_DEVICE: u8 = 2;
const EXIT_NO_IMAGE: u8 = 3;
const EXIT_FLASH_FAIL: u8 = 4;

struct Args {
    detect: bool,
    probe: bool,
    auto: bool,
    list: bool,
    arm: bool,
    json: bool,
    no_fetch: bool,
    fetch: Option<u16>,
    image: Option<PathBuf>,
    stock_dir: PathBuf,
}

fn default_stock_dir() -> PathBuf {
    std::env::var("RY_FLASH_STOCK").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("firmware"))
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        detect: false, probe: false, auto: false, list: false, arm: false, json: false,
        no_fetch: false, fetch: None, image: None, stock_dir: default_stock_dir(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--detect" => a.detect = true,
            "--probe" => a.probe = true,
            "--auto" => a.auto = true,
            "--list" => a.list = true,
            "--arm" => a.arm = true,
            "--json" => a.json = true,
            "--no-fetch" => a.no_fetch = true,
            "--fetch" => {
                let v = it.next().ok_or("--fetch needs a dev_id")?;
                a.fetch = Some(v.parse::<u16>().map_err(|_| format!("--fetch: invalid dev_id {v:?}"))?);
            }
            "--image" => a.image = Some(PathBuf::from(it.next().ok_or("--image needs a path")?)),
            "--stock-dir" => a.stock_dir = PathBuf::from(it.next().ok_or("--stock-dir needs a path")?),
            "--version" => {
                println!("ry-flash {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    Ok(a)
}

fn print_help() {
    print!(
        "ry-flash — RY5088 stock-firmware flasher (RongYuan magnetic keyboards)\n\n\
         (no args)             launch the interactive TUI\n\
         --detect              read dev_id / model / version (read-only)\n\
         --probe               detect + fetch stock fw + USB strings -> one JSON evidence bundle (add-a-board)\n\
         --auto [--arm]        detect + flash the connected board's MATCHING stock image (dry-run w/o --arm)\n\
         --image PATH [--arm]  flash a specific image (full or 0x5000-sliced)\n\
         --fetch <dev_id>      download that model's stock image from the vendor cloud into --stock-dir\n\
         --no-fetch            with --auto, don't auto-download a missing image (fail with no_image instead)\n\
         --list                list local stock images under --stock-dir\n\
         --stock-dir DIR       where <dev_id>/fw_at32.bin images live (default ./firmware or $RY_FLASH_STOCK)\n\
         --json                machine-readable JSON on stdout (for agents/scripts; progress -> stderr)\n\
         --version, --help\n\n\
         Exit codes: 0 ok · 1 error · 2 no device · 3 no image · 4 flash failed.\n"
    );
}

/// Emit a single JSON object (compact, one line) to stdout.
fn emit(v: serde_json::Value) {
    println!("{v}");
}

/// Stable non-crypto fingerprint of a blob (FNV-1a 64-bit) for the evidence bundle.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Report a failure in the right format and return its exit code.
fn fail(json: bool, code: u8, error: &str, message: &str) -> ExitCode {
    if json {
        emit(json!({"ok": false, "error": error, "message": message}));
    } else {
        eprintln!("{message}");
    }
    ExitCode::from(code)
}

fn cmd_detect(a: &Args) -> ExitCode {
    let dev = match device::Dev::new() {
        Ok(d) => d,
        Err(e) => return fail(a.json, EXIT_ERR, "hidapi", &e),
    };
    match dev.read_infor() {
        Some((id, ver)) => {
            let rec = devmap::lookup(id);
            if a.json {
                emit(json!({
                    "ok": true, "dev_id": id, "version": ver,
                    "display_name": rec.as_ref().map(|r| r.display_name.clone()),
                    "name": rec.as_ref().map(|r| r.name.clone()),
                }));
            } else {
                println!("dev_id {id} = {}, {ver}", devmap::label(id));
            }
            ExitCode::SUCCESS
        }
        None => fail(a.json, EXIT_NO_DEVICE, "no_device", "no GET_INFOR (keyboard in normal mode 3151:5030, app closed?)"),
    }
}

/// `--probe`: one-shot evidence bundle for adding a board — read the dev_id/version + USB strings, then
/// fetch this model's stock firmware and report its size, embedded chip-ID and a stable fingerprint.
/// Read-only on the device; composes the existing detect + fetch paths into a single JSON object.
fn cmd_probe(a: &Args) -> ExitCode {
    let dev = match device::Dev::new() {
        Ok(d) => d,
        Err(e) => return fail(a.json, EXIT_ERR, "hidapi", &e),
    };
    let (id, ver) = match dev.read_infor() {
        Some(x) => x,
        None => return fail(a.json, EXIT_NO_DEVICE, "no_device", "no GET_INFOR (keyboard in normal mode 3151:5030, app closed?)"),
    };
    let rec = devmap::lookup(id);
    let (mfr, prod, serial) = dev.usb_strings();
    if !a.json {
        eprintln!("probing dev {id}; fetching its stock firmware...");
    }
    let path = match firmware::local_image(&a.stock_dir, id) {
        Some(p) => p,
        None if a.no_fetch => {
            return fail(a.json, EXIT_NO_IMAGE, "no_image",
                &format!("no local stock image for dev {id} (drop --no-fetch to download it)"));
        }
        None => match fetch::fetch_to(id, &a.stock_dir) {
            Ok(p) => p,
            Err(e) => return fail(a.json, EXIT_NO_IMAGE, "fetch_failed", &e),
        },
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return fail(a.json, EXIT_ERR, "read_failed", &format!("read {}: {e}", path.display())),
    };
    let chip = firmware::chip_id(&bytes);
    let fp = fnv1a(&bytes);
    if a.json {
        emit(json!({
            "ok": true, "dev_id": id, "version": ver,
            "display_name": rec.as_ref().map(|r| r.display_name.clone()),
            "name": rec.as_ref().map(|r| r.name.clone()),
            "usb": {
                "vid": format!("{:#06x}", device::VID),
                "pid": format!("{:#06x}", device::PID_NORMAL),
                "manufacturer": mfr, "product": prod, "serial": serial,
            },
            "firmware": {
                "path": path.display().to_string(), "bytes": bytes.len(),
                "chip_id": chip, "fnv1a": format!("{fp:#018x}"),
            },
        }));
    } else {
        println!("dev_id {id} = {} {ver}", devmap::label(id));
        if let Some(p) = &prod { println!("  usb product : {p}"); }
        if let Some(m) = &mfr { println!("  usb vendor  : {m}"); }
        println!("  stock fw    : {} ({} B){}", path.display(), bytes.len(),
            chip.map(|c| format!(", chip-id \"{c}\"")).unwrap_or_default());
    }
    ExitCode::SUCCESS
}

fn cmd_list(a: &Args) -> ExitCode {
    let mut images: Vec<serde_json::Value> = Vec::new();
    let mut human: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&a.stock_dir) {
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if let Some(id) = e.file_name().to_str().and_then(|s| s.parse::<u16>().ok()) {
                let p = e.path().join("fw_at32.bin");
                if p.is_file() {
                    let bytes = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                    let rec = devmap::lookup(id);
                    let dn = rec.as_ref().map(|r| r.display_name.clone()).unwrap_or_default();
                    let nm = rec.as_ref().map(|r| r.name.clone()).unwrap_or_default();
                    human.push(format!("  {id:5}  {dn:14}  {nm}  ({bytes} B)"));
                    images.push(json!({"dev_id": id, "display_name": dn, "name": nm, "path": p.display().to_string(), "bytes": bytes}));
                }
            }
        }
    }
    if a.json {
        emit(json!({"ok": true, "stock_dir": a.stock_dir.display().to_string(), "images": images}));
    } else {
        println!("local stock images in {} ({}):", a.stock_dir.display(), images.len());
        for l in human {
            println!("{l}");
        }
    }
    ExitCode::SUCCESS
}

fn cmd_fetch(a: &Args, dev_id: u16) -> ExitCode {
    if !a.json {
        eprintln!("fetching stock firmware for dev_id {dev_id} from the vendor cloud...");
    }
    let (image, version) = match fetch::fetch_image(dev_id) {
        Ok(x) => x,
        Err(e) => return fail(a.json, EXIT_NO_IMAGE, "fetch_failed", &e),
    };
    let bytes = image.len();
    let path = match fetch::save_image(dev_id, &a.stock_dir, &image) {
        Ok(p) => p,
        Err(e) => return fail(a.json, EXIT_ERR, "fetch_failed", &e),
    };
    if a.json {
        emit(json!({
            "ok": true, "dev_id": dev_id, "version": version,
            "path": path.display().to_string(), "bytes": bytes,
        }));
    } else {
        let label = devmap::lookup(dev_id).map(|r| r.display_name).unwrap_or_default();
        println!("fetched dev {dev_id} {label} {version} -> {} ({bytes} bytes)", path.display());
    }
    ExitCode::SUCCESS
}

fn cmd_auto(a: &Args) -> ExitCode {
    let mut dev = match device::Dev::new() {
        Ok(d) => d,
        Err(e) => return fail(a.json, EXIT_ERR, "hidapi", &e),
    };
    // Recovery: if the board is already in its bootloader (e.g. a previous flash was interrupted), its
    // dev_id cannot be read — flash an explicitly-supplied --image directly to bring it back.
    if dev.boot_present() {
        return cmd_recover(a, &mut dev);
    }
    let (id, ver) = match dev.read_infor() {
        Some(x) => x,
        None => return fail(a.json, EXIT_NO_DEVICE, "no_device", "could not read GET_INFOR (keyboard in normal mode, app closed?)"),
    };
    let rec = devmap::lookup(id);
    if let Some(r) = &rec {
        if !r.is_ry5088() {
            return fail(a.json, EXIT_ERR, "wrong_platform", &format!(
                "dev {id} = {} is a {} board, not RY5088 (AT32F405) — this flasher cannot flash it",
                r.display_name, r.platform));
        }
    }
    let path = match &a.image {
        Some(p) => p.clone(),
        None => match firmware::local_image(&a.stock_dir, id) {
            Some(p) => p,
            None if a.no_fetch => {
                return fail(a.json, EXIT_NO_IMAGE,
                    "no_image", &format!("no local stock image for dev {id} under {} (use --image, or drop --no-fetch to download it)", a.stock_dir.display()));
            }
            None => {
                // self-contained: pull the matching stock image from the vendor cloud. Log to stderr so
                // stdout stays a single clean JSON object.
                eprintln!("no local image for dev {id}; fetching from the vendor cloud...");
                match fetch::fetch_to(id, &a.stock_dir) {
                    Ok(p) => {
                        eprintln!("fetched -> {}", p.display());
                        p
                    }
                    Err(e) => {
                        return fail(a.json, EXIT_NO_IMAGE,
                            "fetch_failed", &format!("no local image for dev {id} and auto-fetch failed: {e}"));
                    }
                }
            }
        },
    };
    let slice = match firmware::load_slice(&path) {
        Ok(s) => s,
        Err(e) => return fail(a.json, EXIT_ERR, "bad_image", &e),
    };
    let cks = proto::fw_checksum(&slice);
    let chunks = slice.len().div_ceil(64);

    if !a.arm {
        if a.json {
            emit(json!({
                "ok": true, "armed": false, "dev_id": id, "version": ver,
                "display_name": rec.as_ref().map(|r| r.display_name.clone()),
                "image": path.display().to_string(), "slice_bytes": slice.len(),
                "chunks": chunks, "checksum": format!("{cks:#08x}"),
                "plan": "would enter-boot (wipes config), flash, verify; pass --arm to do it",
            }));
        } else {
            println!("detected dev_id {id} = {}, running {ver}", devmap::label(id));
            println!("image {} -> {} B slice, {chunks} chunks, checksum {cks:#08x}", path.display(), slice.len());
            println!("DRY-RUN: would enter-boot (WIPES config), flash, verify. Pass --arm to do it.");
        }
        return ExitCode::SUCCESS;
    }

    // ---- armed flash; progress to stderr so stdout stays clean for JSON ----
    if !a.json {
        println!("detected dev_id {id} = {}, running {ver}; flashing {}", devmap::label(id), path.display());
    }
    match flash_round_trip(&mut dev, &slice) {
        Ok((post_id, post_ver)) => {
            if let Some(pid) = post_id {
                if pid != id {
                    // Flashed, but the board now identifies as a different model — a variant-safety failure.
                    if a.json {
                        emit(json!({"ok": false, "error": "post_flash_mismatch", "dev_id": id, "post_dev_id": pid,
                            "message": "flash completed but the board now reports a different dev_id"}));
                    } else {
                        eprintln!("MISMATCH: flashed dev {id} but the board now reports {pid}");
                    }
                    return ExitCode::from(EXIT_FLASH_FAIL);
                }
            }
            let verified = post_id == Some(id);
            if a.json {
                emit(json!({
                    "ok": true, "armed": true, "flashed": true, "dev_id": id,
                    "checksum": format!("{cks:#08x}"), "post_dev_id": post_id, "post_version": post_ver,
                    "match": verified, "message": "flash ok; config wiped — set actuation + calibrate in the app",
                }));
            } else {
                let note = if verified { "MATCH" } else { "post-flash dev_id unread" };
                println!("OK — flashed dev {id} ({note}). Config wiped: set actuation + calibrate in the app.");
            }
            ExitCode::SUCCESS
        }
        Err((code, e)) => fail(a.json, code, "flash_failed", &e),
    }
}

/// Flash directly from the bootloader when the dev_id can't be read (a board stuck in DFU). Requires an
/// explicit `--image`; verifies re-enumeration afterward and reports the recovered dev_id.
fn cmd_recover(a: &Args, dev: &mut device::Dev) -> ExitCode {
    let path = match &a.image {
        Some(p) => p.clone(),
        None => {
            return fail(a.json, EXIT_NO_IMAGE, "no_image",
                "board is in the bootloader (502A); supply --image <stock image> to recover (the dev_id cannot be read in this mode)");
        }
    };
    let slice = match firmware::load_slice(&path) {
        Ok(s) => s,
        Err(e) => return fail(a.json, EXIT_ERR, "bad_image", &e),
    };
    let cks = proto::fw_checksum(&slice);
    if !a.arm {
        if a.json {
            emit(json!({"ok": true, "armed": false, "mode": "recover", "image": path.display().to_string(),
                "checksum": format!("{cks:#08x}"), "plan": "board is in the bootloader; would flash this image"}));
        } else {
            println!("board in bootloader (502A). DRY-RUN: would flash {} (checksum {cks:#08x}). Pass --arm.", path.display());
        }
        return ExitCode::SUCCESS;
    }
    if !a.json {
        println!("board in bootloader (502A); recovering with {}", path.display());
    }
    match flash_round_trip(dev, &slice) {
        Ok((post_id, post_ver)) => {
            if a.json {
                emit(json!({"ok": true, "armed": true, "flashed": true, "mode": "recover",
                    "checksum": format!("{cks:#08x}"), "post_dev_id": post_id, "post_version": post_ver,
                    "message": "recovery flash ok; config wiped — set actuation + calibrate in the app"}));
            } else {
                match post_id {
                    Some(pid) => println!("OK — recovered; board now reports dev {pid} {post_ver}."),
                    None => println!("OK — recovered (post-flash dev_id unread)."),
                }
            }
            ExitCode::SUCCESS
        }
        Err((code, e)) => fail(a.json, code, "flash_failed", &e),
    }
}

/// Flash, then verify re-enumeration to normal mode. Returns the post-flash `(dev_id, version)` — `dev_id`
/// is `None` if the board re-enumerated but GET_INFOR could not be read. `Err((exit_code, message))` on a
/// flash failure. Progress goes to stderr. Enters the bootloader first only if not already in it.
fn flash_round_trip(dev: &mut device::Dev, slice: &[u8]) -> Result<(Option<u16>, String), (u8, String)> {
    use std::io::Write;
    if !dev.boot_present() {
        eprint!("entering bootloader (wipes config)... ");
        let _ = std::io::stderr().flush();
        dev.enter_bootloader().map_err(|e| (EXIT_FLASH_FAIL, e))?;
        eprintln!("502A present.");
    }
    let r = dev.flash_slice(slice, |done, tot| {
        if done % 250 == 0 || done == tot {
            eprint!("\r  chunk {done}/{tot}   ");
            let _ = std::io::stderr().flush();
        }
    });
    eprintln!();
    match r {
        Ok(Some(false)) => return Err((EXIT_FLASH_FAIL, "checksum mismatch (ACK 0xAA) — board stays in 502A, safe to re-run".into())),
        Err(e) => return Err((EXIT_FLASH_FAIL, e)),
        _ => {}
    }
    eprint!("verifying re-enumeration to normal mode... ");
    let _ = std::io::stderr().flush();
    if dev.wait_normal() {
        match dev.read_infor() {
            Some((id2, ver2)) => {
                eprintln!("ok");
                Ok((Some(id2), ver2))
            }
            None => {
                eprintln!("ok (dev_id unread)");
                Ok((None, String::new()))
            }
        }
    } else {
        Err((EXIT_FLASH_FAIL, "did not return to normal mode (likely still in 502A; re-run)".into()))
    }
}

fn main() -> ExitCode {
    let a = match parse() {
        Ok(a) => a,
        Err(e) => {
            if std::env::args().any(|x| x == "--json") {
                emit(json!({"ok": false, "error": "bad_args", "message": e}));
            } else {
                eprintln!("{e}\n");
                print_help();
            }
            return ExitCode::from(EXIT_ERR);
        }
    };
    if a.detect {
        cmd_detect(&a)
    } else if a.probe {
        cmd_probe(&a)
    } else if a.list {
        cmd_list(&a)
    } else if let Some(id) = a.fetch {
        cmd_fetch(&a, id)
    } else if a.auto || a.image.is_some() {
        cmd_auto(&a)
    } else {
        match tui::run(a.stock_dir) {
            Ok(()) => ExitCode::from(EXIT_OK),
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(EXIT_ERR)
            }
        }
    }
}
