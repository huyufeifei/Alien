use crate::{SharedHeapAllocator, TaskShimImpl};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use config::{FRAME_BITS, FRAME_SIZE};
use core::ops::Range;
use core::sync::atomic::AtomicBool;
use interface::{
    BlkDeviceDomain, CacheBlkDeviceDomain, FsDomain, GpuDomain, InputDomain, RtcDomain,
};
use ksync::Mutex;
use libsyscall::{DeviceType, Syscall};
use log::{info, warn};
use platform::config::DEVICE_SPACE;
use platform::iprint;
use spin::Lazy;

static DOMAIN_PAGE_MAP: Lazy<Mutex<BTreeMap<u64, Vec<(usize, usize)>>>> =
    Lazy::new(|| Mutex::new(BTreeMap::new()));

static DOMAIN_SYSCALL: Lazy<Mutex<BTreeMap<u64, usize>>> =
    Lazy::new(|| Mutex::new(BTreeMap::new()));
static DOMAIN_SHARE_ALLOCATOR: Lazy<Mutex<BTreeMap<u64, usize>>> =
    Lazy::new(|| Mutex::new(BTreeMap::new()));
static DOMAIN_TASKSHIM_IMPL: Lazy<Mutex<BTreeMap<u64, usize>>> =
    Lazy::new(|| Mutex::new(BTreeMap::new()));

pub fn register_domain_syscall_resource(domain_id: u64, syscall_addr: usize) {
    DOMAIN_SYSCALL.lock().insert(domain_id, syscall_addr);
}

pub fn register_domain_heap_resource(domain_id: u64, heap_addr: usize) {
    DOMAIN_SHARE_ALLOCATOR.lock().insert(domain_id, heap_addr);
}

pub fn register_domain_taskshim_resource(domain_id: u64, taskshim_addr: usize) {
    DOMAIN_TASKSHIM_IMPL.lock().insert(domain_id, taskshim_addr);
}

pub struct DomainSyscall;

impl Syscall for DomainSyscall {
    fn sys_alloc_pages(&self, domain_id: u64, n: usize) -> *mut u8 {
        let n = n.next_power_of_two();
        let page = mem::alloc_frames(n);
        info!(
            "[Domain: {}] alloc pages: {}, range:[{:#x}-{:#x}]",
            domain_id,
            n,
            page as usize,
            page as usize + n * FRAME_SIZE
        );
        let mut binding = DOMAIN_PAGE_MAP.lock();
        let vec = binding.entry(domain_id).or_insert(Vec::new());
        vec.push((page as usize >> FRAME_BITS, n));
        page
    }

    fn sys_free_pages(&self, domain_id: u64, p: *mut u8, n: usize) {
        let n = n.next_power_of_two();
        info!("[Domain: {}] free pages: {}, ptr: {:p}", domain_id, n, p);
        let mut binding = DOMAIN_PAGE_MAP.lock();
        let vec = binding.entry(domain_id).or_insert(Vec::new());
        let start = p as usize >> FRAME_BITS;
        vec.retain(|(s, _)| *s != start);
        mem::free_frames(p, n);
    }

    fn sys_write_console(&self, s: &str) {
        iprint!("{}", s);
    }

    fn sys_backtrace(&self, domain_id: u64) {
        warn!("[Domain: {}] panic, resource should recycle.", domain_id);
        let mut binding = DOMAIN_PAGE_MAP.lock();
        if let Some(vec) = binding.remove(&domain_id) {
            for (page_start, n) in vec {
                let page_end = page_start + n;
                warn!(
                    "[Domain: {}] free pages: [{:#x}-{:#x}]",
                    domain_id,
                    page_start << FRAME_BITS,
                    page_end << FRAME_BITS
                );
                mem::free_frames((page_start << FRAME_BITS) as *mut u8, n);
            }
        }
        drop(binding); // release lock
        {
            let mut binding = DOMAIN_SYSCALL.lock();
            let ptr = binding.remove(&domain_id).unwrap();
            let _syscall_resource = unsafe { Box::from_raw(ptr as *mut DomainSyscall) };
            drop(_syscall_resource);
            warn!("[Domain: {}] free DomainSyscall resource", domain_id);
        }
        {
            let mut binding = DOMAIN_SHARE_ALLOCATOR.lock();
            let ptr = binding.remove(&domain_id).unwrap();
            let _allocator = unsafe { Box::from_raw(ptr as *mut SharedHeapAllocator) };
            drop(_allocator);
            warn!("[Domain: {}] free SharedHeapAllocator resource", domain_id);
        }

        {
            let mut binding = DOMAIN_TASKSHIM_IMPL.lock();
            let ptr = binding.remove(&domain_id).unwrap();
            let _taskshim = unsafe { Box::from_raw(ptr as *mut TaskShimImpl) };
            drop(_taskshim);
            warn!("[Domain: {}] free TaskShimImpl resource", domain_id);
        }
        unwind();
    }

    fn sys_read_timer(&self) -> u64 {
        timer::read_timer() as u64
    }

    fn sys_device_space(&self, ty: DeviceType) -> Option<Range<usize>> {
        let find_f = |name: &str| -> Option<Range<usize>> {
            DEVICE_SPACE.iter().find_map(|(n, start, size)| {
                if *n == name {
                    Some(*start..*start + *size)
                } else {
                    None
                }
            })
        };
        match ty {
            DeviceType::Block => find_f("virtio-mmio-blk"),
            DeviceType::Uart => find_f("uart"),
            DeviceType::Gpu => find_f("virtio-mmio-gpu"),
            DeviceType::Input => find_f("virtio-mmio-mouse"),
            DeviceType::Rtc => find_f("rtc"),
        }
    }

    fn check_kernel_space(&self, start: usize, size: usize) -> bool {
        mem::is_in_kernel_space(start, size)
    }

    fn sys_get_blk_domain(&self) -> Option<Arc<dyn interface::BlkDeviceDomain>> {
        crate::query_domain("blk").map(|blk| unsafe { core::mem::transmute(blk) })
    }

    fn sys_get_shadow_blk_domain(&self) -> Option<Arc<dyn BlkDeviceDomain>> {
        crate::query_domain("shadow_blk").map(|blk| unsafe { core::mem::transmute(blk) })
    }

    fn sys_get_uart_domain(&self) -> Option<Arc<dyn interface::UartDomain>> {
        crate::query_domain("uart").map(|uart| unsafe { core::mem::transmute(uart) })
    }

    fn sys_get_gpu_domain(&self) -> Option<Arc<dyn GpuDomain>> {
        crate::query_domain("gpu").map(|gpu| unsafe { core::mem::transmute(gpu) })
    }

    fn sys_get_input_domain(&self, ty: &str) -> Option<Arc<dyn InputDomain>> {
        crate::query_domain(ty).map(|input| unsafe { core::mem::transmute(input) })
    }

    fn sys_get_rtc_domain(&self) -> Option<Arc<dyn RtcDomain>> {
        crate::query_domain("rtc").map(|rtc| unsafe { core::mem::transmute(rtc) })
    }
    fn sys_get_cache_blk_domain(&self) -> Option<Arc<dyn CacheBlkDeviceDomain>> {
        crate::query_domain("cache_blk").map(|cache_blk| unsafe { core::mem::transmute(cache_blk) })
    }

    fn blk_crash_trick(&self) -> bool {
        BLK_CRASH.load(core::sync::atomic::Ordering::Relaxed)
    }
}

static BLK_CRASH: AtomicBool = AtomicBool::new(true);

fn unwind() -> ! {
    BLK_CRASH.store(false, core::sync::atomic::Ordering::Relaxed);
    continuation::unwind()
}