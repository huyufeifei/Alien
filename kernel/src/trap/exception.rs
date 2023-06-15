use riscv::register::scause::{Exception, Trap};
use rvfs::file::vfs_read_file;

use crate::arch::interrupt_enable;
use crate::error::{AlienError, AlienResult};
use crate::fs::vfs::VfsProvider;
use crate::syscall;
use crate::task::{current_process, current_trap_frame, do_exit};

pub fn syscall_exception_handler() {
    // enable interrupt
    interrupt_enable();
    // jump to next instruction anyway
    let mut cx = current_trap_frame();
    cx.update_sepc();
    // get system call return value
    let parameters = cx.parameters();
    let result = syscall::do_syscall(parameters[0], &parameters[1..]);
    // cx is changed during sys_exec, so we have to call it again
    cx = current_trap_frame();
    cx.update_res(result as usize);
}

/// the solution for page fault
pub fn page_exception_handler(trap: Trap, addr: usize) -> AlienResult<()> {
    match trap {
        Trap::Exception(Exception::LoadPageFault) => load_page_fault_exception_handler(addr)?,
        Trap::Exception(Exception::StorePageFault) => store_page_fault_exception_handler(addr)?,
        _ => {
            return Err(AlienError::Other);
        }
    }
    Ok(())
}

pub fn load_page_fault_exception_handler(addr: usize) -> AlienResult<()> {
    let info = {
        let process = current_process().unwrap();
        process.access_inner().do_load_page_fault(addr)
    };
    if info.is_err() {
        return Err(AlienError::Other);
    }
    let (file, buf, offset) = info.unwrap();
    let _r = vfs_read_file::<VfsProvider>(file, buf, offset);
    Ok(())
}

pub fn store_page_fault_exception_handler(addr: usize) -> AlienResult<()> {
    let process = current_process().unwrap();
    trace!(
        "[pid: {}] do store page fault addr:{:#x}",
        process.get_pid(),
        addr
    );
    let res = process.access_inner().do_store_page_fault(addr)?;
    if res.is_some() {
        let (file, buf, offset) = res.unwrap();
        let _r = vfs_read_file::<VfsProvider>(file, buf, offset);
    }
    Ok(())
}

/// the solution for illegal instruction
pub fn illegal_instruction_exception_handler() -> AlienResult<()> {
    Err(AlienError::Other)
}
