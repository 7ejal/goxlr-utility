/* Lab-only expanded-section loading hypothesis. Entry replaces VA 0x314.
 * Copy completes before the stock zero-clear and constructor calls. */
.syntax unified
.arch armv5te
.arm
.text
.balign 4
.global stage_reset
.type stage_reset, %function
stage_reset:
    push {r0-r3, r12, lr}
    mrs r12, cpsr
    mrc p15, 0, r3, c1, c0, 0
    tst r3, #4
    bne resume_stock
    ldr r0, =stage_payload
    ldr r1, =0x4f000
    ldr r2, =__bridge_load_end
1:
    ldr r3, [r0], #4
    str r3, [r1], #4
    cmp r1, r2
    blo 1b
    ldr r1, =__bridge_bss_start
    ldr r2, =__bridge_bss_end
    mov r3, #0
2:
    str r3, [r1], #4
    cmp r1, r2
    blo 2b
    ldr r0, =0x12818
    ldr r1, =0xe2612a4f
    str r1, [r0]
    ldr r0, =0xa624
    ldr r1, =observer_branch
    str r1, [r0]
    ldr r0, =0xb328
    ldr r1, =registration_branch
    str r1, [r0]
    mov r3, #0
    mcr p15, 0, r3, c7, c10, 4
    mcr p15, 0, r3, c7, c5, 0
    nop
    nop
resume_stock:
    msr cpsr_f, r12
    pop {r0-r3, r12, lr}
    ldr r1, =0x3a458
    ldr pc, =0x318
    .ltorg
.size stage_reset, .-stage_reset
.balign 4
stage_payload:
    .incbin "eq-bridge.bin"
.global stage_end
stage_end:
