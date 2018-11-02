// Copyright 2018 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
use std::io;
use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;
use std::ops::DerefMut;
use std::os::unix::io::{AsRawFd, RawFd};
use std::ptr;
use std::ptr::{read_volatile, write_volatile};
use std::slice;
use std::sync::Arc;

use libc;

use cras_common::gen::*;

#[repr(C, packed)]
pub struct cras_timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C, packed)]
struct cras_audio_shm_config {
    used_size: u32,
    frame_bytes: u32,
}

const CRAS_NUM_SHM_BUFFERS: usize = 2;
const CRAS_NUM_SHM_BUFFERS_MASK: u32 = (CRAS_NUM_SHM_BUFFERS - 1) as u32;

#[repr(C, packed)]
struct cras_audio_shm_area_header {
    config: cras_audio_shm_config,
    /// Use buffer A or B for reading. Must be 0 or 1.
    read_buf_idx: u32,
    /// Use buffer A or B for writing. Must be 0 or 1.
    write_buf_idx: u32,
    read_offset: [u32; CRAS_NUM_SHM_BUFFERS],
    write_offset: [u32; CRAS_NUM_SHM_BUFFERS],
    write_in_progress: [i32; CRAS_NUM_SHM_BUFFERS],
    volume_scaler: f32,
    mute: i32,
    callback_pending: i32,
    num_overruns: u32,
    ts: cras_timespec,
}

// [TODO] this could be merged with CrasAudioHeader
/// A wrapper for the raw structure `cras_audio_shm_area_header` with
/// size information of `samples` in `cras_audio_shm_area`.
pub struct CrasAudioShmAreaHeader<'a> {
    header: &'a mut cras_audio_shm_area_header,
    /// Size of the buffer for samples
    samples_len: usize,
}

impl<'a> CrasAudioShmAreaHeader<'a> {
    /// Gets write offset of the buffer and writable length.
    pub fn get_offset_and_len(&self) -> (usize, usize) {
        let used_size = self.get_used_size();
        let offset = unsafe {
            if (read_volatile(&self.header.write_buf_idx as *const u32) & 1u32)
                == 1
            {
                used_size
            } else {
                0usize
            }
        };
        (offset, used_size)
    }

    /// Gets number of bytes per frame from the shared memory structure.
    ///
    /// # Returns
    ///
    /// * `usize` - Number of bytes per frame
    pub fn get_frame_size(&self) -> usize {
        unsafe {
            read_volatile(&self.header.config.frame_bytes as *const u32)
                as usize
        }
    }

    /// Gets the size in bytes of the shared memory buffer.
    fn get_used_size(&self) -> usize {
        unsafe {
            read_volatile(&self.header.config.used_size as *const u32) as usize
        }
    }

    /// Gets index of the current written buffer.
    ///
    /// # Returns
    /// `u32` - the returned index is less then `CRAS_NUM_SHM_BUFFERS`.
    fn get_write_buf_idx(&self) -> u32 {
        unsafe {
            read_volatile(&self.header.write_buf_idx as *const u32)
                & CRAS_NUM_SHM_BUFFERS_MASK
        }
    }

    /// Switches the written buffer.
    fn switch_write_buf_idx(&mut self) {
        unsafe {
            // Switch write_buf_idx
            write_volatile(
                &mut self.header.write_buf_idx as *mut _,
                self.get_write_buf_idx() as u32 ^ 1u32,
            );
        }
    }

    /// Sets `write_offset[idx]` of to count of written bytes.
    ///
    /// # Arguments
    /// `idx` - 0 <= `idx` < `CRAS_NUM_SHM_BUFFERS`
    /// `offset` - 0 <= `offset` <= `used_size` && `offset` + `used_size` <=
    /// `samples.len()`. Writable size equals to 0 when offset equals to
    /// used_size.
    ///
    /// # Errors
    /// Returns error if index out of range.
    fn set_write_offset(&mut self, idx: usize, offset: u32) -> io::Result<()> {
        if idx >= CRAS_NUM_SHM_BUFFERS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of range.",
            ));
        }
        if offset as usize > self.get_used_size()
            || offset as usize + self.get_used_size() > self.samples_len
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of range.",
            ));
        }

        unsafe {
            write_volatile(
                &mut self.header.write_offset[idx] as *mut u32,
                offset,
            );
        }
        Ok(())
    }

    /// Sets `read_offset[idx]` of to count of written bytes.
    ///
    /// # Arguments
    /// `idx` - 0 <= `idx` < `CRAS_NUM_SHM_BUFFERS`
    /// `offset` - 0 <= `offset` <= `used_size` && `offset` + `used_size` <=
    /// `samples.len()`. Readable size equals to 0 when offset equals used_size.
    ///
    /// # Errors
    /// Returns error if index out of range.
    fn set_read_offset(&mut self, idx: usize, offset: u32) -> io::Result<()> {
        if idx >= CRAS_NUM_SHM_BUFFERS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of range.",
            ));
        }
        if offset as usize > self.get_used_size()
            || offset as usize + self.get_used_size() > self.samples_len
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of range.",
            ));
        }

        unsafe {
            write_volatile(
                &mut self.header.read_offset[idx] as *mut u32,
                offset,
            );
        }
        Ok(())
    }

    /// Commits written frames by switching the current buffer to the other one
    /// after samples are ready and indexes of current buffer are all set.
    /// - Sets `write_offset` of current buffer to `frame_count * frame_size`
    /// - Sets `read_offset` of current buffer to `0`.
    ///
    /// # Arguments
    ///
    /// * `frame_count` - Number of frames written to the current buffer
    ///
    /// # Errors
    ///
    /// * Returns error if `frame_count` is larger then buffer size
    ///
    /// This function is safe because we switch `write_buf_idx` after letting
    /// `write_offset` and `read_offset` ready and we read / write shared memory
    /// variables with volatile operations.
    pub fn commit_written_frames(
        &mut self,
        frame_count: u32,
    ) -> io::Result<()> {
        // Uses `u64` to prevent possible overflow
        let byte_count = frame_count as u64 * self.get_frame_size() as u64;
        if byte_count > self.get_used_size() as u64 {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "frame_count * frame_size is larger than used_size",
            ))
        } else {
            let idx = self.get_write_buf_idx() as usize;
            // Sets `write_offset` of current buffer to frame_count * frame_size
            self.set_write_offset(idx, byte_count as u32)?;
            // Sets `read_offset` of current buffer to `0`.
            self.set_read_offset(idx, 0)?;
            // Switch to the other buffer
            self.switch_write_buf_idx();
            Ok(())
        }
    }
}

/// A structure mapped to shared memory structure in
/// CRAS server.
#[repr(C, packed)]
pub struct cras_audio_shm_area {
    header: cras_audio_shm_area_header,
    samples: [u8],
}

impl cras_audio_shm_area {
    /// Returns the memory offset of samples slice. This function should have a
    /// constant results.
    fn offset_of_samples() -> usize {
        // 1000 is larger then the header part of `cras_audio_shm_area`
        let buf = [0u8; 1000];
        // Create a `cras_audio_shm_area` reference with zero sized `samples`
        let area: &cras_audio_shm_area =
            unsafe { mem::transmute((&buf, 1usize)) };
        area.samples.as_ptr() as usize - buf.as_ptr() as usize
    }
}

// A safe mmap function with error handling
fn cras_mmap(
    len: usize,
    prot: libc::c_int,
    fd: libc::c_int,
) -> io::Result<*mut libc::c_void> {
    match unsafe {
        libc::mmap(ptr::null_mut(), len, prot, libc::MAP_SHARED, fd, 0)
    } {
        libc::MAP_FAILED => Err(io::Error::last_os_error()),
        shm_ptr => Ok(shm_ptr),
    }
}

/// A generic structure for an opened shared memory file descriptor with its
/// real type `T`.
pub struct CrasShm<'a, T: 'a + ?Sized> {
    /// A shared memory fd for creating shared memory pointer `shm_ptr`.
    shm_fd: libc::c_int,
    /// A shared memory pointer created from `shm_fd`.
    /// The structure will call `munmap` for this pointer in `drop`.
    shm_ptr: *mut libc::c_void,
    /// Size of the shared memory.
    shm_size: usize,
    /// A reference point to the type `T` in the shared memory.
    data_view: &'a mut T,
}

/// A structure for shared `cras_server_state` from server.
pub type CrasShmServerState<'a> = CrasShm<'a, cras_server_state>;
// Implementation for sized type like `CrasShmServerState`
impl<'a, T: 'a + Sized> CrasShm<'a, T> {
    /// Attempts to return a shared and sized type `T` with given `shm_fd`.
    ///
    /// # Arguments
    /// * `shm_fd` - The shared memory for the sized type.
    ///
    /// # Errors
    /// * Failed if `mmap` failed.
    pub fn new(shm_fd: libc::c_int) -> io::Result<CrasShm<'a, T>> {
        let shm_size = mem::size_of::<T>();
        let shm_ptr = cras_mmap(shm_size, libc::PROT_READ, shm_fd)?;
        let data_view = unsafe { mem::transmute(shm_ptr) };
        Ok(CrasShm {
            shm_fd,
            shm_ptr,
            shm_size,
            data_view,
        })
    }
}

impl<'a, T: 'a + ?Sized> Deref for CrasShm<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.data_view
    }
}

impl<'a, T: 'a + ?Sized> DerefMut for CrasShm<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data_view
    }
}

impl<'a, T: 'a + ?Sized> Drop for CrasShm<'a, T> {
    /// Call `munmap` for `shm_ptr`.
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.shm_ptr, self.shm_size);
            libc::close(self.shm_fd);
        }
    }
}

/// CrasAudioBuffer and CrasAudioHeader both share the same CrasShareMemory but
/// they point to different parts of the memory.
/// The lifetime could be removed since the buffer only stay in stream and it
/// owns the data.
pub struct CrasAudioBuffer<'a> {
    addr: *mut u8,
    size: usize,
    phantom: PhantomData<&'a u8>,
    cras_shm: Arc<CrasSharedMemory>,
}

impl<'a> CrasAudioBuffer<'a> {
    pub fn get(&mut self, offset: isize, len: usize) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.addr.offset(offset), len) }
    }
}

pub struct CrasAudioHeader {
    addr: *mut cras_audio_shm_area_header,
    cras_shm: Arc<CrasSharedMemory>,
    samples_len: usize,
}

impl CrasAudioHeader {
    pub fn get(&mut self) -> CrasAudioShmAreaHeader {
        CrasAudioShmAreaHeader {
            header: unsafe{ self.addr.as_mut().unwrap() },
            samples_len: self.samples_len,
        }
    }
}

/// Create header and buffer from given shared memory.
pub fn create_header_and_buffers<'a>(cras_shm: CrasSharedMemory)
    -> (CrasAudioBuffer<'a>, CrasAudioHeader) {
    let cras_shm = Arc::new(cras_shm);
    let samples_offset = cras_audio_shm_area::offset_of_samples();
    // [TODO] check if this failed
    let samples_len = cras_shm.shm_size - samples_offset;
    let mut header = CrasAudioHeader {
            addr: cras_shm.shm_ptr as *mut _,
            cras_shm: cras_shm.clone(),
            samples_len,
    };

    let buffer_ptr = unsafe {
        cras_shm.shm_ptr.offset(samples_offset as isize)
    };
    (CrasAudioBuffer {
        addr: buffer_ptr as *mut _,
        size: samples_len,
        cras_shm: cras_shm.clone(),
        phantom: PhantomData,
    }, header)
}

pub struct CrasSharedMemory {
    shm_fd: libc::c_int,
    /// A shared memory pointer created from `shm_fd`.
    /// The structure will call `munmap` for this pointer in `drop`.
    shm_ptr: *mut libc::c_void,
    /// Size of the shared memory.
    shm_size: usize,
}

impl CrasSharedMemory {
    pub fn new(fd: CrasShmFd) -> io::Result<CrasSharedMemory> {
        let shm_ptr = cras_mmap(fd.shm_max_size, libc::PROT_READ | libc::PROT_WRITE, fd.fd)?;
        Ok(CrasSharedMemory{shm_fd: fd.fd, shm_ptr, shm_size: fd.shm_max_size})
    }
}

impl Drop for CrasSharedMemory {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.shm_ptr, self.shm_size);
            libc::close(self.shm_fd);
        }
    }
}

/// A structure wrapping a shared memory and its size.
/// * `fd` - The shared memory file descriptor, a `libc::c_int`.
/// * `shm_max_size` - Size of the shared memory.
pub struct CrasShmFd {
    fd: libc::c_int,
    shm_max_size: usize,
}

impl CrasShmFd {
    /// Creates a `CrasShmFd` by shared memory fd and size
    /// # Arguments
    /// * `fd` - A shared memory file descriptor.
    /// * `shm_max_size` - Size of the shared memory.
    ///
    /// # Returns
    /// * `CrasShmFd` - Wrap the input arguments without doing anything
    pub fn new(fd: libc::c_int, shm_max_size: usize) -> CrasShmFd {
        CrasShmFd { fd, shm_max_size }
    }
}

impl AsRawFd for CrasShmFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cras_audio_shm_area_header_switch_test() {
        let mut raw_header: cras_audio_shm_area_header =
            unsafe { mem::zeroed() };
        let mut header = CrasAudioShmAreaHeader {
            header: &mut raw_header,
            samples_len: 0,
        };

        assert_eq!(0, header.get_write_buf_idx());
        header.switch_write_buf_idx();
        assert_eq!(1, header.get_write_buf_idx());
    }

    #[test]
    fn cras_audio_shm_area_header_write_offset_test() {
        let mut raw_header: cras_audio_shm_area_header =
            unsafe { mem::zeroed() };
        let mut header = CrasAudioShmAreaHeader {
            header: &mut raw_header,
            samples_len: 20,
        };
        header.header.config.frame_bytes = 2;
        header.header.config.used_size = 5;

        assert_eq!(0, header.header.write_offset[0]);
        // Index out of bound
        assert!(header.set_write_offset(2, 5).is_err());
        // Offset out of bound
        assert!(header.set_write_offset(0, 6).is_err());
        assert_eq!(0, header.header.write_offset[0]);
        assert!(header.set_write_offset(0, 5).is_ok());
        assert_eq!(5, header.header.write_offset[0]);
    }

    #[test]
    fn cras_audio_shm_area_header_read_offset_test() {
        let mut raw_header: cras_audio_shm_area_header =
            unsafe { mem::zeroed() };
        let mut header = CrasAudioShmAreaHeader {
            header: &mut raw_header,
            samples_len: 20,
        };
        header.header.config.frame_bytes = 2;
        header.header.config.used_size = 5;

        assert_eq!(0, header.header.read_offset[0]);
        // Index out of bound
        assert!(header.set_read_offset(2, 5).is_err());
        // Offset out of bound
        assert!(header.set_read_offset(0, 6).is_err());
        assert_eq!(0, header.header.read_offset[0]);
        assert!(header.set_read_offset(0, 5).is_ok());
        assert_eq!(5, header.header.read_offset[0]);
    }

    #[test]
    fn cras_audio_shm_area_header_commit_written_frame_test() {
        let mut raw_header: cras_audio_shm_area_header =
            unsafe { mem::zeroed() };
        let mut header = CrasAudioShmAreaHeader {
            header: &mut raw_header,
            samples_len: 20,
        };

        header.header.config.frame_bytes = 2;
        header.header.config.used_size = 10;
        header.header.read_offset[0] = 10;
        assert_eq!(header.header.read_offset[0], 10);

        assert!(header.commit_written_frames(5).is_ok());
        assert_eq!(header.header.write_offset[0], 10);
        assert_eq!(header.header.read_offset[1], 0);
    }

    fn cras_shm_open_rw(name: &str, size: usize) -> libc::c_int {
        unsafe {
            let fd = libc::shm_open(
                &name as *const _ as *const _,
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0x0600,
            );
            assert!(fd > 0);
            libc::ftruncate(fd, size as i64);
            fd
        }
    }

    #[test]
    fn cras_mmap_pass() {
        let fd = cras_shm_open_rw("/cras_shm_test_1", 100);
        let rc = cras_mmap(10, libc::PROT_READ, fd);
        assert!(rc.is_ok());
    }

    #[test]
    fn cras_mmap_failed() {
        let rc = cras_mmap(10, libc::PROT_READ, -1);
        assert!(rc.is_err());
    }
}
