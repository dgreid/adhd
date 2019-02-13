// Copyright 2019 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::fs::OpenOptions;
use std::io::{BufReader, BufWriter, Read, Write};
use std::time::Instant;

use sample::{signal, Frame, Sample, Signal};

use cras_processing::DspProcessable;

/// Reads samples from the input, passes them through the biquads and writes to the
/// output specified in argument 2.
fn main() -> std::result::Result<(), Box<std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let input = args.get(1).unwrap_or_else(|| {
        println!("Usage: dc_block input.raw output.raw");
        std::process::exit(1);
    });
    let output = args.get(2).unwrap_or_else(|| {
        println!("Usage: dc_block input.raw output.raw");
        std::process::exit(1);
    });

    let in_file = OpenOptions::new().read(true).open(input)?;
    let num_frames = in_file.metadata()?.len() / 4;
    let mut in_file = BufReader::new(in_file);
    let mut frames_in = vec![[0.0f32; 2]; num_frames as usize];
    for frame_in in frames_in.iter_mut() {
        let mut buf = [0u8; 4];
        in_file.read_exact(&mut buf)?;
        *frame_in = [
            i16::from_le_bytes([buf[0], buf[1]]).to_sample::<f32>(),
            i16::from_le_bytes([buf[2], buf[3]]).to_sample::<f32>(),
        ];
    }

    let mut frames_out = vec![[0i16; 2]; num_frames as usize];

    const NQ: f64 = 48000.0 / 2.0;

    let start_time = Instant::now();
    let signal = signal::from_iter(frames_in.into_iter());
    let left = signal
        .clone()
        .map(|f| [*f.channel(0).unwrap(); 1])
        .peaking(380.0 / NQ, 3.0, -10.0)
        .peaking(720.0 / NQ, 3.0, -12.0)
        .peaking(1705.0 / NQ, 3.0, -8.0)
        .high_pass(218.0 / NQ, 0.7)
        .peaking(580.0 / NQ, 6.0, -8.0)
        .high_shelf(8000.0 / NQ, -2.0);
    let right = signal
        .map(|f| [*f.channel(1).unwrap(); 1])
        .peaking(450.0 / NQ, 3.0, -12.0)
        .peaking(721.0 / NQ, 3.0, -12.0)
        .peaking(1800.0 / NQ, 8.0, -10.2)
        .peaking(580.0 / NQ, 6.0, -8.0)
        .high_pass(250.0 / NQ, 0.6578)
        .high_shelf(8000.0 / NQ, -2.0);

    let out_signal = left
        .zip_map(right, |l, r| [l[0], r[0]])
        .map(|f: [f32; 2]| f.map(|s| s.to_sample::<i16>()));
    for (frame_out, signal_frame) in frames_out.iter_mut().zip(out_signal.until_exhausted()) {
        *frame_out = signal_frame;
    }
    let elapsed = start_time.elapsed();

    let mut out_file = BufWriter::new(
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(output)?,
    );

    unsafe {
        // It's fine. The slice is dropped right away, frames_out will certainly out live it.
        let out_buf: &[u8] = std::slice::from_raw_parts(
            frames_out.as_ptr() as *const u8,
            frames_out.len() * std::mem::size_of::<i16>(),
        );
        out_file.write(&out_buf)?;
    }
    println!(
        "processing took {} seconds {} nanoseconds for {} frames",
        elapsed.as_secs(),
        elapsed.subsec_nanos(),
        num_frames,
    );
    Ok(())
}
