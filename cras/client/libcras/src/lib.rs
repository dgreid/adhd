// Copyright 2018 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
mod socket;
use self::socket::*;
use std::io;
use std::mem;
use std::{error, fmt};

extern crate libc;
use std::os::unix::io::RawFd;

extern crate cras_common;
use cras_common::gen::*;

mod cras_shm;
use cras_shm::*;
pub type CrasShmServerState<'a> = CrasShm<'a, cras_server_state>;

mod cras_stream;
use cras_stream::CrasStream;

extern crate sys_util;
use sys_util::*;

extern crate data_model;

use std::result::Result;

mod audio_fd;
use audio_fd::AudioFd;

#[derive(Debug)]
pub enum ErrorType {
    IoError(io::Error),
    SysUtilError(sys_util::Error),
    MessageTypeError,
    UnexpectedExitError,
    StringError(String),
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
            ErrorType::SysUtilError(ref err) => err.description(),
            ErrorType::MessageTypeError => "Message type error",
            ErrorType::UnexpectedExitError => "Unexpected exit",
            ErrorType::StringError(ref s) => s.as_str(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.error_type {
            ErrorType::IoError(ref err) => err.fmt(f),
            ErrorType::SysUtilError(ref err) => err.fmt(f),
            ErrorType::MessageTypeError => write!(f, "Message type error"),
            ErrorType::UnexpectedExitError => write!(f, "Unexpected exit"),
            ErrorType::StringError(ref s) => write!(f, "{}", s.as_str()),
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

impl From<sys_util::Error> for Error {
    fn from(sys_util_err: sys_util::Error) -> Error {
        Error {
            error_type: ErrorType::SysUtilError(sys_util_err),
        }
    }
}

impl From<String> for Error {
    fn from(string: String) -> Error {
        Error {
            error_type: ErrorType::StringError(string),
        }
    }
}

enum HandleResult {
    // client_id
    ClientId(u32),
    // stream_id, CrasShmFd
    ClientStreamShm(u32, CrasShmFd),
}

const BUFFER_LEN: usize = 100;
struct CrasClientMessage {
    fds: [RawFd; 2],
    _data: [u8; BUFFER_LEN],
    len: usize,
}

impl CrasClientMessage {
    fn new() -> CrasClientMessage {
        CrasClientMessage {
            fds: [-1; 2],
            _data: [0; BUFFER_LEN],
            len: 0,
        }
    }

    pub fn try_new(server_socket: &CrasServerSocket) -> Result<CrasClientMessage, String> {
        let mut message = CrasClientMessage::new();
        let (len, fd_nums) = server_socket
            .recv_with_fds(&mut message._data, &mut message.fds)
            .unwrap();

        if len < mem::size_of::<cras_client_message>() {
            Err(("Read message len is too short.").into())
        } else {
            message.len = len;
            message.check_fd_nums(fd_nums)?;
            Ok(message)
        }
    }

    fn check_fd_nums(&self, fd_nums: usize) -> Result<(), String> {
        match self.get_id() {
            CRAS_CLIENT_MESSAGE_ID::CRAS_CLIENT_CONNECTED => {
                if fd_nums == 1 {
                    Ok(())
                } else {
                    Err(format!("fd_nums not match"))
                }
            }
            CRAS_CLIENT_MESSAGE_ID::CRAS_CLIENT_STREAM_CONNECTED => {
                if fd_nums == 2 {
                    Ok(())
                } else {
                    Err(format!("fd_nums not match"))
                }
            }
            _ => Err("No such message id".into()),
        }
    }

    pub fn get_id(&self) -> u32 {
        let msg: &cras_client_message = unsafe { mem::transmute(self._data.as_ptr()) };
        msg.id
    }

    pub fn get_message<'a, T>(&self) -> &'a T {
        let msg: &T = unsafe { mem::transmute(self._data.as_ptr()) };
        msg
    }
}

fn handle_connect_message<'a>(server_socket: &CrasServerSocket) -> Result<HandleResult, Error> {
    let message = CrasClientMessage::try_new(&server_socket)?;
    match message.get_id() {
        CRAS_CLIENT_MESSAGE_ID::CRAS_CLIENT_CONNECTED => {
            let cmsg = message.get_message::<cras_client_connected>();
            // Unused server_state
            let _server_state = CrasShmServerState::new(message.fds[0])?;
            Ok(HandleResult::ClientId(cmsg.client_id))
        }
        CRAS_CLIENT_MESSAGE_ID::CRAS_CLIENT_STREAM_CONNECTED => {
            let cmsg = message.get_message::<cras_client_stream_connected>();
            let stream_id = cmsg.stream_id & ((1 << 16) - 1);
            println!("stream_id {}", &stream_id);
            Ok(HandleResult::ClientStreamShm(
                cmsg.stream_id,
                CrasShmFd::new(message.fds[0], cmsg.shm_max_size as usize),
            ))
        }
        _ => Err(Error::new(ErrorType::MessageTypeError)),
    }
}

pub struct CrasClient {
    server_socket: CrasServerSocket,
    client_id: i32,
    next_stream_id: u32,
}

// Utils
pub fn cras_audio_format_packed_new(
    format: i32,
    rate: usize,
    num_channels: usize,
) -> cras_audio_format_packed {
    let mut res = cras_audio_format_packed {
        format,
        frame_rate: rate as u32,
        num_channels: num_channels as u32,
        channel_layout: [-1; CRAS_CHANNEL::CRAS_CH_MAX as usize],
    };
    for i in 0..CRAS_CHANNEL::CRAS_CH_MAX {
        if i < num_channels as u32 {
            res.channel_layout[i as usize] = i as i8;
        } else {
            break;
        }
    }
    res
}

impl CrasClient {
    pub fn new() -> Result<CrasClient, Error> {
        let server_socket = CrasServerSocket::new()?;

        Ok(CrasClient {
            server_socket,
            client_id: -1,
            next_stream_id: 0,
        })
    }

    fn get_stream_id(&mut self) -> u32 {
        let res = self.next_stream_id;
        self.next_stream_id += 1;
        self.server_stream_id(&res)
    }

    fn server_stream_id(&self, stream_id: &u32) -> u32 {
        (self.client_id as u32) << 16 | stream_id
    }

    pub fn create_stream(
        &mut self,
        block_size: u32,
        direction: u32,
        rate: usize,
        channel_num: usize,
        format: snd_pcm_format_t,
    ) -> CrasStream {
        let stream_id = self.get_stream_id();

        let audio_format = cras_audio_format_packed_new(format, rate, channel_num);
        let msg_header = cras_server_message {
            length: mem::size_of::<cras_connect_message>() as u32,
            id: CRAS_SERVER_MESSAGE_ID::CRAS_SERVER_CONNECT_STREAM,
        };
        let server_cmsg = cras_connect_message {
            header: msg_header,
            proto_version: CRAS_PROTO_VER,
            direction,
            stream_id,
            stream_type: CRAS_STREAM_TYPE::CRAS_STREAM_TYPE_DEFAULT,
            buffer_frames: block_size,
            cb_threshold: block_size,
            flags: 0,
            format: audio_format,
            dev_idx: CRAS_SPECIAL_DEVICE::NO_DEVICE as u32,
            effects: 0,
        };

        // Create audio_fd
        let mut socket_vector: [libc::c_int; 2] = [-1, -1];
        let res = unsafe {
            libc::socketpair(
                libc::AF_UNIX,
                libc::SOCK_STREAM,
                0,
                socket_vector.as_mut_ptr() as *mut _ as *mut _,
            )
        };

        // Send `CRAS_SERVER_CONNECT_STREAM` message
        let res = &self
            .server_socket
            .send_server_message_with_fds(&server_cmsg, &socket_vector[1..]);
        println!("res: {:?}", res);
        unsafe { libc::close(socket_vector[1]) };
        let audio_fd = AudioFd::new(socket_vector[0]).unwrap();

        let mut stream = CrasStream::new(
            stream_id,
            self.server_socket.try_clone().unwrap(),
            block_size,
            direction,
            rate,
            channel_num,
            format,
            audio_fd,
        );

        loop {
            match self.wait_and_handle_server_message() {
                Ok(HandleResult::ClientStreamShm(stream_id, shm_fd)) => {
                    stream.init_shm(shm_fd).unwrap();
                    break;
                }
                Ok(_) => (),
                Err(_) => break, // TODO - should return an error instead.
            };
        }

        stream
    }

    fn wait_and_handle_server_message(&self) -> Result<HandleResult, Error> {
        #[derive(PollToken)]
        enum Token {
            ServerMsg,
        }
        let poll_ctx: PollContext<Token> = PollContext::new()
            .and_then(|pc| pc.add(&self.server_socket, Token::ServerMsg).and(Ok(pc)))?;
        let events = poll_ctx.wait()?;
        for event in events.iter_readable() {
            match event.token() {
                Token::ServerMsg => {
                    println!("poll by server msg!");
                    return handle_connect_message(&self.server_socket);
                }
            }
        }
        Err(Error::new(ErrorType::UnexpectedExitError))
    }

    pub fn new_and_connect_blocking() -> Result<CrasClient, Error> {
        let mut cras_client = CrasClient::new()?;
        cras_client.client_id = {
            match cras_client.wait_and_handle_server_message() {
                Ok(HandleResult::ClientId(res)) => res as i32,
                _ => {
                    error!("return error");
                    -1
                }
            }
        };

        println!("CrasClient id: {}", &cras_client.client_id);
        Ok(cras_client)
    }
}
