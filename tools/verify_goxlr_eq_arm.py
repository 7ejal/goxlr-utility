"""Execute the linked ARM EQ bridge with the existing Unicorn ARM emulator.

Requires Unicorn 2.1.x. Supply the untouched 907 image and linked bridge ELF.
The emulator supplies RAM and captures the USB completion call. No device I/O.
"""
import argparse
import hashlib
import struct
import zlib
from pathlib import Path

from unicorn import Uc, UC_ARCH_ARM, UC_MODE_ARM, UC_HOOK_CODE
from unicorn.arm_const import (
    UC_ARM_REG_CPSR, UC_ARM_REG_LR, UC_ARM_REG_PC, UC_ARM_REG_R0,
    UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3, UC_ARM_REG_R4,
    UC_ARM_REG_R5, UC_ARM_REG_R7, UC_ARM_REG_SP,
    UC_ARM_REG_CP_REG,
)

STOCK_SHA = "eea92925a5d4eb0111943f72ce7bfeb0351e9390506feca17ac6a0c3b8346c13"
RETURN = 0x1000


def read_elf(path):
    data = path.read_bytes()
    assert data[:7] == b"\x7fELF\x01\x01\x01"
    assert struct.unpack_from("<H", data, 18)[0] == 40
    section_offset = struct.unpack_from("<I", data, 32)[0]
    section_size, section_count = struct.unpack_from("<HH", data, 46)
    sections = [
        struct.unpack_from("<10I", data, section_offset + i * section_size)
        for i in range(section_count)
    ]
    loaded = []
    symbols = {}
    for section in sections:
        _, kind, flags, address, offset, size, link, _, _, entry_size = section
        if flags & 2 and kind != 8:
            loaded.append((address, data[offset:offset + size]))
        if kind != 2:
            continue
        strings = sections[link]
        names = data[strings[4]:strings[4] + strings[5]]
        for at in range(offset, offset + size, entry_size):
            name, value, _, _, _, index = struct.unpack_from("<IIIBBH", data, at)
            if name and index:
                end = names.index(0, name)
                symbols[names[name:end].decode()] = value
            elif name and not index:
                raise AssertionError("Unresolved ARM symbol")
    return loaded, symbols


class Board:
    def __init__(self, stock, sections, symbols):
        self.cpu = Uc(UC_ARCH_ARM, UC_MODE_ARM)
        self.cpu.mem_map(0, 0x60000)
        arm_end = 0x54 + 4 * struct.unpack_from("<I", stock, 0x28)[0] - 4
        self.cpu.mem_write(0x200, stock[0x54:arm_end])
        for address, data in sections:
            self.cpu.mem_write(address, data)
        self.symbols = symbols
        self.replies = []
        self.cpu.hook_add(UC_HOOK_CODE, self.capture_usb)
        # Stock tick wrapper dereferences the runtime clock object.
        self.write32(0x3fa10, 0x44000)
        self.set_ticks(1)

    def write32(self, address, value):
        self.cpu.mem_write(address, struct.pack("<I", value))

    def read32(self, address):
        return struct.unpack("<I", self.cpu.mem_read(address, 4))[0]

    def set_ticks(self, value):
        self.cpu.mem_write(0x44008, struct.pack("<Q", value))

    def capture_usb(self, cpu, address, _size, _context):
        if address != 0x266c8:
            return
        status = cpu.reg_read(UC_ARM_REG_R0)
        length = cpu.reg_read(UC_ARM_REG_R2)
        payload = bytes(cpu.mem_read(cpu.reg_read(UC_ARM_REG_R1), length))
        self.replies.append((status, payload))
        cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))

    def call(self, name, args=(), stack=0x4d000):
        cpu = self.cpu
        cpu.reg_write(UC_ARM_REG_CPSR, 0x13)
        cpu.reg_write(UC_ARM_REG_SP, stack)
        cpu.reg_write(UC_ARM_REG_LR, RETURN)
        for register, value in zip(
            [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3], args
        ):
            cpu.reg_write(register, value)
        address = self.symbols[name] if isinstance(name, str) else name
        cpu.emu_start(address, RETURN, count=100000)
        assert cpu.reg_read(UC_ARM_REG_PC) == RETURN, name
        assert cpu.reg_read(UC_ARM_REG_SP) == stack, name
        assert cpu.reg_read(UC_ARM_REG_CPSR) & 0xff == 0x13, name
        return cpu.reg_read(UC_ARM_REG_R0)

    def dispatch(self, command, payload=b""):
        self.cpu.mem_write(0x45000, payload or b"\x00")
        self.write32(0x4d000, len(payload))
        self.call("goxlr_eq_usb_dispatch", (0, 0, command, 0x45000))
        status, result = self.replies[-1]
        assert status == 0
        return result


def verify(stock, sections, symbols):
    make = lambda: Board(stock, sections, symbols)
    board = make()
    # The actual reset clear leaves the proposed top region intact.
    before = bytes(board.cpu.mem_read(0x4f000, 0x1000))
    board.cpu.emu_start(0x314, 0x334, count=60000)
    assert board.cpu.reg_read(UC_ARM_REG_PC) == 0x334
    assert bytes(board.cpu.mem_read(0x4f000, 0x1000)) == before
    # Replay stock USB registration and the hook after its 24 registrations.
    hook = symbols["goxlr_eq_usb_registration_hook"]
    branch = 0xea000000 | (((hook - 0xb328 - 8) >> 2) & 0xffffff)
    board.write32(0xb328, branch)
    board.cpu.reg_write(UC_ARM_REG_CPSR, 0x13)
    board.cpu.reg_write(UC_ARM_REG_R4, 0)
    board.cpu.reg_write(UC_ARM_REG_R5, 0x38e18)
    board.cpu.reg_write(UC_ARM_REG_SP, 0x4d000)
    board.cpu.mem_write(0x4d000, struct.pack("<IIII", 3, 4, 5, RETURN))
    board.cpu.emu_start(0xb314, RETURN, count=20000)
    assert board.cpu.reg_read(UC_ARM_REG_PC) == RETURN
    # Stock registration routine reads its dispatcher root from a literal.
    root = board.read32(0x26398)
    head = board.read32(root + 0x44c)
    groups = []
    while head:
        groups.append(board.read32(head + 4))
        head = board.read32(head)
        assert len(groups) <= 25
    assert len(groups) == 25 and groups[0] == 0x819
    expected = [struct.unpack_from("<I", stock, 0x38c70 + 16 * i)[0] for i in range(24)]
    assert groups[1:] == expected[::-1]
    print("PASS serialized stock reset clear and all 25 USB registrations")

    board = make()
    board.write32(0x38dd4 + 8, 0)
    board.write32(0x38dd4 + 12, 0)
    assert board.dispatch(0, b"\xf6\x00") == b"\x00"
    query = bytes.fromhex("f0 00 01 38 00 10 47 01 76 f7 ff ff")
    assert bytes(board.cpu.mem_read(0x3d79d, 12)) == query
    assert board.read32(0x38dd4 + 8) == 12
    assert board.dispatch(1) == b"\x01"
    assert board.dispatch(0, b"\xf7\x00") == b"\x02"
    for word in [0xf00001, 0x380010, 0xd20001, 0x22ffff, 0x0176ff, 0x7f7fff, 0x7f7dff, 0xf7ffff]:
        board.call("goxlr_eq_observe_shi_word", (word,))
    assert board.dispatch(1) == bytes.fromhex("00 f6 00 fd ff ff ff")
    assert board.dispatch(1) == b"\x05"
    print("PASS linked submit, padded query, interleaved status, signed reply and IRQ restore")

    # Exercise actual compiled publisher across the effects-ring wrap boundary.
    board = make()
    board.write32(0x38dd4 + 8, 0x3ff)
    board.write32(0x38dd4 + 12, 0x20)
    assert board.dispatch(0, b"\xf6\x00") == b"\x00"
    result = bytes(board.cpu.mem_read(0x3d79d + 0x3ff, 5)) + bytes(board.cpu.mem_read(0x3d79d, 7))
    assert result == query and board.read32(0x38dd4 + 8) == 7
    board = make()
    board.write32(0x38dd4 + 8, 0)
    board.write32(0x38dd4 + 12, 1)
    before = bytes(board.cpu.mem_read(0x3d79d, 0x404))
    assert board.dispatch(0, b"\xf6\x00") == b"\x02"
    assert board.read32(0x38dd4 + 8) == 0
    assert bytes(board.cpu.mem_read(0x3d79d, 0x404)) == before
    assert board.dispatch(1) == b"\x05"
    print("PASS ring wrap and atomic rejection under queue pressure")

    board = make()
    board.set_ticks(0xfffffff0)
    assert board.dispatch(0, b"\x0e\x01") == b"\x00"
    board.set_ticks(0x100000054)
    assert board.dispatch(1) == b"\x04"
    assert board.dispatch(0, b"\x0e\x01") == b"\x04"
    print("PASS native tick wrap, timeout and required resynchronization")

def verify_serialized_lab(lab, sections, symbols):
    assert lab[:16] == b"GXLAB_NOT_FLASH!"
    checksum_at = 0x54 + 4 * struct.unpack_from("<I", lab, 0x28)[0] - 4
    assert lab[checksum_at + 4:checksum_at + 14] == b"TCHDSP_V2:"
    assert zlib.crc32(lab[0x18:]) == struct.unpack_from("<I", lab, 0x14)[0]
    assert (zlib.crc32(lab[0x54:checksum_at], 0xffffffff) ^ 0xffffffff) == struct.unpack_from("<I", lab, checksum_at)[0]
    # Load only the serialized ARM section. The early-copy code must populate the reservation.
    board = Board(lab, [], symbols)
    board.cpu.reg_write(UC_ARM_REG_CPSR, 0xd3)
    board.cpu.reg_write(UC_ARM_REG_SP, 0x3a960)
    board.cpu.emu_start(0x314, 0x334, count=100000)
    assert board.cpu.reg_read(UC_ARM_REG_PC) == 0x334
    for address, data in sections:
        assert bytes(board.cpu.mem_read(address, len(data))) == data
    assert board.read32(0x12818) == 0xe2612a4f
    assert bytes(board.cpu.mem_read(0x3a458, 0x8a18)) == bytes(0x8a18)
    assert board.read32(symbols["__bridge_bss_start"]) == 0
    reservation = bytes(board.cpu.mem_read(0x4f000, 4096))
    board.call(0x127fc, (1,), stack=0x3a960)
    allocations = []
    for i in range(64):
        pointer = board.call(0x17bfc, (0x3f3f4, 1024), stack=0x3a960)
        if not pointer:
            break
        assert 0x42e70 <= pointer and pointer + 1024 <= 0x4f000
        board.cpu.mem_write(pointer, bytes([i]) * 1024)
        allocations.append(pointer)
    assert len(allocations) == 48
    assert board.call(0x17bfc, (0x3f3f4, 1024), stack=0x3a960) == 0
    assert bytes(board.cpu.mem_read(0x4f000, 4096)) == reservation
    print("PASS original heap initialization and 48 allocations through exhaustion preserve reservation")
    # Register through the stock loop, including the hook installed by reset.
    board.cpu.reg_write(UC_ARM_REG_R4, 0)
    board.cpu.reg_write(UC_ARM_REG_R5, 0x38e18)
    board.cpu.reg_write(UC_ARM_REG_SP, 0x4d000)
    board.cpu.mem_write(0x4d000, struct.pack("<IIII", 3, 4, 5, RETURN))
    board.cpu.emu_start(0xb314, RETURN, count=20000)
    assert board.cpu.reg_read(UC_ARM_REG_PC) == RETURN
    root = board.read32(0x26398)
    assert board.read32(root + 0x44c) == symbols["goxlr_eq_usb_registration"]
    board.write32(0x3fa10, 0x44000)
    board.set_ticks(1)
    assert board.dispatch(0, b"\xf7\x00") == b"\x00"
    assert bytes(board.cpu.mem_read(0x3d79d, 12)) == bytes.fromhex("f0 00 01 38 00 10 47 01 77 f7 ff ff")
    # Enter through the receive hook written by reset, preserving the stock continuation.
    for word in [0xf00001, 0x380010, 0x22ffff, 0x0177ff, 0x0000ff, 0x0010ff, 0xf7ffff]:
        board.cpu.reg_write(UC_ARM_REG_CPSR, 0x13)
        board.cpu.reg_write(UC_ARM_REG_SP, 0x4d000)
        board.cpu.mem_write(0x4d004, word.to_bytes(3, "big"))
        board.cpu.emu_start(0xa624, 0xa628, count=20000)
        assert board.cpu.reg_read(UC_ARM_REG_PC) == 0xa628
        assert board.cpu.reg_read(UC_ARM_REG_SP) == 0x4d000
    assert board.dispatch(1) == bytes.fromhex("00 f7 00 10 00 00 00")
    print("PASS serialized lab early copy, zero-clear, registration, query and receive hook")
    fallback = Board(lab, [], symbols)
    fallback.cpu.reg_write(UC_ARM_REG_CPSR, 0xd3)
    fallback.cpu.reg_write(UC_ARM_REG_SP, 0x3a960)
    fallback.cpu.reg_write(UC_ARM_REG_CP_REG, (15, 0, 0, 1, 0, 0, 0, 4))
    fallback.cpu.emu_start(0x314, 0x334, count=100000)
    assert fallback.cpu.reg_read(UC_ARM_REG_PC) == 0x334
    assert bytes(fallback.cpu.mem_read(0x4f000, 0x1000)) == bytes(0x1000)
    assert fallback.read32(0x12818) == 0xe2612805
    assert fallback.read32(0xa624) == 0xe5dd3004
    assert fallback.read32(0xb328) == 0xe8bd8038
    print("PASS data-cache-enabled fallback retains stock hooks and heap ceiling")
    print("Boot2 copying, peripheral SHI and physical cache behavior are external assumptions")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("firmware", type=Path)
    parser.add_argument("bridge_elf", type=Path)
    parser.add_argument("--lab", type=Path)
    args = parser.parse_args()
    stock = args.firmware.read_bytes()
    assert hashlib.sha256(stock).hexdigest() == STOCK_SHA
    sections, symbols = read_elf(args.bridge_elf)
    assert symbols["__bridge_bss_end"] <= 0x50000
    verify(stock, sections, symbols)
    if args.lab:
        verify_serialized_lab(args.lab.read_bytes(), sections, symbols)


if __name__ == "__main__":
    main()
