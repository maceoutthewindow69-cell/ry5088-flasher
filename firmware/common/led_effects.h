/*
 * led_effects.h - pure RGB effect engine (hardware-independent, host-testable).
 *
 * Produces a per-LED RGB frame from the decoded LED parameters plus a free-
 * running millisecond time base. The SPI/WS2812 driver (rgb.c) then encodes the
 * frame into the 1464-byte bit-stream and DMAs it to SPI2. Keeping the engine
 * pure lets test/test_functional.c verify every mode on the host.
 *
 * The exact per-mode maths is cosmetic, so this is an equivalent implementation of
 * the standard effects rather than a bit-for-bit copy of any reference.
 */
#ifndef LED_EFFECTS_H
#define LED_EFFECTS_H

#include <stdint.h>
#include "led_layout.h"

/* Output frame: one RGB triple per LED, chain order. rgb.c converts to the
 * WS2812 GRB byte order on the wire. */
typedef uint8_t led_frame_t[MG_NUM_LEDS][3];

/*
 * Render one frame.
 *   p        : decoded LED parameters (mode/speed/brightness/dir/flag/RGB).
 *   userpic  : per-key colours for Static-per-key / UserPicture modes, or NULL.
 *              Layout: userpic[led][0..2] = R,G,B in chain order.
 *   t_ms     : free-running time in milliseconds (drives animation phase).
 *   out      : destination RGB frame.
 * Deterministic in (p, userpic, t_ms): the same inputs always yield the same
 * frame, which is what the host tests rely on.
 */
void led_effects_render(const led_params_t *p,
                        const uint8_t (*userpic)[3],
                        uint32_t t_ms,
                        led_frame_t out);

/* HSV->RGB helper (h 0..255 hue, s 0..255, v 0..255). Exposed for tests. */
void led_hsv2rgb(uint8_t h, uint8_t s, uint8_t v, uint8_t *r, uint8_t *g, uint8_t *b);

/* Scale an 8-bit channel by a 0..255 brightness (rounded). Exposed for tests. */
uint8_t led_scale8(uint8_t c, uint8_t bright);

#endif /* LED_EFFECTS_H */
