/*
 * test_engine.c - host-side self-test for the pure-logic engine (hall + hid_report).
 * Builds natively (no AT32/USB). Run via `make test`.
 */
#include <stdio.h>
#include <string.h>
#include "hall.h"
#include "hid_report.h"
#include "keyscan.h"
#include "board_keymap.h"

static int fails = 0;
#define CHECK(cond, msg) do { \
  if (!(cond)) { printf("  FAIL: %s\n", msg); fails++; } \
  else         { printf("  ok  : %s\n", msg); } } while (0)

/* feed one frame where every key reads `def` except key `idx` which reads `val`. */
static void frame(hall_engine_t *e, uint8_t *pressed, unsigned idx, uint16_t val, uint16_t def)
{
  uint16_t raw[KS_NUM_KEYS];
  for (unsigned i = 0; i < KS_NUM_KEYS; i++) raw[i] = def;
  raw[idx] = val;
  hall_process(e, raw, pressed);
}

static uint16_t raw_for_travel(uint16_t rest, uint32_t travel)
{
#if HALL_PRESS_DECREASES
  return (uint16_t)(rest - travel);
#else
  return (uint16_t)(rest + travel);
#endif
}

int main(void)
{
  hall_engine_t e;
  uint8_t pressed[KS_NUM_KEYS];
  const unsigned K = 9;            /* key index 9 = 'A' (0x04) in the default map */
  const uint16_t REST = 3000;
  const uint32_t span = HALL_CMM_TO_COUNTS(HALL_FULL_TRAVEL_CMM);
  const uint32_t act = HALL_CMM_TO_COUNTS(200);
  const uint32_t rel = HALL_CMM_TO_COUNTS(150);
  const uint32_t rtd = HALL_CMM_TO_COUNTS(50);
  const uint16_t DEEP = raw_for_travel(REST, span);

  printf("[1] Normal-mode actuation + hysteresis\n");
  hall_init(&e);
  frame(&e, pressed, K, REST, REST);                 /* prime baseline */
  CHECK(pressed[K] == 0, "released after baseline seed");
  frame(&e, pressed, K, DEEP, REST);                 /* full press */
  CHECK(pressed[K] == 1, "full press actuates");
  {
    uint32_t mid = (act + rel) / 2u;
    frame(&e, pressed, K, raw_for_travel(REST, mid), REST);
    CHECK(pressed[K] == 1, "stays pressed in hysteresis band");
  }
  frame(&e, pressed, K, raw_for_travel(REST, rel / 2u), REST);
  CHECK(pressed[K] == 0, "releases once travel drops below release point");

  printf("[2] Rapid Trigger: release/re-press on direction reversal\n");
  hall_init(&e);
  hall_keycfg_t rt = { .mode = HALL_MODE_RAPID_TRIGGER, .press_cmm = 200,
                       .release_cmm = 150, .rt_press_cmm = 50, .rt_release_cmm = 50 };
  hall_set_global(&e, &rt);
  frame(&e, pressed, K, REST, REST);                 /* prime */
  frame(&e, pressed, K, DEEP, REST);                 /* press deep */
  CHECK(pressed[K] == 1, "RT press on downstroke");
  {
    uint32_t up = rtd + 50u;
    uint32_t released_travel = span > up ? span - up : 0u;
    frame(&e, pressed, K, raw_for_travel(REST, released_travel), REST);
    CHECK(pressed[K] == 0, "RT release on upstroke past rt_release");
    uint32_t repress_travel = released_travel + rtd + 50u;
    if (repress_travel > span) repress_travel = span;
    frame(&e, pressed, K, raw_for_travel(REST, repress_travel), REST);
    CHECK(pressed[K] == 1, "RT re-press on downstroke past rt_press");
  }

  printf("[2b] Rapid Trigger honours the configured actuation point on first press\n");
  hall_init(&e);
  hall_set_global(&e, &rt);
  frame(&e, pressed, K, REST, REST);                 /* prime (travel 0) */
  frame(&e, pressed, K, raw_for_travel(REST, act / 2u), REST);
  CHECK(pressed[K] == 0, "RT does NOT actuate before the actuation point");
  {
    uint32_t over = act + 100u;
    if (over > span) over = span;
    frame(&e, pressed, K, raw_for_travel(REST, over), REST);
  }
  CHECK(pressed[K] == 1, "RT actuates once travel reaches the actuation point");
  frame(&e, pressed, K, REST, REST);                 /* full release */
  CHECK(pressed[K] == 0, "RT releases and re-arms on full release");
  frame(&e, pressed, K, raw_for_travel(REST, act / 2u), REST);
  CHECK(pressed[K] == 0, "re-armed: the next first press again needs the actuation point");

  printf("[3] Boot report: modifiers fold, keycodes fill, de-dupe\n");
  {
    uint8_t pr[KS_NUM_KEYS]; memset(pr, 0, sizeof pr);
    uint8_t rep[HID_BOOT_REPORT_LEN];
    pr[1] = 1;   /* Esc    0x29 */
    pr[4] = 1;   /* LShift 0xE1 -> modifier bit1 */
    pr[9] = 1;   /* A      0x04 */
    uint8_t n = hid_build_boot_report(pr, keymap_default, rep);
    CHECK(n == 2, "two non-modifier keys");
    CHECK(rep[0] == 0x02, "LShift folded into modifier byte (bit1)");
    CHECK(rep[1] == 0x00, "reserved byte zero");
    int has_esc = 0, has_a = 0;
    for (int i = 2; i < 8; i++) { if (rep[i] == 0x29) has_esc = 1; if (rep[i] == 0x04) has_a = 1; }
    CHECK(has_esc && has_a, "Esc + A present in keycode slots");
  }

  printf("[4] Boot report: >6 keys -> ErrorRollOver\n");
  {
    uint8_t pr[KS_NUM_KEYS]; memset(pr, 0, sizeof pr);
    uint8_t rep[HID_BOOT_REPORT_LEN];
    unsigned ks[7] = {7,13,19,25,31,37,43};
    for (int i = 0; i < 7; i++) pr[ks[i]] = 1;
    hid_build_boot_report(pr, keymap_default, rep);
    int rollover = 1;
    for (int i = 2; i < 8; i++) if (rep[i] != 0x01) rollover = 0;
    CHECK(rollover, "all six slots = 0x01 ErrorRollOver");
  }

  printf("\n%s (%d failure%s)\n", fails ? "TESTS FAILED" : "ALL TESTS PASSED",
         fails, fails == 1 ? "" : "s");
  return fails ? 1 : 0;
}
