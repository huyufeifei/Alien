pub use block_device::{QemuBlockDevice, QEMU_BLOCK_DEVICE};
pub use dtb::{init_dt, DEVICE_TABLE, PLIC};
pub use input::sys_event_get;
pub use mpci::pci_probe;


mod block_device;
mod dtb;
mod hal;
mod mpci;

pub mod gpu;
pub mod input;
pub mod rtc;
pub mod uart;
pub mod net;

pub trait DeviceBase: Sync + Send {
    fn hand_irq(&self);
}
