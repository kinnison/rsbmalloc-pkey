#![allow(deref_nullptr)]

use std::{
    panic::panic_any,
    ptr::{null, null_mut},
};

struct SegmentationViolation;

#[inline(never)]
fn actual_panic() {
    panic_any(SegmentationViolation)
}

#[inline(never)]
fn segvpanic() -> ! {
    // We can panic here because the signal has "returned"
    // so unwinding should be possible
    actual_panic();
    loop {}
}

unsafe extern "C" fn sigaction_handler(
    _signum: libc::c_int,
    _info: *mut libc::siginfo_t,
    data: *mut libc::c_void,
) {
    let context = &mut *(data as *mut libc::ucontext_t);
    let gregs = &mut context.uc_mcontext.gregs;
    let mut rsp = gregs[libc::REG_RSP as usize] as *mut u64;
    let rip = gregs[libc::REG_RIP as usize] as u64;

    // We want to simulate the effect of a CALL to segvpanic
    // as such, we want to decrement the stack pointer (descending stack)
    rsp = rsp.sub(1);
    gregs[libc::REG_RSP as usize] = rsp as i64;
    // And write into that the old return address
    rsp.write(rip);
    // Before replacing the EIP of the faulting instruction with the start
    // of segvpanic
    gregs[libc::REG_RIP as usize] = (segvpanic as *mut libc::c_void) as i64;
    // Now we return which allows the kernel to let us try again our faulting
    // instruction which is actually a call to segvpanic
}

unsafe fn setup_signals() {
    let mut action: libc::sigaction = std::mem::zeroed();
    action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
    action.sa_sigaction = sigaction_handler as libc::sighandler_t;
    libc::sigaction(libc::SIGSEGV, &action, null_mut());
}

fn danger_func() -> u64 {
    unsafe { *null::<u64>() }
}

fn main() {
    println!("Hello, let's try and segfault ourselves...");
    unsafe { setup_signals() }
    println!("We've set up our signal handler, now let's segfault");
    std::hint::black_box(danger_func());
}
