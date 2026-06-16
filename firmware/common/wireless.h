/*
 * wireless.h - SPI3 wireless (2.4 GHz / BLE) report bus for the Fun60 Ultra.
 *
 * Host (AT32F405) is SPI master to the PAN1080 radio module. The on-wire frame
 * is  [CMD][LEN][DATA x LEN][CHK]  with  CHK = (sum of DATA) & 0xFF  (DATA only -
 * CMD and LEN are NOT summed), zero-padded so the byte count rounds up to a
 * multiple of 4. There is no 0x55 lead byte.
 *
 * SPI3 = master, full-duplex, 8-bit, MSB-first, /16, SPI Mode 1 (CPOL0/CPHA1),
 * software CS on PA15; SCK/MISO/MOSI = PB3/PB4/PB5 (AF6); INT = PD2 (input).
 * The frame builder (wl_build_frame) is pure logic and host-testable; the rest is
 * AT32/BSP-dependent and excluded from the host build (-DHOST_TEST).
 */
#ifndef WIRELESS_H
#define WIRELESS_H

#include <stdint.h>

/* ---- opcodes actually emitted host->module ---- */
#define WL_CMD_KEYBOARD    0x81u   /* 8-byte boot keyboard report                   */
#define WL_CMD_CONN_STATE  0x93u   /* connection-state report, link 0..6            */
#define WL_CMD_IDENTITY    0x94u   /* device identity "FUN60 Ultra-1"               */

/* Largest padded frame this driver builds: the 0x40-byte vendor/config block ->
 * round_up(0x40 + 3, 4) = 68 bytes (padding rule; 0x91/0x92 carry LEN 0x40). */
#define WL_FRAME_MAX       68u

/* Build a wire frame into `out` (>= WL_FRAME_MAX bytes). Lays out
 * [0]=cmd [1]=len [2..len+1]=data [len+2]=CHK, then zero-pads to a multiple of 4.
 * CHK = (sum of data[0..len-1]) & 0xFF. Returns the padded byte count
 * (= round_up(len + 3, 4)), which is the DMA transfer length. Pure / testable. */
uint8_t wl_build_frame(uint8_t cmd, const uint8_t *data, uint8_t len, uint8_t *out);

/* Bring up SPI3 (master, Mode 1, /16) + DMA1 channel 2 + PB3/4/5 AF6 + PA15 CS +
 * PD2 INT, with CS idle-high. Call once after the system clock is configured. */
void wireless_init(void);

/* Send the 8-byte boot keyboard report (modifier, reserved, 6 keycodes) as a
 * 0x81 / LEN 8 frame. The 8 DATA bytes are a leading 0 followed by report[1..7].
 * Blocks out any prior transfer. */
void wl_send_keyboard(const uint8_t report[8]);

/* Send the 0x94 device-identity / handshake frame ("FUN60 Ultra-1" + link-state
 * digit), LEN 0x21. */
void wl_send_identity(void);

/* Send the 0x93 connection-state report (current link state 0..6), LEN 1.
 * Also caches `state` for the 0x94 identity digit. */
void wl_send_conn_state(uint8_t state);

/* Poll the PD2 INT line (module -> host). Returns 1 when asserted (PD2 LOW =
 * "module ready / has data"), else 0. */
int wl_poll_int(void);

#endif /* WIRELESS_H */
