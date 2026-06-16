# Flashing RY5088 Keyboards

`ry-flash` is a single-binary flasher and terminal UI for RongYuan **RY5088** magnetic
keyboards (Artery AT32F405 + Panchip PAN1080). It speaks the
[HID-DFU protocol](protocol.md) directly — no case-opening, no BOOT0 strap, no vendor
tool — and is **variant-safe**: it reads the connected keyboard's `dev_id` and flashes
that model's matching image.

This document covers the variant-safety model, interactive and command-line use, the
JSON/agent contract, exit codes, the fail-safe property, and last-resort recovery. The
byte-level protocol is in [protocol.md](protocol.md); per-model metadata lives in
[models.md](models.md).

---

## 1. Variant safety — the one rule

A large set of RY5088 models share the normal-mode USB PID `0x5030`, so the **PID cannot
identify a variant**. The only safe discriminator is the firmware **`dev_id`**, read from
the device with the `GET_INFOR` vendor command.

> **The keyboard's `GET_INFOR` id *is* its cloud `dev_id`** — a 16-bit little-endian value.
> For example `GET_INFOR` bytes `03 09` → `0x0903` → `dev_id 2307`.

Flashing a different variant's firmware boots and lights up but **breaks key-scan**: the
Hall/TMR front-end, ADC-to-key mapping, matrix dimensions, and calibration tables are
model-specific (see [hardware.md](hardware.md)). HE and TMR builds of the same model carry
**distinct** `dev_id`s. Accordingly the tool keys everything off the live `dev_id`, never
the PID:

- `--auto` flashes only the image matching the detected `dev_id`.
- After flashing it re-reads `GET_INFOR` and confirms the `dev_id` still matches.
- It refuses any image whose chip-ID is not the keyboard's `AT32F405 8KMKB` (this also
  excludes the dongle's `AT32F405 8K-DGKB`), and verifies a fetched image's SHA-256 against the
  published firmware manifest when the `dev_id` is listed (see [`cloud-api.md`](cloud-api.md)).
- Sibling-platform boards in the catalog (YiChip YC3121/YC500, mice, …) are recognised by name
  but refused early with `wrong_platform` — they are not RY5088 (AT32F405) keyboards.

---

## 2. Interactive use (TUI)

Run with no arguments to launch the terminal UI:

```bash
ry-flash
```

The TUI detects the connected keyboard, shows its `dev_id` / model / version, asks for
confirmation, and flashes with a live progress gauge. It resolves (and, if needed,
downloads) the matching stock image automatically.

---

## 3. Command-line use

```bash
ry-flash --detect                 # read dev_id / model / version (read-only)
ry-flash --auto                   # detect + locate/fetch image + DRY-RUN plan
ry-flash --auto --arm             # flash the connected board's matching stock image
ry-flash --image PATH --arm       # flash a specific image (full or 0x5000-sliced)
ry-flash --fetch 2381             # download a dev_id's stock image into --stock-dir
ry-flash --list                   # list locally available stock images
ry-flash --stock-dir DIR          # image root (default ./firmware or $RY_FLASH_STOCK)
ry-flash --no-fetch               # forbid auto-download; fail with no_image instead
```

| Flag | Purpose |
|---|---|
| `--detect` | Read-only identification. No device changes. |
| `--auto` | Detect, resolve the matching image, and (without `--arm`) print the plan. |
| `--arm` | Explicit consent. **Everything is a dry run unless `--arm` is given.** |
| `--image PATH` | Flash a specific image instead of the auto-resolved one. |
| `--fetch <dev_id>` | Download that `dev_id`'s stock image from the vendor cloud. No device needed. |
| `--list` | List images present under `--stock-dir`. |
| `--stock-dir DIR` | Where `<dev_id>/fw_at32.bin` images live. |
| `--no-fetch` | Disable `--auto`'s on-demand download. |

`--auto` is self-contained: when no local image exists for the detected `dev_id` it
downloads the matching one from the vendor cloud (the
[cloud firmware API](cloud-api.md)) before flashing, unless `--no-fetch`
is set. Both image layouts are accepted: a **full** image (chip-ID `AT32F405 8KMKB` at
offset `0x5000`) or an **already-sliced** image (chip-ID at offset 0). The tool slices a
full image at `0x5000` after verifying the chip-ID, so it refuses anything that is not a
real RY5088 keyboard image.

### What `--auto --arm` does

1. `GET_INFOR` → `dev_id` + version; look up the model.
2. Resolve (or fetch) `<dev_id>/fw_at32.bin`; verify chip-ID; slice at `0x5000`.
3. Send `0xC5 0x3A` then `0x7F 55AA55AA` → wipes app config (calibration at `0x08032000`
   preserved) → reboot to bootloader `0x502A`.
4. Bootloader auto-erases `0x08005000`–`0x08028000`; stream START + raw 64-byte chunks +
   COMPLETE with the 24-bit checksum.
5. ACK `0x55` → mailbox cleared → reboot → re-enumerate `0x5030`; re-read `GET_INFOR` and
   confirm the `dev_id` matches.

---

## 4. Agent / script mode (JSON)

Every headless command accepts `--json`. The contract is: **exactly one JSON object on
stdout**; all progress and logs go to **stderr**, so stdout stays a single clean object.
There are no prompts — `--arm` is the explicit consent.

A typical agent flow is `--detect --json` → if `ok`, `--auto --json` to preview →
`--auto --arm --json` to flash, then check `post_dev_id == dev_id`.

### Success shapes

```jsonc
// --detect --json
{ "ok": true, "dev_id": 2307, "display_name": "FUN60 Ultra",
  "name": "ry5088_fun60max2_8k", "version": "v306" }

// --list --json --stock-dir DIR
{ "ok": true, "stock_dir": "DIR",
  "images": [ { "dev_id": 2307, "display_name": "FUN60 Ultra",
                "name": "ry5088_fun60max2_8k", "path": "...", "bytes": 110268 } ] }

// --auto --json (dry run)
{ "ok": true, "armed": false, "dev_id": 2307, "version": "v306",
  "image": "...", "slice_bytes": 89788, "chunks": 1403, "checksum": "0x8687e8",
  "plan": "..." }

// --auto --arm --json (flash result)
{ "ok": true, "armed": true, "flashed": true, "dev_id": 2307,
  "checksum": "0x8687e8", "post_dev_id": 2307, "post_version": "v306",
  "match": true, "message": "..." }

// --fetch 2381 --json --stock-dir DIR  (no device needed)
{ "ok": true, "dev_id": 2381, "version": "v302",
  "path": "DIR/2381/fw_at32.bin", "bytes": 124664 }
```

### Error shape

```jsonc
{ "ok": false,
  "error": "bad_args" | "no_device" | "no_image" | "bad_image"
         | "fetch_failed" | "flash_failed" | "post_flash_mismatch",
  "message": "..." }
```

`post_flash_mismatch` means the flash completed but the board then reported a *different*
`dev_id` than the one flashed — treated as a failure. On a successful flash, `"match"` is
`true` when the post-flash `dev_id` was re-read and matches; it is `false` only when the
board returned to normal mode but `GET_INFOR` could not be re-read (the flash still completed).

---

## 5. Exit codes

| Code | Meaning |
|---|---|
| `0` | OK |
| `1` | Generic error |
| `2` | No device |
| `3` | No image (includes an unavailable/failed download, reported as `fetch_failed`) |
| `4` | Flash failed |

---

## 6. Fail-safe property

A flash is recoverable by construction. Any framing or transfer error produces a checksum
mismatch; the board then stays in the bootloader (`0x502A`) with the application region
already erased. The board boots a written image only after the bootloader verifies the
24-bit checksum, so a partially transferred image cannot run. See
[protocol.md §8](protocol.md#8-fail-safe-property).

**Recovering a board stuck in the bootloader.** While in `0x502A` the keyboard cannot report
its `dev_id`, so auto-detection is unavailable. Flash an explicit stock image to bring it back:

```bash
ry-flash --image <stock_image> --arm        # flashes directly from the bootloader, then verifies
```

`--auto`/`--image` detect this state automatically and take the direct-flash path; supply the
correct image for your model (see [models.md](models.md)). If the resident bootloader itself
becomes unreachable, use the BOOT0 ROM-DFU backstop in §9.

---

## 7. Config wipe note

Entering the bootloader wipes the application configuration (active profile, polling rate,
LED settings, keymap, macros, sleep timers). These are re-settable from the host
application after flashing. Hall calibration at `0x08032000` is preserved, but magnetic
keys may still need an actuation setting and a calibration pass in the application because
the config was wiped.

---

## 8. Stock images

Place images at `<stock-dir>/<dev_id>/fw_at32.bin`. Obtain them with the built-in
`ry-flash --fetch <dev_id>` (downloads, unpacks, validates the chip-ID, and writes
`<dev_id>/fw_at32.bin`); `--auto` fetches automatically when an image is missing. Variants
with no published cloud firmware must be supplied as a local image file.

---

## 9. Last-resort recovery — BOOT0 ROM-DFU

If the resident bootloader (`0x502A`) ever becomes unreachable, the AT32F405 on-chip ROM
bootloader is the backstop. Hold the **BOOT0** strap at power-on so the MCU enters its ROM
DFU mode; it enumerates as a standard DFU device:

```
USB id:  2e3c:df11
tool:    dfu-util
```

This path reprograms flash from a full image regardless of the resident bootloader's state,
and is the only recovery that does not rely on any code already on the chip.
