/* Same-size experiment. The source spans are owned initialized data.
 * Restore their zero values before the stock hardware initialization runs. */
.syntax unified
.arch armv5te
.arm
.text
.balign 4
.global same_size_stage
.type same_size_stage, %function
.equ bridge_source, 0x3967c
.equ stage_source, 0x39f38
same_size_stage:
    push {r0-r5, r12, lr}
    mrs r12, cpsr
    orr r3, r12, #0xc0
    msr cpsr_c, r3
    ldr r0, =stage_source
    ldr r1, =same_size_stage
    ldr r2, =same_size_end
1:
    ldr r3, [r0], #4
    str r3, [r1], #4
    cmp r1, r2
    blo 1b
    ldr r0, =bridge_source
    ldr r1, =0x4f000
    ldr r2, =__bridge_load_end
2:
    ldr r3, [r0], #4
    str r3, [r1], #4
    cmp r1, r2
    blo 2b
    ldr r0, =__bridge_bss_start
    ldr r1, =__bridge_bss_end
    mov r3, #0
3:
    str r3, [r0], #4
    cmp r0, r1
    blo 3b
    ldr r0, =0x4f000
    ldr r1, =same_size_end
4:
    mcr p15, 0, r0, c7, c10, 1
    add r0, r0, #16
    cmp r0, r1
    blo 4b
    mov r3, #0
    mcr p15, 0, r3, c7, c10, 4
    mcr p15, 0, r3, c7, c5, 0
    nop
    nop
    ldr pc, =relocated_cleanup
relocated_cleanup:
    ldr r0, =bridge_source
    ldr r1, =bridge_source + __bridge_load_end - 0x4f000
    mov r3, #0
5:
    str r3, [r0]
    mcr p15, 0, r0, c7, c10, 1
    add r0, r0, #4
    cmp r0, r1
    blo 5b
#ifdef GOXLR_ARM_DIAGNOSTICS
    ldr r0, =bridge_source + 1000
    ldr r1, =original_source_tail
    ldr r2, =original_source_tail_end
7:
    ldr r3, [r1], #4
    str r3, [r0]
    mcr p15, 0, r0, c7, c10, 1
    add r0, r0, #4
    cmp r1, r2
    blo 7b
    mov r3, #0
#endif
    ldr r0, =stage_source
    ldr r1, =stage_restore_end
6:
    str r3, [r0]
    mcr p15, 0, r0, c7, c10, 1
    add r0, r0, #4
    cmp r0, r1
    blo 6b
    ldr r0, =0x12818
    ldr r1, =0xe2612a4f
    str r1, [r0]
    mcr p15, 0, r0, c7, c10, 1
    ldr r0, =0xa624
    ldr r1, =observer_branch
    str r1, [r0]
    mcr p15, 0, r0, c7, c10, 1
    ldr r0, =0xb328
    ldr r1, =registration_branch
    str r1, [r0]
    mcr p15, 0, r0, c7, c10, 1
    ldr r0, =0x244
    ldr r1, =0xeb003c51
    str r1, [r0]
    mcr p15, 0, r0, c7, c10, 1
    mov r3, #0
    mcr p15, 0, r3, c7, c10, 4
    mcr p15, 0, r3, c7, c5, 0
    nop
    nop
    msr cpsr_fc, r12
    pop {r0-r5, r12, lr}
    ldr pc, =0x244
    .ltorg
#ifdef GOXLR_ARM_DIAGNOSTICS
.global original_source_tail
original_source_tail:
    .incbin "eq-original-tail.bin"
.global original_source_tail_end
original_source_tail_end:
#endif
.global same_size_end
same_size_end:
.equ stage_restore_end, stage_source + same_size_end - same_size_stage
.size same_size_stage, .-same_size_stage
