// Copyright 2018 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
use std::env;
use std::fs::File;
use std::io::Read;

extern crate libcras;
use libcras::*;

extern crate cras_common;
use cras_common::gen::*;

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.len() {
        2 => {
            let mut cras_client = match CrasClient::new_and_connect_blocking() {
                Ok(cras_client) => cras_client,
                Err(error_msg) => {
                    println!("{}", error_msg);
                    return;
                }
            };
            let mut stream = cras_client.create_stream(
                256, // block_size
                CRAS_STREAM_DIRECTION::CRAS_STREAM_OUTPUT,
                44100, //rate
                2,     // channel_num
                _snd_pcm_format::SND_PCM_FORMAT_S16_LE,
            );

            // Play raw file and call get_playback_buffer 1000 times
            // than close the stream
            let mut file = File::open(&args[1]).unwrap();
            let mut local_buffer = [0u8; 1024];
            for _i in 0..1000 {
                let mut buffer = stream.next_playback_buffer().unwrap();

                // read to local buffer from file
                let read_count = file.read(&mut local_buffer).unwrap();
                println!("read_count: {}", read_count);

                let write_frames = buffer.write_frames(&local_buffer).unwrap();
                println!(
                    "write_frames: {}, frame_size {}",
                    write_frames, buffer.frame_size
                );
            }
            // Stream and client should gracefully be closed out of this scope
        }
        _ => {
            println!("cras_tests /path/to/playback_file.raw");
        }
    };
}
