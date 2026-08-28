/*
 * hall.h - per-key Hall-effect processing: baseline calibration, travel,
 *          actuation and Rapid Trigger. Pure logic (no hardware), so it can be
 *          unit-tested on the host.
 *
 * Sign convention: pressing moves the magnet toward the sensor and the ADC
 * reading DECREASES, so travel = baseline - raw. Flip HALL_PRESS_DECREASES to 0
 * if a board reads the other way.
 */
#ifndef HALL_H
#define HALL_H

#include <stdint.h>
#include "keyscan.h"
#include "board_config.h"

/* Global fallback scale. X65 builds replace this at runtime with each key's
 * preserved stock calibration span when the calibration pages are valid. */
#define HALL_CMM_TO_COUNTS(cmm) (((uint32_t)(cmm) * HALL_ASSUMED_SPAN) / HALL_FULL_TRAVEL_CMM)

/* Per-key actuation modes (matches the magnetism KEY_MODE table). */
enum {
  HALL_MODE_NORMAL = 0,
  HALL_MODE_RAPID_TRIGGER = 1,
  HALL_MODE_DKS = 2,
  HALL_MODE_MODTAP = 3,
  HALL_MODE_TOGGLE = 4,
  HALL_MODE_SNAPTAP = 5
};

/* Per-key tunables (centi-mm). */
typedef struct {
  uint8_t  mode;            /* HALL_MODE_*                                       */
  uint16_t press_cmm;       /* actuation depth (Normal)        default 200 (2.0) */
  uint16_t release_cmm;     /* release depth   (Normal)        default 150 (1.5) */
  uint16_t rt_press_cmm;    /* rapid-trigger press delta       default  50 (0.5) */
  uint16_t rt_release_cmm;  /* rapid-trigger release delta     default  50 (0.5) */
} hall_keycfg_t;

/* Live per-key runtime state. */
typedef struct {
  uint16_t baseline;        /* released rest reference (running extreme)         */
  uint16_t extreme;         /* deepest-press reference (running extreme)         */
  uint16_t span_counts;     /* raw ADC counts over full travel; 0 => fallback    */
  int16_t  travel;          /* current travel in ADC counts (>=0 when pressed)   */
  int16_t  rt_ref;          /* rapid-trigger pivot (peak/valley travel)          */
  uint8_t  pressed;         /* actuation output                                  */
  uint8_t  primed;          /* baseline has been seeded                          */
  uint8_t  rt_armed;        /* RT: actuation point reached this press-session    */
} hall_key_t;

typedef struct {
  hall_key_t    key[KS_NUM_KEYS];
  hall_keycfg_t cfg[KS_NUM_KEYS];
} hall_engine_t;

/* Convert a physical distance to raw ADC counts for one key. A valid preserved
 * calibration span wins; otherwise the profile's HALL_ASSUMED_SPAN is used. */
static inline uint32_t hall_key_cmm_to_counts(const hall_engine_t *e, unsigned idx, uint16_t cmm)
{
  uint32_t span = e->key[idx].span_counts ? e->key[idx].span_counts : HALL_ASSUMED_SPAN;
  return ((uint32_t)cmm * span) / HALL_FULL_TRAVEL_CMM;
}

/* Initialise all keys to defaults (actuation 2.0 mm, Rapid Trigger off). */
void hall_init(hall_engine_t *e);

/* Apply one default config to every key. */
void hall_set_global(hall_engine_t *e, const hall_keycfg_t *cfg);

/* Process one scanned frame: raw[KS_NUM_KEYS] -> updates pressed[] (0/1).
 * pressed may be NULL if the caller reads e->key[i].pressed directly. */
void hall_process(hall_engine_t *e, const uint16_t *raw, uint8_t *pressed);

#endif /* HALL_H */
