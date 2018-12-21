// Copyright 2018 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
use std::io;
use std::io::Write;
use std::mem;
use std::sync::mpsc::{Receiver, RecvError};
use std::{error, fmt};

use cras_common::gen::*;
use cras_shm::*;
use socket::CrasServerSocket;
use AudioFd;

#[derive(Debug)]
pub enum ErrorType {
    IoError(io::Error),
    RecvError(RecvError),
    MessageTypeError,
    NoShmError,
}

#[derive(Debug)]
pub struct Error {
    error_type: ErrorType,
}

impl Error {
    fn new(error_type: ErrorType) -> Error {
        Error { error_type }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match self.error_type {
            ErrorType::IoError(ref err) => err.description(),
            ErrorType::RecvError(ref err) => err.description(),
            ErrorType::MessageTypeError => "Message type error",
            ErrorType::NoShmError => "CrasAudioShmArea is not created",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.error_type {
            ErrorType::IoError(ref err) => err.fmt(f),
            ErrorType::RecvError(ref err) => err.fmt(f),
            ErrorType::MessageTypeError => write!(f, "Message type error"),
            ErrorType::NoShmError => write!(f, "CrasAudioShmArea is not created"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(io_err: io::Error) -> Error {
        Error {
            error_type: ErrorType::IoError(io_err),
        }
    }
}

pub struct CrasPlaybackBuffer<'a> {
    pub buffer: &'a mut [u8],
    write_frames: usize,
    pub frame_size: usize,
    drop: &'a mut CrasPlaybackDrop,
}

impl<'a> CrasPlaybackBuffer<'a> {
    fn new(
        frame_size: usize,
        buffer: &'a mut [u8],
        drop: &'a mut CrasPlaybackDrop,
    ) -> CrasPlaybackBuffer<'a> {
        CrasPlaybackBuffer {
            buffer,
            write_frames: 0,
            frame_size,
            drop,
        }
    }

    pub fn write_frames(&mut self, buf: &[u8]) -> io::Result<usize> {
        let count = self.buffer.write(buf)?;
        self.write_frames = count / self.frame_size;

        Ok(count / self.frame_size)
    }
}

impl<'a> Drop for CrasPlaybackBuffer<'a> {
    fn drop(&mut self) {
        self.drop.trigger(self.write_frames);
    }
}

pub trait CrasPlaybackDrop {
    fn trigger(&mut self, nframes: usize);
}

/// A structure to control the state of `CrasAudioShmAreaHeader` and
/// interact with server's audio thread through `AudioFd`.
pub struct CrasAudioShmAreaControl<'a> {
    pub header: CrasAudioShmAreaHeader<'a>,
    audio_fd: &'a AudioFd,
}

impl<'a> CrasAudioShmAreaControl<'a> {
    fn new(
        header: CrasAudioShmAreaHeader<'a>,
        audio_fd: &'a mut AudioFd,
    ) -> CrasAudioShmAreaControl<'a> {
        CrasAudioShmAreaControl { header, audio_fd }
    }
}

impl<'a> CrasPlaybackDrop for CrasAudioShmAreaControl<'a> {
    fn trigger(&mut self, nframes: usize) {
        println!("trigger!!");
        self.header.commit_written_frames(nframes as u32);
        self.audio_fd.data_ready(nframes as u32);
    }
}

/// Message returned from `CrasClient` through stream_channel
pub enum CrasStreamRc {
    ClientStreamShm(CrasShmFd),
    RemoveSuccess,
}

struct CrasStreamControls {
    audio_fd: AudioFd,
    header: Option<CrasAudioHeader>,
}

impl CrasPlaybackDrop for CrasStreamControls {
    fn trigger(&mut self, nframes: usize) {
        // [TODO] error handling
        self.header
            .as_mut()
            .unwrap()
            .get()
            .commit_written_frames(nframes as u32);
        self.audio_fd.data_ready(nframes as u32);
    }
}

#[allow(dead_code)]
pub struct CrasStream<'a> {
    stream_id: u32,
    server_socket: CrasServerSocket,
    block_size: u32,
    direction: u32,
    rate: usize,
    channel_num: usize,
    format: snd_pcm_format_t,
    /// A structure for stream to interact with server audio thread
    controls: CrasStreamControls,
    audio_buffer: Option<CrasAudioBuffer<'a>>,
    /// A receiver for message from `CrasClient`
    stream_channel: Receiver<CrasStreamRc>,
}

impl<'a> CrasStream<'a> {
    /// Create a CrasStream by given arguments.
    ///
    /// # Returns
    /// `CrasStream` - CRAS client stream.
    pub fn new(
        stream_id: u32,
        server_socket: CrasServerSocket,
        block_size: u32,
        direction: u32,
        rate: usize,
        channel_num: usize,
        format: snd_pcm_format_t,
        aud_fd: AudioFd,
        stream_channel: Receiver<CrasStreamRc>,
    ) -> CrasStream<'a> {
        CrasStream {
            stream_id,
            server_socket,
            block_size,
            direction,
            rate,
            channel_num,
            format,
            controls: CrasStreamControls {
                audio_fd: aud_fd,
                header: None,
            },
            audio_buffer: None,
            stream_channel,
        }
    }

    /// Receive shared memory fd and initialize stream audio shared memory area
    pub fn init_shm(&mut self, shm_fd: CrasShmFd) -> Result<(), Error> {
        let shm = CrasSharedMemory::new(shm_fd)?;
        let (buffer, header) = create_header_and_buffers(shm);
        self.controls.header = Some(header);
        self.audio_buffer = Some(buffer);
        Ok(())
    }

    fn wait_request_data(&self) -> Result<(), Error> {
        let aud_msg = self.controls.audio_fd.read_audio_message()?;
        match aud_msg.id {
            CRAS_AUDIO_MESSAGE_ID::AUDIO_MESSAGE_REQUEST_DATA => Ok(()),
            _ => Err(Error::new(ErrorType::MessageTypeError)),
        }
    }

    /// Gets next `CrasPlaybackBuffer` from stream.
    ///
    /// # Returns
    ///
    /// * `CrasPlaybackBuffer` - A buffer for user to write audio data
    pub fn next_playback_buffer(&mut self) -> Result<CrasPlaybackBuffer, Error> {
        // Wait for request audio message
        self.wait_request_data()?;
        // [TODO] error handling
        let frame_size = self
            .controls
            .header
            .as_mut()
            .unwrap()
            .get()
            .get_frame_size();
        let (offset, len) = self
            .controls
            .header
            .as_mut()
            .unwrap()
            .get()
            .get_offset_and_len();
        let buf = self
            .audio_buffer
            .as_mut()
            .unwrap()
            .get(offset as isize, len);
        Ok(CrasPlaybackBuffer::new(frame_size, buf, &mut self.controls))
    }
}

impl<'a> Drop for CrasStream<'a> {
    /// A blocking drop function, send message to `CrasClient` and wait for
    /// return message.
    /// Write error message to stderr if the method fail.
    fn drop(&mut self) {
        // Send stream disconnect message
        let msg_header = cras_server_message {
            length: mem::size_of::<cras_disconnect_stream_message>() as u32,
            id: CRAS_SERVER_MESSAGE_ID::CRAS_SERVER_DISCONNECT_STREAM,
        };
        let server_cmsg = cras_disconnect_stream_message {
            header: msg_header,
            stream_id: self.stream_id,
        };
        let _res = self // TODO - log errors if needed.
            .server_socket
            .send_server_message_with_fds(&server_cmsg, &[]);
    }
}

///[TODO] test
#[cfg(test)]
mod tests {
    use super::*;
}
