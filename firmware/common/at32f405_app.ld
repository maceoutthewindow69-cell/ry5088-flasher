/* Linker script: MonsGeek Fun60 Ultra firmware -> APP SLOT.
 *
 * Links into the application slot behind the factory RY bootloader. The bootloader
 * (0x08000000-0x08004FFF) + chip-id header (0x08005000) are preserved; the app
 * vector table sits at 0x08005200, where the bootloader jumps and where
 * SystemInit() points VTOR (VECT_TAB_OFFSET = 0x5200).
 *
 *   0x08000000  RY bootloader            [DO NOT overwrite]
 *   0x08005000  chip-id "AT32F405 8KMKB"
 *   0x08005200  THIS app's vector table  (FLASH ORIGIN below)
 *   0x08025800  patch zone               (stay below this)
 *   0x08028000  config / keymaps / magnetism cal
 *
 * Full-chip takeover (replace the RY bootloader, SWD-only, no DFU recovery):
 * set FLASH ORIGIN = 0x08000000, LENGTH = 256K, and VECT_TAB_OFFSET = 0x0.
 */
ENTRY(Reset_Handler)

_estack         = 0x20019800;   /* top of 102 KB SRAM (0x20000000 + 0x19800) */
_Min_Heap_Size  = 0x400;
_Min_Stack_Size = 0x800;

MEMORY
{
  /* app vector table .. just before the 10 KB patch zone @0x08025800 */
  FLASH (rx)  : ORIGIN = 0x08005200, LENGTH = 0x20600
  RAM   (xrw) : ORIGIN = 0x20000000, LENGTH = 102K
}

SECTIONS
{
  .isr_vector : { . = ALIGN(4); KEEP(*(.isr_vector)) . = ALIGN(4); } >FLASH

  .text : {
    . = ALIGN(4);
    *(.text) *(.text*) *(.glue_7) *(.glue_7t) *(.eh_frame)
    KEEP(*(.init)) KEEP(*(.fini))
    . = ALIGN(4); _etext = .;
  } >FLASH

  .rodata : { . = ALIGN(4); *(.rodata) *(.rodata*) . = ALIGN(4); } >FLASH
  .ARM.extab : { *(.ARM.extab* .gnu.linkonce.armextab.*) } >FLASH
  .ARM : { __exidx_start = .; *(.ARM.exidx*) __exidx_end = .; } >FLASH

  .preinit_array : {
    PROVIDE_HIDDEN(__preinit_array_start = .);
    KEEP(*(.preinit_array*))
    PROVIDE_HIDDEN(__preinit_array_end = .);
  } >FLASH
  .init_array : {
    PROVIDE_HIDDEN(__init_array_start = .);
    KEEP(*(SORT(.init_array.*))) KEEP(*(.init_array*))
    PROVIDE_HIDDEN(__init_array_end = .);
  } >FLASH
  .fini_array : {
    PROVIDE_HIDDEN(__fini_array_start = .);
    KEEP(*(SORT(.fini_array.*))) KEEP(*(.fini_array*))
    PROVIDE_HIDDEN(__fini_array_end = .);
  } >FLASH

  _sidata = LOADADDR(.data);
  .data : {
    . = ALIGN(4); _sdata = .;
    *(.data) *(.data*) *(.RamFunc) *(.RamFunc*)
    . = ALIGN(4); _edata = .;
  } >RAM AT> FLASH

  .bss : {
    . = ALIGN(4); _sbss = .; __bss_start__ = _sbss;
    *(.bss) *(.bss*) *(COMMON)
    . = ALIGN(4); _ebss = .; __bss_end__ = _ebss;
  } >RAM

  ._user_heap_stack : {
    . = ALIGN(8); PROVIDE ( end = . ); PROVIDE ( _end = . );
    . = . + _Min_Heap_Size; . = . + _Min_Stack_Size; . = ALIGN(8);
  } >RAM

  .ARM.attributes 0 : { *(.ARM.attributes) }
}
