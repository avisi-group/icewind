use {common::ktest, core::arch::global_asm};

global_asm!(
    r#"
        .global record_safepoint_raw
        record_safepoint_raw:
            pushf
	        pop 64(%rdi)

	        movq %rbx, (%rdi)
	        pop %rcx
	        movq %rsp, 8(%rdi)
	        push %rcx
	        movq %rbp, 16(%rdi)
	        movq %r12, 24(%rdi)
	        movq %r13, 32(%rdi)
	        movq %r14, 40(%rdi)
	        movq %r15, 48(%rdi)
	        movq %rcx, 56(%rdi)

	        xor %eax, %eax
	        ret

        .global restore_safepoint_raw
        restore_safepoint_raw:
            movq (%rdi), %rbx
            movq 8(%rdi), %rsp
            movq 16(%rdi), %rbp
            movq 24(%rdi), %r12
            movq 32(%rdi), %r13
            movq 40(%rdi), %r14
            movq 48(%rdi), %r15

            push 64(%rdi)
            popf

            mov %rsi, %rax

            push 56(%rdi)	// RIP
            retq

        .global interrupt_restore_safepoint_raw
        interrupt_restore_safepoint_raw:
            mov %rsi, %rax

            movq (%rdi), %rbx
            movq 8(%rdi), %rsp
            movq 16(%rdi), %rbp
            movq 24(%rdi), %r12
            movq 32(%rdi), %r13
            movq 40(%rdi), %r14
            movq 48(%rdi), %r15

            // We should return into ring0
            push $0x10		// SS
            push 8(%rdi)	// RSP
            push 64(%rdi)	// RFLAGS
            push $0x08		// CS
            push 56(%rdi)	// RIP

            iretq
    "#,
    options(att_syntax)
);

static mut SAFEPOINT: SafepointContext = SafepointContext {
    rbx: 0,
    rsp: 0,
    rbp: 0,
    r12: 0,
    r13: 0,
    r14: 0,
    r15: 0,
    rip: 0,
    rflags: 0,
};

#[repr(C)]
pub struct SafepointContext {
    rbx: u64,
    rsp: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rip: u64,
    rflags: u64,
}

unsafe extern "C" {
    fn record_safepoint_raw(safepoint: *mut SafepointContext) -> u32;
    fn restore_safepoint_raw(safepoint: *const SafepointContext, return_value: u32);
    fn interrupt_restore_safepoint_raw(safepoint: *const SafepointContext, return_value: u32);
}

#[inline(never)]
pub fn record_safepoint() -> u32 {
    unsafe { record_safepoint_raw(&mut SAFEPOINT as *mut _) }
}

pub fn restore_safepoint(return_value: u32) -> ! {
    unsafe { restore_safepoint_raw(&SAFEPOINT as *const _, return_value) }
    unreachable!()
}

pub fn interrupt_restore_safepoint(return_value: u32) -> ! {
    unsafe { interrupt_restore_safepoint_raw(&SAFEPOINT as *const _, return_value) }
    unreachable!()
}

#[ktest]
fn safepoint_basic() {
    let result = record_safepoint();

    if result == 0 {
        restore_safepoint(1);
    }
}
