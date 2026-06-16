/*
 * monsgeek_desc.c - descriptor handler for the Fun60 Ultra firmware.
 *
 * The standard device/config/qualifier/report descriptors live as pure byte
 * tables in descriptors.c (host-verified). This file adapts them to the Artery
 * usbd_desc_handler interface and builds the UTF-16 string descriptors.
 */
#include "usb_std.h"
#include "usbd_core.h"
#include "monsgeek_desc.h"

static usbd_desc_t *get_device_descriptor(void);
static usbd_desc_t *get_device_qualifier(void);
static usbd_desc_t *get_device_configuration(void);
static usbd_desc_t *get_device_other_speed(void);
static usbd_desc_t *get_device_lang_id(void);
static usbd_desc_t *get_device_manufacturer_string(void);
static usbd_desc_t *get_device_product_string(void);
static usbd_desc_t *get_device_serial_string(void);
static usbd_desc_t *get_device_interface_string(void);
static usbd_desc_t *get_device_config_string(void);
static usbd_desc_t *get_hs_device_configuration(void);

usbd_desc_handler monsgeek_desc_handler =
{
  get_device_descriptor,
  get_device_qualifier,
  get_device_configuration,
  get_device_other_speed,
  get_device_lang_id,
  get_device_manufacturer_string,
  get_device_product_string,
  get_device_serial_string,
  get_device_interface_string,
  get_device_config_string,
  get_hs_device_configuration,
};

#if defined ( __ICCARM__ )
  #pragma data_alignment=4
#endif
ALIGNED_HEAD static uint8_t g_str_buffer[256] ALIGNED_TAIL;

ALIGNED_HEAD static uint8_t g_string_serial[26] ALIGNED_TAIL =
{
  26, USB_DESCIPTOR_TYPE_STRING,
};

static usbd_desc_t device_descriptor   = { MG_DEVICE_DESC_SIZE,    (uint8_t *)monsgeek_device_desc    };
static usbd_desc_t config_descriptor   = { MG_CONFIG_DESC_SIZE,    (uint8_t *)monsgeek_config_desc    };
static usbd_desc_t qualifier_descriptor= { MG_QUALIFIER_DESC_SIZE, (uint8_t *)monsgeek_qualifier_desc };
static usbd_desc_t langid_descriptor   = { 4,                      (uint8_t *)monsgeek_langid_desc    };
static usbd_desc_t serial_descriptor   = { 26,                     g_string_serial                    };
static usbd_desc_t vp_desc;

/* convert an ASCII C-string into a USB UTF-16LE string descriptor */
static uint16_t unicode_convert(const char *s, uint8_t *buf)
{
  uint16_t len = 0, pos = 2;
  while (*s != '\0') { len++; buf[pos++] = (uint8_t)*s++; buf[pos++] = 0x00; }
  len = (uint16_t)(len * 2 + 2);
  buf[0] = (uint8_t)len;
  buf[1] = USB_DESCIPTOR_TYPE_STRING;
  return len;
}

static void int_to_unicode(uint32_t value, uint8_t *pbuf, uint8_t len)
{
  uint8_t i;
  for (i = 0; i < len; i++)
  {
    uint8_t nib = (uint8_t)((value >> 28) & 0xF);
    pbuf[2 * i] = (uint8_t)(nib < 0xA ? (nib + '0') : (nib + 'A' - 10));
    pbuf[2 * i + 1] = 0;
    value <<= 4;
  }
}

static void build_serial(void)
{
  uint32_t s0 = *(uint32_t *)MONSGEEK_MCU_ID1;
  uint32_t s1 = *(uint32_t *)MONSGEEK_MCU_ID2;
  uint32_t s2 = *(uint32_t *)MONSGEEK_MCU_ID3;
  s0 += s2;
  if (s0 != 0)
  {
    int_to_unicode(s0, &g_string_serial[2], 8);
    int_to_unicode(s1, &g_string_serial[18], 4);
  }
}

static usbd_desc_t *get_device_descriptor(void)    { return &device_descriptor;    }
static usbd_desc_t *get_device_qualifier(void)     { return &qualifier_descriptor; }
static usbd_desc_t *get_device_configuration(void) { return &config_descriptor;    }
static usbd_desc_t *get_hs_device_configuration(void){ return &config_descriptor;  }
static usbd_desc_t *get_device_other_speed(void)   { return &config_descriptor;    }
static usbd_desc_t *get_device_lang_id(void)       { return &langid_descriptor;    }

static usbd_desc_t *get_device_manufacturer_string(void)
{
  vp_desc.length = unicode_convert(MONSGEEK_STR_MANUFACTURER, g_str_buffer);
  vp_desc.descriptor = g_str_buffer;
  return &vp_desc;
}
static usbd_desc_t *get_device_product_string(void)
{
  vp_desc.length = unicode_convert(MONSGEEK_STR_PRODUCT, g_str_buffer);
  vp_desc.descriptor = g_str_buffer;
  return &vp_desc;
}
static usbd_desc_t *get_device_serial_string(void)
{
  build_serial();
  return &serial_descriptor;
}
static usbd_desc_t *get_device_interface_string(void)
{
  vp_desc.length = unicode_convert(MONSGEEK_STR_INTERFACE, g_str_buffer);
  vp_desc.descriptor = g_str_buffer;
  return &vp_desc;
}
static usbd_desc_t *get_device_config_string(void)
{
  vp_desc.length = unicode_convert(MONSGEEK_STR_CONFIG, g_str_buffer);
  vp_desc.descriptor = g_str_buffer;
  return &vp_desc;
}
