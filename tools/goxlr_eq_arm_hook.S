/* Proposed 907 A32 observer at 0xA624. Link only after executable placement,
 * scheduler ownership, and the observer implementation have been verified. */
.syntax unified
.arch armv5te
.arm
.text
.balign 4
.global goxlr_eq_shi_observer_hook
.type goxlr_eq_shi_observer_hook, %function
goxlr_eq_shi_observer_hook:
    push {r0-r5, r12, lr}
    mrs r4, cpsr
    mov r5, sp
    bic sp, sp, #7
    ldrb r0, [r5, #36]
    ldrb r1, [r5, #37]
    ldrb r2, [r5, #38]
    orr r0, r2, r0, lsl #16
    orr r0, r0, r1, lsl #8
    bl goxlr_eq_observe_shi_word
    msr cpsr_f, r4
    mov sp, r5
    pop {r0-r5, r12, lr}
    /* Replay the displaced instruction and resume stock status handling. */
    ldrb r3, [sp, #4]
    ldr pc, =0xA628
.size goxlr_eq_shi_observer_hook, .-goxlr_eq_shi_observer_hook

/* Proposed replacement for the return at 0xB328, after all 24 stock entries
 * have been registered. The dispatcher owns the writable link in the node. */
.global goxlr_eq_usb_registration_hook
.type goxlr_eq_usb_registration_hook, %function
goxlr_eq_usb_registration_hook:
    push {r0-r4, r12, lr}
    mrs r4, cpsr
    sub sp, sp, #4
    ldr r0, =goxlr_eq_usb_registration
    ldr r12, =0x26384
    blx r12
    msr cpsr_f, r4
    add sp, sp, #4
    pop {r0-r4, r12, lr}
    pop {r3-r5, pc}
.size goxlr_eq_usb_registration_hook, .-goxlr_eq_usb_registration_hook

.global goxlr_eq_usb_reply
.type goxlr_eq_usb_reply, %function
goxlr_eq_usb_reply:
    ldr pc, =0x266C8
.size goxlr_eq_usb_reply, .-goxlr_eq_usb_reply
.ltorg

.data
.balign 4
.type goxlr_eq_usb_registration, %object
goxlr_eq_usb_registration:
    .word 0, 0x819, goxlr_eq_usb_dispatch, 0
.size goxlr_eq_usb_registration, .-goxlr_eq_usb_registration
