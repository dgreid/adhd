// Copyright 2019 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use num_traits::cast::{FromPrimitive, ToPrimitive};
use sample::{self, Frame, Sample, Signal};

pub fn process_to<S, T>(samples_in: &[S], samples_out: &mut [T])
where
    S: Default + ToPrimitive,
    T: Default + FromPrimitive,
{
    for (processed_sample, sample_out) in samples_in
        .iter()
        .map(|sample_in| sample_in.to_f32().unwrap_or(Default::default()))
        .map(|float_sample| T::from_f32(float_sample).unwrap_or(Default::default()))
        .zip(samples_out.iter_mut())
    {
        *sample_out = processed_sample as T;
    }
}

/// Provides an iterator that offsets the amplitude of every channel in each frame of the
/// /// signal by some sample value and yields the resulting frames.
//#[derive(Clone)]
pub struct SignalMixer<'a, S>
where
    S: Signal,
{
    signals: &'a mut [S],
    equilibrium_frame: <<S::Frame as Frame>::Sample as Sample>::Signed,
}

impl<'a, S> Signal for SignalMixer<'a, S>
where
    S: Signal,
{
    type Frame = S::Frame;

    #[inline]
    fn next(&mut self) -> Self::Frame {
        let mut frame = self.signals[0].next();
        for s in &mut self.signals[0..] {
            frame = frame.add_amp(s.next().to_signed_frame());
        }
        frame
    }

    #[inline]
    fn is_exhausted(&self) -> bool {
        self.signals.iter().any(|s| s.is_exhausted())
    }
}
/*
impl<'a, S, F> Signal for SignalMixer<'a, S, F>
where
    S: Signal,
    F: Frame<
        Sample = <<S::Frame as Frame>::Sample as Sample>::Signed,
        NumChannels = <S::Frame as Frame>::NumChannels,
    >,
{
    type Frame = S::Frame;

    fn next(&mut self) -> Self::Frame {
        let mut frame = self.equilibrium_frame;
        for s in self.signals {
            frame.add_amp(s.next());
        }
        frame
    }

    fn is_exhausted(&self) -> bool {
        self.signals.iter().any(|s| s.is_exhausted())
    }
}
*/
pub fn streams_ready<S>(streams: &mut [S])
where
    S: Signal,
{
}

#[cfg(test)]
mod tests {
    use super::*;

    use sample::signal;

    #[test]
    fn frames() {
        let pb_buf = [0x5500i16; 480];
        let mut out = [0i32; 480];
        process_to(&pb_buf, &mut out[..]);
        assert_eq!(out[0], 0x5500);
    }

    #[test]
    fn single_stream() {
        let signal_in = signal::rate(48000.0).const_hz(440.0).sine();
    }
}
