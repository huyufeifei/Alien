#![feature(panic_info_message)]
#![feature(naked_functions)]
#![no_std]
#![no_main]
mod panic;

#[macro_use]
extern crate platform;
#[macro_use]
extern crate log;
extern crate alloc;
mod bus;
mod domain;
mod domain_helper;
mod domain_loader;
mod domain_proxy;
mod task;
mod timer;
mod trap;

use core::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

use basic::time::read_time_ms;
use interface::DomainType;
use rref::RRef;

use crate::domain_helper::query_domain;

/// 多核启动标志
static STARTED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
fn main(hart_id: usize) {
    if STARTED
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        println!("Boot hart {}", hart_id);
        let machine_info = platform::platform_machine_info();
        println!("{:#?}", machine_info);
        mem::init_memory_system(machine_info.memory.end, true);
        trap::init_trap_subsystem();
        arch::allow_access_user_memory();
        bus::init_with_dtb().unwrap();
        domain::load_domains();
        STARTED.store(false, Ordering::Relaxed);
    } else {
        while STARTED.load(Ordering::Relaxed) {
            spin_loop();
        }
        mem::init_memory_system(0, false);
        arch::allow_access_user_memory();
        trap::init_trap_subsystem();
        println!("hart {} start", arch::hart_id());
    }
    timer::set_next_trigger();

    println!("===============START BLOCK RESTART TEST================");
    let blk = query_domain("shadow_blk-1").unwrap();
    if let DomainType::ShadowBlockDomain(b) = blk {
        let mut buf = RRef::new([9u8; 512]);
        b.write_block(0, &buf).unwrap();
        buf = b.read_block(0, buf).unwrap();
        info!("{:?}", buf);
        println!("================START BLOCK SPEED TEST=================");
        // let capc = b.get_capacity().unwrap() / 512;
        // let t = read_time_ms();
        // let scale = 10;
        // for _ in 0..scale {
        //     for i in 0..capc {
        //         buf = b.read_block(i as _, buf).unwrap();
        //     }
        // }
        // let t2 = read_time_ms();
        // println!(
        //     "read {} Bytes, used {} ms, speed: {}MB/s",
        //     capc * 512 * scale,
        //     t2 - t,
        //     (scale * capc * 512 * 1000) as f64 / (t2 - t) as f64 / 1024.0 / 1024.0
        // );
    }
    println!("====================END BLOCK TEST=====================");

    println!("Begin run task...");
    task::run_task();
    platform::system_shutdown();
}
