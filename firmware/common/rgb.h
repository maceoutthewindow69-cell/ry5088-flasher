/*
 * rgb.h - WS2812 RGB driver for the Fun60 Ultra (SPI2 + DMA1, AT32F405).
 *
 * Hardware back end for the pure effect engine (led_effects.c): takes a per-LED
 * RGB frame, encodes it into the 1464-byte WS2812 bit-stream (one SPI byte per
 * data bit, GRB order) and DMAs it out on SPI2 MOSI (PA10).
 *
 * AT32/BSP-dependent - not part of the host test build.
 */
#ifndef RGB_H
#define RGB_H

#include <stdint.h>
#include "led_effects.h"

/* Bring up SPI2 (half-duplex master, 6.75 MHz) + DMA1 channel 1 + PA10 AF5, and
 * clear the chain to all-off. Call once after the system clock is configured. */
void rgb_init(void);

/* Encode `frame` (RGB per LED, chain order) into the WS2812 buffer and start the
 * DMA transfer. Non-blocking apart from waiting out any still-running transfer. */
void rgb_show(const led_frame_t frame);

/* 1 while a DMA frame transfer is still in flight, else 0. */
int rgb_busy(void);

#endif /* RGB_H */
