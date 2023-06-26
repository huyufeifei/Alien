use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::cell::UnsafeCell;

use kernel_sync::Mutex;

use crate::task::{current_process, Process, PROCESS_MANAGER, ProcessState};
use crate::task::schedule::schedule;

pub struct SleepLock<T> {
    data: UnsafeCell<T>,
    inner: Mutex<SleepLockInner>,
}

struct SleepLockInner {
    locked: bool,
    queue: VecDeque<Arc<Process>>,
}


pub struct SleepLockGuard<'a, T> {
    lock: &'a SleepLock<T>,
    data: &'a mut T,
}


impl<T> SleepLock<T> {
    pub fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
            inner: Mutex::new(SleepLockInner {
                locked: false,
                queue: VecDeque::new(),
            }),
        }
    }
    pub fn lock(&self) -> SleepLockGuard<T> {
        let mut inner = self.inner.lock();
        if inner.locked {
            let process = current_process().unwrap();
            process.update_state(ProcessState::Waiting);
            inner.queue.push_back(process.clone());
            drop(inner);
            schedule();
            self.lock()
        } else {
            inner.locked = true;
            SleepLockGuard {
                lock: self,
                data: unsafe { &mut *self.data.get() },
            }
        }
    }
}


impl<T> Drop for SleepLockGuard<'_, T> {
    fn drop(&mut self) {
        let mut inner = self.lock.inner.lock();
        inner.locked = false;
        if let Some(process) = inner.queue.pop_front() {
            process.update_state(ProcessState::Ready);
            let mut guard = PROCESS_MANAGER.lock();
            guard.push_back(process);
        }
    }
}

impl<T> core::ops::Deref for SleepLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data.deref()
    }
}

impl<T> core::ops::DerefMut for SleepLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data.deref_mut()
    }
}
