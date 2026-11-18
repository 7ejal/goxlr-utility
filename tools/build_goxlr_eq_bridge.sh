#!/usr/bin/env bash
set -euo pipefail

# Builds the complete experimental bridge ELF in the proposed 4 KiB region.
# This does not modify or produce a firmware image.
toolchain_dir="${1:?Pass the ARM toolchain bin directory}"
output_dir="${2:?Pass a dedicated build directory}"
timeout_ticks="${3:?Pass a positive timeout in native firmware ticks}"
source_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

[[ "$timeout_ticks" =~ ^[0-9]+$ ]] || { echo "Invalid timeout" >&2; exit 1; }
[[ -x "$toolchain_dir/arm-none-eabi-gcc" ]] || { echo "ARM compiler unavailable" >&2; exit 1; }
toolchain_dir="$(cd -- "$toolchain_dir" && pwd)"
mkdir -p -- "$output_dir"
output_dir="$(cd -- "$output_dir" && pwd)"
[[ "$output_dir" != "$source_dir" ]] || { echo "Choose a separate build directory" >&2; exit 1; }

common=(-B "$toolchain_dir/" -march=armv5te -ffreestanding -fno-builtin -Wall -Wextra -Werror)
for source in goxlr_eq_readback_bridge goxlr_eq_board; do
    "$toolchain_dir/arm-none-eabi-gcc" "${common[@]}" -mthumb -mthumb-interwork -Oz \
        -fno-jump-tables -DGOXLR_EQ_FIRMWARE -DGOXLR_EQ_TIMEOUT_TICKS="$timeout_ticks" \
        -c "$source_dir/$source.c" -o "$output_dir/$source.o"
done
for source in goxlr_eq_arm_hook goxlr_eq_board; do
    "$toolchain_dir/arm-none-eabi-gcc" "${common[@]}" -marm \
        -c "$source_dir/$source.S" -o "$output_dir/$source-asm.o"
done
"$toolchain_dir/arm-none-eabi-ld" -T "$source_dir/goxlr_eq_bridge.ld" \
    -Map="$output_dir/eq-bridge.map" -o "$output_dir/eq-bridge.elf" \
    "$output_dir/goxlr_eq_readback_bridge.o" "$output_dir/goxlr_eq_board.o" \
    "$output_dir/goxlr_eq_arm_hook-asm.o" "$output_dir/goxlr_eq_board-asm.o"
"$toolchain_dir/arm-none-eabi-objcopy" -O binary "$output_dir/eq-bridge.elf" "$output_dir/eq-bridge.bin"
"$toolchain_dir/arm-none-eabi-size" "$output_dir/eq-bridge.elf"
"$toolchain_dir/arm-none-eabi-nm" -n "$output_dir/eq-bridge.elf"

# Build the early-copy experiment from the same linked bytes and symbols.
observer="$("$toolchain_dir/arm-none-eabi-nm" "$output_dir/eq-bridge.elf" | awk '$3 == "goxlr_eq_shi_observer_hook" { print "0x" $1 }')"
registration="$("$toolchain_dir/arm-none-eabi-nm" "$output_dir/eq-bridge.elf" | awk '$3 == "goxlr_eq_usb_registration_hook" { print "0x" $1 }')"
observer_branch=$((0xea000000 | (((observer - 0xa624 - 8) >> 2) & 0xffffff)))
registration_branch=$((0xea000000 | (((registration - 0xb328 - 8) >> 2) & 0xffffff)))
(
    cd -- "$output_dir"
    "$toolchain_dir/arm-none-eabi-gcc" "${common[@]}" -marm \
        -c "$source_dir/goxlr_eq_stage_reset.S" -o eq-stage.o
    "$toolchain_dir/arm-none-eabi-ld" --just-symbols=eq-bridge.elf \
        --defsym="observer_branch=$observer_branch" \
        --defsym="registration_branch=$registration_branch" \
        -Ttext=0x3a458 -e stage_reset eq-stage.o -o eq-stage.elf
    "$toolchain_dir/arm-none-eabi-objcopy" -O binary -j .text eq-stage.elf eq-stage.bin
)
