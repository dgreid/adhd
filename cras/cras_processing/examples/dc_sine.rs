// Copyright 2019 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};

use sample::{signal, Frame, Sample, Signal};

use cras_processing::DspProcessable;

/// Write a stereo, 440Hz sine wave with a DC offset to the filename specified in the argument.
fn main() -> std::result::Result<(), Box<std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let filename = args.get(1).unwrap_or_else(|| {
        println!("Usage: dc_sine filename");
        std::process::exit(1);
    });

    let mut file = BufWriter::new(
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(filename)?,
    );
    let sine = signal::rate(48000.0)
        .const_hz(440.0)
        .sine()
        .map(|f| f.map(|s| s.to_sample::<f32>()))
        .scale_amp(0.5)
        .add_amp(signal::gen(|| [0.1]))
        .map(|f: [f32; 1]| [f[0], f[0]])
        .dc_block(0.99)
        .map(|f| f.map(|s| s.to_sample::<i16>()))
        .map(|f: [i16; 2]| f.add_amp([0i16, 0i16]));

    for f in sine.take(48000 * 10) {
        file.write(&f[0].to_le_bytes())?;
        file.write(&f[1].to_le_bytes())?;
    }

    Ok(())
}
