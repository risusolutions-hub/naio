//! Anonymous OS pipes — byte streaming between read/write handles.

use std::fs::File;
use std::io::{self, Read, Write};

#[cfg(unix)]
extern "C" {
    fn pipe(fds: *mut i32) -> i32;
}

#[cfg(windows)]
mod win {
    use std::os::windows::io::RawHandle;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreatePipe(
            h_read_pipe: *mut RawHandle,
            h_write_pipe: *mut RawHandle,
            sa: *mut core::ffi::c_void,
            n_size: u32,
        ) -> i32;
    }
}

#[cfg(any(unix, windows))]
pub struct PipeReader {
    inner: Option<File>,
}

#[cfg(any(unix, windows))]
pub struct PipeWriter {
    inner: Option<File>,
}

#[cfg(not(any(unix, windows)))]
pub struct PipeReader {
    shared: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    pos: usize,
    open: bool,
}

#[cfg(not(any(unix, windows)))]
pub struct PipeWriter {
    shared: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    open: bool,
}

pub struct OsPipe {
    pub reader: PipeReader,
    pub writer: PipeWriter,
}

impl OsPipe {
    /// Create an anonymous OS pipe pair.
    pub fn new() -> io::Result<Self> {
        #[cfg(unix)]
        {
            let mut fds = [0i32; 2];
            let rc = unsafe { pipe(fds.as_mut_ptr()) };
            if rc != 0 {
                return Err(io::Error::last_os_error());
            }
            use std::os::fd::FromRawFd;
            let read = unsafe { File::from_raw_fd(fds[0]) };
            let write = unsafe { File::from_raw_fd(fds[1]) };
            Ok(Self {
                reader: PipeReader { inner: Some(read) },
                writer: PipeWriter { inner: Some(write) },
            })
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::{FromRawHandle, RawHandle};
            let mut read: RawHandle = std::ptr::null_mut();
            let mut write: RawHandle = std::ptr::null_mut();
            let ok = unsafe { win::CreatePipe(&mut read, &mut write, std::ptr::null_mut(), 0) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            let read = unsafe { File::from_raw_handle(read) };
            let write = unsafe { File::from_raw_handle(write) };
            Ok(Self {
                reader: PipeReader { inner: Some(read) },
                writer: PipeWriter { inner: Some(write) },
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let shared = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            Ok(Self {
                reader: PipeReader {
                    shared: std::sync::Arc::clone(&shared),
                    pos: 0,
                    open: true,
                },
                writer: PipeWriter { shared, open: true },
            })
        }
    }
}

impl PipeReader {
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        #[cfg(any(unix, windows))]
        {
            self.inner
                .as_mut()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "pipe read end closed"))?
                .read(buf)
        }
        #[cfg(not(any(unix, windows)))]
        {
            if !self.open {
                return Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    "pipe read end closed",
                ));
            }
            let data = self.shared.lock().unwrap();
            let avail = data.len().saturating_sub(self.pos);
            if avail == 0 {
                return Ok(0);
            }
            let n = avail.min(buf.len());
            buf[..n].copy_from_slice(&data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    pub fn close(&mut self) {
        #[cfg(any(unix, windows))]
        {
            self.inner = None;
        }
        #[cfg(not(any(unix, windows)))]
        {
            self.open = false;
        }
    }
}

impl PipeWriter {
    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        #[cfg(any(unix, windows))]
        {
            self.inner
                .as_mut()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotConnected, "pipe write end closed")
                })?
                .write_all(data)
        }
        #[cfg(not(any(unix, windows)))]
        {
            if !self.open {
                return Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    "pipe write end closed",
                ));
            }
            self.shared.lock().unwrap().extend_from_slice(data);
            Ok(())
        }
    }

    pub fn close(&mut self) {
        #[cfg(any(unix, windows))]
        {
            self.inner = None;
        }
        #[cfg(not(any(unix, windows)))]
        {
            self.open = false;
        }
    }
}
