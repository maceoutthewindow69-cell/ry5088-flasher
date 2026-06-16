//! RY5088 HID-DFU protocol — frame construction and checksums. Pure (no I/O), covered by unit tests.
//! See `../../docs/protocol.md` for the wire format.

pub const REPORT_LEN: usize = 64;
/// Keyboard chip-ID header at flash 0x08005000. (The 2.4G dongle's is `AT32F405 8K-DGKB` — never flash a
/// keyboard image to it; `slice_image` rejects anything without this exact keyboard chip-ID.)
pub const CHIP_ID: &[u8] = b"AT32F405 8KMKB";
pub const SLICE_OFFSET: usize = 0x5000; // the RY bootloader writes the image starting here (header + app)

/// 64-byte feature report with the Bit7 header checksum at index 7. NORMAL-mode (app) commands only —
/// the bootloader does NOT validate any header checksum, so flash frames are raw (see `build_*`).
pub fn hdr_bit7(payload: &[u8]) -> [u8; REPORT_LEN] {
    let mut r = [0u8; REPORT_LEN];
    let n = payload.len().min(7);
    r[..n].copy_from_slice(&payload[..n]);
    let s = r[..7].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    r[7] = 0xFFu8.wrapping_sub(s); // header sums to 0xFF
    r
}

/// 24-bit checksum the bootloader verifies at FW_TRANSFER_COMPLETE: sum of EVERY transmitted byte,
/// including the 0xFF padding of the final 64-byte chunk, masked to 24 bits.
pub fn fw_checksum(data: &[u8]) -> u32 {
    let pad = (REPORT_LEN - data.len() % REPORT_LEN) % REPORT_LEN;
    let s: u64 = data.iter().map(|&b| b as u64).sum::<u64>() + (pad as u64) * 0xFF;
    (s & 0xFF_FFFF) as u32
}

/// Split into 64-byte data chunks; the last chunk is 0xFF-padded (matches `fw_checksum`).
pub fn chunkify(data: &[u8]) -> Vec<[u8; REPORT_LEN]> {
    let mut out = Vec::with_capacity(data.len() / REPORT_LEN + 1);
    let mut i = 0;
    while i < data.len() {
        let mut c = [0xFFu8; REPORT_LEN];
        let end = (i + REPORT_LEN).min(data.len());
        c[..end - i].copy_from_slice(&data[i..end]);
        out.push(c);
        i += REPORT_LEN;
    }
    out
}

/// FW_TRANSFER_START: [0xBA,0xC0, chunk_count(u16 LE), size(u24 LE)].
pub fn build_start(chunk_count: u16, size: u32) -> [u8; REPORT_LEN] {
    let mut f = [0u8; REPORT_LEN];
    f[0] = 0xBA;
    f[1] = 0xC0;
    f[2] = chunk_count as u8;
    f[3] = (chunk_count >> 8) as u8;
    f[4] = size as u8;
    f[5] = (size >> 8) as u8;
    f[6] = (size >> 16) as u8;
    f
}

/// FW_TRANSFER_COMPLETE: [0xBA,0xC2, chunk_count(u16 LE), checksum(u24 LE)].
pub fn build_complete(chunk_count: u16, checksum: u32) -> [u8; REPORT_LEN] {
    let mut f = [0u8; REPORT_LEN];
    f[0] = 0xBA;
    f[1] = 0xC2;
    f[2] = chunk_count as u8;
    f[3] = (chunk_count >> 8) as u8;
    f[4] = checksum as u8;
    f[5] = (checksum >> 8) as u8;
    f[6] = (checksum >> 16) as u8;
    f
}

pub fn isp_prepare() -> [u8; REPORT_LEN] { hdr_bit7(&[0xC5, 0x3A]) }
pub fn enter_bootloader() -> [u8; REPORT_LEN] { hdr_bit7(&[0x7F, 0x55, 0xAA, 0x55, 0xAA]) }
pub fn get_infor() -> [u8; REPORT_LEN] { hdr_bit7(&[0x8F]) }

/// App vector table offset within the slice (chip-ID header is 0x200 bytes; app starts at 0x08005200).
const APP_VT: usize = 0x200;
/// Max bytes the bootloader will accept: the erased app region 0x08005000..0x08028000.
const APP_REGION_MAX: usize = 0x0002_8000 - 0x0000_5000;

/// The 0x5000-slice the RY-DFU writes, from a FULL image (chip-ID @0x5000) OR an already-sliced image
/// (chip-ID @0x0). Returns the slice only if it carries the keyboard chip-ID, a plausible app vector
/// table (initial SP in SRAM, reset handler in the app flash region with the Thumb bit), and fits the
/// erasable app region — so a chip-ID-only, truncated, or garbage image is rejected rather than flashed
/// into a non-bootable app. `None` otherwise (e.g. a dongle image, or corrupt data).
pub fn slice_image(image: &[u8]) -> Option<Vec<u8>> {
    let n = CHIP_ID.len();
    let slice: Vec<u8> = if image.len() >= 0x5200 && &image[0x5000..0x5000 + n] == CHIP_ID {
        image[SLICE_OFFSET..].to_vec()
    } else if image.len() >= n && &image[..n] == CHIP_ID {
        image.to_vec()
    } else {
        return None;
    };
    (slice.len() <= APP_REGION_MAX && valid_app_vector(&slice)).then_some(slice)
}

/// Sanity-check the app vector table at slice offset `APP_VT`: a 32-bit initial stack pointer in the SRAM
/// window and a reset-handler address in the app flash region with the Thumb bit set.
fn valid_app_vector(slice: &[u8]) -> bool {
    if slice.len() < APP_VT + 8 {
        return false;
    }
    let sp = u32::from_le_bytes(slice[APP_VT..APP_VT + 4].try_into().unwrap());
    let rv = u32::from_le_bytes(slice[APP_VT + 4..APP_VT + 8].try_into().unwrap());
    let reset = rv & !1; // handler address (clear the Thumb bit)
    (0x2000_0000..=0x2002_0000).contains(&sp)            // initial SP in SRAM
        && rv & 1 == 1                                   // reset vector carries the Thumb bit
        && (0x0800_5200..0x0802_8000).contains(&reset) // handler within the erasable app region
}

/// (dev_id, version) from a GET_INFOR reply. dev_id = the cloud dev_id (16-bit LE in bytes 1..2).
pub fn parse_infor(resp: &[u8]) -> Option<(u16, String)> {
    if resp.len() < 9 || resp[0] != 0x8F {
        return None;
    }
    let dev_id = resp[1] as u16 | ((resp[2] as u16) << 8);
    let ver = format!("v{}{:02}", resp[8], resp[7]); // bytes[8]=major, [7]=minor -> "v306"/"v302"
    Some((dev_id, ver))
}

/// ACK status byte from a GET_REPORT after COMPLETE: 0x55 = success, 0xAA = fail. ack[0] must be 0xAB.
pub fn ack_ok(ack: &[u8]) -> bool {
    ack.len() >= 5 && ack[0] == 0xAB && ack[4] == 0x55
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_bytes_match_spec() {
        // Example image: chunk_count 1403 (0x57b), size 89788 (0x15ebc), checksum 0x8687e8.
        assert_eq!(&build_start(1403, 89788)[..7], &[0xba, 0xc0, 0x7b, 0x05, 0xbc, 0x5e, 0x01]);
        assert_eq!(&build_complete(1403, 0x86_87e8)[..7], &[0xba, 0xc2, 0x7b, 0x05, 0xe8, 0x87, 0x86]);
    }

    #[test]
    fn checksum_includes_ff_pad() {
        assert_eq!(fw_checksum(&[1, 2]), 1 + 2 + 62 * 0xFF); // 15813
        let cs = chunkify(&[1, 2]);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0][2], 0xFF); // padded after the 2 data bytes
        assert_eq!(cs[0][63], 0xFF);
    }

    #[test]
    fn header_sums_to_ff() {
        let r = get_infor();
        assert_eq!(r[0], 0x8F);
        assert_eq!(r[7], 0x70); // 0xFF - 0x8F
        assert_eq!(r[..8].iter().fold(0u8, |a, &b| a.wrapping_add(b)), 0xFF);
    }

    #[test]
    fn parse_infor_decodes_dev_id() {
        assert_eq!(parse_infor(&[0x8f, 0x03, 0x09, 0, 0, 0, 0, 0x06, 0x03]), Some((2307, "v306".into())));
        assert_eq!(parse_infor(&[0x8f, 0x53, 0x09, 0, 0, 0, 0, 0x02, 0x03]), Some((2387, "v302".into())));
    }

    #[test]
    fn slice_image_validates_vector_table() {
        // Build a minimal full image: chip-ID @0x5000, a plausible app vector table @0x5200.
        let mut img = vec![0u8; 0x5208];
        img[0x5000..0x5000 + CHIP_ID.len()].copy_from_slice(CHIP_ID);
        img[0x5200..0x5204].copy_from_slice(&0x2001_0000u32.to_le_bytes()); // SP in SRAM
        img[0x5204..0x5208].copy_from_slice(&0x0800_5201u32.to_le_bytes()); // reset, Thumb, app region
        assert!(slice_image(&img).is_some());
        // chip-ID present but no/garbage vector table -> rejected.
        assert!(slice_image(CHIP_ID).is_none());
        let mut bad = img.clone();
        bad[0x5204..0x5208].copy_from_slice(&0x2000_0000u32.to_le_bytes()); // reset not in flash
        assert!(slice_image(&bad).is_none());
        // reset handler in flash but PAST the erasable app region -> rejected.
        let mut past = img.clone();
        past[0x5204..0x5208].copy_from_slice(&0x0803_0001u32.to_le_bytes());
        assert!(slice_image(&past).is_none());
        // wrong chip-ID (e.g. dongle) -> rejected.
        assert!(slice_image(b"AT32F405 8K-DGKB").is_none());
    }

    #[test]
    fn ack_status() {
        assert!(ack_ok(&[0xAB, 0xC2, 0, 0, 0x55]));
        assert!(!ack_ok(&[0xAB, 0xC2, 0, 0, 0xAA]));
    }
}
