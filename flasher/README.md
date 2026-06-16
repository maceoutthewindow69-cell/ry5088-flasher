# ry-flash

A single-binary **stock-firmware flasher + TUI** for RongYuan **RY5088** (Artery AT32F405 + Panchip PAN1080)
magnetic keyboards — FUN60 / Pro / Max / Ultra (HE + TMR), FUN68, M1/M2/M3 V5, Akko 5075B/5087B, and other
`ry5088_*` models. It speaks the **RY HID-DFU** protocol (no case-opening, no BOOT0, no vendor tool) and is
**variant-safe**: it reads the connected keyboard's `dev_id` (via `GET_INFOR`) and flashes *that model's* matching
image — never the wrong one (USB PID `0x5030` is shared across many models; only the dev_id discriminates).

Protocol: [`../docs/protocol.md`](../docs/protocol.md) · usage guide: [`../docs/flashing.md`](../docs/flashing.md).

## Build
```bash
cargo build --release        # -> target/release/ry-flash  (single binary)
cargo test                   # protocol unit tests (frame + checksum construction)
```
Needs `libhidapi` (macOS: `brew install hidapi`; Linux: `libhidapi-dev`).

## Use
```bash
ry-flash                     # launch the TUI: detect -> confirm -> flash with a progress gauge
ry-flash --detect            # read dev_id / model / version (read-only)
ry-flash --auto              # headless: detect + locate image + DRY-RUN plan
ry-flash --auto --arm        # headless: flash the connected board's matching stock image
ry-flash --image PATH --arm  # flash a specific image (full image or 0x5000-sliced)
ry-flash --fetch 2381        # download that dev_id's stock image from the vendor cloud into --stock-dir
ry-flash --list              # list local stock images under --stock-dir
ry-flash --stock-dir DIR     # where <dev_id>/fw_at32.bin images live (default ./firmware or $RY_FLASH_STOCK)
```
`--auto` is **self-contained**: if no local image exists for the detected dev_id it downloads the matching one
from the vendor cloud first (pass `--no-fetch` to disable that and fail with `no_image` instead).

## Agent / script mode (non-interactive)
Every headless command takes `--json` for structured stdout (progress + logs go to **stderr**, so stdout is a
single clean JSON object). No prompts — `--arm` is the explicit consent. **Exit codes:** `0` ok · `1` error ·
`2` no device · `3` no image · `4` flash failed.

```bash
ry-flash --detect --json
#   {"ok":true,"dev_id":2307,"display_name":"FUN60 Ultra","name":"ry5088_fun60max2_8k","version":"v306"}
ry-flash --list --json --stock-dir DIR
#   {"ok":true,"stock_dir":"DIR","images":[{"dev_id":2307,"display_name":"FUN60 Ultra","name":"…","path":"…","bytes":110268}, …]}
ry-flash --auto --json --stock-dir DIR            # dry-run plan
#   {"ok":true,"armed":false,"dev_id":2307,"version":"v306","image":"…","slice_bytes":89788,"chunks":1403,"checksum":"0x8687e8","plan":"…"}
ry-flash --auto --arm --json --stock-dir DIR      # flash; final result on stdout
#   {"ok":true,"armed":true,"flashed":true,"dev_id":2307,"checksum":"0x8687e8","post_dev_id":2307,"post_version":"v306","match":true}
ry-flash --fetch 2381 --json --stock-dir DIR      # download stock image (no device needed); progress -> stderr
#   {"ok":true,"dev_id":2381,"version":"v302","path":"DIR/2381/fw_at32.bin","bytes":124664}
```
On failure the object is `{"ok":false,"error":"<no_device|no_image|bad_image|fetch_failed|flash_failed|wrong_platform|post_flash_mismatch|bad_args>","message":"…"}`.
A typical agent flow: `--detect --json` → if `ok`, `--auto --json` to preview → `--auto --arm --json` to flash, then
confirm `post_dev_id == dev_id`.

## Stock images
Place full images at `<stock-dir>/<dev_id>/fw_at32.bin`. Both layouts are accepted — a full image (chip-ID
`AT32F405 8KMKB` at offset `0x5000`) or an already-sliced image (chip-ID at offset `0x0`). Get an image with the
built-in `ry-flash --fetch <dev_id>` (downloads, unpacks, and writes `<dev_id>/fw_at32.bin`, validating the
chip-ID); `--auto` fetches automatically when the image is missing.

## Safety
- **Variant-matched:** `--auto` flashes only the dev_id it detected, and re-reads `GET_INFOR` afterward to confirm.
- **Fail-safe:** any transfer error → checksum mismatch → the board stays in the bootloader (`502A`) with the app
  region erased → just flash again. Stock is always one flash away. (Last-resort backstop: BOOT0 ROM-DFU.)
- **Refuses non-keyboard images:** anything without the `AT32F405 8KMKB` chip-ID (e.g. the 2.4 GHz dongle's `8K-DGKB`).
- **Config wipe:** entering the bootloader clears the app config (re-settable in the vendor app; the per-key Hall
  calibration at `0x08032000` is preserved).
