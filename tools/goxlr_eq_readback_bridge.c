/* Reference for the ARM receive task. Callers serialize access and supply a
 * monotonic tick counter. This file is not linked into the GoXLR image. */
#include <stdbool.h>
#include <stdint.h>

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
    *state = (EqReadback){.key = key, .started = now, .timeout = timeout, .pending = true};
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

#ifdef GOXLR_EQ_BRIDGE_TEST
#include <assert.h>

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
    return 0;
}
#endif
