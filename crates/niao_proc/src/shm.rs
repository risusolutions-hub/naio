//! Read-write shared memory segments backed by a named temp file + OS `mmap`.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::slice;
use std::sync::Mutex;

#[cfg(unix)]
extern "C" {
    fn mmap(
        addr: *mut core::ffi::c_void,
        length: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut core::ffi::c_void;
    fn munmap(addr: *mut core::ffi::c_void, length: usize) -> i32;
    fn msync(addr: *mut core::ffi::c_void, length: usize, flags: i32) -> i32;
}

#[cfg(windows)]
#[allow(non_snake_case)]
extern "system" {
    fn CreateFileMappingW(
        h_file: *mut core::ffi::c_void,
        attrs: *mut core::ffi::c_void,
        protect: u32,
        max_size_high: u32,
        max_size_low: u32,
        name: *const u16,
    ) -> *mut core::ffi::c_void;
    fn MapViewOfFile(
        h_map: *mut core::ffi::c_void,
        access: u32,
        offset_high: u32,
        offset_low: u32,
        bytes: usize,
    ) -> *mut core::ffi::c_void;
    fn UnmapViewOfFile(base: *const core::ffi::c_void) -> i32;
    fn FlushViewOfFile(base: *const core::ffi::c_void, bytes: usize) -> i32;
    fn CloseHandle(h: *mut core::ffi::c_void) -> i32;
}

static SHM_REGISTRY: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

fn with_registry<T>(f: impl FnOnce(&mut HashMap<String, PathBuf>) -> T) -> T {
    let mut slot = SHM_REGISTRY.lock().unwrap();
    if slot.is_none() {
        *slot = Some(HashMap::new());
    }
    f(slot.as_mut().unwrap())
}

fn shm_dir() -> PathBuf {
    std::env::temp_dir().join("niao_shm")
}

fn path_for(name: &str) -> PathBuf {
    shm_dir().join(format!("{name}.bin"))
}

fn sanitize_name(name: &str) -> io::Result<String> {
    if name.is_empty() || name.len() > 128 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "shared memory name must be 1..=128 chars",
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "shared memory name must be alphanumeric, '_' or '-'",
        ));
    }
    Ok(name.to_string())
}

enum Backing {
    Empty,
    #[cfg(unix)]
    Unix,
    #[cfg(windows)]
    Windows {
        mapping: *mut core::ffi::c_void,
    },
    #[cfg(not(any(unix, windows)))]
    Heap(Vec<u8>),
}

/// A read-write shared memory mapping.
pub struct SharedMemory {
    name: String,
    path: PathBuf,
    ptr: *mut u8,
    len: usize,
    backing: Backing,
}

unsafe impl Send for SharedMemory {}
unsafe impl Sync for SharedMemory {}

impl SharedMemory {
    /// Create a new segment (truncates if it already exists).
    pub fn create(name: &str, size: usize) -> io::Result<Self> {
        if size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shared memory size must be > 0",
            ));
        }
        let name = sanitize_name(name)?;
        fs::create_dir_all(shm_dir())?;
        let path = path_for(&name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        file.set_len(size as u64)?;
        with_registry(|r| r.insert(name.clone(), path.clone()));
        // SAFETY: file length is `size`; caller keeps file on disk until unlink.
        unsafe { Self::map_file(name, path, &file, size) }
    }

    /// Open an existing segment by name.
    pub fn open(name: &str) -> io::Result<Self> {
        let name = sanitize_name(name)?;
        let path = with_registry(|r| r.get(&name).cloned()).unwrap_or_else(|| path_for(&name));
        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("shared memory '{name}' not found"),
            ));
        }
        let file = OpenOptions::new().read(true).write(true).open(&path)?;
        let size = file.metadata()?.len() as usize;
        if size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "shared memory file is empty",
            ));
        }
        with_registry(|r| r.insert(name.clone(), path.clone()));
        unsafe { Self::map_file(name, path, &file, size) }
    }

    #[cfg(unix)]
    unsafe fn map_file(name: String, path: PathBuf, file: &File, len: usize) -> io::Result<Self> {
        use std::os::unix::io::AsRawFd;
        const PROT_READ: i32 = 1;
        const PROT_WRITE: i32 = 2;
        const MAP_SHARED: i32 = 1;
        let addr = mmap(
            std::ptr::null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            file.as_raw_fd(),
            0,
        );
        if addr as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            name,
            path,
            ptr: addr as *mut u8,
            len,
            backing: Backing::Unix,
        })
    }

    #[cfg(windows)]
    unsafe fn map_file(name: String, path: PathBuf, file: &File, len: usize) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        const PAGE_READWRITE: u32 = 0x04;
        const FILE_MAP_ALL_ACCESS: u32 = 0x000F_001F;
        let mapping = CreateFileMappingW(
            file.as_raw_handle() as *mut core::ffi::c_void,
            std::ptr::null_mut(),
            PAGE_READWRITE,
            0,
            len as u32,
            std::ptr::null(),
        );
        if mapping.is_null() {
            return Err(io::Error::last_os_error());
        }
        let base = MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, len);
        if base.is_null() {
            let err = io::Error::last_os_error();
            CloseHandle(mapping);
            return Err(err);
        }
        Ok(Self {
            name,
            path,
            ptr: base as *mut u8,
            len,
            backing: Backing::Windows { mapping },
        })
    }

    #[cfg(not(any(unix, windows)))]
    unsafe fn map_file(name: String, path: PathBuf, file: &File, len: usize) -> io::Result<Self> {
        use std::io::Read;
        let mut buf = vec![0u8; len];
        let mut f = file.try_clone()?;
        f.read_exact(&mut buf)?;
        Ok(Self {
            name,
            path,
            ptr: buf.as_mut_ptr(),
            len,
            backing: Backing::Heap(buf),
        })
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.ptr, self.len) }
        }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        if self.len == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(self.ptr, self.len) }
        }
    }

    pub fn read(&self, offset: usize, len: usize) -> io::Result<Vec<u8>> {
        if offset > self.len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "offset out of range",
            ));
        }
        let end = offset.saturating_add(len).min(self.len);
        Ok(self.as_slice()[offset..end].to_vec())
    }

    pub fn write(&mut self, offset: usize, data: &[u8]) -> io::Result<usize> {
        if offset > self.len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "offset out of range",
            ));
        }
        let end = offset.saturating_add(data.len()).min(self.len);
        let n = end - offset;
        self.as_mut_slice()[offset..end].copy_from_slice(&data[..n]);
        Ok(n)
    }

    /// Push dirty pages to the backing store (async on Unix).
    pub fn flush(&self) -> io::Result<()> {
        if self.len == 0 {
            return Ok(());
        }
        #[cfg(unix)]
        {
            const MS_ASYNC: i32 = 1;
            let rc = unsafe { msync(self.ptr as *mut core::ffi::c_void, self.len, MS_ASYNC) };
            if rc != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        #[cfg(windows)]
        {
            if let Backing::Windows { .. } = &self.backing {
                let ok = unsafe { FlushViewOfFile(self.ptr as *const core::ffi::c_void, self.len) };
                if ok == 0 {
                    return Err(io::Error::last_os_error());
                }
            }
        }
        Ok(())
    }

    pub fn unlink(name: &str) -> io::Result<bool> {
        let name = sanitize_name(name)?;
        let path = with_registry(|r| r.remove(&name)).unwrap_or_else(|| path_for(&name));
        if path.exists() {
            fs::remove_file(&path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        match &self.backing {
            Backing::Empty => {}
            #[cfg(unix)]
            Backing::Unix => unsafe {
                munmap(self.ptr as *mut core::ffi::c_void, self.len);
            },
            #[cfg(windows)]
            Backing::Windows { mapping } => unsafe {
                UnmapViewOfFile(self.ptr as *const core::ffi::c_void);
                CloseHandle(*mapping);
            },
            #[cfg(not(any(unix, windows)))]
            Backing::Heap(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_read_write_unlink() {
        let name = format!("t_{}", std::process::id());
        let mut shm = SharedMemory::create(&name, 64).unwrap();
        assert_eq!(shm.write(0, b"hello").unwrap(), 5);
        assert_eq!(&shm.read(0, 5).unwrap(), b"hello");
        #[cfg(not(windows))]
        {
            drop(shm);
            let shm2 = SharedMemory::open(&name).unwrap();
            assert_eq!(&shm2.read(0, 5).unwrap(), b"hello");
        }
        let _ = &shm;
        assert!(SharedMemory::unlink(&name).unwrap());
    }
}
