//! 有关 Unix 协议族下的套接字结构。(目前有关的功能有待支持)
use alloc::string::String;
use alloc::sync::Arc;
use core::cell::UnsafeCell;

use pconst::LinuxErrno;

use crate::fs::file::KFile;

/// Unix 协议族下的套接字结构
#[allow(unused)]
pub struct UnixSocket {
    /// 文件路径，即套接字地址
    file_path: UnsafeCell<Option<String>>,
    /// 套接字数据
    file: UnsafeCell<Option<Arc<KFile>>>,
}

impl UnixSocket {
    /// 创建一个新的 Unix 协议族下的套接字结构
    pub fn new() -> Self {
        Self {
            file_path: UnsafeCell::new(None),
            file: UnsafeCell::new(None),
        }
    }

    /// UnixSocket 的 connect 操作
    pub fn connect(&self, _file_path: String) -> Result<(), LinuxErrno> {
        Err(LinuxErrno::ENOENT)
    }
}
