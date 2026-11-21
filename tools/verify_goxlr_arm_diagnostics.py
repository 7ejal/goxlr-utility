"""Additional serialized 910 reset/diagnostic invariants; never opens USB.

Requires the same Unicorn environment as verify_goxlr_eq_arm.py. Malformed
images exist only in memory; rejected builder inputs use an explicit lab folder.
Run with --help for the preserved-image and linked-artifact arguments.
"""
import argparse
import hashlib
import json
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path

from unicorn import UcError, UC_HOOK_MEM_WRITE
from unicorn.arm_const import (
    UC_ARM_REG_CPSR, UC_ARM_REG_LR, UC_ARM_REG_PC, UC_ARM_REG_R0,
    UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3, UC_ARM_REG_R4,
    UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7, UC_ARM_REG_R8,
    UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11, UC_ARM_REG_SP,
)

import verify_goxlr_eq_arm as bridge


BRIDGE_FILE = 0x394D0
STAGE_FILE = 0x39D8C
EXPECTED_910 = "b524ac6cf1b937e9c5d224ce9794e807e3b6561652b94dc6b26111fc9ef49744"
EXPECTED_909 = "74555b3667dcb7150930ae544bdfcf732febf7bb6a0aa9b7cfd9b513ffa3c74b"


def u32(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def literal_offset(image, instruction_offset, register):
    instruction = u32(image, instruction_offset)
    assert instruction & 0xFFFFF000 == 0xE59F0000 | (register << 12)
    return instruction_offset + 8 + (instruction & 0xFFF)


def repair_checksums(image):
    image = bytearray(image)
    struct.pack_into("<I", image, 0x3A2AC, zlib.crc32(image[0x54:0x3A2AC], 0xFFFFFFFF) ^ 0xFFFFFFFF)
    struct.pack_into("<I", image, 0x14, zlib.crc32(image[0x18:]))
    return bytes(image)


def dispatch(board, metric, cpsr=0x13, payload=None, command=2):
    payload = metric.to_bytes(2, "little") if payload is None else payload
    board.cpu.mem_write(0x45000, payload or b"\0")
    board.write32(0x4D000, len(payload))
    preserved = [UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7,
                 UC_ARM_REG_R8, UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11]
    for i, register in enumerate(preserved):
        board.cpu.reg_write(register, 0xA0000000 + i)
    board.cpu.reg_write(UC_ARM_REG_CPSR, cpsr)
    board.cpu.reg_write(UC_ARM_REG_SP, 0x4D000)
    board.cpu.reg_write(UC_ARM_REG_LR, bridge.RETURN)
    for register, value in zip([UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3],
                               [0, 0, command, 0x45000]):
        board.cpu.reg_write(register, value)
    board.cpu.emu_start(board.symbols["goxlr_eq_usb_dispatch"], bridge.RETURN, count=100000)
    assert board.cpu.reg_read(UC_ARM_REG_PC) == bridge.RETURN
    assert board.cpu.reg_read(UC_ARM_REG_SP) == 0x4D000
    assert board.cpu.reg_read(UC_ARM_REG_CPSR) & 0xFF == cpsr & 0xFF
    assert all(board.cpu.reg_read(register) == 0xA0000000 + i for i, register in enumerate(preserved))
    status, response = board.replies[-1]
    assert status == 0
    return response


def reply(metric, value):
    return b"\0" + metric.to_bytes(2, "little") + value.to_bytes(4, "little")


def run(args):
    original = args.original.read_bytes()
    image = args.image.read_bytes()
    baseline = args.baseline909.read_bytes()
    assert hashlib.sha256(original).hexdigest() == bridge.STOCK_SHA
    assert hashlib.sha256(image).hexdigest() == args.expected_sha256.lower()
    assert hashlib.sha256(baseline).hexdigest() == EXPECTED_909
    sections, symbols = bridge.read_elf(args.elf)
    load_bytes = symbols["__bridge_load_end"] - 0x4F000
    assert symbols["__bridge_bss_end"] <= symbols["__reset_helper_start"] == 0x4F800
    helper_literal = literal_offset(image, STAGE_FILE + 0x14, 1)
    end_literal = literal_offset(image, STAGE_FILE + 0x18, 2)
    assert u32(image, helper_literal) == 0x4F800
    helper_end = u32(image, end_literal)
    stage_bytes = helper_end - 0x4F800
    assert 0 < stage_bytes <= 864 and helper_end <= 0x4FFC0
    original_tail = original[BRIDGE_FILE + 1000:BRIDGE_FILE + load_bytes]
    assert original_tail and any(original_tail)
    tail_offset = STAGE_FILE + stage_bytes - len(original_tail)
    assert image[tail_offset:STAGE_FILE + stage_bytes] == original_tail
    assert len(image) == len(baseline) == 640044
    assert all(u32(image, offset) == 910 for offset in (0x1C, 0x2C, 0x3A2BE))
    dsp_diff = [i for i in range(0x3A2B0, len(image)) if image[i] != baseline[i]]
    assert all(0x3A2BE <= i < 0x3A2C2 for i in dsp_diff)

    board = bridge.verify_serialized_lab(image, sections, symbols, same_size=True, original=original)
    cases = ["serialized reset restores exact nonzero initialized tail and zero carrier",
             "relocated helper, worker-address bounds and preserved DSP section"]
    protected = [(0x3ED64, 0x3ED68), (0x3ED88, 0x3ED8C),
                 (0x38DD4, 0x38DE4), (0x3D79D, 0x3DBA1)]
    def protect(_cpu, _access, address, size, _value, _context):
        assert not any(address < end and address + size > start for start, end in protected)

    board.write32(0x38DD4, 0x3D79D)
    board.write32(0x38DD8, 0x404)
    board.write32(0x38DDC, 1020)
    board.write32(0x38DE0, 1000)
    board.set_ticks(0xFFFFFFF0)
    assert board.dispatch(0, b"\xf6\0") == b"\0"
    state_start = symbols["live_readback"]
    state_bytes = symbols["saved_cpsr"] - state_start
    state_before = bytes(board.cpu.mem_read(state_start, state_bytes))
    ring_before = bytes(board.cpu.mem_read(0x38DD4, 16))
    ring_data_before = bytes(board.cpu.mem_read(0x3D79D, 0x404))
    board.write32(0x3ED64, 0xFFFFFFFF)
    board.write32(0x3ED88, 0x80000000)
    board.set_ticks(0x100000020)
    hook = board.cpu.hook_add(UC_HOOK_MEM_WRITE, protect)
    for cpsr in (0x13, 0x53, 0x93, 0xD3):
        for metric, value in {1:0xFFFFFFFF, 2:0x80000000, 3:0x404, 4:0x3D79D, 5:32, 6:0x20}.items():
            assert dispatch(board, metric, cpsr) == reply(metric, value)
            assert bytes(board.cpu.mem_read(state_start, state_bytes)) == state_before
            assert bytes(board.cpu.mem_read(0x38DD4, 16)) == ring_before
            assert bytes(board.cpu.mem_read(0x3D79D, 0x404)) == ring_data_before
            cases.append(f"pending EQ, metric{metric}, CPSR{cpsr:#x}, preserved IRQ/registers/ring/state")
    for metric in (0, 7, 8, 255, 256, 32768, 65535):
        assert dispatch(board, metric) == b"\3"
        cases.append(f"reject unknown metric{metric}")
    for payload in (b"", b"\1", b"\1\0\0", bytes(16)):
        assert dispatch(board, 0, payload=payload) == b"\3"
        cases.append(f"reject malformed payload length{len(payload)}")
    board.cpu.hook_del(hook)
    for ticks in (0, 1, 0xFFFFFFFF, 0x100000000, 0x100000001, 0xFFFFFFFFFFFFFFFF):
        board.set_ticks(ticks)
        assert dispatch(board, 6) == reply(6, ticks & 0xFFFFFFFF)
        cases.append(f"unsigned tick low word {ticks:#x}")
    board.set_ticks(0x100000020)
    assert board.dispatch(1) == b"\1"
    raw = -12
    pieces = [(raw >> shift) & 127 for shift in (21, 14, 7, 0)]
    for word in [0xF00001, 0x380010, 0x22FFFF, 0x0176FF,
                 pieces[0] << 16 | pieces[1] << 8 | 255,
                 pieces[2] << 16 | pieces[3] << 8 | 255, 0xF7FFFF]:
        board.call("goxlr_eq_observe_shi_word", (word,))
    assert board.dispatch(1) == b"\0\xf6\0" + raw.to_bytes(4, "little", signed=True)
    cases.append("pending EQ reply survives diagnostics and tick wrap")

    # Repair both checksums so these cases exercise reset/preservation gates, not only CRC checks.
    mutations = []
    bad = bytearray(image)
    bad[tail_offset + len(original_tail) - 1] ^= 1
    mutations.append(("changed original-tail byte", repair_checksums(bad)))
    for name, offset, value in (("helper end reaches worker region", end_literal, 0x50004),
                                 ("helper copied to wrong address", helper_literal, 0x4FFC0)):
        bad = bytearray(image)
        struct.pack_into("<I", bad, offset, value)
        mutations.append((name, repair_checksums(bad)))
    for name, bad in mutations:
        try:
            bridge.verify_serialized_lab(bad, sections, symbols, same_size=True, original=original)
        except (AssertionError, UcError):
            cases.append(f"reject checksum-valid mutation: {name}")
        else:
            raise AssertionError(f"Accepted malformed serialized reset: {name}")

    if args.builder:
        scratch_parent = args.output.parent.resolve()
        with tempfile.TemporaryDirectory(prefix="goxlr-diag-inputs-", dir=scratch_parent) as folder:
            folder = Path(folder).resolve()
            assert folder.is_relative_to(scratch_parent)
            stage = args.stage.read_bytes()
            bad_tail = bytearray(stage)
            bad_tail[-1] ^= 1
            invalid_stages = [("mismatched-tail", bytes(bad_tail))]
            for name, offset, register, value in (("wrong-source", 0x10, 0, 0x39F3C),
                                                   ("wrong-helper-start", 0x14, 1, 0x4FFC0),
                                                   ("wrong-helper-end", 0x18, 2, 0x50004)):
                bad = bytearray(stage)
                struct.pack_into("<I", bad, literal_offset(stage, offset, register), value)
                invalid_stages.append((name, bytes(bad)))
            for name, bad_stage in invalid_stages:
                stage_path = folder / (name + ".bin")
                stage_path.write_bytes(bad_stage)
                destination = folder / (name + "-REJECTED-DO-NOT-FLASH.gxlab")
                result = subprocess.run([str(args.builder), "--build", "910", str(args.original),
                                         str(args.bridge), str(stage_path), str(destination)],
                                        capture_output=True, text=True, timeout=15)
                assert result.returncode != 0 and not destination.exists(), name
                cases.append(f"actual builder rejects {name} before publishing output")
            destination = folder / "EXACT-REBUILD.bin"
            result = subprocess.run([str(args.builder), "--build", "910", str(args.original),
                                     str(args.bridge), str(args.stage), str(destination)],
                                    capture_output=True, text=True, timeout=15)
            assert result.returncode == 0, result.stderr
            assert destination.read_bytes() == image
            cases.append("guarded builder reproduces exact preserved 910 candidate bytes")
    if args.regression909:
        for filename in ("eq-bridge.bin", "eq-same-size-stage.bin"):
            assert (args.regression909 / filename).read_bytes() == (args.original909 / filename).read_bytes()
            cases.append(f"909 byte-exact regression: {filename}")
    report = {"image_sha256": hashlib.sha256(image).hexdigest(), "elf_sha256": hashlib.sha256(args.elf.read_bytes()).hexdigest(),
              "load_bytes": load_bytes, "initialized_tail_bytes": len(original_tail),
              "stage_bytes": stage_bytes, "helper_end": hex(helper_end), "case_count": len(cases), "cases": cases,
              "limits": "Actual serialized ARM bytes execute with synthetic RAM, clock object, ring/DSP replies and USB completion capture. Cache-maintenance opcodes execute but cache behavior, real counters/rates, hardware boot and recovery remain physically unverified. Negative images stay in memory; no hardware access."}
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"PASS {len(cases)} additional serialized diagnostics/reset cases; no physical metrics measured")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original", "image", "baseline909", "elf", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--expected-sha256", default=EXPECTED_910)
    parser.add_argument("--builder", type=Path)
    parser.add_argument("--bridge", type=Path)
    parser.add_argument("--stage", type=Path)
    parser.add_argument("--regression909", type=Path)
    parser.add_argument("--original909", type=Path)
    args = parser.parse_args()
    if args.builder and not (args.bridge and args.stage):
        parser.error("--builder needs --bridge and --stage")
    if args.regression909 and not args.original909:
        parser.error("--regression909 needs --original909")
    run(args)


if __name__ == "__main__":
    main()
