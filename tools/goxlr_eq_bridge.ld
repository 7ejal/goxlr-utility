/* Offline link target. Startup reservation/loading is not yet installed. */
OUTPUT_ARCH(arm)
ENTRY(goxlr_eq_shi_observer_hook)
MEMORY { bridge (rwx) : ORIGIN = 0x4f000, LENGTH = 0x1000 }
SECTIONS
{
    .text : { *(.text*) *(.rodata*) } > bridge
    .data : ALIGN(4) { *(.data*) } > bridge
    __bridge_load_end = .;
    .bss (NOLOAD) : ALIGN(4) {
        __bridge_bss_start = .;
        *(.bss*) *(COMMON)
        __bridge_bss_end = .;
    } > bridge
    /DISCARD/ : { *(.comment*) *(.note*) *(.ARM.attributes) }
    ASSERT(__bridge_bss_end <= 0x50000, "Bridge exceeds experimental reservation")
}
