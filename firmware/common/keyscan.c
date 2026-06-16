/*
 * keyscan.c - Fun60 Ultra analog key-scan front-end (AT32F405).
 *
 * Hardware:
 *   ADC1 ch2 / PA2  (mux common output, single SW-triggered conversion)
 *   PA4/PA5/PA6     column counter   (PA5 clock)
 *   PC6/PC7/PC8     row mux address  (3-bit binary, PC6 = LSB)
 *   PF7             sensor/mux enable (driven high) [bring-up]
 *
 * The column-counter pulse sequence drives the fitted 74-series counter; the
 * exact part (CD4017 vs a cascade) is a bring-up detail.
 */
#include "at32f402_405.h"
#include "at32f402_405_crm.h"
#include "at32f402_405_gpio.h"
#include "at32f402_405_adc.h"
#include "keyscan.h"

/* column counter (GPIOA) */
#define PA_RESET   0x0010u   /* PA4 - counter master reset / clock gate          */
#define PA_CLOCK   0x0020u   /* PA5 - counter clock                              */
#define PA_STROBE  0x0040u   /* PA6 - counter output strobe / enable            */
/* row mux address (GPIOC) */
#define PC_A0      0x0040u   /* PC6 - mux address bit 0 (LSB)                    */
#define PC_A1      0x0080u   /* PC7 - mux address bit 1                          */
#define PC_A2      0x0100u   /* PC8 - mux address bit 2 (MSB)                    */
/* sensor / mux enable (GPIOF) */
#define PF_EN      0x0080u   /* PF7                                              */

#define ADC_COMMON_CCTRL  (*(volatile uint32_t *)0x40012304u)  /* ADC common ctrl */

/* ~settle delays (busy loops; tune on hardware - timing is not build-critical). */
static void ks_delay(volatile uint32_t n) { while (n--) __asm volatile("nop"); }
#define KS_SETTLE_COL   200u   /* after a column counter move                     */
#define KS_SETTLE_MUX    40u   /* after a mux address change                      */

/* Select the 3-bit mux row address on PC6/PC7/PC8 (binary, PC6=LSB). */
static void ks_set_mux(uint8_t row)
{
  if (row & 1u) GPIOC->scr = PC_A0; else GPIOC->clr = PC_A0;
  if (row & 2u) GPIOC->scr = PC_A1; else GPIOC->clr = PC_A1;
  if (row & 4u) GPIOC->scr = PC_A2; else GPIOC->clr = PC_A2;
}

/* Reset the column counter and clock it to position `col`. */
static void ks_select_column(uint8_t col)
{
  GPIOA->scr = PA_RESET;                 /* PA4 = 1 (assert reset)                */
  GPIOA->scr = PA_STROBE;                /* PA6 = 1                               */
  GPIOC->clr = PC_A0 | PC_A1 | PC_A2;    /* mux address -> 0                      */
  for (int i = 0; i <= (int)col; i++) {
    GPIOA->scr = PA_CLOCK;               /* clock rising                          */
    GPIOA->clr = PA_RESET;               /* deassert reset on the first edge      */
    GPIOA->clr = PA_CLOCK;               /* clock falling                         */
  }
  GPIOA->clr = PA_STROBE;                /* PA6 = 0                               */
  ks_delay(KS_SETTLE_COL);
}

/* Advance the counter the remaining (KS_COLS-col) steps to complete one full
 * sweep, leaving it in a known state. */
static void ks_complete_column(uint8_t col)
{
  uint8_t rem = (uint8_t)(KS_COLS - col);
  GPIOA->scr = PA_STROBE;
  GPIOC->clr = PC_A0 | PC_A1 | PC_A2;
  for (uint8_t i = 0; i < rem; i++) {
    GPIOA->scr = PA_CLOCK;
    GPIOA->clr = PA_RESET;
    GPIOA->clr = PA_CLOCK;
  }
}

/* One software-triggered ordinary conversion of ADC ch2 (PA2). */
static uint16_t ks_sample_adc(void)
{
  adc_ordinary_software_trigger_enable(ADC1, TRUE);     /* CTRL2.OCSWTRG          */
  while (adc_flag_get(ADC1, ADC_CCE_FLAG) == RESET) { }  /* STS bit1 (OCCE)        */
  return adc_ordinary_conversion_data_get(ADC1);         /* ODT (also clears CCE)  */
}

void keyscan_init(void)
{
  gpio_init_type gi;

  crm_periph_clock_enable(CRM_GPIOA_PERIPH_CLOCK, TRUE);
  crm_periph_clock_enable(CRM_GPIOC_PERIPH_CLOCK, TRUE);
  crm_periph_clock_enable(CRM_GPIOF_PERIPH_CLOCK, TRUE);
  crm_periph_clock_enable(CRM_ADC1_PERIPH_CLOCK, TRUE);

  /* PA2 = analog input (ADC1_IN2) */
  gpio_default_para_init(&gi);
  gi.gpio_pins = GPIO_PINS_2;
  gi.gpio_mode = GPIO_MODE_ANALOG;
  gi.gpio_pull = GPIO_PULL_NONE;
  gpio_init(GPIOA, &gi);

  /* PA4/PA5/PA6 = push-pull outputs (column counter) */
  gpio_default_para_init(&gi);
  gi.gpio_pins = GPIO_PINS_4 | GPIO_PINS_5 | GPIO_PINS_6;
  gi.gpio_mode = GPIO_MODE_OUTPUT;
  gi.gpio_out_type = GPIO_OUTPUT_PUSH_PULL;
  gi.gpio_drive_strength = GPIO_DRIVE_STRENGTH_STRONGER;
  gpio_init(GPIOA, &gi);

  /* PC6/PC7/PC8 = push-pull outputs (mux address) */
  gi.gpio_pins = GPIO_PINS_6 | GPIO_PINS_7 | GPIO_PINS_8;
  gpio_init(GPIOC, &gi);

  /* PF7 = push-pull output, driven high (sensor/mux enable [bring-up]) */
  gi.gpio_pins = GPIO_PINS_7;
  gpio_init(GPIOF, &gi);
  GPIOF->scr = PF_EN;

  /* ADC common clock divider = 6. */
  ADC_COMMON_CCTRL = (ADC_COMMON_CCTRL & 0xFFF0FFFFu) | (6u << 16);

  /* ADC base: independent, single (no scan / no repeat), right-aligned, len 1. */
  adc_base_config_type ab;
  adc_base_default_para_init(&ab);
  ab.sequence_mode = FALSE;
  ab.repeat_mode   = FALSE;
  ab.data_align    = ADC_RIGHT_ALIGNMENT;
  ab.ordinary_channel_length = 1;
  adc_base_config(ADC1, &ab);

  /* single ordinary channel: ch2 (PA2) at sequence position 1 */
  adc_ordinary_channel_set(ADC1, ADC_CHANNEL_2, 1, ADC_SAMPLETIME_28_5);

  adc_enable(ADC1, TRUE);
  ks_delay(1000);

  /* on-chip calibration */
  adc_calibration_init(ADC1);
  while (adc_calibration_init_status_get(ADC1) == SET) { }
  adc_calibration_start(ADC1);
  while (adc_calibration_init_status_get(ADC1) == SET) { }

  /* warm-up conversions */
  for (int i = 0; i < 256; i++) (void)ks_sample_adc();

  /* leave the counter/mux in a defined state */
  ks_select_column(0);
  ks_set_mux(0);
}

uint16_t keyscan_sample_site(uint8_t col, uint8_t row)
{
  uint16_t v;
  ks_select_column(col);
  ks_set_mux(row);
  ks_delay(KS_SETTLE_MUX);
  v = ks_sample_adc();
  ks_complete_column(col);
  return v;
}

void keyscan_frame(uint16_t *raw)
{
  for (uint8_t col = 0; col < KS_COLS; col++) {
    ks_select_column(col);
    for (uint8_t row = 0; row < KS_ROWS; row++) {
      ks_set_mux(row);
      ks_delay(KS_SETTLE_MUX);
      raw[ks_index(col, row)] = ks_sample_adc();
    }
    ks_complete_column(col);
  }
}
