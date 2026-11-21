/* Offline-only diagnostics for the hash-verified 909 ARM layout.
 * Caller holds the existing IRQ/FIQ lock. No register writes or DSP access.
 * These are CPU-audio event counts, not overruns or DSP utilization. */
#include <stdbool.h>
#include <stdint.h>

extern uint32_t goxlr_eq_now_ticks(void);

bool goxlr_arm_diagnostic_read(uint16_t metric, uint32_t *value)
{
    uint32_t result;
    if (!value)
        return false;

    switch (metric) {
    case 1:
        result = *(volatile const uint32_t *)0x3ed64;
        break;
    case 2:
        result = *(volatile const uint32_t *)0x3ed88;
        break;
    case 3:
    case 4:
    case 5: {
        volatile const uint32_t *ring = (volatile const uint32_t *)0x38dd4;
        uint32_t buffer = ring[0], size = ring[1];
        uint32_t write = ring[2], read = ring[3];
        if (buffer != 0x3d79d || size != 0x404 || write >= size || read >= size)
            return false;
        result = metric == 3 ? size : metric == 4 ? buffer
            : write >= read ? write - read : size - read + write;
        break;
    }
    case 6:
        result = goxlr_eq_now_ticks();
        break;
    default:
        return false;
    }
    *value = result;
    return true;
}
