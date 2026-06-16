/*
 * persist.c - pack/unpack + load/save of the Fun60 Ultra flash config image.
 * Pure logic (no hardware); the EFC is reached only through the injected
 * flash_ops_t. See persist.h.
 */
#include "persist.h"
#include <string.h>

#define CFG_MAGIC    0x3143474Du     /* "MGC1" little-endian (4D 47 43 31)      */
#define CFG_VERSION  0x0001u

/* On-flash image. Packed so the byte layout is identical on the target (ARM) and
 * the host test build (both little-endian). Holds the complete app-visible
 * config inside the single 0x08028000 sector. */
typedef struct __attribute__((packed)) {
  uint32_t magic;
  uint16_t version;
  uint16_t length;                         /* sizeof(config_image_t)            */
  uint8_t  cfg[256];                       /* config header (0..0xEC used)              */
  uint8_t  rgb[256];                       /* LED per-mode table                */
  uint8_t  userpic[MG_NUM_LEDS * 3];       /* per-key Static colours            */
  uint8_t  keymatrix[MG_KEYMATRIX_BYTES];  /* active remap                      */
  uint16_t mag_press[MG_NUM_MAG_KEYS];
  uint16_t mag_lift[MG_NUM_MAG_KEYS];
  uint16_t mag_rt_press[MG_NUM_MAG_KEYS];
  uint16_t mag_rt_lift[MG_NUM_MAG_KEYS];
  uint8_t  mag_mode[MG_NUM_MAG_KEYS];
  uint8_t  mag_global_mode;
  uint32_t crc;                            /* CRC32 of everything above         */
} config_image_t;

/* one static image buffer (~1.5 KB BSS) - avoids a large stack frame */
static config_image_t g_img;

uint32_t persist_image_size(void) { return (uint32_t)sizeof(config_image_t); }

static uint32_t crc32(const void *data, uint32_t len)
{
  const uint8_t *p = (const uint8_t *)data;
  uint32_t crc = 0xFFFFFFFFu;
  uint32_t i; int b;
  for (i = 0; i < len; i++) {
    crc ^= p[i];
    for (b = 0; b < 8; b++)
      crc = (crc >> 1) ^ (0xEDB88320u & (uint32_t)(-(int32_t)(crc & 1u)));
  }
  return crc ^ 0xFFFFFFFFu;
}

static void pack(const monsgeek_state_t *st, config_image_t *im)
{
  memset(im, 0, sizeof(*im));
  im->magic   = CFG_MAGIC;
  im->version = CFG_VERSION;
  im->length  = (uint16_t)sizeof(*im);
  memcpy(im->cfg,       st->cfg,       sizeof(im->cfg));
  memcpy(im->rgb,       st->rgb,       sizeof(im->rgb));
  memcpy(im->userpic,   st->userpic,   sizeof(im->userpic));
  memcpy(im->keymatrix, st->keymatrix, sizeof(im->keymatrix));
  memcpy(im->mag_press,    st->mag_press,    sizeof(im->mag_press));
  memcpy(im->mag_lift,     st->mag_lift,     sizeof(im->mag_lift));
  memcpy(im->mag_rt_press, st->mag_rt_press, sizeof(im->mag_rt_press));
  memcpy(im->mag_rt_lift,  st->mag_rt_lift,  sizeof(im->mag_rt_lift));
  memcpy(im->mag_mode,     st->mag_mode,     sizeof(im->mag_mode));
  im->mag_global_mode = st->mag_global_mode;
  im->crc = crc32(im, (uint32_t)(sizeof(*im) - sizeof(im->crc)));
}

/* unpack image -> state (leaves runtime-only fields cleared). */
static void unpack(const config_image_t *im, monsgeek_state_t *st)
{
  monsgeek_state_init(st);             /* sane base for any fields not stored    */
  memcpy(st->cfg,       im->cfg,       sizeof(st->cfg));
  memcpy(st->rgb,       im->rgb,       sizeof(st->rgb));
  memcpy(st->userpic,   im->userpic,   sizeof(st->userpic));
  memcpy(st->keymatrix, im->keymatrix, sizeof(st->keymatrix));
  memcpy(st->mag_press,    im->mag_press,    sizeof(st->mag_press));
  memcpy(st->mag_lift,     im->mag_lift,     sizeof(st->mag_lift));
  memcpy(st->mag_rt_press, im->mag_rt_press, sizeof(st->mag_rt_press));
  memcpy(st->mag_rt_lift,  im->mag_rt_lift,  sizeof(st->mag_rt_lift));
  memcpy(st->mag_mode,     im->mag_mode,     sizeof(st->mag_mode));
  st->mag_global_mode = im->mag_global_mode;
  st->cfg_dirty = 0;
  st->enter_bootloader = 0;
}

int persist_load(const flash_ops_t *ops, monsgeek_state_t *st)
{
  if (!ops || !ops->read || ops->read(CONFIG_FLASH_ADDR, &g_img, sizeof(g_img)) != 0) {
    monsgeek_state_init(st);
    return 0;
  }
  if (g_img.magic   != CFG_MAGIC ||
      g_img.version != CFG_VERSION ||
      g_img.length  != sizeof(g_img) ||
      g_img.crc     != crc32(&g_img, (uint32_t)(sizeof(g_img) - sizeof(g_img.crc)))) {
    /* blank (0xFF), foreign, or corrupt -> clean factory defaults (first boot). */
    monsgeek_state_init(st);
    return 0;
  }
  unpack(&g_img, st);
  return 1;
}

int persist_save(const flash_ops_t *ops, monsgeek_state_t *st)
{
  if (!ops || !ops->erase || !ops->program) return -1;
  pack(st, &g_img);
  if (ops->erase(CONFIG_FLASH_ADDR) != 0) return -2;
  /* program is word-granular; sizeof(image) is a multiple of 4 by construction. */
  if (ops->program(CONFIG_FLASH_ADDR, &g_img, (uint32_t)sizeof(g_img)) != 0) return -3;
  st->cfg_dirty = 0;
  return 0;
}
