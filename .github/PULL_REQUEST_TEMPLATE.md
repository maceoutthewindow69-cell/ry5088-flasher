<!--
Thanks for the contribution! This template targets a new-board PR (adding a
profile). For other changes, just delete the parts that don't apply.
-->

## What does this PR do?

<!-- Brief summary. For a new board: which model + dev_id. -->

- Model:
- dev_id:
- Switch type: HE / TMR
- Wireless: yes / no

## New-board checklist

- [ ] Profile added at `firmware/profiles/<dev_id>.toml`
- [ ] `make -C firmware PROFILE=<dev_id>` builds cleanly (image produced)
- [ ] `make -C firmware check` passes (profile validates through the generator)
- [ ] Host tests still pass: `make -C firmware test && make -C firmware test-wireless`
- [ ] Evidence bundle attached (descriptors from `ry-flash --detect --json` and/or stock firmware hash)

## Hardware confirmation

- [ ] Flashed to a real device with `ry-flash`
- [ ] Confirmed working on hardware: **typing** (all keys / layers) and **LEDs** (RGB effects)
  - If wireless: 2.4GHz / Bluetooth checked too
- [ ] Not yet tested on hardware (build-only) — note this so reviewers know

<!-- Flashing is recoverable: ry-flash writes only the app slot and is
     variant-safe; a board can always be re-flashed (ROM-DFU fallback). -->

## Notes

<!-- Anything reviewers should know: layout, RGB map, quirks, references. -->
