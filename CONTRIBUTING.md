# Contributing — add your board

Welcome! This is a crowdsourced reverse-engineering project for **RongYuan RY5088** magnetic
keyboards (an **Artery AT32F405** main MCU + a **Panchip PAN1080** wireless co-processor, shipped
by MonsGeek, Akko, and others). The single most valuable contribution you can make is **adding a
board** — teaching the tools and the custom firmware about a model they don't know yet.

The headline: **a board is added by one file.** You don't touch the firmware engine, the USB stack,
or the vendor protocol — those are shared and already live in `firmware/common/`. You write one
profile that captures how *your* board differs, and the build generates the rest.

This project is **MIT-licensed** (see [`LICENSE`](LICENSE)). By contributing you agree your work is
released under the same terms.

---

## 1. The one-file model

Every supported keyboard is the **same firmware** specialised by **one profile**:

```
firmware/
  common/                 # shared: key-scan, Hall/Rapid-Trigger, RGB, USB, vendor protocol, wireless
  gen/gen_profile.py      # turns a profile into board-specific C
  profiles/
    2307.toml             # <- the Fun60 Ultra reference profile
    <your_dev_id>.toml    # <- the ONE file you add
  build/<dev_id>/         # generated board_config.h / board_keymap.h / board_led_map.c + image
```

The **platform is shared** (AT32F405 + PAN1080, the analog matrix front-end, the WS2812 RGB driver,
the vendor HID feature-report protocol). A board only differs in its **delta**:

- **identity** — `dev_id`, names, USB strings, version;
- **matrix geometry** — column/row counts;
- **keymap** — which HID usage sits at each scan site;
- **LEDs** — per-key count and the serpentine chain → physical-position map;
- **Hall/switch params** — travel model and default actuation;
- **wireless** — present or not.

`gen/gen_profile.py` reads your profile, merges it over the platform defaults, and emits three files
the shared firmware compiles against:

| Generated file | What it carries |
|---|---|
| `board_config.h` | identity, matrix geometry, Hall/switch model, LED counts, wireless flag (`#define`s) |
| `board_keymap.h` | the default keymap table |
| `board_led_map.c` | the LED serpentine map + colour palette |

That's the whole contract. If your board is an RY5088 (AT32F405) keyboard, you should never need to
touch C to bring it up — only the profile.

### Why boards are keyed by `dev_id` (and filenames are the `dev_id`)

The USB product ID **`0x5030` is shared across ~12 models**, so the PID *cannot* tell variants apart.
The only safe discriminator is the firmware **`dev_id`** — the value returned by the vendor
`GET_INFOR` command (a 16-bit little-endian number; e.g. `GET_INFOR` bytes `03 09` → `0x0903` →
**`dev_id` 2307**). HE and TMR builds of the *same* model carry **distinct** `dev_id`s, and flashing
the wrong one boots and lights up but breaks key-scan.

So this project keys everything off `dev_id`:

- **the filename is the `dev_id`** — `profiles/2307.toml`, decimal, no zero-padding;
- **the human name lives in `[meta].display_name`** ("Fun60 Ultra"), not the filename.

One unambiguous key per variant, machine-checkable, collision-free. See
[`docs/models.md`](docs/models.md) for the full `dev_id` → model table.

---

## 2. Two paths to a profile

### (a) Quick path — the `/re-tmr` Claude Code skill

If you're working in Claude Code, run the project skill from the repo root with the keyboard plugged
in:

```
/re-tmr
```

One command drives the whole loop: **detect** the connected board's `dev_id` → **fetch** its stock
firmware → **analyze** descriptors + image → **draft** a `profiles/<dev_id>.toml` (diffed against the
`2307` reference) → **build** it. It stops and shows you the drafted profile so you can review every
field before it's committed. It automates exactly the manual steps below — use it to get a strong
first draft, then verify against your hardware.

### (b) Manual path — five steps

```bash
# 1. Identify the connected board (read-only — no device changes).
ry-flash --detect                 # prints dev_id / model / version
ry-flash --detect --json          # same, machine-readable (USB descriptors too)

# 2. Pull its stock firmware (to study and to keep as your restore image).
ry-flash --fetch <dev_id>         # downloads <dev_id>/fw_at32.bin; no device needed

# 3. Start from the reference profile.
cp firmware/profiles/2307.toml firmware/profiles/<dev_id>.toml

# 4. Edit the fields for your board (see the schema reference in section 3).
$EDITOR firmware/profiles/<dev_id>.toml

# 5. Generate + build, then validate.
make -C firmware PROFILE=<dev_id>   # -> build/<dev_id>/ry5088_<dev_id>.bin
make -C firmware check              # lint every profile through the generator
```

Then flash your build to the board and **confirm typing and LEDs** (section 4 covers building/flashing;
section 5 covers recovery — you can always get back to stock).

`2307.toml` is heavily commented and intentionally lists fields it could have left blank, precisely so
it reads as a worked example. Diff your profile against it as you go.

---

## 3. Profile schema reference

Derived from [`firmware/profiles/2307.toml`](firmware/profiles/2307.toml) and the generator
[`firmware/gen/gen_profile.py`](firmware/gen/gen_profile.py). A profile is TOML with these sections:
`[meta] [usb] [version] [matrix] [hall] [leds] [rgb] [wireless] [keymap]`.

> **Two kinds of field — read this first.**
>
> - **Build parameters** are *consumed by the firmware*: the generator emits them into
>   `board_config.h` / `board_keymap.h` / `board_led_map.c`. Change these in the profile and the
>   behaviour of *your* build changes. This is almost everything you'll edit.
> - **Reference wiring** fields are **documentation**. The vendor reference-design GPIO pins, SPI
>   buses, and WS2812 bit-cell timings are **compiled into `firmware/common/`** (`keyscan.c`,
>   `rgb.c`, `wireless.c`) as defaults; the generator **does not emit them**. They live in the
>   profile so the board is fully documented in one place. **Editing a reference-wiring value in the
>   profile changes nothing on its own** — to actually rewire a board you must edit the corresponding
>   `.c` file in `common/`.

**The minimum valid profile** needs only six fields: `[meta].dev_id`, `[matrix].cols`, `[matrix].rows`,
`[leds].count`, `[leds].phys_col`, `[leds].phys_row`. Everything else inherits a platform default. The
reference profile fills in more so contributors can see the full surface.

The generator enforces three rules and aborts with a clear error otherwise:
- `[keymap].codes`, **if present**, must have exactly `cols * rows` entries;
- `[leds].phys_col` and `[leds].phys_row` must each have exactly `[leds].count` entries.

### `[meta]` — identity *(build parameters + catalog metadata)*

| Field | Required | Meaning |
|---|---|---|
| `dev_id` | **yes** | `GET_INFOR` id (decimal). The key. Emitted as `BOARD_DEV_ID` + the `GET_INFOR` id bytes. Must match the filename. |
| `display_name` | recommended | Human name ("Fun60 Ultra"). Used in the model catalog and as the USB product-string fallback. |
| `name` | optional | Vendor internal model name. **Catalog metadata only** (feeds `make models`); not compiled in. |
| `switch_type` | optional | `"HE"` or `"TMR"`. **Catalog metadata only**; not compiled in. |
| `wireless` | optional (default `false`) | `true` if a 2.4 GHz/BLE co-processor is present. Emitted as `BOARD_WIRELESS`. |

### `[usb]` — descriptors *(build parameters)*

All optional; each has a platform default. Match these to the stock device (`ry-flash --detect --json`).

| Field | Default | Meaning |
|---|---|---|
| `vid` | `0x3151` | USB vendor ID. |
| `pid` | `0x5030` | USB product ID (shared across models — not a discriminator). |
| `bcd_device` | `0x0603` | Device release; conventionally matches `[version]`. |
| `manufacturer_str` | `"MonsGeek Keyboard"` | iManufacturer string. |
| `product_str` | `[meta].display_name` | iProduct string (note: the stock Fun60 Ultra has a *leading space*). |
| `config_str` | `"Keyboard Config"` | iConfiguration string. |
| `interface_str` | `"Keyboard Interface"` | iInterface string. |

### `[version]` — firmware version *(build parameters)*

| Field | Default | Meaning |
|---|---|---|
| `hi` / `lo` | `0` / `0` | `GET_INFOR` firmware version high/low bytes. `hi=0x06, lo=0x03` → reported as `v0603`. |

### `[matrix]` — analog scan geometry

| Field | Required | Kind | Meaning |
|---|---|---|---|
| `cols` | **yes** | build | Column-counter positions. Emitted as `KS_COLS`. |
| `rows` | **yes** | build | Mux rows. Emitted as `KS_ROWS`. **Key index = `col * rows + row`** (sites = `cols * rows`, `MG_NUM_SITES`). |
| `reset_pin`, `clock_pin`, `strobe_pin` | — | reference | CD4017-style column-counter pins (reset / clock / strobe). Compiled into `common/keyscan.c`. |
| `row_addr_pins` | — | reference | 3-bit row-mux address `[A0, A1, A2]`. Compiled into `common/keyscan.c`. |
| `enable_pin`, `adc_pin`, `adc_channel` | — | reference | Sensor/mux enable, analog-mux common output, and its ADC channel. Compiled into `common/keyscan.c`. |
| `settle_col`, `settle_mux` | — | reference | Settle busy-loops after a column/mux move (HW-tunable). Compiled into `common/keyscan.c`. |

> Sites are indexed `key = col * rows + row` — the *leftmost column is indices `0..rows-1`*. The keymap
> and (implicitly) the geometry follow this order. See [`docs/hardware.md`](docs/hardware.md) §3.

### `[hall]` — magnetic travel model *(build parameters)*

All optional; each has a platform default (so a minimal profile can omit the whole section). Emitted as
`HALL_*` / `MG_DEF_MAG_*`. Travel is in **hundredths of a millimetre** (`cmm`); actuation/RT defaults are
re-settable by the user in the vendor app.

| Field | Default | Meaning |
|---|---|---|
| `press_decreases` | `true` | `true` = pressing **lowers** the ADC reading (`travel = baseline - raw`). Flip if your board reads the other way. |
| `full_travel_cmm` | `400` | Nominal full travel (4.00 mm). |
| `assumed_span` | `1600` | ADC counts spanned over full travel. |
| `noise_counts` | `16` | Baseline drift-follow noise band. |
| `press_cmm` / `release_cmm` | `200` / `150` | Default actuation / release points. |
| `rt_press_cmm` / `rt_release_cmm` | `50` / `50` | Default rapid-trigger press / release deltas. |

### `[leds]` — per-key RGB map

| Field | Required | Kind | Meaning |
|---|---|---|---|
| `count` | **yes** | build | Number of WS2812 LEDs in the chain. Emitted as `MG_NUM_LEDS`. |
| `phys_col` | **yes** | build | Array (length `count`): chain index → physical column. Emitted into `board_led_map.c`. |
| `phys_row` | **yes** | build | Array (length `count`): chain index → physical row. Emitted into `board_led_map.c`. |
| `max_col` | optional (default `cols`) | build | Physical column span, for effects. `MG_LED_MAX_COL`. |
| `max_row` | optional (default `rows-1`) | build | Physical row span, for effects. `MG_LED_MAX_ROW`. |
| `palette` | optional (default 8-colour palette) | build | `8 × [r,g,b]` chain palette. Emitted into `board_led_map.c`. |

> The LED chain is wired serpentine; `phys_col[i]` / `phys_row[i]` say where chain LED *i* physically
> sits so position-based effects work. Getting this map right is the fiddliest part of a new board —
> count LEDs along the solder chain and record each one's (col,row).

### `[rgb]` — WS2812 timing/wiring *(reference wiring — documentation)*

Not emitted. Compiled into `common/rgb.c`. Documented here for completeness.

| Field | Meaning |
|---|---|
| `bit0` / `bit1` | WS2812 logical-0 / logical-1 SPI byte patterns (`0xC0` / `0xF0`). |
| `data_pin` | SPI MOSI driving the LED chain (`PA10`, AF5). |
| `order` | Wire colour order (`"GRB"`). |

### `[wireless]` — radio *(one build parameter; pins are reference wiring)*

| Field | Kind | Meaning |
|---|---|---|
| `identity` | **build** | BLE/2.4G advertised base name (`WL_IDENTITY_STR`; defaults to the product string). Firmware appends a slot suffix (`-1`/`-2`/`-3`). |
| `detect_pin`, `detect_pull` | reference | Cable-sense pin (HIGH = wired). Compiled into `common/wireless.c`. |
| `sck`, `miso`, `mosi`, `cs`, `int` | reference | SPI bus + control lines to the PAN1080 radio. Compiled into `common/wireless.c`. |

> `[meta].wireless` is the build switch that turns the radio path on/off. The `[wireless]` SPI pins are
> reference wiring; rewiring requires editing `common/wireless.c`.

### `[keymap]` — default layout *(build parameter)*

| Field | Required | Meaning |
|---|---|---|
| `codes` | optional | `cols * rows` HID Keyboard/Keypad usage IDs, indexed by `key = col * rows + row`. `0xE0..0xE7` are modifiers; `0x00` = no key / unpopulated site. Emitted into `board_keymap.h`. If present, length **must** equal `cols * rows`. |

---

## 4. Building & validating

From the repo root (each target takes `PROFILE=<dev_id>`):

| Command | What it does |
|---|---|
| `make -C firmware PROFILE=<dev_id>` | Generate the board C and build `build/<dev_id>/ry5088_<dev_id>.bin` (+ a vector-table sanity check). |
| `make -C firmware check` | **Lint every profile** through the generator — catches missing required fields and length-mismatch errors. Fast; no toolchain needed. |
| `make -C firmware test` | Host-side unit tests (native `cc`): Hall/Rapid-Trigger math, HID report, vendor protocol, LED engine, persistence. **No ARM toolchain or BSP needed.** |
| `make -C firmware models` | Print the `dev_id ↔ display_name` catalog built from the profiles. |
| `make -C firmware flash-romdfu PROFILE=<dev_id>` | Flash *your built image* to the app slot via `dfu-util` (AT32 ROM-DFU). |
| `make -C firmware clean` | Remove `build/`. |

- `make check`, `make test`, and `make models` are **profile-only** — they need just Python 3.11+ and a C
  compiler, so you can validate a new profile without an embedded toolchain.
- Building the **ARM image** additionally needs `arm-none-eabi-gcc` and the **Artery AT32F402_405
  Firmware Library** (fetched separately into `firmware/vendor/`). See
  [`firmware/README.md`](firmware/README.md) for the toolchain and BSP setup.

A good pre-flash checklist: `make -C firmware check` passes → `make -C firmware test` passes →
`make -C firmware PROFILE=<dev_id>` builds and the vector-table line looks sane.

---

## 5. Safety & recovery — experiment fearlessly

**Flashing is recoverable. You will not brick your keyboard by trying.** This is by design, so please
experiment:

- **Custom firmware loads in the application slot only** (`0x08005200`), *behind* the factory
  bootloader. **The factory bootloader is never touched** by building or flashing here.
- **`ry-flash` always restores stock.** A failed or wrong flash leaves the board in its bootloader, and
  the flasher re-flashes the official image at any time:

  ```bash
  ry-flash --auto --arm        # detect, fetch the matching stock image if needed, and restore it
  ```

- **`ry-flash` is variant-safe and dry-run by default.** It only flashes the image matching the live
  `dev_id`, re-confirms the `dev_id` afterward, and refuses any image whose chip-ID isn't the keyboard's
  `AT32F405 8KMKB`. **Nothing is written unless you pass `--arm`.**
- Entering the bootloader **clears the app config** (LED settings, remaps — re-settable in the vendor
  app). **Per-key Hall calibration is preserved** (stored separately in flash).

Keep the stock image you pulled with `ry-flash --fetch <dev_id>` — it's your guaranteed way home. Full
details and last-resort recovery: [`docs/flashing.md`](docs/flashing.md).

> Note: the custom firmware builds and passes host tests but **has not completed on-hardware bring-up** —
> treat on-device flashing as experimental. The recovery guarantees above are exactly why that's safe to
> try.

---

## 6. Submitting a PR

Open a pull request that adds **`firmware/profiles/<dev_id>.toml`** and an **evidence bundle** so a
reviewer can trust the profile without owning the board. Include:

- **The profile** — `firmware/profiles/<dev_id>.toml` (filename = `dev_id`, `[meta].display_name` set).
- **`dev_id`** — from `ry-flash --detect` (and confirm it isn't already in `profiles/`).
- **USB descriptors** — paste `ry-flash --detect --json` (VID/PID/bcd, the strings you put in `[usb]`).
- **Stock firmware hash** — the SHA-256 of the image `ry-flash --fetch <dev_id>` downloaded, so the
  identity is pinned to a real factory build.
- **Reasoning / sources** — briefly, how you derived the **matrix geometry**, **keymap**, **LED map**,
  **Hall params**, and the **wireless flag** (datasheet, teardown photos, app behaviour, diffing against
  a known profile…). Note any field you're unsure about.
- **Bring-up result, if you flashed it** — does typing work? do the LEDs light in the right places? This
  is the highest-value evidence of all.

**CI validates automatically**: it runs `make -C firmware check` (every profile through the generator)
and `make -C firmware test`, so required-field and length-mismatch mistakes are caught for you. Make sure
both pass locally first. A profile that's still missing a confident keymap or LED map is welcome as a
**draft PR** — partial, clearly-labelled boards help the next contributor.

---

## 7. Scope

**In scope — RY5088 / AT32F405 keyboards.** Hall-effect (**HE**) and TMR magnetic boards on the RongYuan
RY5088 platform (Artery AT32F405 main MCU, optional Panchip PAN1080 radio), which all share the vendor
HID feature-report protocol and the analog-matrix front-end: the FUN60 family, FUN68/FUN75, M1/M2/M3 V5,
Akko 5075B/5087B HE, Shine68, and other `ry5088_*` boards. These are exactly the boards the firmware in
`common/` already supports — a new one is just a profile. The flasher confirms membership by refusing any
image that isn't an `AT32F405 8KMKB` keyboard build.

**Out of scope — different MCU or protocol.** Some keyboards from the same vendors use **different
silicon** and a different protocol; they are **not** RY5088 and do not belong here (and you must not run
`ry-flash` on them):

- **YiChip YC3121** — MG75S HE, M1W HE, M1 V5 (VIA).
- **YiChip YC500** — ICE75 (a mechanical, not magnetic, board).

Also out of scope: the separate **2.4 GHz dongle** (its firmware carries chip-ID `AT32F405 8K-DGKB`, not
`8KMKB`). See [`docs/models.md`](docs/models.md) for the supported/unsupported breakdown.

---

## Further reading

- [`firmware/profiles/2307.toml`](firmware/profiles/2307.toml) — the annotated reference profile.
- [`firmware/README.md`](firmware/README.md) — firmware build, toolchain, and BSP setup.
- [`docs/models.md`](docs/models.md) — the `dev_id` scheme and the full model table.
- [`docs/flashing.md`](docs/flashing.md) — `ry-flash` usage, the JSON/agent contract, and recovery.
- [`docs/hardware.md`](docs/hardware.md) — the board: MCU, flash map, analog matrix, RGB, wireless.
- [`docs/protocol.md`](docs/protocol.md) — the HID-DFU flash protocol, `GET_INFOR`, and the cloud firmware API.
- [`docs/firmware.md`](docs/firmware.md) — custom-firmware architecture and modules.

Thanks for helping map the RY5088 family. One profile at a time, this gets every one of these boards a
free, open firmware and a safe way home.
