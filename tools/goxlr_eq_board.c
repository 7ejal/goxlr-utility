/* Experimental bindings for the SHA-locked 907 ARM image.
 * The caller masks IRQ/FIQ while reading or publishing the effects ring.
 * Tick duration still needs a board measurement before hardware testing. */
#include <stdbool.h>
#include <stdint.h>

#ifndef GOXLR_EQ_TIMEOUT_TICKS
#error Define GOXLR_EQ_TIMEOUT_TICKS in native firmware ticks
#endif
_Static_assert(GOXLR_EQ_TIMEOUT_TICKS > 0, "A read must have a timeout");
_Static_assert(GOXLR_EQ_TIMEOUT_TICKS < 0x80000000u, "Timeout must fit wrap-safe arithmetic");

typedef struct {
    uint8_t *buffer;
    uint32_t size;
    uint32_t write;
    uint32_t read;
} EffectRing;

bool goxlr_eq_enqueue_frame(const uint8_t *frame, unsigned length)
{
    volatile EffectRing *ring = (volatile EffectRing *)0x38dd4;
    if (length != 10 || ring->size != 0x404 ||
        (uintptr_t)ring->buffer != 0x3d79d ||
        ring->write >= ring->size || ring->read >= ring->size)
        return false;

    uint32_t write = ring->write;
    uint32_t used = write >= ring->read ? write - ring->read
                                       : ring->size - ring->read + write;
    if (ring->size - used - 1 < 12)
        return false;

    for (unsigned i = 0; i < 12; ++i) {
        ring->buffer[write] = i < length ? frame[i] : 0xff;
        if (++write == ring->size)
            write = 0;
    }
    /* Publish only after the entire padded frame has been stored. */
    __asm__ volatile ("" ::: "memory");
    ring->write = write;
    return true;
}

uint32_t goxlr_eq_timeout_ticks(void)
{
    return GOXLR_EQ_TIMEOUT_TICKS;
}
