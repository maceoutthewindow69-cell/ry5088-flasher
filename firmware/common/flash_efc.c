/*
 * flash_efc.c - AT32F405 EFC backend for config persistence. See persist.h.
 *
 * Implements the flash_ops_t the persistence layer needs by calling the Artery
 * BSP flash driver, which performs the standard EFC sequence:
 *   unlock  : EFC_UNLOCK(0x40023C04) = KEY1 0x45670123, then KEY2 0xCDEF89AB
 *             (flash_unlock)
 *   erase   : CTRL.secers=1; ADDR=sector; CTRL.erstr=1; wait BSY; CTRL.secers=0
 *             (flash_sector_erase, 2 KB sector)
 *   program : CTRL.fprgm=1; *addr=word; wait BSY; CTRL.fprgm=0
 *             (flash_word_program)
 *   lock    : CTRL.oplk=1  (flash_lock)
 * Flash is execute-in-place readable, so reads are a plain memcpy from the
 * address.
 *
 * AT32/BSP-dependent - not part of the host test build (the host uses a RAM mock).
 */
#include "at32f402_405.h"
#include "persist.h"
#include <string.h>

static int efc_read(uint32_t addr, void *dst, uint32_t len)
{
  memcpy(dst, (const void *)(uintptr_t)addr, len);
  return 0;
}

static int efc_erase(uint32_t addr)
{
  flash_status_type st;
  flash_unlock();
  st = flash_sector_erase(addr);
  flash_lock();
  return (st == FLASH_OPERATE_DONE) ? 0 : -1;
}

static int efc_program(uint32_t addr, const void *src, uint32_t len)
{
  const uint8_t *p = (const uint8_t *)src;
  uint32_t i;
  int rc = 0;

  flash_unlock();
  for (i = 0; i < len; i += 4) {
    uint32_t word;
    memcpy(&word, p + i, 4);                 /* src may be unaligned */
    if (flash_word_program(addr + i, word) != FLASH_OPERATE_DONE) { rc = -1; break; }
    if (*(volatile uint32_t *)(uintptr_t)(addr + i) != word)      { rc = -2; break; }
  }
  flash_lock();
  return rc;
}

const flash_ops_t flash_efc_ops = { efc_read, efc_erase, efc_program };
