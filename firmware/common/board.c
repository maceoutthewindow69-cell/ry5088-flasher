/*
 * board.c - clock/USB/delay glue for the Fun60 Ultra firmware.
 */
#include "board.h"
#include "system_at32f402_405.h"

/* ---- DWT cycle-counter based delay (precise at 216 MHz) ---- */
void board_delay_init(void)
{
  CoreDebug->DEMCR |= CoreDebug_DEMCR_TRCENA_Msk;
  DWT->CYCCNT = 0;
  DWT->CTRL  |= DWT_CTRL_CYCCNTENA_Msk;
}

static void delay_cycles(uint32_t cycles)
{
  uint32_t start = DWT->CYCCNT;
  while ((DWT->CYCCNT - start) < cycles) { /* spin */ }
}

void usb_delay_us(uint32_t us)
{
  delay_cycles(us * (system_core_clock / 1000000U));
}

void usb_delay_ms(uint32_t ms)
{
  while (ms--) usb_delay_us(1000U);
}

/* ---- USB OTG-FS pin configuration ----
 * DP (PA12) / DM (PA11) are driven by the OTG-FS peripheral automatically.
 * With USB_VBUS_IGNORE set (usb_conf.h) the VBUS-sense pin is not used, so only
 * the OTG GPIO port clock is required. */
void usb_gpio_config(void)
{
  crm_periph_clock_enable(OTG_PIN_GPIO_CLOCK, TRUE);
#ifdef USB_SOF_OUTPUT_ENABLE
  {
    gpio_init_type gpio_init_struct;
    crm_periph_clock_enable(OTG_PIN_SOF_GPIO_CLOCK, TRUE);
    gpio_default_para_init(&gpio_init_struct);
    gpio_init_struct.gpio_drive_strength = GPIO_DRIVE_STRENGTH_STRONGER;
    gpio_init_struct.gpio_out_type  = GPIO_OUTPUT_PUSH_PULL;
    gpio_init_struct.gpio_mode = GPIO_MODE_MUX;
    gpio_init_struct.gpio_pull = GPIO_PULL_NONE;
    gpio_init_struct.gpio_pins = OTG_PIN_SOF;
    gpio_init(OTG_PIN_SOF_GPIO, &gpio_init_struct);
    gpio_pin_mux_config(OTG_PIN_SOF_GPIO, OTG_PIN_SOF_SOURCE, OTG_PIN_MUX);
  }
#endif
}

/* ---- select the 48 MHz USB clock ----
 * HEXT path: derive the USB 48 MHz from PLLU (set up by system_clock_config()).
 * (The HICK/ACC path is omitted on purpose; the keyboard runs from a crystal.) */
void usb_clock48m_select(usb_clk48_s clk_s)
{
  (void)clk_s;
  crm_pllu_output_set(TRUE);
  while (crm_flag_get(CRM_PLLU_STABLE_FLAG) != SET) { /* wait PLLU */ }
  crm_usb_clock_source_select(CRM_USB_CLOCK_SOURCE_PLLU);
}
