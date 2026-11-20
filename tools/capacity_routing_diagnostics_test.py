"""Execute the linked ARM diagnostic helper against guarded synthetic RAM.

No device access. The existing native tick wrapper and stock getter execute.
Run: python tools/capacity_routing_diagnostics_test.py --elf <bridge.elf> --firmware <901-or-909.bin> --output <report.json>
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path

from unicorn import Uc, UC_ARCH_ARM, UC_MODE_ARM, UC_HOOK_MEM_READ, UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7, UC_ARM_REG_R8, UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC, UC_ARM_REG_CPSR

RETURN = 0x70000
OUTPUT = 0x58000
CLOCK = 0x54000
STACK = 0x6F000
KNOWN = {"5ff0d41144b47672c1486d7e7182ae260f7a36e6044860ddfd0f1afceb1436db",
         "74555b3667dcb7150930ae544bdfcf732febf7bb6a0aa9b7cfd9b513ffa3c74b"}


def load_elf(path):
    data = path.read_bytes()
    assert data[:6] == b"\x7fELF\x01\x01", "Expected little-endian ELF32"
    phoff, shoff = struct.unpack_from("<II", data, 28)
    phsize, phcount, shsize, shcount = struct.unpack_from("<HHHH", data, 42)
    segments = []
    for i in range(phcount):
        kind, offset, address, _, size, memory, _, _ = struct.unpack_from("<8I", data, phoff + phsize * i)
        if kind == 1:
            assert 0x40000 <= address < address + memory <= 0x60000
            segments.append((address, data[offset:offset + size], memory))
    sections = [struct.unpack_from("<10I", data, shoff + shsize * i) for i in range(shcount)]
    symbols = {}
    for section in sections:
        if section[1] != 2:
            continue
        strings = sections[section[6]]
        names = data[strings[4]:strings[4] + strings[5]]
        for offset in range(section[4], section[4] + section[5], section[9]):
            name, value, _, _, _, _ = struct.unpack_from("<IIIBBH", data, offset)
            text = names[name:names.find(b"\0", name)].decode()
            symbols[text] = value
    entry = symbols["goxlr_arm_diagnostic_read"]
    assert entry & 1, "Expected the existing Thumb C build"
    return segments, entry


def test(image, segments, entry, metric, expected=None, ring=(0x3D79D, 0x404, 0, 0),
         counter_a=0x12345678, counter_b=0xFFFFFFFF, ticks=0x1122334455667788, null=False):
    u = Uc(UC_ARCH_ARM, UC_MODE_ARM)
    u.mem_map(0, 0x80000)
    u.mem_write(0x200, image[0x54:0x3A2AC])
    for address, data, _size in segments:
        u.mem_write(address, data)
    put = lambda a, v: u.mem_write(a, struct.pack("<I", v & 0xFFFFFFFF))
    get = lambda a: struct.unpack("<I", u.mem_read(a, 4))[0]
    tick_pointer = get(0x10E90)
    put(tick_pointer, CLOCK)
    put(CLOCK + 8, ticks)
    put(CLOCK + 12, ticks >> 32)
    put(0x3ED64, counter_a)
    put(0x3ED88, counter_b)
    for i, value in enumerate(ring):
        put(0x38DD4 + 4 * i, value)
    put(OUTPUT, 0xCAFEF00D)
    registers = [UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7, UC_ARM_REG_R8, UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11]
    for i, register in enumerate(registers):
        u.reg_write(register, 0xA0000000 + i)
    reads, writes = [], []
    allowed = [(a, a + size) for a, _data, size in segments] + [(STACK - 256, STACK)]
    if not null:
        if metric in (1, 2):
            a = 0x3ED64 if metric == 1 else 0x3ED88
            allowed.append((a, a + 4))
        elif metric in (3, 4, 5):
            allowed.append((0x38DD4, 0x38DE4))
        elif metric == 6:
            allowed.extend([(0x10E80, 0x10E94), (tick_pointer, tick_pointer + 4), (CLOCK + 8, CLOCK + 16)])

    def read_guard(_u, _access, address, size, _value, _data):
        assert any(a <= address and address + size <= b for a, b in allowed), f"Unexpected read {address:#x}/{size} for metric{metric}"
        reads.append((address, size))

    def write_guard(_u, _access, address, size, value, _data):
        assert STACK - 256 <= address and address + size <= STACK or address == OUTPUT and size == 4 and not null, f"Unexpected write {address:#x}/{size}"
        writes.append((address, size, value))

    u.hook_add(UC_HOOK_MEM_READ, read_guard)
    u.hook_add(UC_HOOK_MEM_WRITE, write_guard)
    u.reg_write(UC_ARM_REG_CPSR, 0xD3)
    u.reg_write(UC_ARM_REG_SP, STACK)
    u.reg_write(UC_ARM_REG_LR, RETURN)
    u.reg_write(UC_ARM_REG_R0, metric)
    u.reg_write(UC_ARM_REG_R1, 0 if null else OUTPUT)
    u.emu_start(entry, RETURN, count=1000)
    assert u.reg_read(UC_ARM_REG_PC) == RETURN and u.reg_read(UC_ARM_REG_SP) == STACK
    assert u.reg_read(UC_ARM_REG_CPSR) & 0xFF == 0xD3
    for i, register in enumerate(registers):
        assert u.reg_read(register) == 0xA0000000 + i
    success = expected is not None and not null
    assert u.reg_read(UC_ARM_REG_R0) == int(success)
    assert get(OUTPUT) == (expected if success else 0xCAFEF00D)
    out_writes = [w for w in writes if w[0] == OUTPUT]
    assert len(out_writes) == int(success)
    return {"metric": metric, "expected_success": success, "value": get(OUTPUT), "read_accesses": len(reads), "writes_outside_output_or_stack": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--firmware", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    image = args.firmware.read_bytes()
    digest = hashlib.sha256(image).hexdigest()
    assert digest in KNOWN
    segments, entry = load_elf(args.elf)
    cases = []
    for metric in list(range(258)) + [0xFFFE, 0xFFFF]:
        value = {1:0x12345678, 2:0xFFFFFFFF, 3:0x404, 4:0x3D79D, 5:0, 6:0x55667788}.get(metric)
        cases.append(test(image, segments, entry, metric, expected=value))
    for metric in range(1, 7):
        cases.append(test(image, segments, entry, metric, null=True))
    for write, read in ((1, 0), (0x403, 0), (0, 1), (0, 0x403), (7, 1020), (1020, 7)):
        used = write - read if write >= read else 0x404 - read + write
        cases.append(test(image, segments, entry, 5, expected=used, ring=(0x3D79D, 0x404, write, read)))
    for ring in ((0, 0x404, 0, 0), (0x3D79E, 0x404, 0, 0), (0x3D79D, 0, 0, 0),
                 (0x3D79D, 0x403, 0, 0), (0x3D79D, 0x404, 0x404, 0), (0x3D79D, 0x404, 0, 0xFFFFFFFF)):
        for metric in (3, 4, 5):
            cases.append(test(image, segments, entry, metric, ring=ring))
    for value in (0, 1, 0x80000000, 0xFFFFFFFF):
        cases.append(test(image, segments, entry, 1, expected=value, counter_a=value))
        cases.append(test(image, segments, entry, 2, expected=value, counter_b=value))
        cases.append(test(image, segments, entry, 6, expected=value, ticks=(0xFFFFFFFF << 32) | value))
    report = {"firmware_sha256": digest, "elf_sha256": hashlib.sha256(args.elf.read_bytes()).hexdigest(),
              "helper_entry": hex(entry), "case_count": len(cases), "cases": cases,
              "limits": "Linked actual ARM/Thumb helper, established tick wrapper and original10E80 getter execute against synthetic RAM; all accesses guarded. This is not a serialized firmware/recovery test or a physical telemetry measurement."}
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"PASS {len(cases)} actual ARM helper cases; whitelist, null output, corrupt ring, wrap states and access guards; no hardware metrics measured")


if __name__ == "__main__":
    main()
