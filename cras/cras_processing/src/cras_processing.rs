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

/// An `Iterator` that blocks any dc offset present in the signal.
pub struct DCFiltered<S>
where
    S: Signal,
{
    signal: S,
    r: <S::Frame as Frame>::Sample, // coefficient of the filter
    // Filter state.
    x_prev: <S::Frame as Frame>::Signed,
    y_prev: <S::Frame as Frame>::Signed,
}

impl<S> Signal for DCFiltered<S>
where
    S: Signal,
{
    type Frame = S::Frame;

    #[inline]
    fn next(&mut self) -> Self::Frame {
        let x = self.signal.next().to_signed_frame();
        // d = x - x_prev + r * y_prev;
        let neg_prev = self.x_prev.scale_amp((-1.0).to_sample());
        let d = x.add_amp(neg_prev).add_amp(
            self.y_prev
                .scale_amp(self.r.to_signed_sample().to_float_sample()),
        );
        self.y_prev = d;
        self.x_prev = x;
        Self::Frame::equilibrium().add_amp(d)
    }

    #[inline]
    fn is_exhausted(&self) -> bool {
        self.signal.is_exhausted()
    }
}

/// An 'Iterator' that mutes the samples from its signal.
pub struct Muted<S>
where
    S: Signal,
{
    signal: S,
}

impl<S> Signal for Muted<S>
where
    S: Signal,
{
    type Frame = S::Frame;

    #[inline]
    fn next(&mut self) -> Self::Frame {
        Self::Frame::equilibrium()
    }

    #[inline]
    fn is_exhausted(&self) -> bool {
        self.signal.is_exhausted()
    }
}

/// Addition to `Signal` that adds some basic processing functions.
trait DspProcessable {
    fn mute(self) -> Muted<Self>
    where
        Self: Sized + Signal,
    {
        Muted { signal: self }
    }

    fn db_block<S>(self, coefficient: S) -> DCFiltered<Self>
    where
        Self: Sized + Signal,
        S: Sample,
        Self::Frame: Frame<Sample = S>,
    {
        DCFiltered {
            signal: self,
            r: coefficient,
            y_prev: Self::Frame::equilibrium().to_signed_frame(),
            x_prev: Self::Frame::equilibrium().to_signed_frame(),
        }
    }
}

impl<S: Signal> DspProcessable for S {}

pub fn streams_ready<S>(streams: &mut [S])
where
    S: Signal,
{
    let mix_in = SignalMixer { signals: streams };

    mix_in.scale_amp(0.7.to_sample()).mute();
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
