/*
 * socd.c - depth-aware A/D SOCD resolver.
 */
#include "socd.h"
#include "board_config.h"
#include "keyscan.h"

/* This fork targets the Attack Shark X65 HE (RY5088 dev_id 2268). Existing
 * profiles remain unchanged: unless a build explicitly overrides these macros,
 * the resolver is enabled only for that device id. The bottom threshold follows
 * the board profile's calibrated full-travel value. */
#ifndef SOCD_AD_ENABLED
#define SOCD_AD_ENABLED (BOARD_DEV_ID == 2268u)
#endif
#ifndef SOCD_AD_BOTTOM_CMM
#define SOCD_AD_BOTTOM_CMM HALL_FULL_TRAVEL_CMM
#endif

static uint8_t in_bottom_zone(const hall_engine_t *hall, unsigned idx)
{
  int threshold = (int)hall_key_cmm_to_counts(hall, idx, SOCD_AD_BOTTOM_CMM);
  return hall->key[idx].travel >= threshold;
}

void socd_ad_init(socd_ad_state_t *s, const uint8_t *keymap)
{
  s->a_idx = -1;
  s->d_idx = -1;
  s->prev_a = 0;
  s->prev_d = 0;
  s->last = SOCD_LAST_NONE;

  for (unsigned i = 0; i < KS_NUM_KEYS; i++) {
    if (s->a_idx < 0 && keymap[i] == SOCD_HID_A) s->a_idx = (int16_t)i;
    if (s->d_idx < 0 && keymap[i] == SOCD_HID_D) s->d_idx = (int16_t)i;
  }
}

void socd_ad_apply(socd_ad_state_t *s, const hall_engine_t *hall, uint8_t *pressed)
{
#if !SOCD_AD_ENABLED
  (void)s; (void)hall; (void)pressed;
  return;
#else
  if (s->a_idx < 0 || s->d_idx < 0) return;

  unsigned ai = (unsigned)s->a_idx;
  unsigned di = (unsigned)s->d_idx;
  uint8_t a = pressed[ai] ? 1u : 0u;
  uint8_t d = pressed[di] ? 1u : 0u;

  /* Detect rising edges before mutating outputs. If both rise in one scan,
   * preserve the previous winner when available; otherwise choose D
   * deterministically. Real human presses normally land on separate scans. */
  if (a && !s->prev_a && !(d && !s->prev_d)) s->last = SOCD_LAST_A;
  else if (d && !s->prev_d && !(a && !s->prev_a)) s->last = SOCD_LAST_D;
  else if (a && !s->prev_a && d && !s->prev_d && s->last == SOCD_LAST_NONE)
    s->last = SOCD_LAST_D;

  s->prev_a = a;
  s->prev_d = d;

  if (!a && !d) {
    s->last = SOCD_LAST_NONE;
    return;
  }
  if (a && !d) {
    pressed[ai] = 1;
    pressed[di] = 0;
    s->last = SOCD_LAST_A;
    return;
  }
  if (d && !a) {
    pressed[ai] = 0;
    pressed[di] = 1;
    s->last = SOCD_LAST_D;
    return;
  }

  /* Both physically active: bottom+bottom is neutral; otherwise LKP. */
  if (in_bottom_zone(hall, ai) && in_bottom_zone(hall, di)) {
    pressed[ai] = 0;
    pressed[di] = 0;
    return;
  }

  if (s->last == SOCD_LAST_A) {
    pressed[ai] = 1;
    pressed[di] = 0;
  } else {
    pressed[ai] = 0;
    pressed[di] = 1;
    if (s->last == SOCD_LAST_NONE) s->last = SOCD_LAST_D;
  }
#endif
}
