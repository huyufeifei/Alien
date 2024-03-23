use crate::elf::VmmPageAllocator;
use crate::processor::add_task;
use crate::resource::{FdManager, HeapInfo, KStack, TidHandle};
use crate::task::{FsContext, Task, TaskInner, TaskStatus};
use crate::vfs_shim::{STDIN, STDOUT};
use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::sync::Arc;
use config::{FRAME_SIZE, USER_KERNEL_STACK_SIZE};
use constants::AlienResult;
use context::TaskContext;
use interface::VFS_ROOT_ID;
use ksync::Mutex;
use ptable::VmSpace;
use small_index::IndexAllocator;

pub fn ktread_create(func: fn(), name: &str) -> AlienResult<()> {
    let tid = Arc::new(TidHandle::new().unwrap());
    let pid = tid.clone();
    let k_stack = KStack::new(USER_KERNEL_STACK_SIZE / FRAME_SIZE);
    // fake kspace
    let kspace = VmSpace::<VmmPageAllocator>::new();
    let k_stack_top = k_stack.top();
    let func_ptr = func as usize;
    let task = Task {
        tid,
        kernel_stack: k_stack,
        pid,
        address_space: Arc::new(Mutex::new(kspace)),
        fd_table: {
            let mut fd_table = FdManager::new();
            fd_table.insert(STDIN.clone());
            fd_table.insert(STDOUT.clone());
            fd_table.insert(STDOUT.clone());
            Arc::new(Mutex::new(fd_table))
        },
        threads: Arc::new(Mutex::new(IndexAllocator::new())),
        heap: Arc::new(Mutex::new(HeapInfo::new(0, 0))),
        inner: Mutex::new(TaskInner {
            name: name.to_string(),
            thread_number: 0,
            status: TaskStatus::Ready,
            parent: None,
            children: BTreeMap::new(),
            context: TaskContext::new(func_ptr, k_stack_top),
            fs_info: FsContext::new(VFS_ROOT_ID, VFS_ROOT_ID),
            exit_code: 0,
            clear_child_tid: 0,
            // user mode stack info
            stack: 0..0,
        }),
        send_sigchld_when_exit: false,
    };
    let task = Arc::new(task);
    add_task(task);
    Ok(())
}
