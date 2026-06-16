/*
 * board.h - minimal board support for the Fun60 Ultra firmware
 * (clock-48M select for USB, OTG GPIO, and a DWT-based microsecond delay).
 * Replaces the AT-START board package (no LEDs/buttons on the real keyboard).
 */
#ifndef MONSGEEK_BOARD_H
#define MONSGEEK_BOARD_H

#include "at32f402_405.h"
#include "usb_conf.h"

void board_delay_init(void);
void usb_gpio_config(void);
void usb_clock48m_select(usb_clk48_s clk_s);

#endif /* MONSGEEK_BOARD_H */
