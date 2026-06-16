/*
 * wireless.c - SPI3 wireless (2.4 GHz / BLE) report bus, AT32F405 = master.
 *
 * Downlink driver for the PAN1080 radio module. The BSP-call style (crm clock
 * enable, gpio_init, spi_init, dma_init, dmamux_init) mirrors rgb.c (the SPI2
 * LED driver).
 *
 * The pure frame builder (wl_build_frame) compiles in the host test build; all
 * register/DMA/GPIO code is guarded out under -DHOST_TEST.
 */
#include "wireless.h"
#include "board_config.h"

/* The radio identity is sent as "<WL_IDENTITY_STR>-<slot>" into a fixed 33-byte field. */
_Static_assert(sizeof(WL_IDENTITY_STR) <= 32, "WL_IDENTITY_STR too long for the identity frame");

/* ---- peripheral / RAM addresses for reference ------------------------------
 * The driver reaches these through the Artery BSP symbols (SPI3, DMA1_CHANNEL2,
 * GPIOx), which resolve to exactly these literals. ---------------------------- */
#define WL_SPI3_BASE       0x40003C00u  /* SPI3 wireless bus base                   */
#define WL_SPI3_DT         0x40003C0Cu  /* SPI3 data reg = DMA dest                 */
#define WL_DMA1_CH2_CTRL   0x4002601Cu  /* DMA1 channel 2 control                   */
#define WL_GPIOB_BASE      0x40020400u  /* SCK/MISO/MOSI = PB3/PB4/PB5              */
#define WL_GPIOA_BASE      0x40020000u  /* CS = PA15 (software)                     */
#define WL_GPIOD_BASE      0x40020C00u  /* INT = PD2 (input)                        */
#define WL_FRAME_RAM       0x20005BDCu  /* frame buffer in RAM                      */
#define WL_LINKSTATE_RAM   0x2000049Cu  /* link-state struct, state[0x1a]           */

/* ------------------------------------------------------------------------- */
/*  Pure frame builder (host-testable - no BSP)                              */
/* ------------------------------------------------------------------------- */

uint8_t wl_build_frame(uint8_t cmd, const uint8_t *data, uint8_t len, uint8_t *out)
{
  uint8_t chk = 0;
  uint8_t i;
  uint8_t padded;

  out[0] = cmd;                              /* [0] CMD                             */
  out[1] = len;                              /* [1] LEN = N                         */
  for (i = 0; i < len; i++) {
    out[2 + i] = data[i];                    /* [2 .. N+1] DATA                     */
    chk = (uint8_t)(chk + data[i]);          /* CHK = (sum DATA) & 0xFF             */
  }                                          /*   DATA only - CMD/LEN excluded      */
  out[2 + len] = chk;                        /* [N+2] CHK                           */

  /* zero-pad so the total rounds up to a multiple of 4: round_up(N+3, 4)
   * (DMA/word alignment). */
  padded = (uint8_t)((len + 3u + 3u) & ~3u);
  for (i = (uint8_t)(len + 3u); i < padded; i++) out[i] = 0x00;
  return padded;
}

/* ------------------------------------------------------------------------- */
/*  Hardware back end (SPI3 + DMA1 ch2 + GPIO) - excluded from the host test  */
/* ------------------------------------------------------------------------- */
#ifndef HOST_TEST
#include "at32f402_405.h"

/* DMA-visible TX frame buffer. DMA1 ch2 sources from byte 0 = CMD with memory-
 * increment on. */
static uint8_t  g_tx[WL_FRAME_MAX];
static volatile uint8_t g_busy;     /* software busy flag                            */
static uint8_t  g_link_state;       /* last conn-state, feeds the 0x94 id digit      */

/* A new frame is only built when BOTH the hardware SPI_STS.BF (busy) and the
 * software busy flag are clear. */
static int wl_busy(void)
{
  if (g_busy) return 1;
  if (spi_i2s_flag_get(SPI3, SPI_I2S_BF_FLAG) != RESET) return 1;
  return 0;
}

/* Assert CS, kick DMA1 ch2 (mem -> SPI3_DT), then block until the burst has been
 * clocked out and deassert CS.
 *
 * Busy interlock: a software busy flag is set on send and cleared once the burst
 * completes. The completion path is an inline poll of the DMA full-transfer flag
 * plus SPI_STS.BF, after which CS is raised here. A frame is <= 68 bytes at
 * 6.75 MHz (~80 us worst case), so the spin is short. */
static void wl_dma_send(uint8_t n)
{
  while (wl_busy()) { /* wait out any prior burst */ }
  g_busy = 1;

  gpio_bits_reset(GPIOA, GPIO_PINS_15);             /* PA15 CS -> LOW (assert)          */

  dma_channel_enable(DMA1_CHANNEL2, FALSE);
  dma_flag_clear(DMA1_FDT2_FLAG);
  dma_flag_clear(DMA1_HDT2_FLAG);
  dma_flag_clear(DMA1_DTERR2_FLAG);
  dma_data_number_set(DMA1_CHANNEL2, n);            /* count = round_up(N+3,4)          */
  dma_channel_enable(DMA1_CHANNEL2, TRUE);          /* DMA1 ch2 -> SPI3_DT              */

  while (dma_flag_get(DMA1_FDT2_FLAG) == RESET) { /* all bytes handed to SPI */ }
  while (spi_i2s_flag_get(SPI3, SPI_I2S_BF_FLAG) != RESET) { /* shift register empties */ }

  gpio_bits_set(GPIOA, GPIO_PINS_15);               /* PA15 CS -> HIGH (deassert)       */
  g_busy = 0;
}

void wireless_init(void)
{
  gpio_init_type gpio_cfg;
  spi_init_type  spi_cfg;
  dma_init_type  dma_cfg;

  /* peripheral clocks: SPI3 + GPIOA/B/D; DMA1 for the TX channel */
  crm_periph_clock_enable(CRM_GPIOA_PERIPH_CLOCK, TRUE);
  crm_periph_clock_enable(CRM_GPIOB_PERIPH_CLOCK, TRUE);
  crm_periph_clock_enable(CRM_GPIOD_PERIPH_CLOCK, TRUE);
  crm_periph_clock_enable(CRM_SPI3_PERIPH_CLOCK,  TRUE);
  crm_periph_clock_enable(CRM_DMA1_PERIPH_CLOCK,  TRUE);

  /* PB3/PB4/PB5 -> SPI3 SCK/MISO/MOSI, AF mux 6 */
  gpio_default_para_init(&gpio_cfg);
  gpio_cfg.gpio_pins           = GPIO_PINS_3 | GPIO_PINS_4 | GPIO_PINS_5;
  gpio_cfg.gpio_mode           = GPIO_MODE_MUX;
  gpio_cfg.gpio_out_type       = GPIO_OUTPUT_PUSH_PULL;
  gpio_cfg.gpio_pull           = GPIO_PULL_UP;
  gpio_cfg.gpio_drive_strength = GPIO_DRIVE_STRENGTH_STRONGER;
  gpio_init(GPIOB, &gpio_cfg);
  gpio_pin_mux_config(GPIOB, GPIO_PINS_SOURCE3, GPIO_MUX_6);   /* SPI3_SCK  */
  gpio_pin_mux_config(GPIOB, GPIO_PINS_SOURCE4, GPIO_MUX_6);   /* SPI3_MISO */
  gpio_pin_mux_config(GPIOB, GPIO_PINS_SOURCE5, GPIO_MUX_6);   /* SPI3_MOSI */

  /* PA15 -> software CS, push-pull output, idle HIGH (deasserted) */
  gpio_default_para_init(&gpio_cfg);
  gpio_cfg.gpio_pins           = GPIO_PINS_15;
  gpio_cfg.gpio_mode           = GPIO_MODE_OUTPUT;
  gpio_cfg.gpio_out_type       = GPIO_OUTPUT_PUSH_PULL;
  gpio_cfg.gpio_pull           = GPIO_PULL_UP;
  gpio_cfg.gpio_drive_strength = GPIO_DRIVE_STRENGTH_STRONGER;
  gpio_init(GPIOA, &gpio_cfg);
  gpio_bits_set(GPIOA, GPIO_PINS_15);                          /* CS high (idle)     */

  /* PD2 -> INT from module (input, pull-up so it idles high; module pulls it LOW
   * to signal "ready / has data"). */
  gpio_default_para_init(&gpio_cfg);
  gpio_cfg.gpio_pins = GPIO_PINS_2;
  gpio_cfg.gpio_mode = GPIO_MODE_INPUT;
  gpio_cfg.gpio_pull = GPIO_PULL_UP;
  gpio_init(GPIOD, &gpio_cfg);

  /* SPI3: master, full-duplex, /16, 8-bit, MSB-first, Mode 1 (CPOL0/CPHA1),
   * software CS. CPHA1 == BSP SPI_CLOCK_PHASE_2EDGE. */
  spi_default_para_init(&spi_cfg);
  spi_cfg.transmission_mode      = SPI_TRANSMIT_FULL_DUPLEX;   /* cfg[0]=0  */
  spi_cfg.master_slave_mode      = SPI_MODE_MASTER;            /* MSTEN=1   */
  spi_cfg.mclk_freq_division     = SPI_MCLK_DIV_16;            /* MDIV=3    */
  spi_cfg.first_bit_transmission = SPI_FIRST_BIT_MSB;          /* LTF=0     */
  spi_cfg.frame_bit_num          = SPI_FRAME_8BIT;             /* FBN=0     */
  spi_cfg.clock_polarity         = SPI_CLOCK_POLARITY_LOW;     /* CPOL=0    */
  spi_cfg.clock_phase            = SPI_CLOCK_PHASE_2EDGE;      /* CPHA=1    */
  spi_cfg.cs_mode_selection      = SPI_CS_SOFTWARE_MODE;       /* SWCSEN=1  */
  spi_init(SPI3, &spi_cfg);
  /* hold the internal software-NSS high so the master does not see a mode fault;
   * the real CS edge is driven on PA15. */
  spi_software_cs_internal_level_set(SPI3, SPI_SWCS_INTERNAL_LEVEL_HIGHT);
  spi_i2s_dma_transmitter_enable(SPI3, TRUE);                  /* CTRL2.DMATEN = 1   */
  spi_enable(SPI3, TRUE);

  /* DMA1 channel 2: frame buffer -> SPI3->dt, byte, mem-increment, single-shot
   * (re-armed per frame, mirroring rgb.c). Peripheral = SPI3_DT. */
  dma_reset(DMA1_CHANNEL2);
  dma_default_para_init(&dma_cfg);
  dma_cfg.buffer_size           = WL_FRAME_MAX;
  dma_cfg.direction             = DMA_DIR_MEMORY_TO_PERIPHERAL;
  dma_cfg.memory_base_addr      = (uint32_t)g_tx;             /* frame buffer        */
  dma_cfg.memory_data_width     = DMA_MEMORY_DATA_WIDTH_BYTE;
  dma_cfg.memory_inc_enable     = TRUE;
  dma_cfg.peripheral_base_addr  = (uint32_t)&SPI3->dt;        /* = WL_SPI3_DT        */
  dma_cfg.peripheral_data_width = DMA_PERIPHERAL_DATA_WIDTH_BYTE;
  dma_cfg.peripheral_inc_enable = FALSE;
  dma_cfg.priority              = DMA_PRIORITY_HIGH;
  dma_cfg.loop_mode_enable      = FALSE;
  dma_init(DMA1_CHANNEL2, &dma_cfg);
  dmamux_init(DMA1MUX_CHANNEL2, DMAMUX_DMAREQ_ID_SPI3_TX);    /* SPI3 TX request     */
  dmamux_enable(DMA1, TRUE);
}

void wl_send_keyboard(const uint8_t report[8])
{
  uint8_t data[8];                 /* 0x81 boot-kbd carries 8 DATA bytes             */
  uint8_t i;
  uint8_t n;

  /* DATA = leading 0 followed by report[1..7]; report[0] (the modifier byte) is
   * not carried in this 8-DATA window. Worked example: key 'a' ->
   * 81 08 00 00 04 00 00 00 00 00 04 00, CHK 0x04. */
  data[0] = 0x00;
  for (i = 1; i < 8; i++) data[i] = report[i];
  n = wl_build_frame(WL_CMD_KEYBOARD, data, 8, g_tx);        /* CMD 0x81, LEN 8     */
  wl_dma_send(n);
}

void wl_send_identity(void)
{
  static const char id[] = WL_IDENTITY_STR;   /* base name, e.g. "FUN60 Ultra"      */
  uint8_t data[0x21];                          /* LEN 0x21 = 33 DATA bytes          */
  uint8_t base = (uint8_t)(sizeof(id) - 1u);   /* base length (excl. NUL)           */
  uint8_t i;
  uint8_t n;

  /* The host sees "<base>-<slot>". The slot suffix is the connection index: BLE
   * bonds up to 3 hosts (-1/-2/-3, Fn-switched), while the 2.4 GHz dongle is
   * single-host (-1). g_link_state selects the slot (0 => "-1"). The exact
   * in-frame offset is a bring-up item to confirm on the bus. */
  for (i = 0; i < base; i++) data[i] = (uint8_t)id[i];
  data[base]     = '-';
  data[base + 1] = (uint8_t)('1' + g_link_state);
  for (i = (uint8_t)(base + 2u); i < 0x21u; i++) data[i] = 0x00;            /* zero pad */
  n = wl_build_frame(WL_CMD_IDENTITY, data, 0x21, g_tx);    /* CMD 0x94, LEN 0x21  */
  wl_dma_send(n);
}

void wl_send_conn_state(uint8_t state)
{
  uint8_t n;
  g_link_state = state;                                      /* feeds 0x94 id digit  */
  n = wl_build_frame(WL_CMD_CONN_STATE, &state, 1, g_tx);    /* CMD 0x93, LEN 1     */
  wl_dma_send(n);
}

int wl_poll_int(void)
{
  /* PD2 INT (module -> host). LOW => "module ready / has data"; a single-pin read. */
  return (gpio_input_data_bit_read(GPIOD, GPIO_PINS_2) == RESET) ? 1 : 0;
}

#endif /* !HOST_TEST */
