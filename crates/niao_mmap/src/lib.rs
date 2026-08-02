//! `niao_mmap` — read-only memory mapping via direct OS FFI. No third-party crates.
//!
//! [`Mmap`] is a read-only view of a file's bytes:
//! * Unix — `mmap` / `munmap`.
//! * Windows — `CreateFileMappingW` / `MapViewOfFile` / `UnmapViewOfFile`.
//! * Other targets — falls back to reading the whole file into memory.
//!
//! `Mmap` dereferences to `&[u8]`. A read-only mapping is safe to share across
//! threads, so `Mmap` is declared `Send + Sync` (matching the crate it replaces).

use std::fs::File;
use std::io;
use std::ops::Deref;
use std::slice;

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
    fn CloseHandle(h: *mut core::ffi::c_void) -> i32;
}

/// How the mapped bytes are backed, and what to release on drop.
enum Backing {
    /// Empty file — no OS resources held.
    Empty,
    #[cfg(unix)]
    Unix,
    #[cfg(windows)]
    Windows { mapping: *mut core::ffi::c_void },
    #[cfg(not(any(unix, windows)))]
    Heap(Vec<u8>),
}

/// A read-only memory map of a file.
pub struct Mmap {
    ptr: *const u8,
    len: usize,
    backing: Backing,
}

// SAFETY: the mapping is read-only and immutable for the life of the `Mmap`, so
// concurrent reads from multiple threads are sound.
unsafe impl Send for Mmap {}
unsafe impl Sync for Mmap {}

impl Mmap {
    /// Map `file` read-only.
    ///
    /// # Safety
    /// The view reflects the file's bytes; if another process truncates or
    /// resizes the file while it is mapped, reads through this view may fault or
    /// observe changed data. The caller must ensure the file is not shrunk for
    /// the lifetime of the returned map.
    pub unsafe fn map(file: &File) -> io::Result<Mmap> {
        let len = file.metadata()?.len() as usize;
        if len == 0 {
            return Ok(Mmap {
                ptr: std::ptr::null(),
                len: 0,
                backing: Backing::Empty,
            });
        }
        Self::map_impl(file, len)
    }

    #[cfg(unix)]
    unsafe fn map_impl(file: &File, len: usize) -> io::Result<Mmap> {
        use std::os::unix::io::AsRawFd;
        const PROT_READ: i32 = 1;
        const MAP_PRIVATE: i32 = 2;
        let addr = mmap(
            std::ptr::null_mut(),
            len,
            PROT_READ,
            MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        );
        // MAP_FAILED is (void *)-1.
        if addr as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(Mmap {
            ptr: addr as *const u8,
            len,
            backing: Backing::Unix,
        })
    }

    #[cfg(windows)]
    unsafe fn map_impl(file: &File, len: usize) -> io::Result<Mmap> {
        use std::os::windows::io::AsRawHandle;
        const PAGE_READONLY: u32 = 0x02;
        const FILE_MAP_READ: u32 = 0x0004;
        let mapping = CreateFileMappingW(
            file.as_raw_handle() as *mut core::ffi::c_void,
            std::ptr::null_mut(),
            PAGE_READONLY,
            0,
            0,
            std::ptr::null(),
        );
        if mapping.is_null() {
            return Err(io::Error::last_os_error());
        }
        let base = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0);
        if base.is_null() {
            let err = io::Error::last_os_error();
            CloseHandle(mapping);
            return Err(err);
        }
        Ok(Mmap {
            ptr: base as *const u8,
            len,
            backing: Backing::Windows { mapping },
        })
    }

    #[cfg(not(any(unix, windows)))]
    unsafe fn map_impl(file: &File, len: usize) -> io::Result<Mmap> {
        use std::io::Read;
        let mut buf = Vec::with_capacity(len);
        let mut f = file.try_clone()?;
        f.read_to_end(&mut buf)?;
        let ptr = buf.as_ptr();
        let len = buf.len();
        Ok(Mmap {
            ptr,
            len,
            backing: Backing::Heap(buf),
        })
    }

    /// Number of mapped bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the mapped file was empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn as_slice(&self) -> &[u8] {
        if self.len == 0 {
            &[]
        } else {
            // SAFETY: `ptr`/`len` describe a valid read-only region (mapping or
            // owned buffer) that lives as long as `self`.
            unsafe { slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

impl Deref for Mmap {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsRef<[u8]> for Mmap {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Drop for Mmap {
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
    use std::io::Write;

    #[test]
    fn maps_file_contents() {
        let mut path = std::env::temp_dir();
        path.push(format!("niao_mmap_test_{}.bin", std::process::id()));
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"hello world").unwrap();
        }
        let file = File::open(&path).unwrap();
        let map = unsafe { Mmap::map(&file).unwrap() };
        assert_eq!(map.len(), 11);
        assert_eq!(&map[..], b"hello world");
        assert_eq!(map.as_ref(), b"hello world");
        assert_eq!(&map[0..5], b"hello");
        drop(map);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn maps_empty_file() {
        let mut path = std::env::temp_dir();
        path.push(format!("niao_mmap_empty_{}.bin", std::process::id()));
        File::create(&path).unwrap();
        let file = File::open(&path).unwrap();
        let map = unsafe { Mmap::map(&file).unwrap() };
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
        assert_eq!(&map[..], b"");
        drop(map);
        let _ = std::fs::remove_file(&path);
    }
}
