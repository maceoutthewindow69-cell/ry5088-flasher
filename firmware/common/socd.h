/*
 * socd.h - depth-aware A/D SOCD resolver.
 *
 * Policy: ordinary last-input priority while A and D overlap, except when both
 * physical switches are in the configured bottom zone, where both outputs are
 * suppressed (neutral). The resolver runs after Hall/RT processing and before
 * HID report assembly, so no host software or injected input is involved.
 */
#ifndef SOCD_H
#define SOCD_H

#include <stdint.h>
#include "hall.h"

/* HID Keyboard/Keypad usage IDs used by the A/D pair. */
#define SOCD_HID_A 0x04u
#define SOCD_HID_D 0x07u

enum {
  SOCD_LAST_NONE = 0,
  SOCD_LAST_A    = 1,
  SOCD_LAST_D    = 2,
};

typedef struct {
  int16_t a_idx;
  int16_t d_idx;
  uint8_t prev_a;
  uint8_t prev_d;
  uint8_t last;
} socd_ad_state_t;

/* Locate the physical A and D sites in keymap and clear runtime history. */
void socd_ad_init(socd_ad_state_t *s, const uint8_t *keymap);

/* Mutate pressed[] in place according to the depth-aware A/D policy.
 * `hall` supplies live physical travel; pressed[] is the Hall/RT actuation state.
 */
void socd_ad_apply(socd_ad_state_t *s, const hall_engine_t *hall, uint8_t *pressed);

#endif /* SOCD_H */
