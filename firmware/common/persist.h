/*
 * persist.h - internal-flash config persistence for the Fun60 Ultra (EFC).
 *
 * Saves the live vendor settings (monsgeek_state_t) to the internal-flash config
 * region and restores them at boot, so everything the MonsGeek app programs
 * survives a power cycle. The config sector at 0x08028000 is erased and
 * reprogrammed as a whole 2 KB page; this firmware stores a single self-
 * contained, versioned, CRC-checked image that holds the complete app-visible
 * config (core header + LED table + per-key colours + keymatrix + per-key
 * magnetism) inside that one 2 KB sector.
 *
 * The (de)serialisation is pure and host-testable; the actual EFC erase/program
 * is injected via flash_ops_t so test/test_functional.c can drive a RAM mock and
 * the firmware uses flash_efc.c (the Artery BSP flash_* EFC unlock/erase/
 * program/lock sequence).
 */
#ifndef PERSIST_H
#define PERSIST_H

#include <stdint.h>
#include "vendor_proto.h"

/* Config flash sector (2 KB) - lives above the app image (.ld FLASH ends at
 * 0x08025800) and below the keymap pages (0x08028800), so erasing this one
 * sector is self-contained. */
#define CONFIG_FLASH_ADDR    0x08028000u
#define CONFIG_SECTOR_SIZE   0x800u           /* AT32F405 sector = 2 KB          */

/* Injected flash backend. All three return 0 on success, non-zero on error.
 *  read   : copy len bytes from flash 'addr' into dst (flash is XIP-readable).
 *  erase  : erase the 2 KB sector containing 'addr'.
 *  program: program len bytes (len multiple of 4, addr word-aligned) from src.
 */
typedef struct {
  int (*read)(uint32_t addr, void *dst, uint32_t len);
  int (*erase)(uint32_t addr);
  int (*program)(uint32_t addr, const void *src, uint32_t len);
} flash_ops_t;

/* The real AT32 EFC backend (flash_efc.c). NULL-safe to reference on the host. */
extern const flash_ops_t flash_efc_ops;

/*
 * Load config from flash into st.
 *   - if the stored image is valid (magic + version + CRC): unpack it, return 1.
 *   - if blank (0xFF), wrong magic/version or bad CRC: fill st with clean factory
 *     defaults (monsgeek_state_init) and return 0 ("was blank / first boot").
 */
int persist_load(const flash_ops_t *ops, monsgeek_state_t *st);

/*
 * Save st to flash (erase the config sector, then program the packed image).
 * Clears st->cfg_dirty on success. Returns 0 on success, non-zero on error.
 */
int persist_save(const flash_ops_t *ops, monsgeek_state_t *st);

/* Size of the packed on-flash image (for tests / sanity; always < 2 KB). */
uint32_t persist_image_size(void);

#endif /* PERSIST_H */
