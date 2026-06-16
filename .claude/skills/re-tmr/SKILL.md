---
name: re-tmr
description: >-
  Reverse-engineer / add support for a NEW RongYuan RY5088 magnetic
  (Hall-effect or TMR) keyboard from a connected device, producing a
  firmware/profiles/<dev_id>.toml board profile for this repo. Use when a
  contributor plugs in an unsupported RY5088 board (Artery AT32F405 + Panchip
  PAN1080) and wants to: detect its dev_id, fetch + statically analyze the stock
  firmware, draft a profile by copying the 2307 reference, then build and
  validate it. NOT for boards that change the MCU or vendor protocol — the
  YiChip YC3121 (MG75S HE, M1W HE, M1 V5 VIA) or YC500 (ICE75) boards, or the
  separate 2.4 GHz dongle — which are out of scope.
---

# Add a new RY5088 (AT32F405) magnetic keyboard: build its board profile

A **profile** (`firmware/profiles/<dev_id>.toml`) is the ONE file you write to add
a board. The platform (AT32F405 + PAN1080), the analog Hall/TMR scan engine, the
WS2812 RGB pipeline, the vendor HID protocol, and the SPI wireless path are all
**shared** and already implemented in `firmware/common/`. A new board only needs
its profile: identity, matrix geometry, keymap, LED count + serpentine map,
hall/switch params, and the wireless flag. `firmware/gen/gen_profile.py` turns the
profile into `board_config.h` / `board_keymap.h` / `board_led_map.c`, which
`common/` compiles against.

> **Flashing is recoverable.** A custom image loads into the **app slot only**
> (`0x08005200`, behind the factory bootloader); the bootloader is never touched,
> and `ry-flash` always restores stock. Experiment freely.

## Conventions used below

```sh
RY=flasher/target/release/ry-flash   # the flasher binary (or put it on PATH)
STOCK=stock                          # scratch dir for fetched stock images;
                                     # keep it OUT of firmware/ (the flasher's
                                     # default --stock-dir is literally "firmware")
```

Run `make` from the repo with `-C firmware`. Replace `<dev_id>` with the decimal
id you detect in step 1 (e.g. `2381`).

---

## Step 0 — Is this board in scope? (decide first)

This repo handles exactly one platform: **AT32F405 main MCU + the RY HID-DFU
protocol + the `0xFFFF` vendor-HID feature protocol**, chip-ID `AT32F405 8KMKB`.

In scope: any `ry5088_*` magnetic board (FUN60/Pro/Max/Ultra, FUN68/75, M1/M2/M3
V5 HE & TMR, Akko 5075B/5087B HE, Shine68, siblings). **Out of scope, do not
proceed:**

- **YiChip YC3121** boards — **MG75S HE, M1W HE, M1 V5 (VIA)**. Different MCU and
  protocol.
- **YiChip YC500** — **ICE75** (also mechanical, not magnetic).
- The **2.4 GHz USB dongle** — a *separate* AT32F405 board with chip-ID
  `AT32F405 8K-DGKB`; its image is not interchangeable and it is not a keyboard
  profile.

If step 1 cannot read a `dev_id`, or step 3 finds the chip-ID is not
`AT32F405 8KMKB`, **stop** — it is almost certainly one of the above.

---

## Step 1 — Detect (read-only)

Put the keyboard in **normal mode** (`3151:5030`), close the vendor app, then:

```sh
$RY --detect --json
# {"ok":true,"dev_id":2381,"display_name":"FUN60 Ultra","name":"ry5088_fun60ultra_8k_dm","version":"v302"}
```

- `dev_id` (decimal) is the **only safe model discriminator** — USB PID `0x5030`
  is shared across ~12 models. It is also the profile's filename.
- `error:"no_device"` → not in normal mode (app still open? replug). `display_name`
  / `name` come from the embedded 284-device map
  (`flasher/data/ry5088_devices.json`).

Record `dev_id` and `version`. Look the model up for the internal name + infor_id:

```sh
python3 -c "import json;d=json.load(open('flasher/data/ry5088_devices.json'));print([x for x in d['devices'] if x['dev_id']==<dev_id>])"
# {'dev_id':2381,...,'name':'ry5088_fun60ultra_8k_dm','displayName':'FUN60 Ultra','infor_id':[77,9]}
# infor_id is [lo,hi]; dev_id == lo | hi<<8  (77 | 9<<8 == 2381)
```

---

## Step 2 — Fetch the stock firmware (+ evidence bundle)

```sh
$RY --fetch <dev_id> --json --stock-dir $STOCK
# {"ok":true,"dev_id":2381,"version":"v302","path":"stock/2381/fw_at32.bin","bytes":124664}
```

- `error:"fetch_failed"` → the vendor cloud has no published image for this
  dev_id (currently it mainly serves the FUN60 family). Supply a local image at
  `$STOCK/<dev_id>/fw_at32.bin` (your own dump or a vendor package).

**Assemble the evidence bundle** you'll attach to the PR. `$RY --probe --json` is
the primary capture: it bundles the dev_id/version, USB strings, and the fetched
stock firmware (with its embedded chip-ID and an FNV-1a fingerprint) in one shot.
For the raw USB/HID descriptor bytes (not included in the bundle), also capture:

```sh
shasum -a 256 $STOCK/<dev_id>/fw_at32.bin            # firmware hash (provenance)
# live USB/HID descriptors:
system_profiler SPUSBDataType | grep -A25 -i keyboard   # macOS
# or:  ioreg -p IOUSB -l -w0      (macOS)   /   lsusb -v -d 3151:5030   (Linux)
```

The normal-mode vendor HID **report descriptor is fixed** for this platform
(`docs/protocol.md` §1): `06 ff ff 09 02 a1 01 09 02 15 80 25 7f 95 40 75 08 b1 02 c0`.

---

## Step 3 — Static-analyze the stock `.bin`

The fetched `fw_at32.bin` is a **full image**: the chip-ID header sits at file
offset `0x5000` and the app vector table at `0x5200` (see `docs/hardware.md` §2,
`common/at32f405_app.ld`). Confirm it is a real keyboard image and read the
headers:

```sh
FW=$STOCK/<dev_id>/fw_at32.bin

# 3a. chip-ID @0x5000 — MUST be "AT32F405 8KMKB" (a dongle reads "8K-DGKB" => wrong board, abort)
strings -t x "$FW" | grep -i 'AT32F405'

# 3b. app vector table @0x5200: word0 = initial SP (expect ~0x20019800, within
#     0x20000000..0x20020000); word1 = reset vector (within 0x08005200..0x08028000,
#     Thumb bit set => odd). Bytes are little-endian.
od -An -tx4 -j 0x5200 -N8 "$FW"

# 3c. identity / wireless hints: product string (e.g. " FUN60 Ultra-1"), radio name
strings -t x "$FW" | grep -iE 'FUN|MonsGeek|Akko|Ultra|Pro|Max|Keyboard|BLE|2\.4'

# 3d. (deeper) disassemble to eyeball ADC / GPIO / DMA bring-up & table sizes
arm-none-eabi-objdump -D -b binary -m armv7e-m -M force-thumb "$FW" | less
#     rabin2 alt for strings:  rabin2 -zzqq "$FW"
```

**Cross-read against the worked reference** `firmware/profiles/2307.toml` and
`docs/hardware.md` §3–§7 (matrix, RGB, and the *variant-specific / configurable*
list). What static analysis reliably gives you: chip-ID confirmation (in scope?),
identity/product strings, the wireless identity string, and a sense of whether the
board uses the **reference 60% front-end** (CD4017 column counter + 8:1 row mux,
14×6) or something else.

What a stripped stock `.bin` usually will **not** hand you cleanly: exact
`cols`/`rows`, LED `count`, the serpentine map, the hall sign, or the keymap. Treat
those as **confirm-on-hardware** (step 6) and derive your first draft from the
physical board + the closest sibling profile.

---

## Step 4 — Draft `profiles/<dev_id>.toml`

```sh
cp firmware/profiles/2307.toml firmware/profiles/<dev_id>.toml
```

Edit it per the cheat-sheet. **Mark every value you are not certain of** with a
`# TODO: confirm on hardware` comment — that's expected and reviewable.

### Field cheat-sheet (where each value comes from)

| Profile field | Source | Confirm on HW? |
|---|---|---|
| `meta.dev_id` | step 1 `--detect` (decimal); also the filename | fixed |
| `meta.display_name`, `meta.name` | `devices.json` / `--detect` | fixed |
| `meta.switch_type` `"HE"`/`"TMR"` | model (HE & TMR are *distinct* dev_ids; name often hints) | confirm |
| `meta.wireless` | does this model carry the PAN1080 (tri-mode 2.4 G/BLE)? strings + marketing | confirm |
| `usb.product_str` etc. | step 3 strings (e.g. `" FUN60 Ultra-1"`, note leading space); `vid 0x3151`/`pid 0x5030` are shared | mostly fixed |
| `version.hi` / `.lo` | from `--detect` `vMmm`: `lo`=major digit, `hi`=the two minor digits, written in hex (e.g. `v306` → `hi=0x06`, `lo=0x03`). **Cosmetic** (only the GET_INFOR version echo) | optional |
| `matrix.cols` / `.rows` | physical layout + closest sibling; the 60% reference is `14×6=84`. `key = col*rows + row` | **CONFIRM (riskiest)** |
| `hall.*` | defaults in `gen_profile.py` are a fine start; `press_decreases` sign is the one to verify | confirm sign |
| `leds.count`, `max_col`, `max_row`, `phys_col`, `phys_row` | physical LED chain + serpentine tracing | **CONFIRM** |
| `keymap.codes` | physical keymap → HID usage IDs (`0xE0..0xE7` = modifiers, `0x00` = none) | **CONFIRM** |
| `[matrix]`/`[rgb]`/`[wireless]` **pin** fields | the vendor **reference wiring** — documentation only; compiled into `common/keyscan.c`/`rgb.c`/`wireless.c`, **not** emitted by the generator | see note ↓ |

**Generator consistency rules** (`make check` enforces these):

- `keymap.codes` length **must equal** `cols*rows` (if `[keymap]` is present).
- `leds.phys_col` and `leds.phys_row` length **must each equal** `leds.count`.

---

## Step 5 — Build + validate

```sh
# 5a. Quick validation — python only, no toolchain/BSP. Runs EVERY profile through
#     the generator; this is what CI runs.
make -C firmware check

# 5b. Host logic tests against the new profile's generated headers (needs only cc)
make -C firmware PROFILE=<dev_id> test

# 5c. Full flashable image — needs arm-none-eabi-gcc + the Artery BSP fetched into
#     firmware/vendor/ (see firmware/README.md). Prints a vector-table sanity check.
make -C firmware PROFILE=<dev_id>
#   -> firmware/build/<dev_id>/ry5088_<dev_id>.bin   (load @0x08005200)
```

If `make check` fails, the error names the offending field (length mismatch,
missing required key). Required keys: `meta.dev_id`, `matrix.cols`, `matrix.rows`,
`leds.count`, `leds.phys_col`, `leds.phys_row`.

---

## Step 6 — Confirm on hardware (recoverable; iterate)

Flashing the built custom image uses the RY DFU / ROM-DFU path (see
`firmware/Makefile` `flash-romdfu` and `docs/flashing.md`). Then check behavior and
fix the profile:

- **Typing** — every key emits the right code (validates `keymap` + geometry +
  the `col*rows+row` orientation); modifiers fold correctly.
- **LEDs** — correct count, no missing/extra/permuted LEDs (validates `count` +
  serpentine `phys_col`/`phys_row`), correct colors (GRB order + WS2812 bit
  timing).
- **Wireless** — pairs/reports if `meta.wireless = true`.

**First suspects when something's off** (`docs/hardware.md` §7): `hall.press_decreases`
sign; columns permuted (column-counter pin roles); LED count/order; WS2812 bit
timing. Adjust the profile, rebuild, retry.

**Restore stock at any time** (always one command away):

```sh
$RY --auto --arm --json --stock-dir $STOCK
# detects the live dev_id, flashes its matching stock image, re-reads GET_INFOR to confirm
```

---

## Step 7 — Submit

```sh
make -C firmware models      # regenerates the supported-models table; confirm your board appears
```

Open a PR that adds **only** `firmware/profiles/<dev_id>.toml`. Put the evidence in
the description: `dev_id` + version, the USB/HID descriptors, the firmware
**sha256**, your reasoning for each non-default field, and what you confirmed on
hardware vs. left as `TODO`. CI runs `make check` automatically.

---

## Honest hard parts & gotchas

- **Geometry is the riskiest field.** `cols`/`rows` and the LED map rarely fall out
  of a stripped stock binary. Start from the physical board + the nearest sibling
  profile and **confirm on hardware**. Getting them wrong boots and lights up but
  mis-scans.
- **Some boards change the analog front-end, not just the profile.** The 60%
  reference uses a CD4017-style column counter + 8:1 row mux on one ADC channel.
  Other variants (e.g. the 75% M1 V5) use **15 direct ADC channels scanned by
  DMA** with no decade counter. That is a **`common/keyscan.c` change**, not a
  profile-only addition — flag it and coordinate before drafting.
- **Pin wiring is not code-generated.** The `[matrix]`/`[rgb]`/`[wireless]` pin
  fields are documentation of the shared reference design. To actually rewire a
  board you must edit `common/*.c`; changing the TOML pins alone does nothing.
- **Raw USB descriptors aren't in `--probe`.** `$RY --probe --json` gives the
  dev_id/version, USB strings, and the stock-firmware chip-ID + fingerprint; for the
  full descriptor bytes use the manual `lsusb -v` / `ioreg` capture in step 2.
- **Config wipe on flash.** Entering the bootloader wipes the app config (profile,
  polling, LED, keymap, macros); the per-key Hall calibration at `0x08032000` is
  preserved. After any flash, set actuation + run a calibration pass in the app.
- **Don't pollute the source tree.** The flasher's default `--stock-dir` is
  literally `firmware`; always pass an explicit `--stock-dir $STOCK` so fetched
  images don't land in `firmware/`.
