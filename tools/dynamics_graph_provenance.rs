//! Exploratory original graph slice; synthetic allocation, not a complete boot proof.
use dsp56300_core::{MemSpace, reg};
use dsp56300_emu::{core::DspState, jit::JitEngine};

fn original(
    state: &mut DspState,
    jit: &mut JitEngine,
    entry: u32,
    stop: u32,
    next: &mut u32,
) -> u32 {
    state.registers[reg::R7] = 0x1800;
    state.registers[reg::R0] = state.read_memory(MemSpace::X, 0x419e41);
    state.stack_push(0x1f, 0);
    state.pc = entry;
    let mut count = 0;
    let mut bindings = 0;
    while state.pc != stop {
        if state.pc == 0x409820 {
            let root = state.read_memory(MemSpace::X, 0x419e41);
            assert_ne!(root, 0);
            assert_eq!(
                state.registers[reg::R4],
                state.read_memory(MemSpace::X, root + 243)
            );
            assert_eq!(
                state.registers[reg::R1],
                state.read_memory(MemSpace::X, root + 244)
            );
            bindings += 1;
            println!(
                "original compressor table binding: object {:06X}, log {:06X}, exp {:06X}",
                state.registers[reg::R0],
                state.registers[reg::R4],
                state.registers[reg::R1]
            );
        }
        if state.pc == 0x409e6d {
            let words = state.registers[reg::A1];
            let mode = state.registers[reg::B1];
            let stack = state.registers[reg::R7];
            let class = state.read_memory(MemSpace::X, stack - 1);
            let id = if class == 3 {
                state.read_memory(MemSpace::X, stack - 2)
            } else {
                0
            };
            assert!(
                words > 0 && words < 0x10000,
                "Invalid allocation {words:X} at {entry:X}, PC {:X}",
                state.pc
            );
            let cached = if id > 0 {
                state.read_memory(MemSpace::X, 0x351d4 + 2 * id)
            } else {
                0
            };
            let pointer = if cached != 0 {
                cached
            } else {
                let alignment = words.next_power_of_two().max(64);
                *next = (*next + alignment - 1) & !(alignment - 1);
                let pointer = *next;
                *next += alignment;
                assert!(*next < 0x30000);
                for i in 0..words {
                    state.write_memory(MemSpace::X, pointer + i, 0);
                    state.write_memory(MemSpace::Y, pointer + i, 0);
                }
                pointer
            };
            let modifier = if mode == 0 {
                words.next_power_of_two() - 1
            } else {
                0xffffff
            };
            state.registers[reg::R0] = pointer;
            state.registers[reg::M0] = modifier;
            if id > 0 {
                state.write_memory(MemSpace::X, 0x351d4 + 2 * id, pointer);
                state.write_memory(MemSpace::X, 0x351d5 + 2 * id, modifier);
                println!("allocator stub shared ID{id} words{words} mode{mode} -> {pointer:06X}");
            }
            state.pc = 0x409e50;
        }
        state.execute_one(jit);
        count += 1;
        assert!(
            count < 5000000,
            "Bounded graph slice {entry:X} stopped at PC {:X}",
            state.pc
        );
    }
    println!("original slice {entry:06X} -> {stop:06X}: {count} instructions");
    bindings
}

pub fn run(state: &mut DspState, jit: &mut JitEngine, build: u32) {
    println!(
        "EXPLORATORY build{build}: allocator is synthetic; graph slice stops before complete startup; no physical performance or LUT producer proof"
    );
    for id in [9, 10, 11, 13] {
        let address = 0x351d4 + 2 * id;
        println!(
            "build{build} boot shared resource{id}: pointer{:06X} modifier{:06X}",
            state.read_memory(MemSpace::X, address),
            state.read_memory(MemSpace::X, address + 1)
        );
    }
    println!("boot root {:06X}", state.read_memory(MemSpace::X, 0x419e41));
    let mut next = 0x10000;
    assert_eq!(original(state, jit, 0x404739, 0x1f, &mut next), 0);
    let root = state.read_memory(MemSpace::X, 0x419e41);
    assert_eq!(root, 0x10000, "Original root allocation changed");
    for (offset, id) in [(243, 10), (244, 11)] {
        let pointer = state.read_memory(MemSpace::X, root + offset);
        assert_ne!(pointer, 0, "Original graph table was not allocated");
        assert_eq!(pointer, state.read_memory(MemSpace::X, 0x351d4 + 2 * id));
        assert_eq!(state.read_memory(MemSpace::X, 0x351d5 + 2 * id), 0xffffff);
    }
    assert_ne!(
        state.read_memory(MemSpace::X, root + 243),
        state.read_memory(MemSpace::X, root + 244)
    );
    println!(
        "PASS original constructor connects root+243/+244 to distinct shared resources 10/11; allocation addresses remain synthetic"
    );
    println!(
        "constructed root {root:06X}, LUTs {:06X} {:06X}",
        state.read_memory(MemSpace::X, root + 243),
        state.read_memory(MemSpace::X, root + 244)
    );
    assert_eq!(original(state, jit, 0x404969, 0x4058bd, &mut next), 3);
    println!(
        "PASS three original compressor bindings receive the root's shared table pointers; table generation remains unproved"
    );
    for offset in [243, 244] {
        let pointer = state.read_memory(MemSpace::X, root + offset);
        println!(
            "root+{offset} data {:?}",
            (0..33)
                .map(|i| state.read_memory(MemSpace::X, pointer + i))
                .collect::<Vec<_>>()
        );
    }
    println!(
        "LIMITATION: graph table contents are observations only; no assertion of generated LUTs, upstream audio identity, complete boot or hardware behavior"
    );
}
