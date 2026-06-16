# RY5088 Custom Firmware

This document specifies the custom firmware for the RongYuan **RY5088** platform (Artery
AT32F405). The firmware is one shared codebase at **`firmware/common/`**, specialised per board by a
profile at **`firmware/profiles/<dev_id>.toml`** (`make PROFILE=<dev_id>`) — a full analog keyboard in one
image: USB device bring-up and descriptors, the vendor
feature-report protocol, the analog Hall-effect key-scan and rapid-trigger engine, WS2812
RGB, flash-backed configuration persistence, and an optional wireless report path.

It builds for application execution at `0x08005200` and is programmed with the
[HID-DFU flasher](flashing.md). Hardware resources (clocks, pins, peripherals) are
specified in [hardware.md](hardware.md); the on-wire flash protocol is in
[protocol.md](protocol.md).

---

## 1. Architecture

`main.c` initializes the clock, the USB device, and the peripherals, then runs a loop:
`keyscan` samples every key site into `raw[84]`; `hall` converts each reading to travel and
runs actuation plus rapid trigger; `hid_report` builds the boot report from the pressed set
and the keymap; the report is sent on the wired HID IN endpoint (and, on wireless boards,
forwarded over the SPI3 bus). The same loop renders the RGB frame at roughly 60 Hz and
flushes pending configuration changes to flash after a short debounce.

### 1.1 Host-visible identity

The host application recognizes the board by two things:

1. **USB match** — VID `0x3151`, a supported PID, and the vendor HID interface
   (usage page `0xFFFF`, 64-byte Feature reports, report ID 0). The report descriptor is:

   ```
   06 ff ff 09 02 a1 01 09 02 15 80 25 7f 95 40 75 08 b1 02 c0
   ```

2. **Vendor-protocol replies** — the host reads device id / version / config and must get
   valid responses (see [§2](#2-vendor-protocol)). In particular `GET_INFOR` returns the
   device id (the `dev_id`) and firmware version; the rest of the probe set returns valid
   profile / debounce / LED / feature-list values.

### 1.2 Module list (`firmware/common/`)

The board-specific values (identity, geometry, keymap, LED map) are **not** in these files — they are
generated from the active profile into `board_config.h` / `board_keymap.h` / `board_led_map.c` by
`gen/gen_profile.py` and compiled in.

| File | Role | Notes |
|---|---|---|
| `main.c` | Clock/USB/peripheral init, then the `keyscan → hall → build report → send` loop, RGB render tick, and debounced flash save | — |
| `descriptors.{c,h}` | USB device / configuration / string descriptors | — |
| `monsgeek_desc.{c,h}` | Vendor HID report descriptor | — |
| `monsgeek_class.{c,h}` | USB class glue (binds the vendor interface) | — |
| `vendor_proto.{c,h}` | Vendor feature-report dispatcher: GET/SET, including LED table / userpic / keymatrix / magnetism | pure / host-testable |
| `board.{c,h}`, `usb_conf.h` | Board bring-up and USB configuration | — |
| `keyscan.{c,h}` | ADC1 ch2 / PA2 + the PA4/5/6 column counter + PC6/7/8 row mux; fills `raw[84]` | per-site sample sequence ([hardware.md §3](hardware.md#3-analog-key-matrix-hall-effect)) |
| `hall.{c,h}` | Per-key baseline (running max), deepest extreme (running min), travel normalization, fixed-threshold actuation with hysteresis, rapid-trigger state machine | pure / host-testable |
| `hid_report.{c,h}` | `pressed[84]` + keymap → 8-byte boot report (modifiers fold into the modifier byte; up to 6 keycodes; rollover) | — |
| `board_keymap.h` *(generated)* | Default keymap, `key = column × KS_ROWS + row` | from `profiles/<dev_id>.toml` |
| `led_layout.h` + `board_led_map.c` *(generated)* | LED protocol/types; serpentine physical→chain map + palette | map from the profile |
| `led_effects.{c,h}` | Effect engine: Off / Static / Breathing / Wave / Rainbow / per-key | pure / host-testable |
| `rgb.{c,h}` | WS2812 SPI2 + DMA driver and bit encoder (`0xC0`/`0xF0`) | hardware-dependent |
| `flash_efc.c` | AT32 EFC backend (unlock / sector-erase / program / lock) | hardware-dependent |
| `persist.{c,h}` | Config image pack/unpack + load/save over an injected `flash_ops_t` | pure / host-testable |
| `wireless.{c,h}` | SPI3 report bus to the PAN1080 radio ([hardware.md §5](hardware.md#5-wireless-co-processor--panchip-pan1080)) | frame builder is host-testable |
| `at32f402_405_*`, `system_*`, `startup_*.s`, `at32f405_app.ld` | Artery BSP templates, startup, linker script | — |

### 1.3 Key-scan and Hall engine

`keyscan` samples every site into `raw[84]`; `hall` converts each reading to travel and runs
actuation plus rapid trigger; `hid_report` builds the boot report from the pressed set and
the keymap; the report is sent on the wired HID IN endpoint (and, on wireless boards,
forwarded over the SPI3 bus). Travel, calibration, and the rapid-trigger algorithm are
specified in [hardware.md §3.3](hardware.md#33-travel-and-rapid-trigger).

### 1.4 RGB pipeline

`rgb.c` brings up SPI2 (half-duplex master, divider `MCLK_DIV_16`), DMA1 channel 1
(DMAMUX `SPI2_TX`), and PA10/AF5, and encodes a frame into the 1464-byte GRB bit-stream
using the `0xC0` (logical 0) and `0xF0` (logical 1) symbols. `led_effects.c` renders Off,
Static, Breathing, Wave (board-accurate via the serpentine geometry), Rainbow, and per-key
Static; any other requested mode falls back to Static so every host mode still lights.
`main.c` refreshes at roughly 60 Hz and honours the LED-on bit. The electrical timing is in
[hardware.md §4](hardware.md#4-per-key-rgb-ws2812-over-spi).

### 1.5 Persistence

`persist.c` serializes the full app-visible config — the config header, the LED per-mode
table, per-key colours, the keymap, and per-key magnetism — into one versioned, CRC-checked
image that lives inside the single 2 KB config sector at `0x08028000`, above the application
image. `persist_load` returns clean stock defaults when the image is blank, foreign, or
corrupt (a magic + version + CRC guard). `flash_efc.c` performs the erase/program through
the BSP EFC primitives. SET handlers mark the config dirty; `main.c` flushes after a short
debounce to batch bursts and re-loads flash on each USB-configure edge so flash stays the
source of truth.

Default state when flash is blank: profile 0, 8 kHz polling, LED on, Wave (rainbow) at full
brightness, default ANSI keymap, 2.0 mm actuation, Normal key mode, 0.5 mm rapid trigger.

---

## 2. Vendor protocol

The vendor protocol is carried over 64-byte HID Feature reports, report ID 0, on the vendor
interface (usage page `0xFFFF`). The command header occupies the low bytes of the report;
the firmware processes it from an internal command buffer whose layout places the
**opcode at index 2** (indices 0 and 1 are internal pending/busy flags that are not on the
wire). On the wire the opcode is byte 0, arguments are bytes 1–6, and the header checksum is
byte 7 (byte 8 for LED commands).

### 2.1 Header checksum

- **Bit7 (default):** the first 8 report bytes sum to `0xFF` modulo 256; checksum at byte 7.
- **Bit8 (LED commands `0x07` / `0x08` and their reads):** the first 9 report bytes sum to
  `0xFF` modulo 256; checksum at byte 8.

A report whose header does not sum to `0xFF` is rejected. The response is written back into
the report buffer in place, with byte 0 echoing the opcode.

### 2.2 Read = write | 0x80

Read commands are the write opcode with the high bit set: `GET = SET | 0x80`
(for example `SET_PROFILE 0x04` ↔ `GET_PROFILE 0x84`).

### 2.3 Command families

| Family | Write | Read | Function |
|---|---|---|---|
| **Identity** | — | `0x8F` GET_INFOR | device id (`dev_id`, 16-bit LE) + firmware version |
| | — | `0x80` GET_REV | RF / version revision |
| | — | `0xD0` GET_SKU, `0xE6` GET_FEATURE_LIST | SKU and feature bitmap |
| **Profile** | `0x04` SET_PROFILE | `0x84` | active profile 0–3 (reloads keymap/config) |
| **Polling** | `0x03` SET_REPORT | `0x83` | report/polling-rate code |
| **Debounce** | `0x06` SET_DEBOUNCE | `0x86` | debounce setting |
| **Options** | `0x09` SET_KBOPTION | `0x89` | keyboard option bits / Fn-layer bank |
| **LED** | `0x05` LED on/off; `0x07` SET_LED; `0x08` SET_SLED | `0x85` / `0x87` / `0x88` | on/off and `[mode][speed][brightness][options][R][G][B]` params (Bit8 checksum) |
| **Keymap** | `0x0A` SET_KEYMATRIX | `0x8A` | keymap pages (chunked) |
| **Macro** | `0x0B` SET_MACRO | `0x8B` | onboard macros |
| **User picture** | `0x0C` SET_USERPIC | `0x8C` | per-key colours (chunked staging + commit) |
| **Fn / sleep** | `0x11` sleep timers | `0x90` GET_FN, `0x91` GET_SLEEPTIME | Fn-layer data and sleep / RGB-off timers |
| **Magnetism** | `0x65` SET_MULTI_MAGNETISM | `0xE5` | per-key actuation/lift/RT/mode (sub-command keyed) |
| **Bootloader** | `0xC5` ISP_PREPARE; `0x7F` ENTER_BOOTLOADER (`55 AA 55 AA`) | — | enter DFU ([protocol.md §3](protocol.md#3-enter-bootloader-sequence)) |

`GET` for every settable family mirrors its `SET`. The magnetism family is a
structure-of-arrays keyed by sub-command (press travel, lift travel, rapid-trigger press and
lift deltas, dead-zone, per-key mode), with stored units in centi-millimetres (a raw value
of 200 = 2.00 mm).

---

## 3. Building

The firmware builds with `arm-none-eabi-gcc` against the Artery AT32F402/405 BSP and links
the application at `0x08005200`. The BSP is not kept in-tree; it is fetched into a `vendor/`
directory at build time (see [firmware/README.md](../firmware/README.md) for the exact clone
command), and `BSP_DIR` in the `Makefile` points at it.

```bash
# device build (cross-compile), from firmware/  (BSP cloned into firmware/vendor/)
make PROFILE=2307
#   -> build/2307/ry5088_2307.bin, application linked @ 0x08005200
```

The device image is on the order of 17.5 KB flash / 11.2 KB RAM.

The hardware-independent logic is unit-tested natively — no ARM toolchain and no BSP are
needed — from `firmware/`:

```bash
make test            # test_engine (12/12) + test_functional (26/26)
make test-wireless   # wireless frame builder (15/15)
```

`make test` builds and runs `test_engine` (the key-scan/Hall math and HID report assembly)
and `test_functional` (the vendor GET/SET round-trip — covering the protocol replies and the
checksum rule — the LED effect engine and LED-param round-trip, flash-config persistence over
a mock EFC including blank→defaults / save-reload / CRC-corruption→defaults, and the
magnetism SET/GET round-trip). `make test-wireless` builds and runs `test_wireless`, the SPI3
frame builder (`-DHOST_TEST` drops the BSP/SPI/DMA/GPIO code in `wireless.c`, leaving only
`wl_build_frame`).

---

## 4. Status

The pure logic is **host-tested**: the effect engine (four named modes plus per-key and
rainbow), the full vendor GET/SET round trip including the protocol replies and magnetism,
config pack/unpack and persistence over a mock EFC, the clean defaults, and the wireless
frame builder all pass their host self-tests (`make test` engine + functional suites,
`make test-wireless` 15/15, 0 failures).

### 4.1 Feature status

What the firmware actually *acts on*, versus what the vendor protocol accepts and stores but
the engine does not yet apply (a no-op beyond the configurator UI). Research of the FUN60
Ultra confirms the advertised set; this table is the honest implementation state:

| Feature | Status | Notes |
|---|---|---|
| Per-key adjustable actuation (0.01 mm) | **Implemented** | fixed-threshold + hysteresis (`hall.c`) |
| Rapid Trigger | **Implemented** | valley/peak state machine (`hall.c`) |
| Per-key / multi magnetism | **Implemented** | independent config per key |
| RGB effects | **Implemented** | Off / Static / Breathing / Wave / Rainbow / per-key (others fall back to Static) |
| Flash-config persistence | **Implemented** | CRC-checked image over EFC |
| Vendor protocol GET/SET round-trips | **Implemented (most)** | device presents identically to the app; `GET_MACRO` is not implemented |
| Boot HID reports | **Implemented** | the main loop sends boot reports; an NKRO bitmap builder exists but is unused |
| 8 kHz polling | **Config-only** | the rate byte is stored; USB runs full-speed (1 ms `bInterval`) — an 8 kHz transport is not implemented |
| Debounce | **Config-only** | stored; applied at the scan level |
| DKS (Dynamic Keystroke) | **Stored-only** | `mag_mode` stored; the engine treats it as Normal |
| Mod-Tap | **Stored-only** | needs a tap/hold timing source |
| Toggle | **Stored-only** | the engine treats it as Normal |
| Snap-Tap / SOCD | **Stored-only** | needs key-pair config + cross-key resolution |
| Keymap remap | **Stored-only** | persisted + round-tripped; the report path types from the default map |
| Macros / Fn-layer / OLED | **SET stub** | `FEA_SET_MACRO` only marks config dirty; no storage, `GET_MACRO`, or execution |
| Wireless (2.4 GHz / BLE) | **Experimental** | frame path present; the modifier byte is not yet carried in the frame; on-hardware bring-up pending |

The **basic/common magnetic features — per-key actuation and Rapid Trigger — are implemented
and host-tested.** The *stored-only* rows need engine work plus on-hardware verification (and,
for DKS / SOCD / remap, confirmation of the exact vendor wire formats); they are tracked as
follow-ups rather than shipped unverified.

**Already done elsewhere:** [libhmk](https://github.com/peppapighs/libhmk) is a mature open-source
Hall-effect firmware that implements all of these modes (Dynamic Keystroke, Null Bind / SOCD /
Snap-Tap, Tap-Hold, Toggle, 8 kHz) on the same AT32F405 — a strong reference, or a drop-in
*replacement* firmware if you do not need stock-protocol / vendor-app compatibility.

The hardware-facing paths are implemented but **on-hardware bring-up is pending**:

- **WS2812 output** — the SPI2/DMA bring-up and the `0xC0`/`0xF0` encoding need LED
  colour/brightness verified on a board, with the refresh interval tuned if the latch margin
  requires it.
- **EFC save** — a 2 KB erase plus program from execute-in-place flash stalls the core for
  tens of milliseconds, which can disturb USB; the save is debounced, and moving the
  erase/program loop into RAM and/or chunking it across idle windows is a bring-up task.
- **Key-scan electrical details** — the column-counter pin roles/polarity, the ADC press
  polarity, the per-key calibration span, and the physical `column × 6 + row` orientation are
  behind clearly named switches and confirmed on hardware
  ([hardware.md §7](hardware.md#7-variant-specific-and-configurable-details)).

Not yet implemented (see the feature-status table above): the advanced per-key modes — DKS,
Mod-Tap, Toggle, Snap-Tap / SOCD — which the engine currently treats as Normal actuation; the
higher RGB effect modes (fall back to Static); the host
application's exact userpic/keymatrix chunk formats and the app-key↔scan-site index map
(handlers round-trip and persist a self-consistent format), live keymap remap applied to the
scan engine (persisted and round-tripped, but the engine types from the default ANSI map),
and onboard macros / Fn-layer / OLED.
