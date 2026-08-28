/*
 * hall.c - per-key Hall processing: baseline calibration, travel, actuation and
 *          Rapid Trigger. Pure logic apart from the X65's read-only calibration
 *          lookup during initialisation.
 */
#include "hall.h"
#include "socd.h"
#include "board_keymap.h"
#include <stdint.h>

/* HALL_NOISE_COUNTS and the per-key defaults (HALL_DEF_*) come from board_config.h. */

static socd_ad_state_t socd_ad;
static uint8_t socd_ad_ready = 0;

/* X65 v309 stock firmware stores two 126-entry uint16 calibration arrays in
 * flash pages that the RY bootloader deliberately preserves across application
 * reflashes:
 *
 *   0x08032000: released/rest raw ADC average
 *   0x08032800: full-press/bottom raw ADC average
 *   page[0x7FE..0x7FF] = 55 AA validity marker
 *
 * This layout is verified from the uploaded X65 v309 image: the stock save/load
 * routines copy 252 bytes (126 * uint16_t) to/from those exact pages. Pressing
 * decreases ADC on this board, and stock accepts the bottom calibration only
 * when rest > bottom + 500 counts. Reading these pages lets the custom engine
 * retain the factory/per-key counts-per-mm calibration instead of guessing one
 * global ADC span. Existing non-X65 profiles keep the profile fallback. */
#define X65_CAL_REST_PAGE   0x08032000u
#define X65_CAL_BOTTOM_PAGE 0x08032800u
#define X65_CAL_SIG_OFF     0x07FEu

static uint16_t hall_span_for_key(unsigned idx)
{
#if BOARD_DEV_ID == 2268u && (defined(__arm__) || defined(__thumb__))
  const volatile uint8_t *rest_page = (const volatile uint8_t *)(uintptr_t)X65_CAL_REST_PAGE;
  const volatile uint8_t *bottom_page = (const volatile uint8_t *)(uintptr_t)X65_CAL_BOTTOM_PAGE;

  if (rest_page[X65_CAL_SIG_OFF] != 0x55u || rest_page[X65_CAL_SIG_OFF + 1u] != 0xAAu ||
      bottom_page[X65_CAL_SIG_OFF] != 0x55u || bottom_page[X65_CAL_SIG_OFF + 1u] != 0xAAu)
    return (uint16_t)HALL_ASSUMED_SPAN;

  const volatile uint16_t *rest = (const volatile uint16_t *)(uintptr_t)X65_CAL_REST_PAGE;
  const volatile uint16_t *bottom = (const volatile uint16_t *)(uintptr_t)X65_CAL_BOTTOM_PAGE;
  uint16_t hi = rest[idx];
  uint16_t lo = bottom[idx];

  /* AT32F405 ADC is 12-bit. Stock itself requires >500 counts of separation
   * before accepting a bottom calibration, so apply the same sanity floor. */
  if (hi > 4095u || lo > 4095u || hi <= lo || (uint16_t)(hi - lo) <= 500u)
    return (uint16_t)HALL_ASSUMED_SPAN;

  return (uint16_t)(hi - lo);
#else
  (void)idx;
  return (uint16_t)HALL_ASSUMED_SPAN;
#endif
}

static void hall_seed(hall_key_t *k, uint16_t raw)
{
  k->baseline = raw;
  k->extreme  = raw;
  k->travel   = 0;
  k->rt_ref   = 0;
  k->pressed  = 0;
  k->primed   = 1;
  k->rt_armed = 0;
}

void hall_init(hall_engine_t *e)
{
  const hall_keycfg_t d = {
    .mode = HALL_MODE_NORMAL,
    .press_cmm = HALL_DEF_PRESS_CMM,
    .release_cmm = HALL_DEF_RELEASE_CMM,
    .rt_press_cmm = HALL_DEF_RT_PRESS_CMM,
    .rt_release_cmm = HALL_DEF_RT_RELEASE_CMM,
  };
  for (unsigned i = 0; i < KS_NUM_KEYS; i++) {
    e->cfg[i] = d;
    e->key[i].primed = 0;
    e->key[i].span_counts = hall_span_for_key(i);
  }
  socd_ad_ready = 0;
}

void hall_set_global(hall_engine_t *e, const hall_keycfg_t *cfg)
{
  for (unsigned i = 0; i < KS_NUM_KEYS; i++)
    e->cfg[i] = *cfg;
}

/* pressed-positive deflection from the released baseline (>=0 means pressed). */
static int hall_deflection(const hall_key_t *k, uint16_t raw)
{
#if HALL_PRESS_DECREASES
  return (int)k->baseline - (int)raw;
#else
  return (int)raw - (int)k->baseline;
#endif
}

void hall_process(hall_engine_t *e, const uint16_t *raw, uint8_t *pressed)
{
  for (unsigned i = 0; i < KS_NUM_KEYS; i++) {
    hall_key_t *k = &e->key[i];
    const hall_keycfg_t *c = &e->cfg[i];
    uint16_t r = raw[i];

    if (!k->primed) {
      hall_seed(k, r);
      if (pressed) pressed[i] = 0;
      continue;
    }

    /* --- released-rest baseline + deepest-press extreme tracking --- */
#if HALL_PRESS_DECREASES
    if (r > k->baseline) k->baseline = r;
    if (r < k->extreme)  k->extreme  = r;
#else
    if (r < k->baseline) k->baseline = r;
    if (r > k->extreme)  k->extreme  = r;
#endif

    int d = hall_deflection(k, r);
    if (d < 0) d = 0;

    /* follow slow downward drift of the rest level when sitting near rest */
    if (!k->pressed && d <= HALL_NOISE_COUNTS) {
#if HALL_PRESS_DECREASES
      if (k->baseline > r) k->baseline -= 1;
#else
      if (k->baseline < r) k->baseline += 1;
#endif
    }
    k->travel = (int16_t)d;

    int act = (int)hall_key_cmm_to_counts(e, i, c->press_cmm);
    int rel = (int)hall_key_cmm_to_counts(e, i, c->release_cmm);
    if (rel >= act) rel = act - (int)hall_key_cmm_to_counts(e, i, 20);  /* force hysteresis */
    if (rel < 0) rel = 0;

    if (c->mode == HALL_MODE_RAPID_TRIGGER) {
      int rtp = (int)hall_key_cmm_to_counts(e, i, c->rt_press_cmm);
      int rtl = (int)hall_key_cmm_to_counts(e, i, c->rt_release_cmm);
      /* The first actuation of a press requires reaching the configured actuation
       * point `act`; only after that ("armed") does the rapid-trigger re-press
       * delta take over. This honours the user's actuation setting instead of
       * triggering near rest. Releasing past `rel` disarms and re-arms `act`. */
      if (!k->pressed) {
        if (d <= k->rt_ref) k->rt_ref = (int16_t)d;           /* follow the valley */
        int trip = k->rt_armed ? (d - k->rt_ref >= rtp) : (d >= act);
        if (trip) {
          k->pressed   = 1;
          k->rt_armed  = 1;
          k->rt_ref    = (int16_t)d;
        }
      } else {
        if (d >= k->rt_ref) k->rt_ref = (int16_t)d;           /* follow the peak   */
        if (k->rt_ref - d >= rtl) {                           /* rapid-trigger release */
          k->pressed = 0;
          k->rt_ref  = (int16_t)d;
        }
      }
      if (d <= rel) {                          /* released past the release point  */
        k->pressed  = 0;
        k->rt_armed = 0;
        k->rt_ref   = (int16_t)d;
      }
    } else {
      /* Normal + (DKS/ModTap/Toggle/SnapTap fall back to) fixed threshold. */
      if (!k->pressed) { if (d >= act) k->pressed = 1; }
      else             { if (d <= rel) k->pressed = 0; }
      k->rt_ref = (int16_t)d;
    }

    if (pressed) pressed[i] = k->pressed;
  }

  /* Resolve A/D only after every key's Hall/RT state and live travel have been
   * updated for this scan. This keeps physical sensing independent from the HID
   * policy while ensuring the report layer sees the final SOCD result. */
  if (pressed) {
    if (!socd_ad_ready) {
      socd_ad_init(&socd_ad, keymap_default);
      socd_ad_ready = 1;
    }
    socd_ad_apply(&socd_ad, e, pressed);
  }
}
