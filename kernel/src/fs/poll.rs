use crate::task::{current_task, do_suspend};
use alloc::vec::Vec;
use constants::io::{PollEvents, PollFd};
use constants::AlienResult;
use constants::LinuxErrno;
use log::{info, warn};
use syscall_table::syscall_func;
use timer::TimeSpec;

/// 一个系统调用，用于在一些文件描述符上等待事件。作用与 [`pselect6`] 相似。
///
/// 与 'pselect6' 不同，`ppoll` 并不按照等待事件的类型将所有要等待的文件描述符分成`readfds`、`writefds`、`exceptfds`，
/// 而是按照需要等待的文件描述符，将其加入 `fds_ptr`，再对每一个文件描述符进行等待事件的约束。其中 `fds_ptr` 指向的是一个
/// [`PollFd'] 向量，每个 Pollfd 结构中都保存了文件描述符、等待事件类型和获取到的事件类型三方面信息。因此对于 `ppoll`，
/// 会周期性检测 `fds_ptr` 中是否有文件描述符发生了所要等待的事件，如果有，那么就把事件的类型记录在 Pollfd 结构的 revents
/// 字段下，并使得计数器自增。在 `fds_ptr` 指向的向量中所有的文件描述符都被遍历一遍后，如果有需要处理的事件，那么此时 `ppoll`
/// 会返回需要处理的事件个数。如果没有，和 'pselect6' 相同，`ppoll` 也会让渡 CPU 后循环查询，直到发生超时事件，此时会返回 0，
/// 表示没有收到需要处理的事件。
///
/// 参数：
/// + `fds_ptr`: 用于指明需要等待的文件描述符和等待的事件类型。具体可见 [`PollFd`] 结构 和 [`PollEvents`]结构。
/// + `nfds`: 用于指明需要等待的文件描述符中的最大值 + 1，用于作为 `fds` 中查询文件描述符是否含有事件需要处理的迭代过程的边界条件。
/// + `time`: 指明超时的时间限制，是一个 [`TimeSpec`] 结构的指针。根据不同取值，不同的效果如下：(目前需要为0，否则会导致 panic)
///     - 如果该值为空，那么select会一直等待需要处理的IO事件，永远不会超时；
///     - 如果该值不为空，但内部的时间被设为0时，表示即使没有发现需要处理的IO事件，也直接返回。
///     - 否则按照正常的超时时间计算。
/// + `mask`: 用于屏蔽某些信号。目前在 Alien 中未使用。(并且需要为0，否则会导致 panic)
///
/// 当因为检测到需要处理的IO事件返回时，ppoll 会返回接收到的需要处理的IO事件的总数;
/// 当因为超时而返回时，ppoll 会返回0；
/// 当因为接收到信号而返回时， ppoll 会返回 EINTR；
/// 当其他情况导致的函数执行异常，ppoll 将直接返回错误码。
///
/// Reference: [ppoll](https://man7.org/linux/man-pages/man2/ppoll.2.html)
#[syscall_func(73)]
pub fn ppoll(fds_ptr: usize, nfds: usize, time: usize, _mask: usize) -> AlienResult<isize> {
    let task = current_task().unwrap();
    let mut fds = Vec::<PollFd>::with_capacity(nfds);
    unsafe {
        fds.set_len(nfds);
    }
    task.access_inner()
        .copy_from_user_buffer(fds_ptr as *const PollFd, fds.as_mut_ptr(), nfds);

    info!("fds: {:?}", fds);
    let wait_time = if time != 0 {
        let time_spec = task.transfer_raw_ptr(time as *mut TimeSpec);
        Some(time_spec.to_clock() + TimeSpec::now().to_clock())
    } else {
        None
    }; // wait forever
    let mut res = 0;
    let task = current_task().unwrap();
    loop {
        for pfd in fds.iter_mut() {
            if let Some(file) = task.get_file(pfd.fd as usize) {
                let event = file.poll(pfd.events)?;
                if !event.is_empty() {
                    res += 1;
                }
                info!("[ppoll]: event: {:?}", event);
                pfd.revents = event;
            } else {
                // todo: error
                pfd.events = PollEvents::INVAL;
            }
        }

        if res > 0 {
            // copy to user
            task.access_inner()
                .copy_to_user_buffer(fds.as_ptr(), fds_ptr as *mut PollFd, nfds);
            info!("ppoll return {:?}", fds);
            return Ok(res as isize);
        }
        if let Some(wait_time) = wait_time {
            if wait_time <= TimeSpec::now().to_clock() {
                warn!("ppoll timeout");
                return Ok(0);
            }
        }
        info!("[poll] suspend");
        // suspend
        do_suspend();
        let task = current_task().unwrap();
        // interrupt by signal
        // let task = current_task().unwrap();
        let task_inner = task.access_inner();
        let receiver = task_inner.signal_receivers.lock();
        if receiver.have_signal() {
            return Err(LinuxErrno::EINTR.into());
        }
    }
}
