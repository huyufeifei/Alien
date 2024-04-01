use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    sync::Arc,
};
use core::ops::Index;

use basic::println;
use constants::io::OpenFlags;
use dynfs::DynFsKernelProvider;
use interface::{DomainType, VFS_ROOT_ID, VFS_STDERR_ID, VFS_STDIN_ID, VFS_STDOUT_ID};
use ksync::Mutex;
use rref::RRefVec;
use spin::{Lazy, Once};
use vfscore::{dentry::VfsDentry, fstype::VfsFsType, path::VfsPath, utils::VfsTimeSpec, VfsResult};

use crate::{
    insert_dentry, kfile::KernelFile, pipefs, procfs, ramfs::init_ramfs, shim::RootShimDentry, sys,
    VFS_MAP,
};

type SysFs = dynfs::DynFs<CommonFsProviderImpl, Mutex<()>>;
type ProcFs = dynfs::DynFs<CommonFsProviderImpl, Mutex<()>>;
type TmpFs = ::ramfs::RamFs<CommonFsProviderImpl, Mutex<()>>;
type RamFs = ::ramfs::RamFs<CommonFsProviderImpl, Mutex<()>>;
type PipeFs = dynfs::DynFs<CommonFsProviderImpl, Mutex<()>>;
#[derive(Clone)]
pub struct CommonFsProviderImpl;

impl DynFsKernelProvider for CommonFsProviderImpl {
    fn current_time(&self) -> VfsTimeSpec {
        VfsTimeSpec::new(0, 0)
    }
}

impl ::ramfs::RamFsProvider for CommonFsProviderImpl {
    fn current_time(&self) -> VfsTimeSpec {
        DynFsKernelProvider::current_time(self)
    }
}

pub static FS: Lazy<Mutex<BTreeMap<String, Arc<dyn VfsFsType>>>> =
    Lazy::new(|| Mutex::new(BTreeMap::new()));

static SYSTEM_ROOT_FS: Once<Arc<dyn VfsDentry>> = Once::new();

fn register_all_fs() {
    let procfs = Arc::new(ProcFs::new(CommonFsProviderImpl, "procfs"));
    let sysfs = Arc::new(SysFs::new(CommonFsProviderImpl, "sysfs"));
    let ramfs = Arc::new(RamFs::new(CommonFsProviderImpl));
    let tmpfs = Arc::new(TmpFs::new(CommonFsProviderImpl));
    let pipefs = Arc::new(PipeFs::new(CommonFsProviderImpl, "pipefs"));

    FS.lock().insert("procfs".to_string(), procfs);
    FS.lock().insert("sysfs".to_string(), sysfs);
    FS.lock().insert("ramfs".to_string(), ramfs);
    FS.lock().insert("tmpfs".to_string(), tmpfs);
    FS.lock().insert("pipefs".to_string(), pipefs);

    println!("register fs success");
}

/// Init the filesystem
pub fn init_filesystem() -> VfsResult<()> {
    register_all_fs();
    let ramfsfs_domain = basic::get_domain("ramfs").unwrap();
    let ramfs_root = match ramfsfs_domain {
        DomainType::FsDomain(ramfs) => {
            let mp = RRefVec::from_slice(b"/");
            let root_inode_id = ramfs.mount(&mp, None).unwrap();
            let shim_root_dentry = Arc::new(RootShimDentry::new(ramfs, root_inode_id));
            init_ramfs(shim_root_dentry)
        }
        _ => panic!("fatfs domain not found"),
    };

    SYSTEM_ROOT_FS.call_once(|| ramfs_root.clone());

    let procfs = FS.lock().index("procfs").clone();
    let procfs_root = procfs::init_procfs(procfs);

    let devfs_domain = basic::get_domain("devfs").unwrap();
    let devfs_root = match devfs_domain {
        DomainType::DevFsDomain(devfs) => {
            let mp = RRefVec::from_slice(b"/dev");
            let root_inode_id = devfs.mount(&mp, None).unwrap();
            let shim_root_dentry: Arc<dyn VfsDentry> =
                Arc::new(RootShimDentry::new(devfs.clone(), root_inode_id));
            crate::devfs::init_devfs(&devfs, &shim_root_dentry);
            shim_root_dentry
        }
        _ => panic!("fatfs domain not found"),
    };

    let sysfs_root = sys::init_sysfs(FS.lock().index("sysfs").clone());
    let tmpfs_root = FS
        .lock()
        .index("tmpfs")
        .clone()
        .i_mount(0, "/tmp", None, &[])?;

    pipefs::init_pipefs(FS.lock().index("pipefs").clone());

    let path = VfsPath::new(ramfs_root.clone(), ramfs_root.clone());
    path.join("proc")?.mount(procfs_root, 0)?;
    path.join("sys")?.mount(sysfs_root, 0)?;
    path.join("dev")?.mount(devfs_root, 0)?;
    path.join("tmp")?.mount(tmpfs_root.clone(), 0)?;
    let shm_ramfs = FS
        .lock()
        .index("ramfs")
        .clone()
        .i_mount(0, "/dev/shm", None, &[])?;
    path.join("dev/shm")?.mount(shm_ramfs, 0)?;

    // initrd::populate_initrd(ramfs_root.clone())?;

    {
        let mut map = VFS_MAP.write();
        map.insert(
            VFS_ROOT_ID,
            Arc::new(KernelFile::new(ramfs_root.clone(), OpenFlags::O_RDWR)),
        );
        map.insert(VFS_STDIN_ID, STDIN.clone());
        map.insert(VFS_STDOUT_ID, STDOUT.clone());
        map.insert(VFS_STDERR_ID, STDOUT.clone());
    }

    let fatfs_domain = basic::get_domain("fatfs").unwrap();
    match fatfs_domain {
        DomainType::FsDomain(fatfs) => {
            let blk_inode = path
                .join("/dev/sda")?
                .open(None)
                .expect("open /dev/sda failed");
            let id = insert_dentry(blk_inode, OpenFlags::O_RDWR);
            let mp = RRefVec::from_slice(b"/tests");
            let root_inode_id = fatfs.mount(&mp, Some(id)).unwrap();
            let shim_inode = Arc::new(RootShimDentry::new(fatfs, root_inode_id));
            path.join("tests")?.mount(shim_inode, 0)?;
        }
        _ => panic!("fatfs domain not found"),
    }
    println!("Vfs Tree:");
    vfscore::path::print_fs_tree(&mut VfsOutPut, ramfs_root.clone(), "".to_string(), true).unwrap();
    println!("Init filesystem success");
    Ok(())
}

struct VfsOutPut;
impl core::fmt::Write for VfsOutPut {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        basic::write_console(s);
        Ok(())
    }
}

/// Get the root filesystem of the system
#[inline]
pub fn system_root_fs() -> Arc<dyn VfsDentry> {
    SYSTEM_ROOT_FS.get().unwrap().clone()
}

/// Get the filesystem by name
#[inline]
pub fn system_support_fs(fs_name: &str) -> Option<Arc<dyn VfsFsType>> {
    FS.lock().iter().find_map(|(name, fs)| {
        if name == fs_name {
            Some(fs.clone())
        } else {
            None
        }
    })
}

type Stdin = KernelFile;
type Stdout = KernelFile;

pub static STDIN: Lazy<Arc<Stdin>> = Lazy::new(|| {
    let path = VfsPath::new(system_root_fs(), system_root_fs())
        .join("dev/tty")
        .unwrap();
    let dentry = path.open(None).unwrap();
    let file = KernelFile::new(dentry, OpenFlags::O_RDONLY);
    Arc::new(file)
});

pub static STDOUT: Lazy<Arc<Stdout>> = Lazy::new(|| {
    let path = VfsPath::new(system_root_fs(), system_root_fs())
        .join("dev/tty")
        .unwrap();
    let dentry = path.open(None).unwrap();
    let file = KernelFile::new(dentry, OpenFlags::O_WRONLY);
    Arc::new(file)
});
