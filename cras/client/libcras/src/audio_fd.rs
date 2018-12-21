// Copyright 2018 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
use data_model::DataInit;
use std::io;
use std::mem;
use std::os::unix::io::{AsRawFd, RawFd};
use sys_util::*;

use libc;

use cras_common::gen::*;

// Safe because only structure with `Sized` and `DataInit` trait could
// use this method.
fn read_from_fd<T: Sized + DataInit>(fd: &AsRawFd) -> io::Result<T> {
    // Initialize data memory
    let mut aud_msg: T = unsafe { mem::zeroed() };
    let rc = unsafe {
        libc::read(
            fd.as_raw_fd(),
            &mut aud_msg as *mut _ as *mut _,
            mem::size_of::<T>(),
        )
    };
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else if rc < mem::size_of::<T>() as isize {
        Err(io::Error::new(io::ErrorKind::Other, "Truncated data."))
    } else {
        Ok(aud_msg)
    }
}

/// A structure for client to interact with server audio thread through
/// audio socketpair. The structure is created by one `sockfd` from the audio
/// socketpair.
pub struct AudioFd {
    fd: RawFd,
}

/// Audio message results to send
enum AudioMessage {
    /// * `id` - Audio message id, which is a `enum CRAS_AUDIO_MESSAGE_ID`
    /// * `frames` - A `u32` indicate the read or written frame count
    Success { id: u32, frames: u32 },
    /// * `error` - Error code when a error occurs
    Error(i32),
}

impl AudioFd {
    /// Creates `AudioFd` from a `RawFd`
    ///
    /// # Arguments
    /// `fd` - A `RawFd` which should be larger than `0`.
    ///
    /// # Errors
    /// Returns error when the input fd is less or equal to `0`.
    pub fn new(fd: RawFd) -> io::Result<AudioFd> {
        if fd <= 0 {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Input fd should be larger than 0",
            ))
        } else {
            Ok(AudioFd { fd })
        }
    }

    /// Blocks reading an `audio message`.
    ///
    /// # Returns
    /// `audio_message` - Audio message structure
    ///
    /// # Errors
    /// Returns io::Error if error occurs
    pub fn read_audio_message(&self) -> io::Result<audio_message> {
        #[derive(PollToken)]
        enum Token {
            AudioMsg,
        }
        let poll_ctx: PollContext<Token> =
            match PollContext::new().and_then(|pc| pc.add(self, Token::AudioMsg).and(Ok(pc))) {
                Ok(pc) => pc,
                Err(e) => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to create PollContext: {}", e),
                    ));
                }
            };
        let events = {
            match poll_ctx.wait() {
                Ok(v) => v,
                Err(e) => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to poll: {:?}", e),
                    ));
                }
            }
        };

        // Check the first readable message
        let tokens: Vec<Token> = events.iter_readable().map(|e| e.token()).collect();
        if tokens.len() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Unexpected exit"),
            ));
        }
        match tokens[0] {
            Token::AudioMsg => read_from_fd(self),
        }
    }

    /// Send audio message with given arguments
    ///
    /// # Arguments
    /// * `msg` - enum AudioMessage, which could be `Success` with message id
    /// and frames or `Error` with error code.
    ///
    /// # Errors
    /// Returns error if `libc::write` fail.
    fn send_audio_message(&self, msg: AudioMessage) -> io::Result<()> {
        let aud_reply_msg = match msg {
            AudioMessage::Success { id, frames } => audio_message {
                id,
                error: 0,
                frames,
            },
            AudioMessage::Error(error) => audio_message {
                id: 0,
                error,
                frames: 0,
            },
        };

        let res = unsafe {
            libc::write(
                self.as_raw_fd(),
                &aud_reply_msg as *const _ as *const _,
                mem::size_of::<audio_message>(),
            )
        };
        if res < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Send data ready message with written frame count
    ///
    /// # Arguments
    /// * `frames` - A `u32` indicate the written frame count
    pub fn data_ready(&self, frames: u32) -> io::Result<()> {
        self.send_audio_message(AudioMessage::Success {
            id: CRAS_AUDIO_MESSAGE_ID::AUDIO_MESSAGE_DATA_READY,
            frames,
        })
    }
}

impl Drop for AudioFd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

impl AsRawFd for AudioFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn create_socketpair() -> [libc::c_int; 2] {
        // Create audio_fd
        let mut socket_pair: [libc::c_int; 2] = [-1, -1];
        let res = unsafe {
            libc::socketpair(
                libc::AF_UNIX,
                libc::SOCK_STREAM,
                0,
                socket_pair.as_mut_ptr() as *mut _ as *mut _,
            )
        };
        socket_pair
    }

    #[test]
    fn audio_fd_new_with_zero_fd() {
        let audio_fd = AudioFd::new(0);
        assert!(audio_fd.is_err());
    }

    #[test]
    fn audio_fd_new_with_error_fd() {
        let audio_fd = AudioFd::new(-1);
        assert!(audio_fd.is_err());
    }

    #[test]
    fn audio_fd_data_ready_send_and_recv() {
        let socket_pair = create_socketpair();
        let audio_fd_send = AudioFd::new(socket_pair[0]).unwrap();
        let audio_fd_recv = AudioFd::new(socket_pair[1]).unwrap();
        audio_fd_send.data_ready(256).unwrap();

        let audio_msg = audio_fd_recv.read_audio_message().unwrap();
        let ref_audio_msg = audio_message {
            id: CRAS_AUDIO_MESSAGE_ID::AUDIO_MESSAGE_DATA_READY,
            error: 0,
            frames: 256,
        };
        assert_eq!(audio_msg.id, ref_audio_msg.id);
        assert_eq!(audio_msg.error, ref_audio_msg.error);
        assert_eq!(audio_msg.frames, ref_audio_msg.frames);
    }

    #[test]
    fn audio_fd_send_when_broken_pipe() {
        let socket_pair = create_socketpair();
        let audio_fd_client = AudioFd::new(socket_pair[0]).unwrap();
        unsafe { libc::close(socket_pair[1]) };
        let res = audio_fd_client.data_ready(256);
        //Broken pipe
        assert_eq!(
            res.unwrap_err().kind(),
            io::Error::from_raw_os_error(32).kind()
        );
    }
}
