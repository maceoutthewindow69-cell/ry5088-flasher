# RY5088 HID-DFU Flash Protocol

This document specifies the HID-based device-firmware-update (DFU) protocol used by
the RongYuan **RY5088** keyboard platform (Artery **AT32F405** main MCU). It covers the
USB identities the device presents, the command transport, the sequence that puts the
keyboard into its resident bootloader, the firmware transfer framing, the integrity
check, and the boot decision the bootloader makes on reset. It also documents the
vendor cloud firmware API used to obtain stock images.

The protocol is implemented host-side by the [flasher](flashing.md) and device-side by
the resident bootloader at `0x08000000`. For board-level context (flash map, MCU,
peripherals) see [hardware.md](hardware.md); for the application-level vendor protocol
see [firmware.md](firmware.md).

---

## 1. USB identities

The keyboard presents two distinct USB identities depending on whether the application
or the bootloader is running.

| State | VID:PID | Interface | HID usage page | Reports |
|---|---|---|---|---|
| **Normal** (application running) | `3151:5030` | Vendor HID | `0xFFFF` | 64-byte Feature reports, report ID 0 |
| **Bootloader** (DFU) | `3151:502A` | Vendor HID | `0xFF01` | 64-byte Feature reports, report ID 0 |

All host-to-device traffic in both states is a 64-byte HID **SET_REPORT (Feature)** with
report ID 0. Status is read back with **GET_REPORT (Feature)**.

> **PID is not a model discriminator.** A large set of RY5088 models share normal-mode
> PID `0x5030`. The only safe per-variant identifier is the firmware `dev_id` returned by
> the `GET_INFOR` command (see [flashing.md](flashing.md)). The bootloader PID `0x502A`
> is shared across the whole family.

The vendor HID report descriptor presented in normal mode is:

```
06 ff ff 09 02 a1 01 09 02 15 80 25 7f 95 40 75 08 b1 02 c0
```

(usage page `0xFFFF`, one 64-byte Feature report, report ID 0).

---

## 2. Normal-mode command transport

Normal-mode (`0x5030`) vendor commands use a fixed 64-byte report. The first bytes form
a command header protected by an additive checksum byte; the remainder is unused padding.

| Byte | Field |
|---|---|
| 0 | opcode |
| 1..6 | arguments (6 bytes) |
| 7 | header checksum (**Bit7** form) |
| 8 | header checksum (**Bit8** form — LED commands only) |
| 9..63 | unused |

**Header checksum rule:**

- **Bit7 (default):** the first 8 bytes sum to `0xFF` modulo 256. The checksum byte is
  byte 7, computed as `0xFF - (sum(byte[0..6]) & 0xFF)`.
- **Bit8 (LED commands `0x07`, `0x08`, and their reads):** the first 9 bytes sum to `0xFF`
  modulo 256; the checksum byte is byte 8.

A command whose header does not sum to `0xFF` is rejected. Read commands are formed from
the corresponding write opcode by setting the high bit: `GET = SET | 0x80`
(for example `SET_PROFILE = 0x04`, `GET_PROFILE = 0x84`). The full opcode table is in
[firmware.md](firmware.md).

---

## 3. Enter-bootloader sequence

Two normal-mode commands move the device from the application into the resident
bootloader. Both carry the Bit7 header checksum described above.

| Step | Opcode | Payload | Effect |
|---|---|---|---|
| 1. ISP_PREPARE | `0xC5` | `0x3A` | Arms the in-system-programming path. |
| 2. ENTER_BOOTLOADER | `0x7F` | `0x55 0xAA 0x55 0xAA` | Wipes the application config region, writes the bootloader mailbox, resets into DFU. |

On `ENTER_BOOTLOADER` the application:

1. Erases the configuration/keymap/macro/userpic pages (config region at `0x08028000`).
   Hall calibration data at `0x08032000` is **preserved**.
2. Writes the boot mailbox at `0x08004800` to the value `0x55AA55AA`.
3. Issues a software reset.

After reset the device re-enumerates as `3151:502A` with HID usage page `0xFF01`.

---

## 4. Bootloader transfer protocol

In bootloader mode (`0x502A`) frames are **raw** — there is no header checksum on
individual frames; integrity is guaranteed by the 24-bit transfer checksum verified at
completion. The transfer is three phases.

### 4.1 FW_TRANSFER_START

| Byte | Value | Meaning |
|---|---|---|
| 0 | `0xBA` | frame tag |
| 1 | `0xC0` | START sub-op |
| 2..3 | `chunk_count` | total DATA chunks, u16 little-endian |
| 4..6 | `size` | total payload size in bytes, u24 little-endian |

`size` is recorded but does not gate the transfer.

### 4.2 DATA chunks

`chunk_count` consecutive reports, each carrying **64 raw firmware bytes** with no header,
index, or framing. Chunks are positional (chunk *n* programs the *n*-th 64-byte slot).
The final chunk is padded to 64 bytes with `0xFF` if the image length is not a multiple
of 64.

### 4.3 FW_TRANSFER_COMPLETE

| Byte | Value | Meaning |
|---|---|---|
| 0 | `0xBA` | frame tag |
| 1 | `0xC2` | COMPLETE sub-op |
| 2..3 | `chunk_count` | echo of the chunk count, u16 little-endian |
| 4..6 | `checksum` | 24-bit transfer checksum, u24 little-endian |

### 4.4 Transfer checksum

```
checksum = ( sum of every firmware byte transmitted in the DATA chunks,
             including 0xFF padding in the final chunk ) & 0xFFFFFF
```

The bootloader accumulates every programmed byte into a 32-bit register and compares the
low 24 bits against the value in `FW_TRANSFER_COMPLETE`. Completion also requires the
per-chunk flash read-back verification to have passed for every chunk.

### 4.5 Acknowledgement (GET_REPORT)

After `FW_TRANSFER_COMPLETE`, a GET_REPORT (Feature) returns the result:

| Byte | Value | Meaning |
|---|---|---|
| 0 | `0xAB` | ACK tag |
| 1 | echo | echoed sub-op |
| 4 | status | `0x55` = OK, `0xAA` = FAIL |
| 5..7 | checksum | device-computed checksum, u24 little-endian |

A `0x55` status means the image was programmed and verified; the bootloader then clears
the mailbox and resets into the new application. A `0xAA` status (checksum mismatch or
read-back failure) leaves the device in the bootloader.

---

## 5. Flash destination and erase

The bootloader writes the incoming image starting at:

```
FLASH_DEST = 0x08005000
```

This is the **chip-ID header** address; the image stream must therefore begin with the
512-byte header that carries the chip-ID string `AT32F405 8KMKB`, immediately followed by
the application at `0x08005200`. An image that does not carry the correct chip-ID header
at offset `0x5000` will not pass the boot check (see §6).

Erase is **automatic on DFU entry** — it is not a host command. The bootloader erases the
application region `0x08005000`–`0x08028000` (70 pages of 2 KB) before accepting data.

The on-chip flash controller (EFC, base `0x40023C00`) is unlocked by writing the key
sequence `0x45670123` then `0xCDEF89AB` to the unlock register `0x40023C04`; page erase is
driven via the address register `0x40023C14` and the `ERSTR` bit in the control register
`0x40023C10`. Full EFC register details are in [hardware.md](hardware.md).

---

## 6. Boot decision

On every reset the bootloader decides whether to stay in DFU or jump to the application:

```
stay_in_bootloader = ( mailbox@0x08004800 == 0x55AA55AA )
                  || ( chip-ID@0x08005000 != "AT32F405 8KMKB" )

otherwise:           jump_to_app(0x08005200)
```

`jump_to_app` loads the application's main stack pointer from its vector table and
branches to its reset handler; the application installs its own vector table offset
(VTOR). After a successful flash the bootloader page-erases the mailbox and resets — so
the next boot sees an erased mailbox plus a valid chip-ID and runs the freshly written
application.

---

## 7. Config wipe and preserved calibration

Entering the bootloader erases the application configuration region at `0x08028000`
(active profile, polling rate, LED settings, keymap, macros, sleep timers, and similar).
These are re-settable from the host application after flashing.

Hall-effect calibration data at `0x08032000` is **not** erased by the enter-bootloader
sequence and survives a reflash. After a flash, magnetic keys may still need an actuation
setting and a calibration pass from the host application, because the application config
was wiped.

---

## 8. Fail-safe property

The round trip is recoverable by construction:

- The bootloader clears the mailbox and resets into the application **only** when the
  24-bit checksum matched and read-back verified.
- Any framing or transfer error produces a checksum mismatch, the mailbox stays set, and
  the device remains in `0x502A` with the application region already erased — re-running
  the flash recovers it.
- Because the device boots a written image only when the checksum matched the exact bytes
  programmed, a checksum-matching-but-wrong image cannot occur when the host transmits the
  same bytes it checksummed.

If the bootloader (`0x502A`) itself ever becomes unreachable, the AT32F405 ROM bootloader
(BOOT0 strap, USB `2e3c:df11`, `dfu-util`) is the last-resort recovery path — see
[flashing.md](flashing.md).

---

## 9. Cloud firmware API

Stock images are published by the vendor over a small unauthenticated HTTP API keyed by
`dev_id` (no credentials are required beyond the `dev_id`).

### 9.1 Resolve the current version

```
POST https://api2.rongyuan.tech:3816/api/v2/get_fw_version
Content-Type: application/json

{ "dev_id": <N> }
```

Response:

```json
{
  "code": <int>,
  "data": {
    "file_path": "<relative path>",
    "version_str": "<version>"
  }
}
```

### 9.2 Download the image

```
GET https://api2.rongyuan.tech:3816/download/<file_path>
```

The body is either a **ZIP archive** or **raw DEFLATE**. When it is a ZIP, the AT32
keyboard image is the member named `firmwareFile.bin`; companion members (for example a
PAN1080 radio image) may also be present. The downloaded AT32 image is a full RY5088 image
whose chip-ID header `AT32F405 8KMKB` sits at offset `0x5000`; the host slices it at
`0x5000` before streaming it to the bootloader (§4–§5).

### 9.3 Availability

The API serves the published subset of the family. Unknown or unpublished `dev_id`s
return an HTTP error (for example `500 "Record not found"`); those variants have no cloud
firmware and must be supplied as a local image file.
