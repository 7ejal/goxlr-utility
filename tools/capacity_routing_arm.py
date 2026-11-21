"""Execute original CPU-audio registration, IRQ and deferred processing offline.

Reuses the project's Unicorn/Capstone installation. No USB or hardware access.
Run: python tools/capacity_routing_arm.py --firmware <901-or-909.bin> --output <report.json>
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path

from capstone import Cs, CS_ARCH_ARM, CS_MODE_ARM
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_ARM, UC_HOOK_CODE, UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3, UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7, UC_ARM_REG_R8, UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC

KNOWN = {
    "5ff0d41144b47672c1486d7e7182ae260f7a36e6044860ddfd0f1afceb1436db",
    "74555b3667dcb7150930ae544bdfcf732febf7bb6a0aa9b7cfd9b513ffa3c74b",
}
RETURN = 0x70000
OBJECT = 0x60000
EXPONENT = 0x61000
BUFFERS = (0x62000, 0x63000)
STUBS = (0x10E28, 0x10E30, 0x10E2C)


def machine(image):
    u = Uc(UC_ARCH_ARM, UC_MODE_ARM)
    u.mem_map(0, 0x80000)
    u.mem_map(0xC4000000, 0x1000)
    u.mem_map(0x50009000, 0x1000)
    # Original initialized ARM image only; global BSS remains zero, not DSP bytes.
    u.mem_write(0x200, image[0x54:0x3A2AC])
    return u


def construct(image):
    u = machine(image)
    cursor = 0x50000
    requests = []
    get = lambda a: struct.unpack("<I", u.mem_read(a, 4))[0]

    def allocate(_u, address, _size, _data):
        nonlocal cursor
        if address not in (0x1A418, 0x1A4A8):
            return
        size = u.reg_read(UC_ARM_REG_R0)
        assert 0 < size < 4096
        requests.append({"allocator": hex(address), "bytes": size, "pointer": hex(cursor)})
        u.reg_write(UC_ARM_REG_R0, cursor)
        cursor += (size + 3) & ~3
        u.reg_write(UC_ARM_REG_PC, u.reg_read(UC_ARM_REG_LR))

    u.hook_add(UC_HOOK_CODE, allocate)
    u.reg_write(UC_ARM_REG_SP, 0x6F000)
    u.reg_write(UC_ARM_REG_LR, RETURN)
    u.emu_start(0x13E4, RETURN, count=1000000)
    assert u.reg_read(UC_ARM_REG_PC) == RETURN
    assert u.reg_read(UC_ARM_REG_SP) == 0x6F000
    root = get(0x3CA5C)
    assert get(root) == 0x2EE28 and get(root + 4) == 0x3666C
    collection = get(root + 8)
    assert get(collection) == 0x2EE70
    assert get(collection + 8) == get(collection + 12) == 2
    children = [get(get(collection + 4) + 4 * i) for i in range(2)]
    assert [u.mem_read(child + 0x38, 1)[0] for child in children] == [0, 1]
    assert [request["bytes"] for request in requests] == [20, 16, 8, 128, 128, 60, 128, 60, 128]
    return {"root": hex(root), "requested_bytes": sum(r["bytes"] for r in requests),
            "allocations": requests, "processing_children": [hex(c) for c in children],
            "limits": "C++ allocation wrappers stubbed; request sizes and constructors execute original code, not live allocator pressure"}, u


def processing(image):
    construction, u = construct(image)
    get = lambda a: struct.unpack("<I", u.mem_read(a, 4))[0]
    put = lambda a, v: u.mem_write(a, struct.pack("<I", v & 0xFFFFFFFF))
    signed = lambda x: x - 0x100000000 if x & 0x80000000 else x
    visited = set()
    u.hook_add(UC_HOOK_CODE, lambda _u, address, _size, _data: visited.add(address))
    u.reg_write(UC_ARM_REG_SP, 0x6F000)
    u.reg_write(UC_ARM_REG_LR, RETURN)
    u.emu_start(0x1410, RETURN, count=40000)
    assert get(0x3ED60) == 0x1024
    reports = []
    for label, level in (("silence", 0), ("positive_step", 0x600000), ("negative_step", -0x600000), ("silence_after_steps", 0)):
        result = [[], []]
        for _block in range(8):
            for frame in range(16):
                for pair in range(2):
                    for channel in range(2):
                        put(0x50009000 + 32 * frame + 8 * pair + 4 * channel, level & 0xFFFFFF)
            u.reg_write(UC_ARM_REG_SP, 0x6F000)
            u.reg_write(UC_ARM_REG_LR, RETURN)
            u.reg_write(UC_ARM_REG_R1, 28)
            u.emu_start(0xEFA4, RETURN, count=1000000)
            assert u.reg_read(UC_ARM_REG_PC) == RETURN
            assert u.reg_read(UC_ARM_REG_SP) == 0x6F000
            for frame in range(16):
                for pair in range(2):
                    for channel in range(2):
                        result[pair].append(signed(get(0x50009200 + 32 * frame + 8 * pair + 4 * channel)))
        reports.append({"input": label, "raw_level": level, "blocks": 8,
                        "pair0_changed_samples": sum(v != level for v in result[0]), "pair0_min_max": [min(result[0]), max(result[0])],
                        "pair0_first_stereo_frames": result[0][:32], "pair1_changed_samples": sum(v != level for v in result[1]),
                        "pair1_min_max": [min(result[1]), max(result[1])], "pair1_first_stereo_frames": result[1][:32]})
        assert all(sample == level for sample in result[1]), "Native second pair did not preserve these fixtures"
        if level:
            assert any(sample != level for sample in result[0]), "Native first-pair processing was inert"
        elif label == "silence":
            assert all(sample == 0 for sample in result[0])
        else:
            assert sum(sample != 0 for sample in result[0]) == 32
    assert {0xEFA4, 0x1024, 0xF2C}.issubset(visited)
    return {"construction": construction, "fixtures": reports,
            "limits": "Native child processing and original deferred dispatch execute; PCM/routing/hardware/timing remain emulated, effect identity and delay interpretation unassigned"}


def run(image, status, initial_a=0, initial_b=0, callback=True):
    u = machine(image)
    put = lambda a, v: u.mem_write(a, struct.pack("<I", v & 0xFFFFFFFF))
    get = lambda a: struct.unpack("<I", u.mem_read(a, 4))[0]
    put(0x3CA5C, OBJECT)
    put(OBJECT, 0x2EE28)
    put(OBJECT + 4, EXPONENT)
    put(EXPONENT, get(0x3666C))
    put(OBJECT + 8, 0)  # Native algorithm collection is excluded, not mocked as valid.
    put(OBJECT + 12, BUFFERS[0])
    put(OBJECT + 16, BUFFERS[1])
    for a in STUBS:
        put(a, 0xE12FFF1E)  # Original OS IRQ enter/exit helpers are explicit stubs.
    writes = []
    visited = set()
    u.hook_add(UC_HOOK_MEM_WRITE, lambda _u, _access, address, size, value, _data: writes.append((address, size, value)))
    u.hook_add(UC_HOOK_CODE, lambda _u, address, _size, _data: visited.add(address))
    registers = [UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7, UC_ARM_REG_R8, UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11]
    for i, register in enumerate(registers):
        u.reg_write(register, 0xA0000000 + i)

    def call(address, r0=0, r1=0):
        u.reg_write(UC_ARM_REG_R0, r0)
        u.reg_write(UC_ARM_REG_R1, r1)
        u.reg_write(UC_ARM_REG_SP, 0x6F000)
        u.reg_write(UC_ARM_REG_LR, RETURN)
        u.emu_start(address, RETURN, count=40000)
        assert u.reg_read(UC_ARM_REG_PC) == RETURN
        assert u.reg_read(UC_ARM_REG_SP) == 0x6F000
        for i, register in enumerate(registers):
            assert u.reg_read(register) == 0xA0000000 + i
        return u.reg_read(UC_ARM_REG_R0)

    call(0x1410)
    assert [get(a) for a in (0x3ED84, 0x3ED60, 0x3ED8C)] == [0x101C, 0x1024, OBJECT]
    assert get(0xC4000400) == 0xC0000003
    assert get(0x3ED64) == get(0x3ED88) == 0
    put(0x3ED64, initial_a)
    put(0x3ED88, initial_b)
    if not callback:
        put(0x3ED84, 0)
    put(0xC4000400, status)
    start = len(writes)
    result = call(0xEFD4, 28)
    assert get(0x3ED64) == ((initial_a + bool(status & 0x400000)) & 0xFFFFFFFF)
    assert get(0x3ED88) == ((initial_b + bool(status & 0x800000)) & 0xFFFFFFFF)
    expected_result = 3 if status & 0x800000 and callback else 1
    assert result == expected_result
    acks = [v for a, _s, v in writes[start:] if a == 0xC4000404]
    assert acks == [mask for mask in (0x400000, 0x800000) if status & mask]

    expected = {}
    for pair, buffer in enumerate(BUFFERS):
        for frame in range(16):
            for channel in range(2):
                raw = (0x7FFFFF, 0x800000, 0xFFFFFF, 0x123456)[(pair + frame + channel) % 4]
                put(0x50009000 + 32 * frame + 8 * pair + 4 * channel, raw | 0xA5000000)
                expected[buffer + 8 * frame + 4 * channel] = raw | (0xFF000000 if raw & 0x800000 else 0)
    call(0xEFA4, 0xDEADBEEF, 28)
    assert {0xEFA4, 0x1024, 0xF2C}.issubset(visited)
    for address, value in expected.items():
        assert get(address) == value
    for pair, buffer in enumerate(BUFFERS):
        for frame in range(16):
            for channel in range(2):
                assert get(0x50009200 + 32 * frame + 8 * pair + 4 * channel) == expected[buffer + 8 * frame + 4 * channel]
    return {"status": hex(status), "callback": callback, "initial_counters": [initial_a, initial_b], "irq_return": result, "acks": [hex(a) for a in acks], "frames": 16, "signed_samples_per_destination": len(expected)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--firmware", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    image = args.firmware.read_bytes()
    digest = hashlib.sha256(image).hexdigest()
    assert digest in KNOWN, "Unrecognized firmware"
    cases = [run(image, status) for status in (0, 0x400000, 0x800000, 0xC00000)]
    cases += [run(image, 0xC00000, 0xFFFFFFFF, 0xFFFFFFFF), run(image, 0x800000, callback=False)]
    cs = Cs(CS_ARCH_ARM, CS_MODE_ARM)
    anchors = [{"pc": hex(i.address), "raw": i.bytes.hex(), "asm": i.mnemonic + " " + i.op_str}
               for start, end in ((0x12C4, 0x1434), (0x17CC, 0x1868), (0xF2C, 0x103C), (0xEFA4, 0xEFD4), (0xEFD4, 0xF084), (0xF0E8, 0xF144))
               for i in cs.disasm(image[start-0x1AC:end-0x1AC], start)]
    report = {"firmware_sha256": digest, "processing": processing(image), "cases": cases, "anchors": anchors,
              "limits": ["offline only; no live counter, clock, utilization or physical DAC measurements", "OS IRQ helpers stubbed; MMIO is ordinary emulated RAM", "six IRQ/copy cases supply object pointers/buffers with a null native processing collection; four processing fixtures execute the original constructor/children with allocation wrappers stubbed", "ISR/deferred callbacks, register/stack preservation, configured16-frame transfer and wrapping counters execute serialized ARM code", "event bits22/23 are not established DSP overruns, cycles or utilization"]}
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"PASS {len(cases)} serialized registration/IRQ/deferred cases; native construction/4 processing fixtures; {len(anchors)} instruction/data anchors; 64 signed samples per destination/IRQ case; no hardware measurements")


if __name__ == "__main__":
    main()
