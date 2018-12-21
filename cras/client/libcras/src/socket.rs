// Copyright 2018 The Chromium OS Authors. All rights reserved.
// // Use of this source code is governed by a BSD-style license that can be
// // found in the LICENSE file.
use std::cmp::Ordering;
use std::io;
use std::mem;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::slice::from_raw_parts;

extern crate libc;

extern crate sys_util;
use sys_util::ScmSocket;

// Return `sockaddr_un` for a given `path`
fn sockaddr_un<P: AsRef<Path>>(path: P) -> io::Result<(libc::sockaddr_un, libc::socklen_t)> {
    let mut addr = libc::sockaddr_un {
        sun_family: libc::AF_UNIX as libc::sa_family_t,
        sun_path: [0; 108],
    };

    // Check if the input is valid
    let bytes = path.as_ref().as_os_str().as_bytes();
    if bytes.len() == 0 || bytes[0] == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Empty path."));
    };
    match bytes.len().cmp(&addr.sun_path.len()) {
        Ordering::Greater | Ordering::Equal => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Input path size should smaller than the length of sun_path.",
            ));
        }
        _ => {}
    };

    // Copy data from `path` to `addr.sun_path`
    for (dst, src) in addr.sun_path.iter_mut().zip(bytes.iter()) {
        *dst = *src as libc::c_char;
    }

    // Follow the usage in CRAS server, but the len should be
    let len = mem::size_of::<libc::sockaddr_un>();
    Ok((addr, len as libc::socklen_t))
}

// This function is unsafe because only packed structure could use this.
unsafe fn as_u8_slice<T: Sized>(data: &T) -> &[u8] {
    from_raw_parts(data as *const T as *const u8, mem::size_of::<T>())
}

/// A Unix `SOCK_SEQPACKET` socket point to given `path`
pub struct UnixSeqpacket {
    fd: RawFd,
}

impl UnixSeqpacket {
    /// Open a `SOCK_SEQPACKET` connection to socket named by `path`.
    ///
    /// # Arguments
    /// * `path` - Path to `SOCK_SEQPACKET` socket
    ///
    /// # Returns
    /// A `UnixSeqpacket` structure point to the socket
    ///
    /// # Errors
    /// Return `io::Error` when error occurs.
    pub fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixSeqpacket> {
        unsafe {
            let fd = libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET, 0);
            let (addr, len) = sockaddr_un(path.as_ref())?;
            let ret = libc::connect(fd, &addr as *const _ as *const _, len);
            if ret < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(UnixSeqpacket { fd })
            }
        }
    }

    // Write data from a given buffer to the socket fd
    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        unsafe {
            let ret = libc::write(self.fd, buf.as_ptr() as *const _, buf.len());
            if ret < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(ret as usize)
            }
        }
    }

    // Get `RawFd` from this server_socket
    fn socket_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for UnixSeqpacket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

impl AsRawFd for UnixSeqpacket {
    fn as_raw_fd(&self) -> RawFd {
        self.socket_fd()
    }
}

const CRAS_SERVER_SOCKET_PATH: &'static str = "/run/cras/.cras_socket";
/// A socket connect to CRAS server with name equals to
/// `CRAS_SERVER_SOCKET_PATH`
pub struct CrasServerSocket {
    socket: UnixSeqpacket,
}

impl CrasServerSocket {
    pub fn new() -> io::Result<CrasServerSocket> {
        let socket = UnixSeqpacket::connect(CRAS_SERVER_SOCKET_PATH)?;
        Ok(CrasServerSocket { socket })
    }

    /// Send sized and packed server message to server socket
    /// # Arguments
    /// * `message` - A sized and packed message
    /// * `fds` - A slice of fds to send
    ///
    /// # Returns
    /// * Length of written bytes in `usize`
    ///
    /// # Errors
    /// Return error if the socket fails to write message to server
    pub fn send_server_message_with_fds<M: Sized>(
        &self,
        message: &M,
        fds: &[RawFd],
    ) -> io::Result<usize> {
        // We should make sure the input message is packed
        let msg_bytes = unsafe { as_u8_slice(message) };
        match fds.len() {
            0 => self.socket.write(msg_bytes),
            _ => match self.send_with_fds(msg_bytes, fds) {
                Ok(len) => Ok(len),
                Err(err) => Err(io::Error::new(io::ErrorKind::Other, format!("{}", err))),
            },
        }
    }
}

// Implement this for using `recv_with_fds` and 'send_with_fds'
impl ScmSocket for CrasServerSocket {
    fn socket_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

// Implement this for `PollContex`
impl AsRawFd for CrasServerSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;

    #[test]
    fn sockaddr_un_zero_length_input() {
        let res = sockaddr_un(Path::new(""));
        assert!(res.is_err());
    }

    #[test]
    fn sockaddr_un_zero_sized_input() {
        let res = sockaddr_un(Path::new("\x00\x01"));
        assert!(res.is_err());
    }

    #[test]
    fn sockaddr_un_long_input() {
        let res = sockaddr_un(Path::new(&"a".repeat(108)));
        assert!(res.is_err());
    }

    #[test]
    fn sockaddr_un_pass() {
        let path_size = 50;
        let res = sockaddr_un(Path::new(&"a".repeat(path_size)));
        assert!(res.is_ok());
        let (addr, len) = res.unwrap();
        assert_eq!(len, mem::size_of::<libc::sockaddr_un>() as u32);
        assert_eq!(addr.sun_family, libc::AF_UNIX as libc::sa_family_t);

        // Check `sun_path` in returned `sockaddr_un`
        let mut ref_sun_path = [0i8; 108];
        for i in 0..path_size {
            ref_sun_path[i] = 'a' as i8;
        }

        for (addr_char, ref_char) in addr.sun_path.iter().zip(ref_sun_path.iter()) {
            assert_eq!(addr_char, ref_char);
        }
    }

    #[test]
    fn unix_seqpacket_path_not_exists() {
        let res = UnixSeqpacket::connect("/path/not/exists");
        assert!(res.is_err());
    }

    fn tmpdir() -> PathBuf {
        env::temp_dir()
    }

    fn mock_server_socket(socket_path: &Path) {
        unsafe {
            let socket_fd = libc::socket(libc::PF_UNIX, libc::SOCK_SEQPACKET, 0);
            assert!(socket_fd > 0);
            // Bind socket to path
            let (addr, len) = sockaddr_un(socket_path).unwrap();
            libc::unlink(&addr.sun_path as *const _ as *const _);
            let rc = libc::bind(socket_fd, &addr as *const _ as *const _, len);
            assert_eq!(rc, 0);
            // Mark the `socket_fd` as passive socket
            let rc = libc::listen(socket_fd, 5);
            assert_eq!(rc, 0);
        };
    }

    #[test]
    fn unix_seqpacket_path_exists_pass() {
        let mut socket_path = tmpdir();
        socket_path.push("path_to_socket");
        mock_server_socket(socket_path.as_path());
        let res = UnixSeqpacket::connect(socket_path.as_path());
        assert!(res.is_ok());
    }
}
