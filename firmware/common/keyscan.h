/*
 * keyscan.h - Fun60 Ultra analog (Hall-effect) key-scan front-end.
 *
 * Hardware:
 *   ADC1 @0x40012000, channel 2 = PA2 (the analog-mux common output);
 *   row mux  = GPIOC PC6/PC7/PC8 (3-bit binary address, PC6=LSB);
 *   col count= GPIOA PA4/PA5/PA6 (CD4017-style counter, PA5=clock);
 *   matrix   = 14 columns x 6 rows = 84 sites, key_index = col*6 + row.
 *
 * keyscan_init() brings up the ADC + GPIOs; keyscan_frame() samples all 84
 * sites into a caller-supplied raw[] buffer (one full mux/counter sweep).
 */
#ifndef KEYSCAN_H
#define KEYSCAN_H

#include <stdint.h>
#include "board_config.h"

/* KS_COLS and KS_ROWS are per-board and come from the generated board_config.h. */
#define KS_NUM_KEYS  (KS_COLS * KS_ROWS) /* total sense sites                  */

/* key_index <-> (column,row): keymap order is col*KS_ROWS + row. */
static inline uint16_t ks_index(uint8_t col, uint8_t row) { return (uint16_t)(col * KS_ROWS + row); }

/* Configure ADC1/PA2, the PA4/5/6 column counter and PC6/7/8 row mux. */
void keyscan_init(void);

/* Sample every site once. raw must hold KS_NUM_KEYS uint16 (12-bit ADC counts),
 * indexed by ks_index(col,row). Drives the per-key GPIO sequence. */
void keyscan_frame(uint16_t *raw);

/* Low-level primitive (exposed for bring-up): select one site and read the ADC. */
uint16_t keyscan_sample_site(uint8_t col, uint8_t row);

#endif /* KEYSCAN_H */
