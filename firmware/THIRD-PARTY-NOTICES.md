# Third-Party Notices

This firmware is released under the MIT license (see the repository `LICENSE`).
It additionally depends on, and includes customized templates derived from, the
third-party software listed below. That software is **not** covered by this
project's MIT license and remains subject to its own terms.

## Artery AT32F402_405 Firmware Library (BSP)

- **Name:** AT32F402_405 Firmware Library (Board Support Package / standard
  peripheral library, CMSIS device support, and USB device stack)
- **Vendor:** Artery Technology (ArteryTek)
- **Source:** https://github.com/ArteryTek/AT32F402_405_Firmware_Library
- **License:** Artery's own BSP license (see the header of each file in the
  upstream library). The library is provided by Artery for use with Artery
  microcontrollers and is governed by that copyright notice and disclaimer, not
  by this project's MIT license.

### How it is used here

The bulk of the BSP (the CRM / GPIO / SPI / DMA / ADC / PWC / FLASH / MISC
peripheral drivers, the CMSIS core/device support, and the USB device stack) is
**not vendored in this repository**. It is fetched separately from the URL above
into a `vendor/` directory at build time. See `README.md` for the exact steps.
The build is configured with `BSP_DIR` in each `Makefile`.

### BSP-derived template files included in this repository

A small number of files in `common/` are customized copies of Artery BSP
templates (configuration headers, the system/clock init, the interrupt vector
handlers, the USB configuration header, the GCC startup file). They are retained
in-tree because the firmware needs them to build, and each carries a short note
near the top:

> Derived from the Artery AT32F402_405 Firmware Library (see THIRD-PARTY-NOTICES).
> The bulk BSP is fetched separately.

These files remain subject to the Artery BSP license referenced above:

- `common/at32f402_405_clock.c`, `common/at32f402_405_clock.h`
- `common/at32f402_405_conf.h`
- `common/at32f402_405_int.c`, `common/at32f402_405_int.h`
- `common/system_at32f402_405.c`, `common/system_at32f402_405.h`
- `common/usb_conf.h`
- `common/startup_at32f402_405.s`

All other source files in `common/` are original to this project and are
covered by the MIT license.
