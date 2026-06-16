# Custom Firmware (Artery AT32F405 / RY5088)

Open-source custom firmware for RongYuan **RY5088** magnetic (Hall-effect / TMR)
keyboards, built around an Artery **AT32F405** (Cortex-M4) MCU. It links into the
**application slot at `0x08005200`**, behind the factory bootloader, so it can be
flashed without erasing the bootloader and the stock image can always be restored.

## One firmware, many boards

The platform (AT32F405 + PAN1080) and the vendor HID protocol are shared across the
whole RY5088 family, so the firmware is **one shared codebase specialised per board
by a profile**:

```
firmware/
  common/                  shared engine: analog key-scan + Hall Rapid-Trigger,
                           WS2812 RGB (SPI + DMA), flash-config persistence (EFC),
                           the vendor feature-report protocol, optional 2.4 GHz / BLE,
                           the USB device stack and BSP glue
  profiles/<dev_id>.toml   the per-board delta: identity, matrix geometry, keymap,
                           LED count + map, Hall/switch params, wireless flag
  gen/gen_profile.py       turns a profile into board_config.h / board_keymap.h /
                           board_led_map.c (written to build/<dev_id>/)
  Makefile                 make PROFILE=<dev_id>  ->  build/<dev_id>/ry5088_<dev_id>.bin
```

The reference profile is **`profiles/2307.toml`** (MonsGeek Fun60 Ultra, a 61-key /
60% board). Adding another keyboard is one new profile — see
[`../CONTRIBUTING.md`](../CONTRIBUTING.md).

## Stock firmware

The firmware here is an **independent, open-source implementation** — it is **not**
the keyboard's factory image. The real **stock firmware** is the keyboard vendor's
**proprietary binary** and is **not redistributed in this repository**. The flasher
(`../flasher`, binary `ry-flash`) **downloads it on demand** and caches it locally:

```sh
ry-flash --fetch <dev_id>      # download this model's stock image (to flash or study)
```

Building and flashing the firmware here neither requires nor redistributes that
proprietary image.

## Toolchain

- **ARM image:** `arm-none-eabi-gcc` (plus `arm-none-eabi-objcopy` / `arm-none-eabi-size`),
  GNU `make`, and `python3` (3.11+, for the profile generator). Flashing additionally
  needs `dfu-util`.
- **Host unit tests:** any native C compiler (`cc` / `gcc`) and `python3`. **No ARM
  toolchain and no BSP** are required to run the tests.

## Fetching the Artery BSP

The ARM image links against the **Artery AT32F402_405 Firmware Library** (the
CRM/GPIO/SPI/DMA/ADC peripheral drivers, CMSIS, and the USB device stack). It is
**not** included here — it has its own license (see
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md)) and must be fetched separately:

```sh
# from this firmware/ directory
git clone https://github.com/ArteryTek/AT32F402_405_Firmware_Library \
    vendor/AT32F402_405_Firmware_Library
```

The `Makefile` finds the BSP via `BSP_DIR` (default `vendor/AT32F402_405_Firmware_Library`),
overridable from the environment or command line.

## Building a board image

```sh
make PROFILE=2307            # generate config, compile common/, link -> build/2307/ry5088_2307.bin
```

This runs the generator for the profile, compiles `common/` against the generated
`board_config.h` / `board_keymap.h` / `board_led_map.c`, and prints a size summary
plus a vector-table sanity check (loaded at `0x08005200`). `make clean` removes
`build/`. Other targets:

```sh
make models                 # list the dev_id <-> model table for every profile present
make check                  # validate every profile through the generator
```

## Host unit tests (no BSP needed)

The hardware-independent logic is unit-tested natively with the system `gcc`/`cc`
(the generator runs first to provide the board constants):

```sh
make test            # Hall/Rapid-Trigger + HID report, vendor protocol,
                     # LED engine, flash-config persistence, magnetism round-trip
make test-wireless   # SPI wireless frame builder
```

Current results: **all host tests pass** — `make test` (test_engine + test_functional)
and `make test-wireless` (15/15), 0 failures. Tests default to `PROFILE=2307`.

## Status

**Host unit tests pass**, covering the pure logic (protocol, descriptors,
Hall/Rapid-Trigger math, HID report assembly, LED effects, flash-config
(de)serialisation, and the wireless frame builder). The Fun60 Ultra image also
links and passes its app-slot vector-table check.

**Feature status:** the basic magnetic features (per-key actuation + Rapid Trigger),
RGB effects, and flash persistence are implemented and host-tested. Several advanced
configurator features — DKS, Mod-Tap, Toggle, Snap-Tap / SOCD, keymap remap, and macros —
are recognised by the vendor protocol but **not yet acted on by the engine** (DKS/Mod-Tap/
Toggle/SOCD fall back to Normal actuation, remap uses the default keymap, and macros are only
a SET stub). See the feature-status table in [`../docs/firmware.md`](../docs/firmware.md) (§4.1).

**On-hardware bring-up has not yet been performed.** Flashing this custom firmware
to a real keyboard is **experimental** and unverified on physical hardware. Because
the firmware loads into the application slot and leaves the factory bootloader
intact, **the flasher can always restore the stock firmware** if anything misbehaves
(`ry-flash`; see [`../docs/flashing.md`](../docs/flashing.md)).

### Bring-up notes (settings to confirm on real hardware)

A few choices are set to a sensible default but can only be confirmed against a
physical board. If behaviour looks wrong during bring-up, check these first:

- **`[hall].press_decreases`** (in the board profile) — sign of travel. `true`
  assumes a key press *lowers* the ADC reading (`travel = baseline - raw`). Set it
  `false` if a board reads the other way, then rebuild.
- **`[matrix]` geometry / `[keymap]` / `[leds]`** (in the profile) — the per-board
  data the generator compiles in. A wrong key count, keymap, or LED map shows up as
  mis-mapped keys or LEDs.
- **Reference-design wiring** — the GPIO pin roles (`PA4`/`PA5`/`PA6` column counter,
  `PC6`/`PC7`/`PC8` row mux, the WS2812 SPI bit-cells `0xC0`/`0xF0`, the SPI3 radio
  pins) are compiled into `common/keyscan.c`, `common/rgb.c` and `common/wireless.c`.
  The profile documents them, but rewiring a board means editing those `.c` files.

## License

The firmware sources original to this project are released under the **MIT License**
(see the repository `LICENSE`). The Artery BSP and the BSP-derived template files in
`common/` keep their own license — see
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).

## Further documentation

- [`../CONTRIBUTING.md`](../CONTRIBUTING.md) — add your keyboard (the profile schema + workflow).
- [`../docs/firmware.md`](../docs/firmware.md) — firmware architecture and modules.
- [`../docs/hardware.md`](../docs/hardware.md) — board, MCU, switches, and pin map.
