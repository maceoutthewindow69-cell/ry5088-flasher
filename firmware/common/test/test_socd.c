/* Host-side tests for depth-aware A/D SOCD. */
#include <stdio.h>
#include <string.h>
#include "socd.h"
#include "board_keymap.h"

static int fails = 0;
#define CHECK(c, m) do { if (!(c)) { printf("FAIL: %s\n", m); fails++; } else printf(" ok : %s\n", m); } while (0)

static int find_usage(uint8_t usage)
{
  for (unsigned i = 0; i < KS_NUM_KEYS; i++) if (keymap_default[i] == usage) return (int)i;
  return -1;
}

static void set_state(hall_engine_t *h, uint8_t *pressed, int ai, int di,
                      int a_on, int d_on, uint16_t a_cmm, uint16_t d_cmm)
{
  memset(pressed, 0, KS_NUM_KEYS);
  pressed[ai] = a_on ? 1 : 0;
  pressed[di] = d_on ? 1 : 0;
  h->key[ai].travel = (int16_t)HALL_CMM_TO_COUNTS(a_cmm);
  h->key[di].travel = (int16_t)HALL_CMM_TO_COUNTS(d_cmm);
}

int main(void)
{
  hall_engine_t h;
  uint8_t pressed[KS_NUM_KEYS];
  socd_ad_state_t s;
  int ai = find_usage(SOCD_HID_A), di = find_usage(SOCD_HID_D);

  CHECK(ai >= 0 && di >= 0, "default keymap contains A and D");
  if (ai < 0 || di < 0) return 1;
  memset(&h, 0, sizeof h);
  socd_ad_init(&s, keymap_default);

  set_state(&h, pressed, ai, di, 1, 0, 200, 0);
  socd_ad_apply(&s, &h, pressed);
  CHECK(pressed[ai] && !pressed[di], "A only -> A");

  set_state(&h, pressed, ai, di, 1, 1, 200, 150);
  socd_ad_apply(&s, &h, pressed);
  CHECK(!pressed[ai] && pressed[di], "A held + D partial -> D (LKP)");

  set_state(&h, pressed, ai, di, 1, 1, 350, 350);
  socd_ad_apply(&s, &h, pressed);
  CHECK(!pressed[ai] && !pressed[di], "A+D bottom -> neutral");

  set_state(&h, pressed, ai, di, 1, 0, 250, 0);
  socd_ad_apply(&s, &h, pressed);
  CHECK(pressed[ai] && !pressed[di], "release D while A held -> A immediately");

  socd_ad_init(&s, keymap_default);
  set_state(&h, pressed, ai, di, 0, 1, 0, 200);
  socd_ad_apply(&s, &h, pressed);
  CHECK(!pressed[ai] && pressed[di], "D only -> D");

  set_state(&h, pressed, ai, di, 1, 1, 150, 200);
  socd_ad_apply(&s, &h, pressed);
  CHECK(pressed[ai] && !pressed[di], "D held + A partial -> A (LKP)");

  set_state(&h, pressed, ai, di, 1, 1, 350, 350);
  socd_ad_apply(&s, &h, pressed);
  CHECK(!pressed[ai] && !pressed[di], "D+A bottom -> neutral");

  set_state(&h, pressed, ai, di, 0, 1, 0, 250);
  socd_ad_apply(&s, &h, pressed);
  CHECK(!pressed[ai] && pressed[di], "release A while D held -> D immediately");

  printf("%s (%d failure%s)\n", fails ? "TESTS FAILED" : "ALL SOCD TESTS PASSED", fails, fails == 1 ? "" : "s");
  return fails ? 1 : 0;
}
