#![no_std]
#![no_main]
#![feature(panic_info_message)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::panic::PanicInfo;
use interface::UartDomain;
use libsyscall::{KTaskShim, Syscall};
use rref::SharedHeap;

#[no_mangle]
fn main(
    sys: Box<dyn Syscall>,
    domain_id: u64,
    shared_heap: Box<dyn SharedHeap>,
    ktask_shim: Box<dyn KTaskShim>,
) -> Arc<dyn UartDomain> {
    // init rref's shared heap
    rref::init(shared_heap, domain_id);
    // init libsyscall
    libsyscall::init(sys, ktask_shim);
    // activate the domain
    interface::activate_domain();
    // call the real uart driver
    uart::main()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(p) = info.location() {
        libsyscall::println!(
            "line {}, file {}: {}",
            p.line(),
            p.file(),
            info.message().unwrap()
        );
    } else {
        libsyscall::println!("no location information available");
    }
    interface::deactivate_domain();
    libsyscall::backtrace();
    loop {}
}
