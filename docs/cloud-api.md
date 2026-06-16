# RongYuan firmware cloud API (reference)

The vendor (RongYuan, `rongyuan.tech`) runs an **unauthenticated** firmware-distribution API for
RY5088 keyboards. `ry-flash --fetch <dev_id>` and the `--auto` auto-download use it. This is a
condensed reference for that surface — derived from read-only use of the public API and static
analysis of the MonsGeek Driver v4 bundle (the device map and SHA-256 manifest shipped under
`flasher/data/` come from the same source).

## Firmware / version — host `api2.rongyuan.tech:3816` (no auth)

Standard envelope on `/api/v2`: `{"code":0,"err_message":null,"data":{…}}` (`code==0` = ok). Note the
download endpoint lives at the host **root**, not under `/api/v2`.

| Endpoint | Body | Returns |
|---|---|---|
| `POST /api/v2/get_fw_version` | `{"dev_id": <int>}` | `{version_str, file_path, company, lowest_app_version_str, description, …}` — an invalid `dev_id` returns HTTP 400/500 (non-deterministic) |
| `GET /download/<file_path>` | — | the firmware blob (see packaging below) |

Only the `dev_id` is needed; there is **no rate-limiting**, which is why the whole `dev_id`
space is enumerable (the source of `flasher/data/ry5088_devices.json`).

### Packaging

The downloaded blob is either a **PK ZIP** or a **raw DEFLATE** stream (no zlib header — inflate
with a -15 window). ZIP members map to firmware targets:

| Member | Target |
|---|---|
| `firmwareFile.bin` | **AT32F405 keyboard image** — the only one `ry-flash` flashes |
| `firmwareRFFile.bin` | 2.4 GHz radio (non-RY5088 platforms) |
| `firmwareMledFile.bin` | matrix-LED MCU |
| `firmwareOledFile.bin` | OLED |
| `firmwareFlashFile.bin` | external flash |
| `firmwareNordicFile.bin` | PAN1080 — **never present** (the radio firmware is not API-distributed) |

`ry-flash` extracts `firmwareFile.bin`, confirms the `AT32F405 8KMKB` chip-ID, and verifies the
SHA-256 against `flasher/data/ry5088_firmware_sha256.json` when the board is listed (a mismatch is
warned, not fatal — the API serves the latest firmware, so a mismatch usually means an update).

## Host driver transport (reference)

The MonsGeek Driver v4 desktop app talks to a local **gRPC** service (`driver.DriverGrpc`, served by
`iot_driver`) which forwards raw HID reports. The pieces relevant to this project:

- **Checksum modes** (`CheckSumType`): `Bit7` (7-bit header checksum), `Bit8` (8-bit), `None`. The
  firmware here implements the same Bit7/Bit8 scheme (see [`protocol.md`](protocol.md) and the
  vendor protocol in [`firmware.md`](firmware.md)).
- **Report pipes**: `sendMsg`/`readMsg` (HID output/input reports) and `sendRawFeature`/
  `readRawFeature` (HID **feature** reports — how vendor config GET/SET travels).
- **Dongle routing** (`DangleDevType`): when a board is reached over the 2.4 GHz dongle, the target
  sub-device (Keyboard/Mouse) is selected in the request — relevant to the wireless path.
- **OTA**: `upgradeOTAGATT` streams a whole image over BLE GATT (a path `ry-flash` does not use; it
  flashes the AT32 side over USB HID-DFU).

The per-feature payload byte layouts (per-key magnetism, keymap remap, DKS, macros) are built by the
app *renderer* as raw report bytes and are **not** described by the gRPC schema.

## Scope / what the API does not provide

- **No PAN1080 (radio) firmware** — 0 of the published packages contain it.
- **No per-board LED coordinate maps** and **no scan-matrix geometry** — these are board-physical and
  are not in the app bundle (they are why a new board profile still needs hardware/firmware RE; see
  [`../CONTRIBUTING.md`](../CONTRIBUTING.md)).
- **No cloud config / board-sync** — the account API exposes only the user's own profile.

The firmware images served here are the vendor's **proprietary binaries**; `ry-flash` fetches them on
demand and this repository does **not** redistribute them.
