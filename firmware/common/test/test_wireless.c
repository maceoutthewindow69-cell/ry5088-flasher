/*
 * test_wireless.c - host-side self-test for the SPI3 wireless frame builder.
 * Builds natively (no AT32/BSP); wireless.c is compiled with -DHOST_TEST so only
 * the pure wl_build_frame() is linked. Run via `make test-wireless`.
 *
 * Covers the worked example, the DATA-only checksum (CMD/LEN excluded, &0xFF
 * wrap), the round_up(N+3,4) padding rule, and a couple of CHK edge cases.
 */
#include <stdio.h>
#include <string.h>
#include "wireless.h"

static int passes = 0, fails = 0;
#define CHECK(cond, msg) do { \
  if (cond) { printf("  ok  : %s\n", msg); passes++; } \
  else      { printf("  FAIL: %s\n", msg); fails++; } } while (0)

static int eq(const uint8_t *a, const uint8_t *b, unsigned n) { return memcmp(a, b, n) == 0; }

static void dump(const char *tag, const uint8_t *p, unsigned n)
{
  printf("    %s:", tag);
  for (unsigned i = 0; i < n; i++) printf(" %02X", p[i]);
  printf("\n");
}

int main(void)
{
  printf("[1] worked example: key 'a' (0x04) as 3rd DATA byte\n");
  {
    uint8_t data[8]  = { 0, 0, 0x04, 0, 0, 0, 0, 0 };   /* leading 0 + report[1..7] */
    uint8_t expect[12] = { 0x81, 0x08, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
                           0x00, 0x00, 0x04, 0x00 };
    uint8_t out[WL_FRAME_MAX];
    uint8_t n = wl_build_frame(0x81, data, 8, out);
    CHECK(n == 12, "padded length 12 (11 bytes + 1 pad, x4-aligned)");
    CHECK(eq(out, expect, 12), "frame == 81 08 00 00 04 00 00 00 00 00 04 00");
    CHECK(out[10] == 0x04, "CHK = 0x04 (sum of DATA)");
    CHECK(out[11] == 0x00, "trailing byte is zero pad");
    if (!eq(out, expect, 12)) { dump("got   ", out, 12); dump("expect", expect, 12); }
  }

  printf("[2] CHK excludes CMD and LEN, wraps &0xFF\n");
  {
    uint8_t data[2] = { 0xFF, 0x02 };       /* sums past 0xFF -> 0x101 & 0xFF = 0x01 */
    uint8_t out[WL_FRAME_MAX];
    uint8_t n = wl_build_frame(0x81, data, 2, out);   /* 81 02 FF 02 01 + 3 pad -> 8 */
    CHECK(out[0] == 0x81 && out[1] == 0x02, "CMD/LEN written verbatim at [0]/[1]");
    CHECK(out[4] == 0x01, "CHK = (0xFF + 0x02) & 0xFF = 0x01");
    /* had CMD+LEN been summed, CHK would be (0x81+0x02+0xFF+0x02)&0xFF = 0x84 */
    CHECK(out[4] != 0x84, "CHK does NOT include CMD or LEN");
    CHECK(n == 8, "LEN 2 -> round_up(5,4) = 8 bytes");
  }

  printf("[3] padding rounds (N+3) up to a multiple of 4\n");
  {
    uint8_t data[64];
    uint8_t out[WL_FRAME_MAX];
    memset(data, 0, sizeof data);
    CHECK(wl_build_frame(0x81, data, 8,    out) == 12, "LEN 8 (boot kbd)  -> 12 bytes");
    CHECK(wl_build_frame(0x81, data, 0x10, out) == 20, "LEN 0x10 (NKRO)   -> 20 bytes");
    CHECK(wl_build_frame(0x94, data, 0x21, out) == 36, "LEN 0x21 (ident)  -> 36 bytes");
    CHECK(wl_build_frame(0x91, data, 0x40, out) == 68, "LEN 0x40 (config) -> 68 bytes");
  }

  printf("[4] CHK edge cases\n");
  {
    uint8_t out[WL_FRAME_MAX];
    uint8_t n0 = wl_build_frame(0x93, NULL, 0, out);  /* empty DATA: 93 00 00 + 1 pad */
    CHECK(out[2] == 0x00, "empty DATA -> CHK 0");
    CHECK(n0 == 4, "LEN 0 -> round_up(3,4) = 4 bytes");
    uint8_t d2[2] = { 0xFF, 0x02 };
    wl_build_frame(0x90, d2, 2, out);
    CHECK(out[4] == 0x01, "DATA {0xFF,0x02} -> CHK 0x01");
  }

  printf("\n%d passed, %d failed\n", passes, fails);
  return fails ? 1 : 0;
}
