# Supported models

These tools target keyboards built on the **RongYuan RY5088** platform — an **Artery AT32F405** main MCU paired
with a **Panchip PAN1080** wireless co-processor. The same platform ships under several brands (MonsGeek, Akko,
and others). It covers the Hall-effect (**HE**) and tunneling-magnetoresistance (**TMR**) magnetic keyboards;
TMR boards also accept 3/5-pin mechanical switches ("MagMech").

## Identifying a model — use the dev_id, not the USB PID

A keyboard reports a **`dev_id`** (a 16-bit value) over the configuration protocol (`GET_INFOR`). This is the only
reliable model identifier: the **USB product ID `0x5030` is shared across a dozen different models**, so the PID
alone cannot tell variants apart. HE and TMR versions of the same model carry **distinct dev_ids** (e.g. M1 V5 HE
= 2819, M1 V5 TMR = 2949) — and flashing the wrong variant's firmware boots and lights up but breaks key-scan,
because the sensor front-end and calibration differ. The flasher always keys off the live `dev_id`.

`ry-flash --detect` prints the connected board's dev_id and model. The full dev_id → model table (284 RY5088
devices) is embedded in the flasher (`flasher/data/ry5088_devices.json`).

## Magnetic (HE / TMR) model family

| Model | dev_ids |
|---|---|
| FUN60 | 2299 |
| FUN60 Pro | 2304, 2305, 2464, 2600, 2785 |
| FUN60 Max | 2306, 3299 |
| FUN60 Ultra | 2307, 2352, 2381, 2387 |
| FUN68 | 2811, 3091 |
| FUN68 (ISO) | 3429 |
| FUN75 | 2648 |
| M1 V5 (HE) | 2819 |
| M1 V5 (TMR) | 2679, 2949 |
| M2 V5 / M2 V5 HE | 2601, 2845 |
| M3 V5 / M3 V5 HE | 2585, 2874 |
| Akko 5075B V2 HE / 5075S HE | 2807, 3129 |
| Akko 5087B V2 HE / 5087S V2 HE | 2829, 3131 |
| Shine68 | 3269, 3270 |

Other Akko HE boards on the same platform (MOD007 V5 HE, TAC75 HE, and similar) share the protocol and are
flashable the same way; check `--detect` against the embedded map.

## Not supported (different silicon)

Some keyboards listed by the same vendor driver use a **different MCU** and are **not** RY5088 — do not use this
flasher on them:

- **MG75S HE**, **M1W HE**, **M1 V5 (VIA)** — YiChip **YC3121**.
- **ICE75** — YiChip **YC500** (and a mechanical, not magnetic, board).

The flasher refuses to write an image whose chip-ID is not the keyboard's `AT32F405 8KMKB`, so it also will not
flash the separate 2.4 GHz dongle (whose firmware carries `AT32F405 8K-DGKB`).

## Firmware availability

Per-model stock firmware can be downloaded on demand by the flasher (`--fetch <dev_id>`, or automatically by
`--auto`). The vendor cloud currently serves the FUN60 family; some newer models have no published cloud firmware
and would need a locally-supplied image. See [`flashing.md`](flashing.md).
