/*
 * monsgeek_desc.h - usbd_desc_handler for the Fun60 Ultra firmware.
 * Wires the pure descriptor tables in descriptors.c into the Artery USB device
 * core, plus the UTF-16 string descriptors.
 */
#ifndef MONSGEEK_DESC_H
#define MONSGEEK_DESC_H

#include "usb_std.h"
#include "usbd_core.h"
#include "descriptors.h"
#include "board_config.h"

/* Identity strings (MONSGEEK_STR_MANUFACTURER/PRODUCT/CONFIG/INTERFACE) are
 * per-board and come from the generated board_config.h. */

/* MCU 96-bit UID (used to build a unique serial-number string) */
#define MONSGEEK_MCU_ID1            (0x1FFFF7E8)
#define MONSGEEK_MCU_ID2            (0x1FFFF7EC)
#define MONSGEEK_MCU_ID3            (0x1FFFF7F0)

extern usbd_desc_handler monsgeek_desc_handler;

#endif /* MONSGEEK_DESC_H */
