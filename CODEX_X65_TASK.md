# Codex task: Attack Shark X65 HE depth-aware A/D SOCD

Target board: Attack Shark X65 HE, RY5088 / AT32F405, dev_id 2268.

## Required behaviour

- A only pressed -> output A
- D only pressed -> output D
- A held + D partially pressed -> last key pressed wins
- D held + A partially pressed -> last key pressed wins
- both A and D in the bottom zone -> output neither A nor D (neutral)
- leaving the bottom zone restores ordinary LKP immediately
- releasing the newer key while the older key remains physically active restores the older key immediately
- all logic must execute in keyboard firmware, not on the host

## Existing work on this branch

- `firmware/common/socd.h`
- `firmware/common/socd.c`
- `firmware/common/test/test_socd.c`

The resolver intentionally runs conceptually after Hall/Rapid Trigger physical actuation and before HID report assembly. It consumes `hall_engine_t.key[].travel` for live depth and mutates the physical `pressed[]` state into the desired logical A/D output state.

## Tasks

1. Review the resolver for edge cases and fix anything incorrect.
2. Integrate it into `firmware/common/main.c` after `hall_process(&hall, raw, pressed)` and before HID report construction.
3. Keep the feature disabled by default for existing profiles.
4. Extend `firmware/gen/gen_profile.py` with an optional `[socd]` section:
   - `ad_depth_lkp = false` default
   - `bottom_cmm = HALL_FULL_TRAVEL_CMM` default
   Emit `SOCD_AD_ENABLED` and `SOCD_AD_BOTTOM_CMM` into `board_config.h`.
5. Wire `socd.c` into the ARM build and `test_socd.c` into host tests.
6. Add/expand tests covering:
   - A only
   - D only
   - A then D partial -> D
   - D then A partial -> A
   - both in bottom zone -> neutral
   - one key exits bottom zone while both remain actuated -> LKP winner resumes
   - release newer key -> older key resumes immediately
   - Rapid Trigger state is preserved because SOCD operates after Hall processing
7. Add an X65 profile `firmware/profiles/2268.toml`, but do NOT guess matrix geometry, keymap, LED mapping, ADC direction/span, or wiring. Recover/verify those fields from the stock `2268_v309.bin`, live read-only detection, or other defensible evidence.
8. For the X65 profile enable:
   ```toml
   [socd]
   ad_depth_lkp = true
   bottom_cmm = 340
   ```
   Treat 3.40 mm as an initial calibration value, not an assumed mechanical truth. Make it easy to adjust after hardware testing.
9. Do not flash hardware automatically. Produce the build and recovery instructions separately, keeping the factory bootloader and stock restore path intact.

## Acceptance criteria

Host tests must prove the exact six-state user behaviour symmetrically. Existing profile tests must remain unchanged when SOCD is disabled. The X65 build must not be considered safe to flash until the 2268 board profile has been verified against hardware/stock firmware.
