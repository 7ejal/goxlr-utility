.syntax unified
.arch armv5te
.arm
.text
.balign 4
.global goxlr_eq_lock
.type goxlr_eq_lock, %function
goxlr_eq_lock:
    mrs r1, cpsr
    orr r0, r1, #0xc0
    msr cpsr_c, r0
    ldr r0, =saved_cpsr
    str r1, [r0]
    bx lr
.size goxlr_eq_lock, .-goxlr_eq_lock

.global goxlr_eq_unlock
.type goxlr_eq_unlock, %function
goxlr_eq_unlock:
    ldr r0, =saved_cpsr
    ldr r1, [r0]
    msr cpsr_c, r1
    bx lr
.size goxlr_eq_unlock, .-goxlr_eq_unlock

/* Stock wrapper returns a 64-bit eCos tick count in r0:r1.
 * The bridge uses its low word with unsigned timeout subtraction. */
.global goxlr_eq_now_ticks
.type goxlr_eq_now_ticks, %function
goxlr_eq_now_ticks:
    ldr pc, =0x10e80
.size goxlr_eq_now_ticks, .-goxlr_eq_now_ticks
.ltorg

.bss
.balign 4
.type saved_cpsr, %object
saved_cpsr:
    .space 4
.size saved_cpsr, .-saved_cpsr
