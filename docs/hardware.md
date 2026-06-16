# RY5088 Board Hardware

This document specifies the hardware of the RongYuan **RY5088** keyboard platform: the
main MCU and its clock tree, the on-chip flash layout, the analog Hall-effect key matrix,
the per-key RGB chain, the wireless co-processor, and the separate 2.4 GHz dongle. RY5088
is the OEM marketing name for boards built on the Artery **AT32F405** with a Panchip
**PAN1080** radio; the same platform underlies the RongYuan magnetic line (FUN60
Ultra/Pro/Max, FUN68, M1/M2/M3 V5, and siblings).

Some details vary per board variant or are configurable in firmware; these are flagged in
[§7](#7-variant-specific-and-configurable-details). For the flash protocol see
[protocol.md](protocol.md); for firmware structure see [firmware.md](firmware.md).

---

## 1. Main MCU — Artery AT32F405

| Property | Value |
|---|---|
| Core | ARM Cortex-M4F (FPU), up to **216 MHz** |
| Marketing name | RY5088 (RongYuan branding for the same silicon) |
| Package | LQFP64 (keyboard) |
| Flash | **256 KB** internal (`0x08000000`–`0x08040000`), execute-in-place |
| Chip-ID string | `AT32F405 8KMKB` (at flash `0x08005000`) |
| ADC | 12-bit successive-approximation, ADC1 at `0x40012000` |
| USB | OTG (full-speed device, OTGFS1) |
| USB identity (normal) | VID `0x3151` / PID `0x5030`, vendor HID usage page `0xFFFF` |

### 1.1 Clock tree

The MCU runs from a **12 MHz** external crystal (HEXT). The PLL produces the system clock
and the USB clock from that reference:

```
VCO    = HEXT x NS / MS   = 12 MHz x 72 / 1   = 864 MHz
SYSCLK = VCO / FR         = 864 MHz / 4       = 216 MHz   (Cortex-M4F core, AHB, APB2)
USB48M = VCO / PLLU_div   = 864 MHz / 18      = 48.000 MHz (OTGFS)
```

| Bus | Prescale | Frequency |
|---|---|---|
| AHB | /1 | 216 MHz |
| APB2 | /1 | 216 MHz |
| APB1 | /2 | 108 MHz |

The 12 MHz crystal is the platform reference: the AT32F405 high-speed USB PHY PLL is fixed
to a 12 MHz HEXT, and 12 MHz yields both an exact 216 MHz core and an exact 48 MHz USB
clock with the smallest PLL multipliers. SPI2, used for the RGB chain, is on APB1 and so
has a 108 MHz kernel clock ([§4](#4-per-key-rgb-ws2812-over-spi)).

---

## 2. Flash map

Internal flash is organized into 2 KB pages. The low region holds the resident bootloader;
the application and its persisted configuration occupy the middle; calibration and the
factory marker sit above.

| Range | Size | Contents |
|---|---|---|
| `0x08000000`–`0x08004FFF` | 20 KB | **Resident bootloader** (DFU, [protocol.md](protocol.md)) |
| `0x08004800` | (in bootloader region) | **Boot mailbox** — `0x55AA55AA` requests DFU; factory-reset marker |
| `0x08005000` | 512 B | **Chip-ID header** — `AT32F405 8KMKB`; flash destination for an image |
| `0x08005200` | — | **Application** entry point |
| `0x08028000` | 2 KB | **Config header** (profile, polling, LED, options, sleep timers) |
| `0x08028800`–`0x0802A800` | 8 KB | Keymaps (one page per profile) |
| `0x0802A800`–`0x0802B800` | 4 KB | Fn / advanced-key pages |
| `0x0802B800`–`0x0802F800` | 16 KB | Macros |
| `0x0802F800`–`0x08032000` | 10 KB | User-picture (per-key colour) pages |
| `0x08032000` / `0x08032800` | 4 KB | **Magnetism ADC calibration** (preserved across reflash) |
| `0x08033000` | 2 KB | Magnetism travel-curve table |
| `0x08033800`–`0x08037800` | 16 KB | Per-key magnetism config (4 KB per profile) |

The bootloader auto-erases `0x08005000`–`0x08028000` on DFU entry. The custom application
links at `0x08005200` and keeps its persisted state inside the `0x08028000` config sector,
above the application image and clear of the stock keymap pages.

### 2.1 Embedded flash controller (EFC)

| Register | Address | Notes |
|---|---|---|
| EFC base | `0x40023C00` | — |
| UNLOCK | `0x40023C04` | Key sequence `0x45670123` then `0xCDEF89AB` |
| STS | `0x40023C0C` | bit0 = `BSY` |
| CTRL | `0x40023C10` | bit0 `fprgm`, bit1 `secers`, bit6 `erstr`, bit7 `oplk` |
| ADDR | `0x40023C14` | Target address for erase/program |

Erase granularity is one 2 KB sector. Word program: set `fprgm`, write the word to the
target address, wait for `BSY` to clear, clear `fprgm`. Sector erase: set `secers`, write
the sector to `ADDR`, set `erstr`, wait for `BSY` to clear, clear `secers`. Flash is
readable execute-in-place, so reads use ordinary loads.

---

## 3. Analog key matrix (Hall-effect)

Each key has a Hall-effect sensor (or, on TMR variants, a tunnel-magnetoresistance sensor)
whose analog output tracks the magnet position as the key travels. A single ADC channel is
multiplexed across all sense sites.

| Element | Resource | Notes |
|---|---|---|
| ADC | ADC1 @ `0x40012000` | single SW-triggered ordinary conversion, 12-bit result |
| Analog input | **PA2 = ADC1_IN2** (channel 2) | mux common output routed to PA2 |
| Row mux (8:1) | **PC6 / PC7 / PC8** (GPIOC) | 3-bit binary address, PC6 = LSB; 74HC4051-class |
| Column counter | **PA4 / PA5 / PA6** (GPIOA) | CD4017-style one-hot column walk |
| Matrix | **14 columns x 6 rows = 84 sites** | not all sites populated |
| Key index | `key_index = column x 6 + row` | leftmost column = indices 0..5 |

### 3.1 Per-sample sequence

For each site the firmware selects the column on the counter, selects the row on the mux,
triggers one ADC conversion, and reads the 12-bit result:

```
ADC->CTRL2 |= (1 << 22);          // OCSWTRG: start ordinary conversion
while ((ADC->STS & 0x2) == 0) ;   // wait OCCE (conversion complete)
ADC->STS = ~0x2;                  // clear
raw = ADC->ODT & 0xFFFF;          // 12-bit reading
```

### 3.2 Column counter

The column counter is driven as a reset-then-clock device. To select column *c* the
firmware homes the counter and issues clock pulses; PA5 is the clock, PA4 the reset/gate
(toggled once per clock), and PA6 the strobe/output-enable bracketing the count. A
14-column walk uses two cascaded decade counters (one supplies columns 0–9, the other
10–13). The row mux address (PC6/PC7/PC8) selects one of six rows per column step;
channel 7 of the same mux is an auxiliary analog input (slider/mode sensor), not part of
the key matrix.

### 3.3 Travel and rapid trigger

Pressing moves the magnet toward the sensor; the ADC reading **decreases** with depth. The
firmware tracks, per key, a released-rest baseline (running maximum) and a deepest-press
extreme (running minimum), normalizes the current reading to a 0–2048 position, and maps
position to travel (centi-millimetres) through a curve table. Travel feeds an actuation
state machine and a continuous rapid-trigger engine (press/release measured from the point
where each stroke reverses direction). The engine and its host protocol are in
[firmware.md](firmware.md).

---

## 4. Per-key RGB (WS2812 over SPI)

Per-key addressable RGB LEDs (WS2812-class) are driven as a single chain from one SPI
peripheral, with each LED bit encoded as one SPI byte.

| Parameter | Value |
|---|---|
| Peripheral | SPI2 (APB1, 108 MHz kernel) |
| Mode | half-duplex master, MSB-first, 8-bit frames, CPOL 0 / CPHA 1 |
| Baud divider | `MCLK_DIV_16` → 108 MHz / 16 = **6.75 MHz** (≈148 ns / SPI bit) |
| Data pin | **PA10** (alternate function 5), push-pull |
| DMA | DMA1 channel 1, DMAMUX request `SPI2_TX` (`0x0D`), single-shot per frame |
| Colour order | **GRB**, 24 bytes per LED |
| Frame | 61 LEDs × 24 = **1464 bytes** |

Each WS2812 data bit is one 8-bit SPI byte (8 × 148 ns ≈ 1.185 µs/symbol):

| Logical bit | SPI byte | High time |
|---|---|---|
| 0 | `0xC0` = `11000000` | ≈296 ns (T0H) |
| 1 | `0xF0` = `11110000` | ≈593 ns (T1H) |

Both patterns end low, so MOSI idles low after the DMA completes and the >50 µs latch comes
from the inter-frame idle (no trailing reset bytes are sent). Because the baud divider is
symbolic, the bit period auto-tracks the real APB1 clock if a variant uses a different
crystal. The keyboard's physical-position → chain-index mapping is a serpentine layout
(row 1 left-to-right, row 2 right-to-left, and so on).

---

## 5. Wireless co-processor — Panchip PAN1080

Tri-mode boards carry a **PAN1080** radio SoC (Panchip, ARM Cortex-M0; package marking
`APAN1080UA3C`) providing **BLE 5** and **2.4 GHz** connectivity. The AT32F405 is the SPI
master to the PAN1080 over a dedicated report bus:

| Parameter | Value |
|---|---|
| Bus | SPI3, master, full-duplex, 8-bit, MSB-first, /16, Mode 1 (CPOL 0 / CPHA 1) |
| Chip select | software CS on **PA15** |
| SCK / MISO / MOSI | **PB3 / PB4 / PB5** (alternate function 6) |
| Interrupt | **PD2** (input) |
| Frame | `[CMD][LEN][DATA × LEN][CHK]`, zero-padded to a multiple of 4 bytes |
| Frame checksum | `CHK = (sum of DATA bytes) & 0xFF` (CMD and LEN are not summed) |

Reports (boot keyboard, connection state, device identity, vendor/config blocks) are
wrapped in this frame and handed to the radio. Wired-only boards omit the PAN1080 (the
footprint is unpopulated); the same AT32F405 firmware runs in both cases and simply does
not take the wireless report branch.

---

## 6. The 2.4 GHz dongle (separate board)

The 2.4 GHz receiver dongle is a **separate board**, not a peripheral of the keyboard:

| Property | Value |
|---|---|
| MCU | AT32F405 (QFN32) — its own instance |
| Chip-ID string | **`AT32F405 8K-DGKB`** (distinct from the keyboard's `8KMKB`) |
| Radio | Panchip PAN1082 (2.4 GHz) |
| USB | OTG high-speed (OTGHS) |
| USB identity | normal PID `0x5038`, bootloader PID `0x5039` |
| PCB marking | `ry6208 v0.1` |

Because the dongle's chip-ID is `AT32F405 8K-DGKB`, **keyboard and dongle images are not
interchangeable**. The flasher refuses any image whose chip-ID is not the keyboard's
`AT32F405 8KMKB`, and dongle images carry the `8K-DGKB` chip-ID — see
[flashing.md](flashing.md).

---

## 7. Variant-specific and configurable details

Several hardware details differ per board variant or are abstracted behind firmware
switches. Treat the following as configurable / to be confirmed against the specific board:

- **Analog front-end topology.** The 60% board described above uses a single ADC channel
  behind a CD4017-style column counter plus an 8:1 row mux. Other variants (for example the
  75% M1 V5) use **15 direct ADC channels scanned by DMA** with a row multiplexer and no
  decade counter. The matrix dimensions and channel assignment are therefore
  variant-specific.
- **CD4017 pin roles / polarity.** PA5 is unambiguously the clock; the exact roles of PA4
  (reset/gate) and PA6 (strobe / output-enable vs. clock-inhibit) and their active polarity
  depend on the fitted 74-series part. Firmware replays a fixed GPIO write sequence, so
  column ordering is the only thing to revisit if columns come out permuted.
- **ADC press polarity.** Pressed = lower ADC count on the documented board; a single
  firmware switch flips this for sensors wired the other way.
- **WS2812 bit timing.** The `0xC0` / `0xF0` symbol patterns and the SPI baud divider are
  firmware constants; tune them per board if the LED timing margin requires it.
- **Crystal value.** 12 MHz is the platform reference and is mandatory for the OTGHS dongle;
  confirm the can marking on a given keyboard PCB if a variant is suspected to differ (the
  SPI/LED timing scales with it but the firmware does not change).
- **LED count and physical mapping.** The 61-LED count and the serpentine chain layout are
  for the documented board; per-model layouts differ.
