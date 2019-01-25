// Copyright 2019 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::fs::OpenOptions;
use std::io::{BufReader, BufWriter, Read, Write};
use std::time::Instant;

use sample::{signal, Frame, Sample, Signal};

use cras_processing::DspProcessable;

/// Reads samples from the input, passed them through the dc blocking filter and writes to the
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
    let mut frames_in = Vec::with_capacity(num_frames as usize);
    let mut out_buf = Vec::with_capacity(num_frames as usize * 4);
    let mut buf = [0u8; 4];
    while let Ok(4) = in_file.read(&mut buf) {
        frames_in.push([
            i16::from_le_bytes([buf[0], buf[1]]).to_sample::<f32>(),
            i16::from_le_bytes([buf[2], buf[3]]).to_sample::<f32>(),
        ]);
    }

    let out_signal = signal::from_iter(frames_in.into_iter()).dc_block(0.995);

    let mut frames_out = vec![[0.0f32; 2]; num_frames as usize];

    let start_time = Instant::now();
    for (frame_out, signal_frame) in frames_out.iter_mut().zip(out_signal.until_exhausted()) {
        *frame_out = signal_frame;
    }
    let elapsed = start_time.elapsed();

    for f in signal::from_iter(frames_out)
        .map(|f| f.map(|s| s.to_sample::<i16>()))
        //.map(|f: [i16; 2]| f.add_amp([0i16, 0i16]))
        .until_exhausted()
    {
        let _f2: [i16; 2] = f; // just for type annotation.
        let bytes = f[0].to_le_bytes();
        for b in &bytes {
            out_buf.push(*b);
        }
        let bytes = f[1].to_le_bytes();
        for b in &bytes {
            out_buf.push(*b);
        }
    }

    let mut out_file = BufWriter::new(
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(output)?,
    );

    out_file.write(&out_buf)?;
    println!(
        "processing took {} seconds {} nanoseconds for {} frames",
        elapsed.as_secs(),
        elapsed.subsec_nanos(),
        num_frames,
    );
    Ok(())
}
