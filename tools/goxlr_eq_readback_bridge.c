/* Reference for the ARM receive task. Callers serialize access and supply a
 * monotonic tick counter. This file is not linked into the GoXLR image. */
#include <stdbool.h>
#include <stdint.h>

enum {
    EQ_USB_OK = 0,
    EQ_USB_PENDING = 1,
    EQ_USB_BUSY = 2,
    EQ_USB_INVALID = 3,
    EQ_USB_RESYNC = 4,
    EQ_USB_IDLE = 5,
};

typedef struct {
    uint32_t started;
    uint32_t timeout;
    uint16_t key;
    uint8_t next_word;
    int32_t value;
    bool pending;
    bool ready;
    bool needs_resync;
} EqReadback;

static bool is_eq_key(uint16_t key)
{
    static const uint16_t starts[] = {
        0x126, 0xf8, 0x113, 0x129, 0x116, 0x11d, 0x12c, 0x120,
        0x109, 0x12f, 0xf5, 0x10c
    };
    for (unsigned i = 0; i < sizeof(starts) / sizeof(starts[0]); ++i) {
        if (key >= starts[i] && key <= starts[i] + 2)
            return true;
    }
    return false;
}

static void expire(EqReadback *state, uint32_t now)
{
    if (state->pending && now - state->started >= state->timeout) {
        state->pending = false;
        state->next_word = 0;
        state->needs_resync = true;
    }
}

/* Caller must reserve room for the entire frame before starting a request.
 * After a timeout, reinitialize state only when the old reply cannot arrive.
 * The stock response carries a key but has no transaction ID. */
bool goxlr_eq_query(EqReadback *state, uint16_t key, uint32_t now,
                    uint32_t timeout, uint8_t frame[10])
{
    expire(state, now);
    if (state->pending || state->ready || state->needs_resync || !timeout || !is_eq_key(key))
        return false;

    const uint8_t query[] = {
        0xf0, 0, 1, 0x38, 0, 0x10, 0x47, (uint8_t)(key >> 7),
        (uint8_t)(key & 0x7f), 0xf7
    };
    for (unsigned i = 0; i < sizeof(query); ++i)
        frame[i] = query[i];
    state->key = key;
    state->started = now;
    state->timeout = timeout;
    state->next_word = 0;
    state->pending = true;
    return true;
}

/* The stock handler retains ownership of status messages and unrelated data. */
bool goxlr_eq_receive(EqReadback *state, uint32_t word, uint32_t now)
{
    expire(state, now);
    if (!state->pending)
        return false;

    if (word == 0xf00001) {
        state->next_word = 1;
        return true;
    }
    uint32_t key_word = ((uint32_t)(state->key >> 7) << 16)
                      | ((uint32_t)(state->key & 0x7f) << 8) | 0xff;
    switch (state->next_word) {
    case 1:
        if (word != 0x380010) break;
        state->next_word = 2;
        return true;
    case 2:
        if (word != 0x22ffff) break;
        state->next_word = 3;
        return true;
    case 3:
        if (word != key_word) break;
        state->value = 0;
        state->next_word = 4;
        return true;
    case 4:
    case 5:
        if ((word & 0xff8080ff) != 0xff) break;
        state->value = (state->value << 14) | (int32_t)((word >> 9) & 0x3f80)
                     | (int32_t)((word >> 8) & 0x7f);
        ++state->next_word;
        return true;
    case 6:
        if (word != 0xf7ffff) break;
        if (state->value & 0x08000000)
            state->value -= 0x10000000;
        state->pending = false;
        state->ready = true;
        state->next_word = 0;
        return true;
    }
    /* Status words can arrive between DSP reply fragments. */
    if ((word & 0xf00000) != 0xd00000)
        state->next_word = 0;
    return false;
}

bool goxlr_eq_take_reply(EqReadback *state, int32_t *value)
{
    if (!state->ready)
        return false;
    *value = state->value;
    state->ready = false;
    return true;
}

/* The USB dispatcher and SHI receive observer must serialize state access.
 * The enqueuer takes one complete frame through the existing ARM ring owner.
 * It must return false without enqueueing bytes if capacity is insufficient. */
uint8_t goxlr_eq_usb_submit(EqReadback *state, const uint8_t *request, unsigned length,
                            uint32_t now, uint32_t timeout,
                            bool (*enqueue)(const uint8_t *, unsigned))
{
    if (length != 2 || !timeout)
        return EQ_USB_INVALID;
    uint16_t key = (uint16_t)(request[0] | ((uint16_t)request[1] << 8));
    if (!is_eq_key(key))
        return EQ_USB_INVALID;

    expire(state, now);
    if (state->needs_resync)
        return EQ_USB_RESYNC;
    if (state->pending || state->ready)
        return EQ_USB_BUSY;

    uint8_t frame[10];
    if (!goxlr_eq_query(state, key, now, timeout, frame))
        return EQ_USB_INVALID;
    if (!enqueue(frame, sizeof(frame))) {
        state->pending = false;
        return EQ_USB_BUSY;
    }
    return EQ_USB_OK;
}

/* Query completion payload is key LE16 followed by raw value LE32.
 * Only EQ_USB_OK returns a payload. Polling neither waits nor touches SHI. */
uint8_t goxlr_eq_usb_poll(EqReadback *state, uint32_t now, uint8_t reply[6])
{
    expire(state, now);
    if (state->needs_resync)
        return EQ_USB_RESYNC;
    if (state->pending)
        return EQ_USB_PENDING;

    int32_t value;
    if (!goxlr_eq_take_reply(state, &value))
        return EQ_USB_IDLE;
    reply[0] = (uint8_t)state->key;
    reply[1] = (uint8_t)(state->key >> 8);
    for (unsigned i = 0; i < 4; ++i)
        reply[2 + i] = (uint8_t)((uint32_t)value >> (8 * i));
    return EQ_USB_OK;
}

#ifdef GOXLR_EQ_FIRMWARE
/* Board bindings must use the existing scheduler and effects ring owner. */
extern void goxlr_eq_lock(void);
extern void goxlr_eq_unlock(void);
extern uint32_t goxlr_eq_now_ticks(void);
extern uint32_t goxlr_eq_timeout_ticks(void);
extern bool goxlr_eq_enqueue_frame(const uint8_t *frame, unsigned length);
extern void goxlr_eq_usb_reply(unsigned status, const uint8_t *payload, unsigned length);

static EqReadback live_readback;

void goxlr_eq_observe_shi_word(uint32_t word)
{
    goxlr_eq_lock();
    goxlr_eq_receive(&live_readback, word, goxlr_eq_now_ticks());
    goxlr_eq_unlock();
}

void goxlr_eq_usb_dispatch(unsigned unused0, unsigned unused1, unsigned subcommand,
                           const uint8_t *request, uint16_t length)
{
    (void)unused0;
    (void)unused1;
    uint8_t reply[7];
    unsigned reply_length = 1;
    goxlr_eq_lock();
    uint32_t now = goxlr_eq_now_ticks();
    if (subcommand == 0) {
        reply[0] = goxlr_eq_usb_submit(&live_readback, request, length, now,
                                      goxlr_eq_timeout_ticks(), goxlr_eq_enqueue_frame);
    } else if (subcommand == 1 && length == 0) {
        reply[0] = goxlr_eq_usb_poll(&live_readback, now, reply + 1);
        if (reply[0] == EQ_USB_OK)
            reply_length = sizeof(reply);
    } else {
        reply[0] = EQ_USB_INVALID;
    }
    goxlr_eq_unlock();

    // Report bridge status in the payload so firmware errors do not trigger USB retries.
    goxlr_eq_usb_reply(0, reply, reply_length);
}
#endif

#ifdef GOXLR_EQ_BRIDGE_TEST
#include <assert.h>

static bool reject_frame(const uint8_t *frame, unsigned length)
{
    (void)frame;
    (void)length;
    return false;
}

static bool accept_frame(const uint8_t *frame, unsigned length)
{
    assert(length == 10);
    assert(frame[0] == 0xf0 && frame[6] == 0x47 && frame[9] == 0xf7);
    return true;
}

#ifdef GOXLR_EQ_FIRMWARE
static bool locked;
static uint8_t test_reply[7];
static unsigned test_reply_length;

void goxlr_eq_lock(void) { assert(!locked); locked = true; }
void goxlr_eq_unlock(void) { assert(locked); locked = false; }
uint32_t goxlr_eq_now_ticks(void) { assert(locked); return 1; }
uint32_t goxlr_eq_timeout_ticks(void) { assert(locked); return 10; }
bool goxlr_eq_enqueue_frame(const uint8_t *frame, unsigned length)
{
    assert(locked);
    return accept_frame(frame, length);
}
void goxlr_eq_usb_reply(unsigned status, const uint8_t *payload, unsigned length)
{
    assert(!locked);
    assert(status == 0 && length <= sizeof(test_reply));
    test_reply_length = length;
    for (unsigned i = 0; i < length; ++i)
        test_reply[i] = payload[i];
}
#endif

int main(void)
{
    EqReadback state = {0};
    uint8_t frame[10];
    int32_t value;
    assert(!goxlr_eq_query(&state, 0x10f, 0, 10, frame));
    assert(goxlr_eq_query(&state, 0xf6, UINT32_MAX - 2, 10, frame));
    assert(frame[6] == 0x47 && frame[7] == 1 && frame[8] == 0x76 && frame[9] == 0xf7);
    assert(!goxlr_eq_query(&state, 0x10e, 0, 10, frame));
    assert(goxlr_eq_receive(&state, 0xf00001, 0));
    assert(goxlr_eq_receive(&state, 0x380010, 0));
    assert(!goxlr_eq_receive(&state, 0xd20001, 0));
    assert(goxlr_eq_receive(&state, 0x22ffff, 0));
    assert(!goxlr_eq_receive(&state, 0x0177ff, 0));
    assert(!goxlr_eq_take_reply(&state, &value));
    const uint32_t reply[] = {0xf00001, 0x380010, 0x22ffff, 0x0176ff, 0x7f7fff, 0x7f7dff, 0xf7ffff};
    for (unsigned i = 0; i < sizeof(reply) / sizeof(reply[0]); ++i)
        assert(goxlr_eq_receive(&state, reply[i], 1));
    assert(goxlr_eq_take_reply(&state, &value) && value == -3);
    assert(goxlr_eq_query(&state, 0x10e, 5, 10, frame));
    assert(!goxlr_eq_receive(&state, 0xf00001, 15));
    assert(!goxlr_eq_take_reply(&state, &value));
    assert(!goxlr_eq_query(&state, 0x10e, 16, 10, frame));

    state = (EqReadback){0};
    const uint8_t request[] = {0xf6, 0};
    uint8_t payload[6] = {0};
    assert(goxlr_eq_usb_submit(&state, request, 1, 0, 10, accept_frame) == EQ_USB_INVALID);
    assert(goxlr_eq_usb_submit(&state, request, 2, 0, 10, reject_frame) == EQ_USB_BUSY);
    assert(goxlr_eq_usb_poll(&state, 0, payload) == EQ_USB_IDLE);
    assert(goxlr_eq_usb_submit(&state, request, 2, 0, 10, accept_frame) == EQ_USB_OK);
    assert(goxlr_eq_usb_poll(&state, 1, payload) == EQ_USB_PENDING);
    for (unsigned i = 0; i < sizeof(reply) / sizeof(reply[0]); ++i)
        assert(goxlr_eq_receive(&state, reply[i], 1));
    assert(goxlr_eq_usb_poll(&state, 1, payload) == EQ_USB_OK);
    assert(payload[0] == 0xf6 && payload[1] == 0 && payload[2] == 0xfd);
    assert(payload[3] == 0xff && payload[4] == 0xff && payload[5] == 0xff);
    assert(goxlr_eq_usb_submit(&state, request, 2, 5, 10, accept_frame) == EQ_USB_OK);
    assert(goxlr_eq_usb_poll(&state, 15, payload) == EQ_USB_RESYNC);
#ifdef GOXLR_EQ_FIRMWARE
    goxlr_eq_usb_dispatch(0, 0, 0, request, 2);
    assert(test_reply_length == 1 && test_reply[0] == EQ_USB_OK);
    goxlr_eq_usb_dispatch(0, 0, 1, 0, 0);
    assert(test_reply_length == 1 && test_reply[0] == EQ_USB_PENDING);
    for (unsigned i = 0; i < sizeof(reply) / sizeof(reply[0]); ++i)
        goxlr_eq_observe_shi_word(reply[i]);
    goxlr_eq_usb_dispatch(0, 0, 1, 0, 0);
    assert(test_reply_length == 7 && test_reply[0] == EQ_USB_OK);
    for (unsigned i = 0; i < sizeof(payload); ++i)
        assert(test_reply[i + 1] == payload[i]);
#endif
    return 0;
}
#endif
